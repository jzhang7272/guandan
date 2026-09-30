//! Levels and match progression (TECH_SPEC.md §3.10).

use serde::{Deserialize, Serialize};

use super::card::{Rank, Team};
use super::deal::{DealResult, FinishKind};
use super::ranking::Level;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    /// Indexed by `Team::index()`.
    pub(crate) team_levels: [Level; 2],
    /// `None` only before a match's first deal (the lobby may also set a
    /// declaring team for it, GAME_RULES.md house rule #9).
    pub(crate) declaring: Option<Team>,
    /// Failed A attempts per team, 0..=2.
    pub(crate) a_attempts: [u8; 2],
}

impl Default for Progress {
    /// A fresh match: both teams at Two, nobody declaring, no A attempts.
    fn default() -> Self {
        Progress {
            team_levels: [Level(Rank::Two), Level(Rank::Two)],
            declaring: None,
            a_attempts: [0, 0],
        }
    }
}

impl Progress {
    /// A lobby settings edit (GAME_RULES.md house rule #9). A team whose
    /// level changes gets its A-attempt counter reset to 0: the counter is
    /// about attempts at A, so editing a level starts that team's count over
    /// (otherwise a team edited from A down to K and back would keep its old
    /// failures). Whether `declaring: None` is allowed is up to the caller.
    pub fn with_settings(&self, team_levels: [Level; 2], declaring: Option<Team>) -> Progress {
        let mut a_attempts = self.a_attempts;
        for team in [Team::A, Team::B] {
            let t = team.index();
            if team_levels[t] != self.team_levels[t] {
                a_attempts[t] = 0;
            }
        }
        Progress {
            team_levels,
            declaring,
            a_attempts,
        }
    }

    /// The declaring team's level, or Two if `declaring` is `None`.
    pub fn deal_level(&self) -> Level {
        match self.declaring {
            Some(team) => self.team_levels[team.index()],
            None => Level(Rank::Two),
        }
    }
}

