//! The daemon event loop — port of `KeyProcessor.run()` + the asyncio read
//! tasks, restructured around the action-stream processor.
//!
//! Architecture:
//! * one reader thread per grabbed keyboard → `mpsc::Sender<RawEvent>`;
//! * the main loop pulls events (`recv_timeout`, which doubles as the
//!   shutdown poll), feeds the [`ps_engine::Processor`], and executes the
//!   returned [`ps_engine::Action`]s against the virtual keyboard, the layout
//!   backend and the sound player;
//! * a signal thread (SIGINT/SIGTERM/SIGTSTP) raises a stop flag; the main
//!   loop notices within ≤50 ms and performs the Python `finally` cleanup
//!   (ungrab all keyboards, force-save stats).

use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use log::{error, info};

use ps_engine::{Action, Processor, RawEvent};
use ps_input::{find_keyboards, VirtualKeyboard};
use ps_layout::LayoutBackend;

/// Idle poll interval of the main loop (also the shutdown latency bound).
const LOOP_TICK: Duration = Duration::from_millis(50);
/// How often the real compositor layout is re-read (manual switches are
/// invisible to the daemon: Super+Space is consumed by the compositor).
const LAYOUT_SYNC_INTERVAL: Duration = Duration::from_millis(250);
/// Reader-thread idle sleep when no events are available.
const READER_TICK: Duration = Duration::from_millis(1);

fn system_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Resources needed to execute actions.
struct Executor {
    /// Shared with the GNOME backend (Super+Space injection uses the same
    /// virtual device, exactly like Python's `set_uinput`).
    vk: Arc<Mutex<VirtualKeyboard>>,
    backend: Box<dyn LayoutBackend>,
}

impl Executor {
    fn execute(&mut self, action: &Action) {
        match action {
            Action::Emit { code, value } => {
                if let Err(err) = self.vk.lock().unwrap().key(*code, *value) {
                    error!("uinput emit failed: {err}");
                }
            }
            Action::Passthrough { type_, code, value } => {
                // Keep the original event type (EV_MSC/EV_SYN/…).  Writing it
                // as EV_KEY would inject phantom keystrokes (found in the
                // first live test: a scancode of 42 typed a "3").
                let ev = evdev::InputEvent::new(*type_, *code, *value);
                if let Err(err) = self.vk.lock().unwrap().emit(&[ev]) {
                    error!("uinput passthrough failed: {err}");
                }
            }
            Action::Inject {
                code,
                value,
                delay_ms,
            } => {
                if let Err(err) = self.vk.lock().unwrap().key(*code, *value) {
                    error!("uinput inject failed: {err}");
                }
                if *delay_ms > 0 {
                    std::thread::sleep(Duration::from_millis(*delay_ms));
                }
            }
            Action::Pause { ms } => {
                std::thread::sleep(Duration::from_millis(*ms));
            }
            Action::SetLayout { index } => {
                self.backend.set_layout(*index);
            }
            Action::Sound { name } => play_sound(name),
        }
    }
}

