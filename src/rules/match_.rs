//! One deal of a match and its phase transitions (TECH_SPEC.md §3.11,
//! LOBBY_FLOW_SPEC.md §3.2). (`match` is a Rust keyword, hence the trailing
//! underscore.)
//!
//! A `Match` lives for exactly one deal: it starts from the progress so far
//! (and the previous deal's result, for tribute) and ends with
//! `ActionOutcome::DealOver`. What happens between deals (the lobby) is the
//! server's business.

use rand::Rng;
use serde::{Deserialize, Serialize};

use super::card::{Card, SeatId};
use super::combo::{Combo, Play};
use super::deal::{DealResult, DealStart, pick_first_leader};
use super::error::ActionError;
use super::hand::PlayerHand;
use super::level::{DealSummary, Progress, resolve_deal};
use super::play_phase::{PlayOutcome, PlayPhase};
use super::ranking::Level;
use super::tribute::{TributeOutcome, TributePhase, TributePlan, plan_tribute};
use super::trick::TrickEntry;

/// The only thing outside `rules/` that code touches; its methods are the only
/// way `GameState` changes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Match {
    /// The progress this deal started from; `resolve_deal` turns it into the
    /// next one when the deal ends.
    pub(crate) progress: Progress,
    pub(crate) state: GameState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameState {
    Tribute(TributePhase),
    Playing(PlayPhase),
}