/// Steps forward `n` ranks, saturating at Ace.
pub fn advance_level(level: Level, n: u8) -> Level {
    // `Rank::ALL` is in natural order, so moving up a level is moving one
    // step right in that array; Ace is the last entry.
    let current = Rank::ALL
        .iter()
        .position(|&rank| rank == level.0)
        .expect("Rank::ALL contains every rank");
    let target = (current + usize::from(n)).min(Rank::ALL.len() - 1);
    Level(Rank::ALL[target])
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DealSummary {
    pub(crate) result: DealResult,
    pub(crate) before: Progress,
    pub(crate) after: Progress,
    /// A team that hit 3 failed A attempts this deal.
    pub(crate) dropped_to_two: Option<Team>,
    pub(crate) match_winner: Option<Team>,
}

/// All cross-deal scoring (TECH_SPEC.md §3.10 algorithm).
///
/// Levels are per team; the deal level is the *declaring* team's level. A
/// team only gets a shot at the match (an "A attempt") when it declares at
/// A; each failed attempt counts toward the three-strikes drop to Two
/// (GAME_RULES.md "Match End Condition", "Failing at A three times").
pub fn resolve_deal(before: &Progress, result: DealResult) -> DealSummary {
    let winner = result.winning_team();
    let kind = result.kind();

    // Step 1: the declaring team is making an A attempt iff it declares at A.
    let a_attempt_team = before
        .declaring
        .filter(|_| before.deal_level() == Level(Rank::Ace));

    // Step 2: a 1-2 or 1-3 win on an A attempt wins the match. Levels and
    // counters are left as they were.
    let wins_match = matches!(kind, FinishKind::OneTwo | FinishKind::OneThree);
    if a_attempt_team == Some(winner) && wins_match {
        let mut after = before.clone();
        after.declaring = Some(winner);
        return DealSummary {
            result,
            before: before.clone(),
            after,
            dropped_to_two: None,
            match_winner: Some(winner),
        };
    }

    let mut after = before.clone();
    let mut dropped_to_two = None;

    // Step 3: the winner advances from its OWN level, which is not the deal
    // level when the non-declaring team wins.
    let w = winner.index();
    after.team_levels[w] = advance_level(after.team_levels[w], kind.levels());

    // Step 4: any A attempt that reaches here failed (a loss, or a 1-4 win).
    // This runs after step 3 so a third failure on a 1-4 win still ends at
    // Two (the +1 from A saturates to A first, then the drop applies).
    if let Some(team) = a_attempt_team {
        let t = team.index();
        after.a_attempts[t] += 1;
        if after.a_attempts[t] >= 3 {
            after.team_levels[t] = Level(Rank::Two);
            after.a_attempts[t] = 0;
            dropped_to_two = Some(team);
        }
    }

    // Step 5: the winner declares next, even if it just dropped to Two.
    after.declaring = Some(winner);

    DealSummary {
        result,
        before: before.clone(),
        after,
        dropped_to_two,
        match_winner: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::card::SeatId;
    use Rank::{Ace, Five, Four, King, Nine, Queen, Seven, Six, Ten, Two};
    use Team::{A, B};

    fn progress(levels: [Rank; 2], declaring: Option<Team>, a_attempts: [u8; 2]) -> Progress {
        Progress {
            team_levels: [Level(levels[0]), Level(levels[1])],
            declaring,
            a_attempts,
        }
    }

    /// Seats 0 and 2 are team A, 1 and 3 team B.
    fn result(order: &[u8]) -> DealResult {
        DealResult {
            order: order.iter().map(|&i| SeatId::new(i).unwrap()).collect(),
        }
    }

    const A_ONE_TWO: &[u8] = &[0, 2];
    const A_ONE_THREE: &[u8] = &[0, 1, 2, 3];
    const A_ONE_FOUR: &[u8] = &[0, 1, 3, 2];
    const B_ONE_TWO: &[u8] = &[3, 1];
    const B_ONE_THREE: &[u8] = &[1, 0, 3, 2];
    const B_ONE_FOUR: &[u8] = &[1, 2, 0, 3];

    struct Case {
        name: &'static str,
        before: Progress,
        order: &'static [u8],
        after: Progress,
        dropped_to_two: Option<Team>,
        match_winner: Option<Team>,
    }

    #[test]
    fn resolve_deal_table() {
        let cases = [
            // --- Plain advancement ---
            Case {
                name: "first deal (no declaring team), A wins 1-2: +3 from Two",
                before: progress([Two, Two], None, [0, 0]),
                order: A_ONE_TWO,
                after: progress([Five, Two], Some(A), [0, 0]),
                dropped_to_two: None,
                match_winner: None,
            },
            Case {
                name: "first deal, B wins 1-4: +1, B declares",
                before: progress([Two, Two], None, [0, 0]),
                order: B_ONE_FOUR,
                after: progress([Two, Rank::Three], Some(B), [0, 0]),
                dropped_to_two: None,
                match_winner: None,
            },
            Case {
                name: "declaring A wins 1-3: +2",
                before: progress([Five, Two], Some(A), [0, 0]),
                order: A_ONE_THREE,
                after: progress([Seven, Two], Some(A), [0, 0]),
                dropped_to_two: None,
                match_winner: None,
            },
            Case {
                name: "declaring A wins 1-4: +1",
                before: progress([Five, Two], Some(A), [0, 0]),
                order: A_ONE_FOUR,
                after: progress([Six, Two], Some(A), [0, 0]),
                dropped_to_two: None,
                match_winner: None,
            },
            // --- Saturation at A ---
            Case {
                name: "Q + 1-2 saturates at A, no match win",
                before: progress([Queen, Two], Some(A), [0, 0]),
                order: A_ONE_TWO,
                after: progress([Ace, Two], Some(A), [0, 0]),
                dropped_to_two: None,
                match_winner: None,
            },
            Case {
                name: "K + 1-3 saturates at A",
                before: progress([Two, King], Some(B), [0, 0]),
                order: B_ONE_THREE,
                after: progress([Two, Ace], Some(B), [0, 0]),
                dropped_to_two: None,
                match_winner: None,
            },
            Case {
                name: "non-declaring team at K wins 1-2: saturates at A, becomes declaring",
                before: progress([Six, King], Some(A), [0, 0]),
                order: B_ONE_TWO,
                after: progress([Six, Ace], Some(B), [0, 0]),
                dropped_to_two: None,
                match_winner: None,
            },
            // --- Non-declaring team wins: advances from its own level ---
            Case {
                name: "non-declaring B at Four wins 1-3 over declaring A at Nine",
                before: progress([Nine, Four], Some(A), [0, 0]),
                order: B_ONE_THREE,
                after: progress([Nine, Six], Some(B), [0, 0]),
                dropped_to_two: None,
                match_winner: None,
            },
            Case {
                name: "non-declaring A at Two wins 1-2 over declaring B at Ten",
                before: progress([Two, Ten], Some(B), [0, 0]),
                order: A_ONE_TWO,
                after: progress([Five, Ten], Some(A), [0, 0]),
                dropped_to_two: None,
                match_winner: None,
            },
            // --- Match win: 1-2 / 1-3 at A while declaring ---
            Case {
                name: "declaring A at A wins 1-2: match win, nothing else changes",
                before: progress([Ace, Seven], Some(A), [1, 0]),
                order: A_ONE_TWO,
                after: progress([Ace, Seven], Some(A), [1, 0]),
                dropped_to_two: None,
                match_winner: Some(A),
            },
            Case {
                name: "declaring B at A wins 1-3: match win (even after 2 failures)",
                before: progress([Nine, Ace], Some(B), [0, 2]),
                order: B_ONE_THREE,
                after: progress([Nine, Ace], Some(B), [0, 2]),
                dropped_to_two: None,
                match_winner: Some(B),
            },
            // --- Failed A attempts ---
            Case {
                name: "declaring A at A wins 1-4: failure, stays at A, still declaring",
                before: progress([Ace, Seven], Some(A), [0, 0]),
                order: A_ONE_FOUR,
                after: progress([Ace, Seven], Some(A), [1, 0]),
                dropped_to_two: None,
                match_winner: None,
            },
            Case {
                name: "declaring A at A loses: failure, B advances from own level and declares",
                before: progress([Ace, Seven], Some(A), [1, 0]),
                order: B_ONE_TWO,
                after: progress([Ace, Ten], Some(B), [2, 0]),
                dropped_to_two: None,
                match_winner: None,
            },
            Case {
                name: "third failure via 1-4 win: A drops to Two, resets, still declares",
                before: progress([Ace, Seven], Some(A), [2, 0]),
                order: A_ONE_FOUR,
                after: progress([Two, Seven], Some(A), [0, 0]),
                dropped_to_two: Some(A),
                match_winner: None,
            },
            Case {
                name: "third failure via loss: A drops to Two, B declares",
                before: progress([Ace, Seven], Some(A), [2, 1]),
                order: B_ONE_THREE,
                after: progress([Two, Nine], Some(B), [0, 1]),
                dropped_to_two: Some(A),
                match_winner: None,
            },
            Case {
                name: "B's third failure, loss to A (also at A): B drops, A declares",
                before: progress([Ace, Ace], Some(B), [1, 2]),
                order: A_ONE_THREE,
                after: progress([Ace, Two], Some(A), [1, 0]),
                dropped_to_two: Some(B),
                match_winner: None,
            },
            // --- A team at A that isn't declaring ---
            Case {
                name: "non-declaring A at A wins 1-2: not an A attempt, no match win, A declares",
                before: progress([Ace, Five], Some(B), [1, 0]),
                order: A_ONE_TWO,
                after: progress([Ace, Five], Some(A), [1, 0]),
                dropped_to_two: None,
                match_winner: None,
            },
            Case {
                name: "non-declaring A at A loses: not an A attempt, counter unchanged",
                before: progress([Ace, Five], Some(B), [2, 0]),
                order: B_ONE_FOUR,
                after: progress([Ace, Six], Some(B), [2, 0]),
                dropped_to_two: None,
                match_winner: None,
            },
            Case {
                name: "both at A, B declaring, A wins 1-2: B's attempt fails, A declares",
                before: progress([Ace, Ace], Some(B), [0, 0]),
                order: A_ONE_TWO,
                after: progress([Ace, Ace], Some(A), [0, 1]),
                dropped_to_two: None,
                match_winner: None,
            },
        ];
        for case in cases {
            let summary = resolve_deal(&case.before, result(case.order));
            let expected = DealSummary {
                result: result(case.order),
                before: case.before,
                after: case.after,
                dropped_to_two: case.dropped_to_two,
                match_winner: case.match_winner,
            };
            assert_eq!(summary, expected, "case: {}", case.name);
        }
    }

    #[test]
    fn a_attempts_need_not_be_consecutive() {
        // A fails at A, B declares for a while, A wins its way back to
        // declaring and fails twice more: the (non-consecutive) third
        // failure drops A to Two.
        let mut p = progress([Ace, Four], Some(A), [0, 0]);
        let steps: [(&[u8], [u8; 2], Option<Team>); 5] = [
            (B_ONE_FOUR, [1, 0], None),   // A attempt lost: failure 1; B 4 → 5
            (B_ONE_THREE, [1, 0], None),  // B declares at 5 → 7; no A attempt
            (A_ONE_FOUR, [1, 0], None),   // non-declaring A wins, stays at A
            (A_ONE_FOUR, [2, 0], None),   // A attempt, 1-4: failure 2
            (B_ONE_TWO, [0, 0], Some(A)), // A attempt lost: failure 3 → drop
        ];
        for (order, attempts, dropped) in steps {
            let summary = resolve_deal(&p, result(order));
            assert_eq!(summary.after.a_attempts, attempts, "after {order:?}");
            assert_eq!(summary.dropped_to_two, dropped, "after {order:?}");
            assert_eq!(summary.match_winner, None);
            p = summary.after;
        }
        assert_eq!(p, progress([Two, Ten], Some(B), [0, 0]));
    }

    #[test]
    fn drop_after_one_four_makes_next_deal_level_two() {
        let before = progress([Ace, Seven], Some(A), [2, 0]);
        let summary = resolve_deal(&before, result(A_ONE_FOUR));
        assert_eq!(before.deal_level(), Level(Ace));
        assert_eq!(summary.after.deal_level(), Level(Two));
    }

    #[test]
    fn default_is_a_fresh_match_at_two() {
        assert_eq!(Progress::default(), progress([Two, Two], None, [0, 0]));
        // The lobby's JSON (LOBBY_FLOW_SPEC.md §4.2 `LobbyView.progress`).
        assert_eq!(
            serde_json::to_string(&Progress::default()).unwrap(),
            r#"{"team_levels":["Two","Two"],"declaring":null,"a_attempts":[0,0]}"#
        );
    }

    #[test]
    fn with_settings_resets_the_counter_only_of_a_team_whose_level_changed() {
        let before = progress([Ace, Ace], Some(A), [2, 1]);

        // Team B moves A → K: its counter restarts; team A keeps its 2.
        let edited = before.with_settings([Level(Ace), Level(King)], Some(B));
        assert_eq!(edited, progress([Ace, King], Some(B), [2, 0]));

        // Changing only the declaring team keeps both counters.
        let edited = before.with_settings([Level(Ace), Level(Ace)], None);
        assert_eq!(edited, progress([Ace, Ace], None, [2, 1]));

        // A team edited away from A and back again starts over at 0.
        let away = before.with_settings([Level(King), Level(Ace)], Some(A));
        let back = away.with_settings([Level(Ace), Level(Ace)], Some(A));
        assert_eq!(back, progress([Ace, Ace], Some(A), [0, 1]));
    }

    #[test]
    fn deal_level_is_declaring_teams_level_or_two() {
        // With nobody declaring (a match's first deal, unless the lobby set a
        // declaring team) the deal is at Two whatever the team levels are.
        assert_eq!(progress([Nine, Ten], None, [0, 0]).deal_level(), Level(Two));
        assert_eq!(
            progress([Nine, Ten], Some(A), [0, 0]).deal_level(),
            Level(Nine)
        );
        assert_eq!(
            progress([Nine, Ten], Some(B), [0, 0]).deal_level(),
            Level(Ten)
        );
    }

    #[test]
    fn advance_level_steps_and_saturates() {
        assert_eq!(advance_level(Level(Two), 0), Level(Two));
        assert_eq!(advance_level(Level(Two), 1), Level(Rank::Three));
        assert_eq!(advance_level(Level(Two), 3), Level(Five));
        assert_eq!(advance_level(Level(Nine), 2), Level(Rank::Jack));
        assert_eq!(advance_level(Level(Rank::Jack), 3), Level(Ace));
        assert_eq!(advance_level(Level(Queen), 3), Level(Ace));
        assert_eq!(advance_level(Level(King), 1), Level(Ace));
        assert_eq!(advance_level(Level(Ace), 1), Level(Ace));
        assert_eq!(advance_level(Level(Ace), 3), Level(Ace));
    }
}
