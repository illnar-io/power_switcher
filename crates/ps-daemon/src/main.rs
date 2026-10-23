//! `ps` — the PowerSwitcher daemon binary (Rust port, phase 3).
//!
//! Subcommands mirror `python -m power_switcher`:
//! * `ps daemon` — grab keyboards, auto-switch layouts (the strangler target)
//! * `ps setup`  — write the default config and print udev rules
//! * `ps debug`  — list keyboards and echo key presses (no grab)
//! * `ps gui`    — launch the GTK4 settings window (`ps-gui`)
//!
//! The state-management subcommands (`snippets`, `exceptions`, `words`,
//! `stats`) remain in the Python CLI for now — they share file formats with
//! the Rust daemon and do not need the event loop (see handoff.md).

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use ps_daemon::paths::{layouts_json, profiles_dir};
use ps_daemon::{config, daemon, Config};

#[derive(Parser)]
#[command(
    name = "power-switcher",
    about = "PowerSwitcher — DE-independent layout auto-switcher for Linux Wayland",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the layout auto-switcher daemon
    Daemon {
        #[arg(long, default_value = "~/.config/power_switcher/config.toml")]
        config: String,
        #[arg(short, long)]
        verbose: bool,
        /// Disable auto-detect, hotkey only
        #[arg(long)]
        no_auto: bool,
        /// Print every event and its action stream to stderr
        #[arg(long)]
        debug: bool,
    },
    /// Generate default config and show udev rules
    Setup,
    /// Print keyboard events without grabbing (for device discovery)
    Debug {
        #[arg(long, default_value = "~/.config/power_switcher/config.toml")]
        config: String,
    },
    /// Open the settings GUI (launches the `power-switcher-gui` binary)
    Gui,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Daemon {
            config,
            verbose,
            no_auto,
            debug,
        } => {
            env_logger::Builder::from_env(
                env_logger::Env::default().default_filter_or(if verbose {
                    "debug"
                } else {
                    "info"
                }),
            )
            .format_timestamp_millis()
            .init();
            cmd_daemon(&config, no_auto, debug);
        }
        Command::Setup => cmd_setup(),
        Command::Debug { .. } => cmd_debug(),
        Command::Gui => cmd_gui(),
    }
}

/// Launch the GTK4 settings window.  The GUI is a separate binary (GTK must
/// not be linked into the evdev daemon); prefer the sibling build artifact,
/// then `$PATH`.  Installed packages rename the pair to avoid clobbering the
/// procps `ps` command.
fn cmd_gui() -> ! {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("power-switcher-gui"));
            candidates.push(dir.join("ps-gui"));
        }
    }
    candidates.push(PathBuf::from("power-switcher-gui"));
    candidates.push(PathBuf::from("ps-gui"));

    for bin in candidates {
        let status = std::process::Command::new(&bin).status();
        match status {
            Ok(status) => std::process::exit(status.code().unwrap_or(0)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                eprintln!("failed to launch {}: {err}", bin.display());
                std::process::exit(1);
            }
        }
    }
    eprintln!(
        "power-switcher-gui not found next to this binary or in $PATH \
         (cargo build --release -p ps-gui)"
    );
    std::process::exit(1);
}

fn cmd_daemon(config_path: &str, no_auto: bool, trace: bool) -> ! {
    ps_core::fsutil::harden_private_dir(&config::default_config_dir());
    let cfg = Config::load(config::expand_tilde(config_path))
        .unwrap_or_else(|err| panic!("failed to load config {config_path}: {err}"));

    let backend = ps_layout::detect_layout_backend().unwrap_or_else(|err| panic!("{err}"));

    let words_path = cfg.resolve_path("words", "words.json");
    let layout_set = ps_core::LayoutSet::create(
        &cfg.layouts.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        &profiles_dir(),
        &layouts_json(),
        words_path.as_deref(),
    )
    .unwrap_or_else(|err| panic!("failed to build layout set: {err}"));

    let stats_path = cfg.resolve_path("stats", "stats.json").unwrap();
    let stats = ps_core::Stats::load(stats_path);
    let exceptions_path = cfg.resolve_path("exceptions", "exceptions.json");
    let snippets_path = cfg.resolve_path("snippets", "snippets.json");

    let engine_config = ps_engine::ProcessorConfig {
        current_layout: cfg.default_layout.clone(),
        threshold: cfg.threshold,
        min_word_len: cfg.min_word_len,
        snippet_expand_space: cfg.snippet_expand_space,
        snippet_expand_enter: cfg.snippet_expand_enter,
        snippet_expand_tab: cfg.snippet_expand_tab,
        trigger_space: cfg.trigger_space,
        trigger_enter: cfg.trigger_enter,
        trigger_tab: cfg.trigger_tab,
        trigger_punctuation: cfg.trigger_punctuation,
        undo_words: cfg.undo_words,
        undo_seconds: cfg.undo_seconds,
        developer_mode: cfg.developer_mode,
        arrows_cancel: cfg.arrows_cancel,
        live_fix_enabled: cfg.live_fix_enabled,
        spell_check: cfg.spell_check,
        no_correct_after_switch: cfg.no_correct_after_switch,
        lr_shift_pause: cfg.lr_shift_pause,
        hotkey_convert: cfg.hotkey_convert.clone(),
        manual_only_last_word: cfg.manual_only_last_word,
        sound_enabled: cfg.sound_enabled,
        self_learning: cfg.self_learning,
        shift_tap_switch_layout: cfg.shift_tap_switch_layout,
        key_buffer_max_len: 100,
    };

    let focus = ps_focus::ActiveWindowProvider::new();
    let mut processor = ps_engine::Processor::new(
        layout_set,
        engine_config,
        snippets_path,
        stats,
        exceptions_path,
        focus,
    );
    // NOTE: Python's --no-auto only changed the banner (upstream bug); here it
    // actually starts the daemon in hotkey-only mode.
    if no_auto {
        processor.set_auto_paused(true);
    }

    println!("Layouts:      {}", cfg.layouts.join(" ↔ "));
    println!("Auto-detect:  {}", if no_auto { "OFF" } else { "ON" });
    println!(
        "Triggers:     space={} enter={} tab={} punctuation={}",
        cfg.trigger_space, cfg.trigger_enter, cfg.trigger_tab, cfg.trigger_punctuation
    );
    println!("Snippets:     {} loaded", processor.snippets_len());
    println!(
        "Undo:         words={} seconds={}",
        cfg.undo_words, cfg.undo_seconds
    );
    let manual = match ps_engine::hotkey::parse(&cfg.hotkey_convert) {
        Ok(Some(_)) => format!("double Shift / {}", cfg.hotkey_convert),
        Ok(None) => "double Shift".to_string(),
        Err(_) => format!(
            "double Shift (invalid hotkey_convert {:?} ignored)",
            cfg.hotkey_convert
        ),
    };
    println!("Hotkeys:      L+R Shift = pause/resume, manual convert = {manual}");
    println!(
        "Layout switch: {}",
        if cfg.shift_tap_switch_layout {
            "lone Shift tap (legacy)"
        } else {
            "desktop shortcut (Super+Space)"
        }
    );

    daemon::run(processor, backend, trace)
}