/// How a deal ended: handed to the Room by `ActionOutcome::DealOver`, and
/// shown in the lobby afterwards.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DealEnd {
    /// The result, the progress before and after, and any drop to Two or
    /// match winner.
    pub(crate) summary: DealSummary,
    /// The trick in progress when the deal ended, ending with the
    /// deal-ending play. All of it was played face up, so it's public.
    pub(crate) final_trick: Vec<TrickEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    Play {
        cards: Vec<Card>,
        declared: Option<Combo>,
    },
    Pass,
    /// Take back your own play, if nobody has acted since (GAME_RULES.md
    /// house rule #10).
    TakeBack,
    PayTribute {
        card: Card,
    },
    ReturnTribute {
        card: Card,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionOutcome {
    /// State changed; the Room broadcasts.
    Applied,
    /// Nothing changed; the Room asks only this player.
    NeedsDeclaration { options: Vec<Play> },
    /// The play ended the deal. The `Match` is hollow from here on (its
    /// finish order and trick were moved out): the Room must drop it and use
    /// only the `DealEnd`.
    DealOver(DealEnd),
}

impl Match {
    /// Starts a deal (see TECH_SPEC.md §3.11). The Room deals the hands with
    /// `deal_hands`; tests pass exact ones.
    ///
    /// `last_result` `None` = the match's first deal: no tribute, and the rng
    /// picks the first leader — whatever the levels and declaring team are
    /// (GAME_RULES.md house rule #9). `Some` = tribute is planned at the new
    /// deal's level, which may skip straight to play on 抗贡 (rng unused).
    pub fn from_deal(
        progress: Progress,
        last_result: Option<DealResult>,
        hands: [PlayerHand; 4],
        rng: &mut impl Rng,
    ) -> Self {
        let level = progress.deal_level();
        let state = match &last_result {
            None => {
                let (leader, revealed) = pick_first_leader(&hands, rng);
                let start = DealStart::FirstDeal { revealed, leader };
                GameState::Playing(PlayPhase::new(hands, level, leader, start))
            }
            Some(previous) => match plan_tribute(previous, &hands, level) {
                TributePlan::AntiTribute { leader } => {
                    let start = DealStart::AntiTribute { leader };
                    GameState::Playing(PlayPhase::new(hands, level, leader, start))
                }
                TributePlan::Tribute(duties) => {
                    GameState::Tribute(TributePhase::new(hands, level, duties))
                }
            },
        };
        Match { progress, state }
    }

    pub fn apply(&mut self, seat: SeatId, action: Action) -> Result<ActionOutcome, ActionError> {
        // Each arm checks the phase first. The `phase` borrow ends before
        // `self` is touched again (`finish_deal`, `start_play`), which keeps
        // the borrow checker happy.
        match action {
            Action::Play { cards, declared } => {
                let GameState::Playing(phase) = &mut self.state else {
                    return Err(ActionError::WrongPhase);
                };
                match phase.play(seat, &cards, declared.as_ref())? {
                    PlayOutcome::Continues => {}
                    PlayOutcome::NeedsDeclaration { options } => {
                        return Ok(ActionOutcome::NeedsDeclaration { options });
                    }
                    PlayOutcome::DealOver {
                        result,
                        final_trick,
                    } => return Ok(self.finish_deal(result, final_trick)),
                }
            }
            Action::Pass => {
                let GameState::Playing(phase) = &mut self.state else {
                    return Err(ActionError::WrongPhase);
                };
                match phase.pass(seat)? {
                    PlayOutcome::Continues => {}
                    // A pass never ends a deal or needs a declaration (§3.8),
                    // but handling every outcome costs nothing.
                    PlayOutcome::NeedsDeclaration { options } => {
                        return Ok(ActionOutcome::NeedsDeclaration { options });
                    }
                    PlayOutcome::DealOver {
                        result,
                        final_trick,
                    } => return Ok(self.finish_deal(result, final_trick)),
                }
            }
            Action::TakeBack => {
                let GameState::Playing(phase) = &mut self.state else {
                    return Err(ActionError::WrongPhase);
                };
                // Only ever `Continues`: it restores the state from before a
                // play that didn't end the deal.
                phase.take_back(seat)?;
            }
            Action::PayTribute { card } => {
                let GameState::Tribute(phase) = &mut self.state else {
                    return Err(ActionError::WrongPhase);
                };
                let outcome = phase.pay(seat, card)?;
                self.after_tribute(outcome);
            }
            Action::ReturnTribute { card } => {
                let GameState::Tribute(phase) = &mut self.state else {
                    return Err(ActionError::WrongPhase);
                };
                let outcome = phase.give_back(seat, card)?;
                self.after_tribute(outcome);
            }
        }
        Ok(ActionOutcome::Applied)
    }

    // Only tests read it now that the Room no longer looks at the phase
    // (`deal_level` and `view_for` cover what it needs).
    #[cfg(test)]
    pub fn state(&self) -> &GameState {
        &self.state
    }

    /// The level this deal's cards are read at.
    pub fn deal_level(&self) -> Level {
        match &self.state {
            GameState::Tribute(t) => t.level,
            GameState::Playing(p) => p.level,
        }
    }

    // Only tests read it today (`view_for` reads the field directly); kept
    // as the read-only accessor TECH_SPEC §3.11 lists.
    #[cfg(test)]
    pub fn progress(&self) -> &Progress {
        &self.progress
    }

    /// Once the last return is in, play starts: the payer to 1st place leads.
    fn after_tribute(&mut self, outcome: TributeOutcome) {
        match outcome {
            TributeOutcome::Continues => {}
            TributeOutcome::Complete {
                hands,
                leader,
                exchanges,
            } => {
                let level = self.progress.deal_level();
                let start = DealStart::Tribute { exchanges, leader };
                self.state = GameState::Playing(PlayPhase::new(hands, level, leader, start));
            }
        }
    }

    /// Scores the deal. `self.progress` is left as the deal started: the
    /// next progress is in `summary.after`, for the Room to keep.
    fn finish_deal(&self, result: DealResult, final_trick: Vec<TrickEntry>) -> ActionOutcome {
        let summary = resolve_deal(&self.progress, result);
        ActionOutcome::DealOver(DealEnd {
            summary,
            final_trick,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::card::{Face, JokerColor, Rank, Team};
    use crate::rules::deal::{Exchange, deal_hands};
    use crate::rules::ranking::Level;
    use crate::rules::test_util::{cards, hand};
    use crate::rules::trick::TrickEntry;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    // Only `needs_declaration_passes_through_unchanged` plays a wildcard (the
    // heart of the deal's level rank); every other play here is wildcard-free.

    fn seat(i: u8) -> SeatId {
        SeatId::new(i).unwrap()
    }

    fn rng() -> StdRng {
        StdRng::seed_from_u64(7)
    }

    /// Progress where `declaring` declares, with these team levels.
    fn progress(levels: [Rank; 2], declaring: Team) -> Progress {
        Progress {
            team_levels: levels.map(Level),
            declaring: Some(declaring),
            a_attempts: [0, 0],
        }
    }

    fn result(order: &[u8]) -> DealResult {
        DealResult {
            order: order.iter().map(|&i| seat(i)).collect(),
        }
    }

    fn play(m: &mut Match, s: u8, c: &str) -> Result<ActionOutcome, ActionError> {
        m.apply(
            seat(s),
            Action::Play {
                cards: cards(c),
                declared: None,
            },
        )
    }

    fn pass(m: &mut Match, s: u8) -> Result<ActionOutcome, ActionError> {
        m.apply(seat(s), Action::Pass)
    }

    fn playing(m: &Match) -> &PlayPhase {
        match m.state() {
            GameState::Playing(phase) => phase,
            other => panic!("expected Playing, got {other:?}"),
        }
    }

    /// The `DealEnd` a deal-ending action returned.
    fn deal_over(outcome: Result<ActionOutcome, ActionError>) -> DealEnd {
        match outcome {
            Ok(ActionOutcome::DealOver(end)) => end,
            other => panic!("expected DealOver, got {other:?}"),
        }
    }

    fn big_joker() -> Card {
        Card::Joker(JokerColor::Big)
    }

    #[test]
    fn a_fresh_first_deal_deals_27_cards_each_at_level_two() {
        let mut rng = rng();
        let hands = deal_hands(&mut rng);
        let m = Match::from_deal(Progress::default(), None, hands, &mut rng);
        let phase = playing(&m);
        assert!(phase.hands.iter().all(|h| h.len() == 27));
        assert_eq!(phase.level, Level(Rank::Two));
        assert_eq!(m.deal_level(), Level(Rank::Two));
        assert!(matches!(phase.deal_start, DealStart::FirstDeal { .. }));
    }

    #[test]
    fn first_deal_leader_holds_the_revealed_card() {
        let hands = ["3S 4S", "5H 6H", "7D 8D", "9C TC"];
        for seed in 0..20 {
            let m = Match::from_deal(
                Progress::default(),
                None,
                hands.map(hand),
                &mut StdRng::seed_from_u64(seed),
            );
            let phase = playing(&m);
            let DealStart::FirstDeal { revealed, leader } = phase.deal_start else {
                panic!("expected FirstDeal, got {:?}", phase.deal_start);
            };
            assert_eq!(phase.turn, leader);
            assert_eq!(phase.level, Level(Rank::Two));
            assert_eq!(phase.hands[leader.index()].count(revealed), 1);
        }
    }

    /// A full first deal: team A goes out 1-2 (with 接风), then the next deal
    /// (started from the `DealEnd`, as the Room does) is at team A's new level.
    #[test]
    fn full_deal_end_to_end_then_next_deal() {
        // Only seat 0 holds a card `pick_first_leader` may reveal (jokers and
        // 2♥ are excluded), so seat 0 leads whatever the rng does.
        let mut m = Match::from_deal(
            Progress::default(),
            None,
            ["3S 3D", "SJ", "BJ", "2H"].map(hand),
            &mut rng(),
        );
        assert_eq!(playing(&m).turn, seat(0));

        assert_eq!(play(&mut m, 0, "3S 3D"), Ok(ActionOutcome::Applied)); // seat 0 out
        assert_eq!(pass(&mut m, 1), Ok(ActionOutcome::Applied));
        assert_eq!(pass(&mut m, 2), Ok(ActionOutcome::Applied));
        assert_eq!(pass(&mut m, 3), Ok(ActionOutcome::Applied));
        // 接风: seat 0 won the trick but is out, so partner seat 2 leads.
        assert_eq!(playing(&m).turn, seat(2));
        let end = deal_over(play(&mut m, 2, "BJ"));

        assert_eq!(end.summary.result, result(&[0, 2]));
        assert_eq!(end.summary.before, Progress::default());
        // 1-2 → +3 levels from Two.
        let expected_after = progress([Rank::Five, Rank::Two], Team::A);
        assert_eq!(end.summary.after, expected_after);
        assert_eq!(end.summary.match_winner, None);
        // The final trick is the one 接风 started, ending with the
        // deal-ending play.
        assert_eq!(
            end.final_trick,
            vec![TrickEntry::Played {
                seat: seat(2),
                play: Play {
                    cards: cards("BJ"),
                    combo: Combo::Single(Face::BigJoker),
                    wildcard_as: vec![],
                },
            }]
        );

        // The next deal starts from what the deal ended with.
        let mut rng = rng();
        let hands = deal_hands(&mut rng);
        let next = Match::from_deal(
            end.summary.after.clone(),
            Some(end.summary.result.clone()),
            hands,
            &mut rng,
        );
        assert_eq!(next.progress(), &expected_after);
        // Team A declares, so the deal is at Five; seats 1 and 3 owe tribute
        // unless they happen to hold both Big Jokers.
        let hands = match next.state() {
            GameState::Tribute(phase) => &phase.hands,
            GameState::Playing(phase) => {
                assert_eq!(phase.deal_start, DealStart::AntiTribute { leader: seat(0) });
                &phase.hands
            }
        };
        assert_eq!(next.deal_level(), Level(Rank::Five));
        assert!(hands.iter().all(|h| h.len() == 27));
    }

    #[test]
    fn double_tribute_hands_off_to_play() {
        // Previous deal 1-2 by seats 0 and 2; seats 1 and 3 pay. Seat 1's A
        // outranks seat 3's K, so seat 1 pays 1st place (seat 0) and leads.
        let mut m = Match::from_deal(
            progress([Rank::Two, Rank::Two], Team::A),
            Some(result(&[0, 2])),
            ["5D 6D", "AS 3C", "7D 8D", "KS 4C"].map(hand),
            &mut rng(),
        );
        assert!(matches!(m.state(), GameState::Tribute(_)));

        // Wrong-phase actions change nothing.
        let before = m.clone();
        assert_eq!(play(&mut m, 1, "3C"), Err(ActionError::WrongPhase));
        assert_eq!(pass(&mut m, 1), Err(ActionError::WrongPhase));
        assert_eq!(
            m.apply(seat(1), Action::TakeBack),
            Err(ActionError::WrongPhase)
        );
        assert_eq!(m, before);

        let pay = |m: &mut Match, s: u8, c: &str| {
            m.apply(seat(s), Action::PayTribute { card: cards(c)[0] })
        };
        let give_back = |m: &mut Match, s: u8, c: &str| {
            m.apply(seat(s), Action::ReturnTribute { card: cards(c)[0] })
        };
        assert_eq!(pay(&mut m, 3, "KS"), Ok(ActionOutcome::Applied));
        assert_eq!(pay(&mut m, 1, "AS"), Ok(ActionOutcome::Applied));
        assert_eq!(give_back(&mut m, 2, "7D"), Ok(ActionOutcome::Applied));
        assert!(matches!(m.state(), GameState::Tribute(_)));
        assert_eq!(give_back(&mut m, 0, "5D"), Ok(ActionOutcome::Applied));

        let phase = playing(&m);
        assert_eq!(phase.level, Level(Rank::Two));
        assert_eq!(phase.turn, seat(1));
        assert_eq!(phase.hands, ["6D AS", "3C 5D", "8D KS", "4C 7D"].map(hand));
        assert_eq!(
            phase.deal_start,
            DealStart::Tribute {
                exchanges: vec![
                    Exchange {
                        payer: seat(1),
                        receiver: seat(0),
                        tribute: cards("AS")[0],
                        returned: cards("5D")[0],
                    },
                    Exchange {
                        payer: seat(3),
                        receiver: seat(2),
                        tribute: cards("KS")[0],
                        returned: cards("7D")[0],
                    },
                ],
                leader: seat(1),
            }
        );

        // Tribute actions are out of phase once play starts.
        let before = m.clone();
        assert_eq!(pay(&mut m, 1, "3C"), Err(ActionError::WrongPhase));
        assert_eq!(give_back(&mut m, 0, "6D"), Err(ActionError::WrongPhase));
        assert_eq!(m, before);
    }

    #[test]
    fn single_tribute_uses_the_declaring_teams_level() {
        // Team B declares at Seven; previous deal was 1-3 (seat 1 first, seat
        // 3 third), so 4th-place seat 2 pays seat 1.
        let m = Match::from_deal(
            progress([Rank::Four, Rank::Seven], Team::B),
            Some(result(&[1, 0, 3, 2])),
            ["3C", "4C", "AS 5C", "6C"].map(hand),
            &mut rng(),
        );
        let GameState::Tribute(phase) = m.state() else {
            panic!("expected Tribute, got {:?}", m.state());
        };
        assert_eq!(phase.level, Level(Rank::Seven));
        assert_eq!(phase.duties.len(), 1);
        assert_eq!(phase.duties[0].duty.payer, seat(2));
        assert_eq!(phase.duties[0].duty.receiver, seat(1));
    }

    #[test]
    fn anti_tribute_goes_straight_to_playing() {
        // Payers 1 and 3 hold both Big Jokers between them.
        let hands = ["3S", "BJ 4C", "5S", "BJ 6C"].map(hand);
        let m = Match::from_deal(
            progress([Rank::Two, Rank::Two], Team::A),
            Some(result(&[0, 2])),
            hands.clone(),
            &mut rng(),
        );
        let phase = playing(&m);
        assert_eq!(phase.deal_start, DealStart::AntiTribute { leader: seat(0) });
        assert_eq!(phase.turn, seat(0));
        assert_eq!(phase.hands, hands);
        assert_eq!(phase.hands[1].count(big_joker()), 1);
    }

    /// Team A declares at A and wins 1-2 → the match is over.
    #[test]
    fn winning_one_two_at_ace_ends_the_match() {
        // Anti-tribute (seats 1 and 3 hold both Big Jokers), so seat 0 leads.
        // No hearts: A♥ is the wildcard at level Ace.
        let before = progress([Rank::Ace, Rank::Nine], Team::A);
        let mut m = Match::from_deal(
            before.clone(),
            Some(result(&[0, 2])),
            ["3S 3D", "BJ", "4S 4D", "BJ"].map(hand),
            &mut rng(),
        );
        assert_eq!(playing(&m).level, Level(Rank::Ace));

        play(&mut m, 0, "3S 3D").unwrap();
        pass(&mut m, 1).unwrap();
        pass(&mut m, 2).unwrap();
        pass(&mut m, 3).unwrap();
        let end = deal_over(play(&mut m, 2, "4S 4D"));

        assert_eq!(end.summary.match_winner, Some(Team::A));
        assert_eq!(end.summary.before, before);
        assert_eq!(
            end.final_trick.last(),
            Some(&TrickEntry::Played {
                seat: seat(2),
                play: Play {
                    cards: cards("4S 4D"),
                    combo: Combo::Pair(Face::Rank(Rank::Four)),
                    wildcard_as: vec![],
                },
            })
        );
    }

    /// GAME_RULES.md house rule #9: a match set up in the lobby with a
    /// declaring team at A plays its first deal at A, still with no tribute
    /// and a turned-up card picking the leader, and it is a real A attempt.
    #[test]
    fn a_first_deal_declared_at_ace_is_an_a_attempt_with_no_tribute() {
        let before =
            Progress::default().with_settings([Level(Rank::Ace), Level(Rank::Two)], Some(Team::A));
        // Only seat 0 holds a card `pick_first_leader` may reveal, so seat 0
        // leads whatever the rng does.
        let mut m = Match::from_deal(
            before.clone(),
            None,
            ["3S 3D", "SJ", "BJ", "2H"].map(hand),
            &mut rng(),
        );
        let phase = playing(&m);
        assert_eq!(phase.level, Level(Rank::Ace));
        assert!(matches!(
            phase.deal_start,
            DealStart::FirstDeal { leader, .. } if leader == seat(0)
        ));

        // Team A goes out 1-2: that wins the match.
        play(&mut m, 0, "3S 3D").unwrap();
        pass(&mut m, 1).unwrap();
        pass(&mut m, 2).unwrap();
        pass(&mut m, 3).unwrap();
        let end = deal_over(play(&mut m, 2, "BJ"));
        assert_eq!(end.summary.before, before);
        assert_eq!(end.summary.match_winner, Some(Team::A));
    }

    /// `Action::TakeBack` reaches the play phase: seat 0's play is undone
    /// (and it's `Applied`, for the Room to broadcast); a second one has
    /// nothing left to take back.
    #[test]
    fn take_back_is_applied_in_the_play_phase() {
        let mut m = Match::from_deal(
            Progress::default(),
            None,
            ["3S 3D", "SJ", "BJ", "2H"].map(hand),
            &mut rng(),
        );
        let before = m.clone();
        play(&mut m, 0, "3S 3D").unwrap();
        assert_eq!(
            m.apply(seat(0), Action::TakeBack),
            Ok(ActionOutcome::Applied)
        );
        let phase = playing(&m);
        assert_eq!(phase.hands, playing(&before).hands);
        assert_eq!(phase.turn, seat(0));
        assert_eq!(phase.took_back, Some(seat(0)));
        assert_eq!(
            m.apply(seat(0), Action::TakeBack),
            Err(ActionError::NothingToTakeBack)
        );
    }

    #[test]
    fn needs_declaration_passes_through_unchanged() {
        // Team A declares at Six (so 6♥ is the wildcard); anti-tribute (the
        // payers hold both Big Jokers) puts seat 0 straight into the lead.
        let mut m = Match::from_deal(
            progress([Rank::Six, Rank::Two], Team::A),
            Some(result(&[0, 2])),
            ["8S 8H 8D 6H 6H 3C", "BJ 4C", "5S", "BJ 7C"].map(hand),
            &mut rng(),
        );
        assert_eq!(playing(&m).turn, seat(0));
        let before = m.clone();

        // 8,8,8,6♥,6♥ is a full house of 8s or a quintuple bomb of 8s.
        let outcome = play(&mut m, 0, "8S 8H 8D 6H 6H");
        let expected = crate::rules::readings::readings(&cards("8S 8H 8D 6H 6H"), Level(Rank::Six));
        assert_eq!(expected.len(), 2);
        assert_eq!(
            outcome,
            Ok(ActionOutcome::NeedsDeclaration { options: expected })
        );
        assert_eq!(m, before, "NeedsDeclaration must change nothing");

        // Resending with a declaration applies it.
        let declared = Combo::FullHouse {
            triple: Rank::Eight,
        };
        assert_eq!(
            m.apply(
                seat(0),
                Action::Play {
                    cards: cards("8S 8H 8D 6H 6H"),
                    declared: Some(declared),
                },
            ),
            Ok(ActionOutcome::Applied)
        );
        assert_eq!(playing(&m).trick.best.as_ref().unwrap().1.combo, declared);
    }
}