/// Fire-and-forget sound (Python `subprocess.Popen(["paplay", …])`).
fn play_sound(name: &str) {
    let path = format!("/usr/share/sounds/freedesktop/stereo/{name}.oga");
    let _ = Command::new("paplay")
        .arg(&path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

/// Spawn a reader thread for one keyboard device.
fn spawn_reader(
    mut dev: evdev::Device,
    name: String,
    stop: Arc<AtomicBool>,
    tx: mpsc::Sender<RawEvent>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        info!("reading {}", name);
        loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            match dev.fetch_events() {
                Ok(events) => {
                    for ev in events {
                        if tx
                            .send(RawEvent {
                                type_: ev.event_type().0,
                                code: ev.code(),
                                value: ev.value(),
                            })
                            .is_err()
                        {
                            // Main loop gone — shut down.
                            return;
                        }
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(err) => {
                    error!("device read failed: {err}");
                    break;
                }
            }
            std::thread::sleep(READER_TICK);
        }
        info!("reader for {name} stopped");
    })
}

/// Run the daemon (Python `KeyProcessor.run`).  Returns after a stop signal.
pub fn run(mut processor: Processor, backend: Box<dyn LayoutBackend>, trace: bool) -> ! {
    // ── keyboard discovery (Python `_find_keyboards` + grab) ────────────
    let keyboards = find_keyboards();
    if keyboards.is_empty() {
        error!("No physical keyboards found.");
        std::process::exit(1);
    }

    // Virtual device from the first keyboard (Python `UInput.from_device(keyboards[0])`).
    let vk = match VirtualKeyboard::from_template(&keyboards[0]) {
        Ok(vk) => Arc::new(Mutex::new(vk)),
        Err(err) => {
            error!("failed to create uinput device: {err}");
            std::process::exit(1);
        }
    };
    // Python: `await asyncio.sleep(0.5)` — let uinput settle.
    std::thread::sleep(Duration::from_millis(500));

    // GNOME switching injects Super+Space through the same virtual device.
    let mut backend = backend;
    backend.set_virtual_keyboard(vk.clone());

    let (tx, rx) = mpsc::channel::<RawEvent>();
    let stop = Arc::new(AtomicBool::new(false));
    let mut grabbed_any = false;

    for mut dev in keyboards {
        let name = dev.name().unwrap_or("keyboard").to_string();
        if let Err(err) = dev.grab() {
            error!("AUDIT: Failed {name}: {err}");
            continue;
        }
        info!("AUDIT: Grabbed {name}");
        spawn_reader(dev, name, stop.clone(), tx.clone());
        grabbed_any = true;
        // Note: the Device handle moves into the reader thread.  Python
        // ungrabs on shutdown before `os._exit(0)`, but the kernel releases
        // grabs when the process exits and its fds close — the observable
        // behaviour is identical.
    }

    if !grabbed_any {
        error!("AUDIT: no keyboard could be grabbed");
        std::process::exit(1);
    }

    // Initial layout sync (Python: `real_idx = backend.get_current_index()`).
    let real_idx = backend.get_current_index();
    if let Some(layout) = processor.layout_set.layouts.get(real_idx) {
        let layout = layout.clone();
        processor.set_layout_state(&layout);
        info!("AUDIT: Initial layout synced to {layout}");
    }

    // App context refresh (Python `_refresh_front_app()` at startup).
    processor.refresh_front_app(system_now());

    // Signal thread: SIGINT/SIGTERM/SIGTSTP → stop flag.
    {
        let stop = stop.clone();
        std::thread::spawn(move || {
            use signal_hook::consts::signal::{SIGINT, SIGTERM, SIGTSTP};
            if let Ok(mut signals) = signal_hook::iterator::Signals::new([SIGINT, SIGTERM, SIGTSTP])
            {
                // Every signal (re)raises the flag; the main loop breaks
                // within LOOP_TICK and performs the cleanup.
                for _sig in signals.forever() {
                    info!("stop signal received");
                    stop.store(true, Ordering::Relaxed);
                }
            }
        });
    }

    info!("daemon running — Ctrl+C to stop");

    // ── main loop ────────────────────────────────────────────────────────
    let mut executor = Executor { vk, backend };
    let mut last_layout_sync = Instant::now();
    'main: loop {
        // Manual layout switches (Super+Space) never reach us — the
        // compositor consumes them.  Re-read the *real* layout periodically,
        // otherwise every word is decoded with a stale layout and detection
        // silently does the wrong thing.
        if last_layout_sync.elapsed() >= LAYOUT_SYNC_INTERVAL {
            last_layout_sync = Instant::now();
            let index = executor.backend.get_current_index();
            if let Some(layout) = processor.layout_set.layouts.get(index).cloned() {
                if processor.layout() != layout {
                    info!("AUDIT: external layout change → {layout}");
                    processor.set_layout_state(&layout);
                }
            }
        }

        match rx.recv_timeout(LOOP_TICK) {
            Ok(ev) => {
                let now = system_now();
                let actions = processor.on_event(ev, now);
                if trace {
                    trace_event(&ev, &actions);
                }
                for action in &actions {
                    executor.execute(action);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if stop.load(Ordering::Relaxed) {
                    break 'main;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break 'main,
        }
    }

    // ── cleanup (Python `finally` + `_on_stop_signal`) ───────────────────
    info!("shutting down");
    processor.stats.save(true);
    std::process::exit(0);
}

/// `--debug` trace: one line per event with the action summary.
fn trace_event(ev: &RawEvent, actions: &[Action]) {
    use std::io::Write;
    // EV_SYN (0) carries no information for us and doubled the trace size.
    if ev.type_ == 0 {
        return;
    }
    let mut line = format!("EV type={} code={} value={}", ev.type_, ev.code, ev.value);
    for a in actions {
        match a {
            Action::Emit { code, value } => {
                line.push_str(&format!(" | emit({code},{value})"));
            }
            Action::Passthrough { type_, code, value } => {
                line.push_str(&format!(" | pass({type_},{code},{value})"));
            }
            Action::Inject {
                code,
                value,
                delay_ms,
            } => {
                line.push_str(&format!(" | inject({code},{value},{delay_ms}ms)"));
            }
            Action::Pause { ms } => line.push_str(&format!(" | pause({ms}ms)")),
            Action::SetLayout { index } => line.push_str(&format!(" | set_layout({index})")),
            Action::Sound { name } => line.push_str(&format!(" | sound({name})")),
        }
    }
    let mut err = std::io::stderr().lock();
    let _ = writeln!(err, "{line}");
}
