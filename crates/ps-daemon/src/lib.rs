//! PowerSwitcher daemon library: TOML configuration and the evdev event loop.
//!
//! The `ps` binary (`src/main.rs`) is a thin CLI over this library; the GUI
//! (`ps-gui`) reuses [`config::Config`] so both read and write exactly the same
//! `config.toml`.

pub mod config;
pub mod daemon;
pub mod paths;

pub use config::Config;
