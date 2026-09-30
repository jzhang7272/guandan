//! Pure game logic: no IO, no tokio/axum, no clocks (TECH_SPEC.md §2).
//!
//! Files only import from files earlier in this chain (`test_util` may import
//! anything):
//!
//! ```text
//! error → card → ranking → combo → readings → hand → trick → deal
//!       → play_phase → tribute → level → match_ → view
//! ```

mod card;
mod combo;
mod deal;
mod error;
mod hand;
mod level;
mod match_;
mod play_phase;
mod ranking;
mod readings;
mod tribute;
mod trick;
mod view;

#[cfg(test)]
pub(crate) mod test_util;

// The public API as seen from `server/`. A `pub use` that nothing outside
// `rules/` uses yet trips the `unused_imports` lint (this is a binary crate,
// so there is no external user), so this list only holds what `server/` uses
// today. When your task starts using another `rules` item from `server/`, add
// it here rather than reaching into the submodule path.
pub use card::{Card, SeatId, Team};
pub use combo::{Combo, Play};
pub use deal::{DealResult, deal_hands};
pub use error::ActionError;
pub use level::Progress;
pub use match_::{Action, ActionOutcome, DealEnd, Match};
pub use ranking::Level;
pub use readings::readings;
pub use view::{MatchView, view_for};

// Test-only: the wire-format fixture tests (`server/protocol/fixture_tests.rs`,
// Task 0b) build every message with struct literals and need these types. If
// non-test `server/` code starts using one of them, move it into the list
// above and delete it here (a duplicate re-export is a compile error).
#[cfg(test)]
pub use card::{Face, Rank, full_deck};
#[cfg(test)]
pub use combo::Bomb;
#[cfg(test)]
pub use deal::{DealStart, Exchange};
#[cfg(test)]
pub use level::{DealSummary, resolve_deal};
#[cfg(test)]
pub use match_::GameState;
#[cfg(test)]
pub use ranking::{face_value, is_level_card, is_wildcard, natural_value};
#[cfg(test)]
pub use trick::{CompletedTrick, TrickEntry};
#[cfg(test)]
pub use view::{CardCount, DutyView, PhaseView, TributeTask};