fn cmd_setup() {
    ps_core::fsutil::harden_private_dir(&config::default_config_dir());
    let config_path = config::default_config_dir().join("config.toml");
    if !config_path.exists() {
        let default = r#"[general]
layouts = ["en", "ru"]
default_layout = "en"

developer_mode = false
arrows_cancel = true
live_fix_enabled = true
no_correct_after_switch = 1.5
lr_shift_pause = true
manual_only_last_word = false
sound_enabled = true
self_learning = true

# A lone Shift tap switching the layout (legacy Caramba gesture).
# Off by default: use the desktop's Super+Space instead.
shift_tap_switch_layout = false

[detection]
threshold = 2.5
min_word_len = 3

[triggers]
space = true
enter = true
tab = true
punctuation = true
snippet_space = true
snippet_enter = true
snippet_tab = true

[undo]
words = 0
seconds = 3.0
"#;
        let _ = ps_core::fsutil::write_private(&config_path, default.as_bytes());
        println!("Created default config at {}", config_path.display());
    }

    println!(
        "\nTo allow running without sudo, add these udev rules to /etc/udev/rules.d/60-power_switcher.rules:"
    );
    println!(
        "KERNEL==\"uinput\", SUBSYSTEM==\"misc\", GROUP=\"input\", MODE=\"0660\", TAG+=\"uaccess\", OPTIONS+=\"static_node=uinput\""
    );
    println!(
        "KERNEL==\"event*\", SUBSYSTEM==\"input\", ENV{{ID_INPUT_KEYBOARD}}==\"1\", GROUP=\"input\", MODE=\"0660\", TAG+=\"uaccess\""
    );
    println!(
        "KERNEL==\"event*\", SUBSYSTEM==\"input\", ENV{{ID_INPUT_KEYBOARD}}==\"1\", ENV{{ID_SEAT}}==\"\", ENV{{ID_SEAT}}=\"seat0\""
    );
    println!("\nThen run: sudo udevadm control --reload-rules && sudo udevadm trigger");
    println!("Log out and back in so udev uaccess applies to your session.");
    println!("Only if that still fails: sudo usermod -aG input $USER");
    println!("Load the module now: sudo modprobe uinput");
}

fn cmd_debug() {
    // Python debug: looser filter — any device with EV_KEY capability.
    let mut kbds: Vec<(std::path::PathBuf, String)> = Vec::new();
    for (path, dev) in evdev::enumerate() {
        if dev.supported_events().contains(evdev::EventType::KEY) {
            kbds.push((path, dev.name().unwrap_or("<unnamed>").to_string()));
        }
    }
    println!("Found keyboards: {kbds:?}");
    if kbds.is_empty() {
        return;
    }
    println!("\nListening to: {} (no grab)", kbds[0].1);
    let mut dev = match evdev::Device::open(&kbds[0].0) {
        Ok(d) => d,
        Err(err) => {
            eprintln!("cannot open {}: {err}", kbds[0].0.display());
            return;
        }
    };
    loop {
        match dev.fetch_events() {
            Ok(events) => {
                for ev in events {
                    if ev.event_type().0 == ps_input::keycodes::EV_KEY && ev.value() == 1 {
                        println!(
                            "  code={:4}  name={:?}",
                            ev.code(),
                            evdev::KeyCode::new(ev.code())
                        );
                    }
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(err) => {
                eprintln!("read failed: {err}");
                return;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
