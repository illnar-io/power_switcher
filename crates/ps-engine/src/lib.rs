//! PowerSwitcher engine (Rust port, phase 2): the event processor.
//!
//! A faithful port of `power_switcher/core/key_processor.py`'s `KeyProcessor`
//! with one structural change that makes it testable: instead of performing
//! I/O (uinput writes, layout-backend calls, sounds) inline, [`Processor::on_event`]
//! is a **pure state machine** that consumes one raw input event (plus its
//! timestamp) and returns the ordered [`Action`] stream the daemon must
//! perform.  The daemon loop executes those actions against the real
//! hardware/backends; replay tests execute them against a recorder.
//!
//! What is preserved exactly:
//! * every branch of `_on_event` / `_update_buf` / `_maybe_auto` /
//!   `_replace_text` / `_expand_snippet` / `_do_manual_convert`;
//! * the injection queue: physical events arriving mid-injection are held and
//!   replayed afterwards (the Python 50 ms post-injection blackout was dropped:
//!   it also discarded key releases, sticking keys down);
//! * injection pacing (the 5/8/10 ms inter-event delays) as
//!   [`Action::Inject::delay_ms`];
//! * Caramba gestures: double-tap Shift converts, single Shift toggles,
//!   L+R Shift pauses.
//!
//! Known divergences (both improvements, documented in
//! `docs/RUST_MIGRATION.md`):
//! * Python ran mid-word conversions as `asyncio.create_task` — a race window
//!   where later keystrokes could interleave with the injection.  The Rust
//!   processor converts synchronously, which cannot interleave.
//! * Python's audit log went to `~/.cache/power_switcher/audit.log`; here
//!   audit messages use the `log` crate with an `AUDIT:` prefix.

pub mod hotkey;
pub mod processor;

pub use processor::{Action, Processor, ProcessorConfig, RawEvent, EV_KEY};
