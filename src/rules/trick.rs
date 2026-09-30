//! Tricks (TECH_SPEC.md §3.6).

use serde::{Deserialize, Serialize};

use super::card::SeatId;
use super::combo::Play;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trick {
    /// Everything that happened this trick, in order (for display).
    pub(crate) entries: Vec<TrickEntry>,
    /// The play to beat; `None` only before the lead.
    pub(crate) best: Option<(SeatId, Play)>,
    /// Who has passed since `best` was played; reset on every play.
    pub(crate) passed: [bool; 4],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrickEntry {
    Played { seat: SeatId, play: Play },
    Passed { seat: SeatId },
}

/// The most recently finished trick, kept so clients can still see the winning
/// play (and whether 接风 happened) after the table is cleared for the next lead.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletedTrick {
    pub(crate) entries: Vec<TrickEntry>,
    pub(crate) winner: SeatId,
    /// `== winner`, or `winner.partner()` on 接风.
    pub(crate) next_leader: SeatId,
}

impl Trick {
    /// An empty trick, waiting for its lead.
    pub fn new() -> Self {
        Trick::default()
    }

    /// Records a play: it becomes the play to beat, and everyone's earlier
    /// passes are forgotten (a pass only skips that one turn — GAME_RULES.md
    /// "Turn Play & Trick Resolution").
    pub(crate) fn record_play(&mut self, seat: SeatId, play: Play) {
        self.entries.push(TrickEntry::Played {
            seat,
            play: play.clone(),
        });
        self.best = Some((seat, play));
        self.passed = [false; 4];
    }

    pub(crate) fn record_pass(&mut self, seat: SeatId) {
        self.passed[seat.index()] = true;
        self.entries.push(TrickEntry::Passed { seat });
    }
}
