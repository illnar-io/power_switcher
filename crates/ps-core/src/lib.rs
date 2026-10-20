//! PowerSwitcher detection engine (Rust port, phase 1).
//!
//! This crate is a **faithful port** of the Python engine in
//! `power_switcher/detection/ngram_detector.py`,
//! `power_switcher/maps/layout_set.py` and `power_switcher/core/`,
//! including the exact scoring weights, thresholds and decision branches.
//! Parity is enforced by golden tests that replay decisions exported from the
//! Python engine — see `tests/parity.rs` and `tools/export_parity_data.py` —
//! and by ported unit tests next to each module.
//!
//! Layer map (Python → Rust):
//! * `NGramDetector` → [`ngram::NGramDetector`]
//! * `keycode_map.build_char_map` / `LayoutSet.convert` → [`layouts::LayoutMaps`]
//! * `LayoutSet` (`decision_score`, `find_best_layout`, …) → [`layout_set::LayoutSet`]
//! * `core.key_buffer.KeyBuffer` / `SentenceBuffer` → [`key_buffer`]
//! * `core.undo_history.UndoHistory` → [`undo_history::UndoHistory`]
//! * `core.anti_resonance.AntiResonanceGuard` → [`anti_resonance::AntiResonanceGuard`]
//! * `core.snippets.SnippetStore` → [`snippets::SnippetStore`]
//! * `core.modifier_state.ModifierState` → [`modifier_state::ModifierState`]
//! * `KeyProcessor` text helpers → [`text`]

pub mod anti_resonance;
pub mod app_exceptions;
pub mod key_buffer;
pub mod layout_set;
pub mod layouts;
pub mod modifier_state;
pub mod ngram;
pub mod snippets;
pub mod stats;
pub mod text;
pub mod undo_history;
pub mod word_store;

pub use anti_resonance::AntiResonanceGuard;
pub use app_exceptions::ExceptionStore;
pub use key_buffer::{KeyBuffer, SentenceBuffer};
pub use layout_set::LayoutSet;
pub use modifier_state::{ModifierState, ShiftAction};
pub use ngram::NGramDetector;
pub use snippets::SnippetStore;
pub use stats::Stats;
pub use undo_history::{UndoEntry, UndoHistory};
pub use word_store::WordStore;
