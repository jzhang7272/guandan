//! The play phase (TECH_SPEC.md §3.8): turn order, play/pass, trick end,
//! 接风, going out, deal end.

use serde::{Deserialize, Serialize};

use super::card::{Card, SeatId};
use super::combo::{Combo, Play, beats};
use super::deal::{DealResult, DealStart};
use super::error::ActionError;
use super::hand::PlayerHand;
use super::ranking::Level;
use super::readings::readings;
use super::trick::{CompletedTrick, Trick, TrickEntry};

/// A seat is "active" iff its hand is non-empty — derived, never stored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayPhase {
    pub(crate) hands: [PlayerHand; 4],
    pub(crate) level: Level,
    /// Whose turn; always an active seat.
    pub(crate) turn: SeatId,
    pub(crate) trick: Trick,
    /// `None` until the deal's first trick ends.
    pub(crate) last_trick: Option<CompletedTrick>,
    /// Seats that have gone out, in order.
    pub(crate) finish_order: Vec<SeatId>,
    pub(crate) deal_start: DealStart,
    /// What the most recent play changed, so its player can take it back
    /// (GAME_RULES.md house rule #10). `None` once anyone has acted since,
    /// and never set by a play that ended the deal.
    pub(crate) undo: Option<Box<UndoPoint>>,
    /// The seat whose play was just taken back, so every client can say so.
    /// Cleared by the next play or pass.
    pub(crate) took_back: Option<SeatId>,
}

/// Everything a `play` changes, as it was just before the play.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoPoint {
    /// Who played: the only seat that may take it back.
    pub(crate) seat: SeatId,
    /// That seat's hand before the play.
    pub(crate) hand: PlayerHand,
    pub(crate) trick: Trick,
    pub(crate) turn: SeatId,
    pub(crate) finish_order: Vec<SeatId>,
    pub(crate) last_trick: Option<CompletedTrick>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayOutcome {
    Continues,
    /// The cards are legal but ambiguous; nothing changed.
    NeedsDeclaration {
        options: Vec<Play>,
    },
    /// The finish order and the final trick are moved out with
    /// `std::mem::take`, leaving the `PlayPhase` hollow; the deal is over and
    /// the `Match` holding it is dropped. Nothing may use a `PlayPhase` after
    /// it returned `DealOver`.
    DealOver {
        result: DealResult,
        /// The trick in progress, ending with the deal-ending play.
        final_trick: Vec<TrickEntry>,
    },
}

impl PlayPhase {
    pub fn new(
        hands: [PlayerHand; 4],
        level: Level,
        leader: SeatId,
        deal_start: DealStart,
    ) -> Self {
        PlayPhase {
            hands,
            level,
            turn: leader,
            trick: Trick::new(),
            last_trick: None,
            finish_order: Vec::new(),
            deal_start,
            undo: None,
            took_back: None,
        }
    }

    /// `seat` plays `cards`, optionally naming the reading it means
    /// (`declared`). Follows TECH_SPEC.md §3.8 step by step; every error is
    /// returned before anything is changed.
    pub fn play(
        &mut self,
        seat: SeatId,
        cards: &[Card],
        declared: Option<&Combo>,
    ) -> Result<PlayOutcome, ActionError> {
        // 1–2: whose turn, and are the cards really theirs.
        if seat != self.turn {
            return Err(ActionError::NotYourTurn);
        }
        if !self.hands[seat.index()].contains_all(cards) {
            return Err(ActionError::CardsNotInHand);
        }

        // 3: every legal reading of these cards.
        let all_readings = readings(cards, self.level);
        if all_readings.is_empty() {
            return Err(ActionError::NotAValidCombo);
        }

        // 4: when following, only readings that beat the current best count.
        let options: Vec<Play> = match &self.trick.best {
            None => all_readings.clone(),
            Some((_, best)) => all_readings
                .iter()
                .filter(|play| beats(&play.combo, &best.combo, self.level))
                .cloned()
                .collect(),
        };
        if options.is_empty() {
            return Err(ActionError::DoesNotBeatCurrent);
        }

        // 5: pick the reading (GAME_RULES.md "Resolving wildcard plays").
        let chosen = match declared {
            Some(combo) => match options.iter().find(|play| play.combo == *combo) {
                Some(play) => play.clone(),
                // A real reading of these cards that just doesn't beat the
                // table is a different mistake from naming a reading the
                // cards can't make at all.
                None if all_readings.iter().any(|play| play.combo == *combo) => {
                    return Err(ActionError::DoesNotBeatCurrent);
                }
                None => return Err(ActionError::InvalidDeclaration),
            },
            None if options.len() == 1 => options[0].clone(),
            // Ambiguous: the player did nothing wrong, they just have to say
            // which reading they mean. Nothing has changed yet.
            None => return Ok(PlayOutcome::NeedsDeclaration { options }),
        };

        // 6: commit. `contains_all` passed above, so this can't fail. First
        // note what the play is about to change, so it can be taken back.
        let undo = UndoPoint {
            seat,
            hand: self.hands[seat.index()].clone(),
            trick: self.trick.clone(),
            turn: self.turn,
            finish_order: self.finish_order.clone(),
            last_trick: self.last_trick.clone(),
        };
        self.hands[seat.index()]
            .remove_all(cards)
            .expect("contains_all was checked above");
        self.trick.record_play(seat, chosen);
        self.took_back = None;

        // 7: going out, and possibly ending the deal. A deal-ending play
        // can't be taken back, so it leaves no undo point.
        if self.hands[seat.index()].is_empty() {
            self.finish_order.push(seat);
            if self.finish_order.contains(&seat.partner()) {
                self.undo = None;
                return Ok(self.finish_deal());
            }
        }

        // 8: on to the next player still holding cards.
        self.turn = self.next_active_after(seat);
        self.undo = Some(Box::new(undo));
        Ok(PlayOutcome::Continues)
    }

    /// `seat` takes back the play it just made (GAME_RULES.md house rule
    /// #10): everything that play changed goes back to how it was, and it's
    /// `seat`'s turn again. Only one step back, and only until someone else
    /// acts.
    pub fn take_back(&mut self, seat: SeatId) -> Result<PlayOutcome, ActionError> {
        if !self.can_take_back(seat) {
            return Err(ActionError::NothingToTakeBack);
        }
        let undo = *self.undo.take().expect("can_take_back checked it");
        self.hands[seat.index()] = undo.hand;
        self.trick = undo.trick;
        self.turn = undo.turn;
        self.finish_order = undo.finish_order;
        self.last_trick = undo.last_trick;
        self.took_back = Some(seat);
        Ok(PlayOutcome::Continues)
    }

    /// Whether `seat` made the most recent play and nobody has acted since.
    pub(crate) fn can_take_back(&self, seat: SeatId) -> bool {
        self.undo.as_ref().is_some_and(|undo| undo.seat == seat)
    }

    /// `seat` passes. Follows TECH_SPEC.md §3.8; a pass never ends the deal.
    pub fn pass(&mut self, seat: SeatId) -> Result<PlayOutcome, ActionError> {
        // 1
        if seat != self.turn {
            return Err(ActionError::NotYourTurn);
        }
        let Some((best_seat, _)) = self.trick.best else {
            return Err(ActionError::CannotPassWhenLeading);
        };

        // 2. A pass means someone has acted since the last play, so that
        // play can no longer be taken back. (This also covers trick end,
        // which only a pass can cause.)
        self.trick.record_pass(seat);
        self.undo = None;
        self.took_back = None;

        // 3: the trick is over once every other active player has passed
        // since the last play. If `best_seat` has gone out it isn't active,
        // so then that's simply every active player.
        let trick_over = SeatId::ALL
            .into_iter()
            .filter(|&s| s != best_seat && self.is_active(s))
            .all(|s| self.trick.passed[s.index()]);

        if trick_over {
            let winner = best_seat;
            // 接风: a winner who went out on that play hands the lead to
            // their partner. The partner must still be in — if both were
            // out, the deal would already have ended.
            let next_leader = if self.is_active(winner) {
                winner
            } else {
                debug_assert!(
                    self.is_active(winner.partner()),
                    "both {winner:?} and their partner are out, but the deal didn't end"
                );
                winner.partner()
            };
            let finished = std::mem::take(&mut self.trick);
            self.last_trick = Some(CompletedTrick {
                entries: finished.entries,
                winner,
                next_leader,
            });
            self.turn = next_leader;
        } else {
            // 4
            self.turn = self.next_active_after(seat);
        }

        // 5
        Ok(PlayOutcome::Continues)
    }

    /// A seat is active iff it still holds cards.
    fn is_active(&self, seat: SeatId) -> bool {
        !self.hands[seat.index()].is_empty()
    }

    /// The next seat in turn order after `seat` that still holds cards.
    fn next_active_after(&self, seat: SeatId) -> SeatId {
        let mut candidate = seat.next();
        // While the deal is on, at least two seats are active, so this stops
        // within three steps.
        while !self.is_active(candidate) {
            debug_assert!(candidate != seat, "no active seat left");
            candidate = candidate.next();
        }
        candidate
    }

    /// Step 7's deal end: builds the result and moves the final trick out.
    fn finish_deal(&mut self) -> PlayOutcome {
        let mut order = std::mem::take(&mut self.finish_order);
        // A 1-2 finish leaves the losers unranked (GAME_RULES.md "Deal End").
        // Otherwise three seats are out and the one still holding cards is 4th.
        if order.len() == 3 {
            let fourth = SeatId::ALL
                .into_iter()
                .find(|&s| self.is_active(s))
                .expect("a 1-3 or 1-4 finish leaves exactly one seat holding cards");
            order.push(fourth);
        }
        PlayOutcome::DealOver {
            result: DealResult { order },
            final_trick: std::mem::take(&mut self.trick).entries,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::card::{Face, Rank};
    use crate::rules::combo::Bomb;
    use crate::rules::deal::FinishKind;
    use crate::rules::test_util::{cards, hand};

    // Level Two, and no hand holds 2♥, so the plays up to the "Ambiguous
    // plays" section are wildcard-free.
    const LEVEL: Level = Level(Rank::Two);

    fn seat(i: u8) -> SeatId {
        SeatId::new(i).unwrap()
    }

    /// A fresh play phase with these hands, `leader` to lead.
    fn phase(hands: [&str; 4], leader: u8) -> PlayPhase {
        PlayPhase::new(
            hands.map(hand),
            LEVEL,
            seat(leader),
            DealStart::AntiTribute {
                leader: seat(leader),
            },
        )
    }

    fn play(p: &mut PlayPhase, s: u8, c: &str) -> Result<PlayOutcome, ActionError> {
        p.play(seat(s), &cards(c), None)
    }

    fn single(rank: Rank) -> Combo {
        Combo::Single(Face::Rank(rank))
    }

    fn played(s: u8, c: &str, combo: Combo) -> TrickEntry {
        TrickEntry::Played {
            seat: seat(s),
            play: Play {
                cards: cards(c),
                combo,
                wildcard_as: vec![],
            },
        }
    }

    fn passed(s: u8) -> TrickEntry {
        TrickEntry::Passed { seat: seat(s) }
    }

    /// Asserts that `action` fails with `expected` and changes nothing.
    fn assert_rejected(
        p: &mut PlayPhase,
        action: impl FnOnce(&mut PlayPhase) -> Result<PlayOutcome, ActionError>,
        expected: ActionError,
    ) {
        let before = p.clone();
        assert_eq!(action(p), Err(expected));
        assert_eq!(
            *p, before,
            "a rejected action must leave the phase unchanged"
        );
    }

    const HANDS: [&str; 4] = ["5S 9S 9D", "3S 7S 7D", "6S 8S KS", "4S 4D 4H 4C"];

    #[test]
    fn new_starts_with_leader_and_empty_trick() {
        let p = phase(HANDS, 2);
        assert_eq!(p.turn, seat(2));
        assert_eq!(p.trick, Trick::new());
        assert_eq!(p.last_trick, None);
        assert!(p.finish_order.is_empty());
    }

    #[test]
    fn not_your_turn() {
        let mut p = phase(HANDS, 0);
        assert_rejected(&mut p, |p| play(p, 1, "3S"), ActionError::NotYourTurn);
        assert_rejected(&mut p, |p| p.pass(seat(1)), ActionError::NotYourTurn);
    }

    #[test]
    fn cards_not_in_hand() {
        let mut p = phase(HANDS, 0);
        assert_rejected(&mut p, |p| play(p, 0, "3S"), ActionError::CardsNotInHand);
        // Multiset: one 9♠ in hand, not two.
        assert_rejected(&mut p, |p| play(p, 0, "9S 9S"), ActionError::CardsNotInHand);
    }

    #[test]
    fn not_a_valid_combo() {
        let mut p = phase(HANDS, 0);
        assert_rejected(&mut p, |p| play(p, 0, "5S 9S"), ActionError::NotAValidCombo);
        assert_rejected(&mut p, |p| play(p, 0, ""), ActionError::NotAValidCombo);
    }

    #[test]
    fn does_not_beat_current() {
        let mut p = phase(HANDS, 0);
        assert_eq!(play(&mut p, 0, "9S"), Ok(PlayOutcome::Continues));
        // Lower single.
        assert_rejected(
            &mut p,
            |p| play(p, 1, "7S"),
            ActionError::DoesNotBeatCurrent,
        );
        // Different type (a pair on a single).
        assert_rejected(
            &mut p,
            |p| play(p, 1, "7S 7D"),
            ActionError::DoesNotBeatCurrent,
        );

        // A tie never beats: 9♦ on 9♠.
        let mut p = phase(["9S 5S", "9D 3S", "4S", "4D"], 0);
        play(&mut p, 0, "9S").unwrap();
        assert_rejected(
            &mut p,
            |p| play(p, 1, "9D"),
            ActionError::DoesNotBeatCurrent,
        );
    }

    #[test]
    fn leader_cannot_pass() {
        let mut p = phase(HANDS, 0);
        assert_rejected(
            &mut p,
            |p| p.pass(seat(0)),
            ActionError::CannotPassWhenLeading,
        );
        // Also the leader of a later trick.
        play(&mut p, 0, "9S").unwrap();
        for s in [1, 2, 3] {
            p.pass(seat(s)).unwrap();
        }
        assert_eq!(p.turn, seat(0));
        assert_rejected(
            &mut p,
            |p| p.pass(seat(0)),
            ActionError::CannotPassWhenLeading,
        );
    }

    /// GAME_RULES.md "Turn Play & Trick Resolution": seat 0 leads, seat 1
    /// passes, seat 2 beats seat 0, seat 3 passes, seat 0 passes — seat 1 now
    /// gets a turn again and may beat seat 2.
    #[test]
    fn pass_then_play_again() {
        let mut p = phase(["5S 9D", "3S KD", "8S 6S", "4S 4D"], 0);
        play(&mut p, 0, "5S").unwrap();
        p.pass(seat(1)).unwrap();
        play(&mut p, 2, "8S").unwrap();
        p.pass(seat(3)).unwrap();
        p.pass(seat(0)).unwrap();
        assert_eq!(
            p.turn,
            seat(1),
            "the trick isn't over: seat 1 gets another turn"
        );
        assert_eq!(play(&mut p, 1, "KD"), Ok(PlayOutcome::Continues));
        assert_eq!(p.turn, seat(2));
        assert_eq!(
            p.trick.entries,
            vec![
                played(0, "5S", single(Rank::Five)),
                passed(1),
                played(2, "8S", single(Rank::Eight)),
                passed(3),
                passed(0),
                played(1, "KD", single(Rank::King)),
            ]
        );
        assert_eq!(p.trick.passed, [false; 4], "a play resets the passes");
        assert_eq!(p.last_trick, None);
    }

    #[test]
    fn bomb_beats_non_bomb() {
        let mut p = phase(HANDS, 0);
        play(&mut p, 0, "9S 9D").unwrap();
        p.pass(seat(1)).unwrap();
        p.pass(seat(2)).unwrap();
        assert_eq!(play(&mut p, 3, "4S 4D 4H 4C"), Ok(PlayOutcome::Continues));
        let (best_seat, best) = p.trick.best.clone().unwrap();
        assert_eq!(best_seat, seat(3));
        assert_eq!(
            best.combo,
            Combo::Bomb(Bomb::OfAKind {
                size: 4,
                rank: Rank::Four
            })
        );
    }

    #[test]
    fn trick_ends_when_everyone_else_passes() {
        let mut p = phase(HANDS, 0);
        play(&mut p, 0, "9S").unwrap();
        p.pass(seat(1)).unwrap();
        p.pass(seat(2)).unwrap();
        assert_eq!(p.turn, seat(3));
        assert_eq!(p.pass(seat(3)), Ok(PlayOutcome::Continues));

        assert_eq!(
            p.last_trick,
            Some(CompletedTrick {
                entries: vec![
                    played(0, "9S", single(Rank::Nine)),
                    passed(1),
                    passed(2),
                    passed(3),
                ],
                winner: seat(0),
                next_leader: seat(0),
            })
        );
        assert_eq!(p.trick, Trick::new());
        assert_eq!(p.turn, seat(0));
    }

    /// 接风: the winner went out on the winning play, so the lead goes to
    /// their partner, not the next seat.
    #[test]
    fn trick_won_by_gone_out_player_passes_lead_to_partner() {
        let mut p = phase(["AS", "3S 5S", "4S 6S", "7S 8S"], 0);
        assert_eq!(play(&mut p, 0, "AS"), Ok(PlayOutcome::Continues));
        assert_eq!(p.finish_order, vec![seat(0)]);
        assert_eq!(p.turn, seat(1));
        p.pass(seat(1)).unwrap();
        p.pass(seat(2)).unwrap();
        p.pass(seat(3)).unwrap();

        let last = p.last_trick.clone().unwrap();
        assert_eq!(last.winner, seat(0));
        assert_eq!(last.next_leader, seat(2));
        assert_eq!(p.turn, seat(2));
    }

    /// Going out mid-trick doesn't end the trick; the others keep playing it,
    /// and the gone-out seat is skipped from then on.
    #[test]
    fn going_out_mid_trick_continues_and_is_skipped() {
        let mut p = phase(["5S 3D", "9S", "4S 6S", "7S 8S"], 0);
        play(&mut p, 0, "5S").unwrap();
        assert_eq!(play(&mut p, 1, "9S"), Ok(PlayOutcome::Continues));
        assert_eq!(p.finish_order, vec![seat(1)]);
        assert!(p.trick.best.is_some(), "the trick is still in progress");
        assert_eq!(p.turn, seat(2));

        p.pass(seat(2)).unwrap();
        p.pass(seat(3)).unwrap();
        assert_eq!(p.last_trick, None, "seat 0 hasn't passed yet");
        p.pass(seat(0)).unwrap();
        let last = p.last_trick.clone().unwrap();
        assert_eq!((last.winner, last.next_leader), (seat(1), seat(3)));

        // Next trick: turn order skips seat 1.
        assert_eq!(p.turn, seat(3));
        play(&mut p, 3, "7S").unwrap();
        assert_eq!(p.turn, seat(0));
        p.pass(seat(0)).unwrap();
        assert_eq!(p.turn, seat(2), "seat 1 is out and skipped");
    }

    /// 1-2: seat 0 goes out, then seat 2 (its partner) — the deal ends at
    /// once, mid-trick, with only the two out seats ranked.
    #[test]
    fn one_two_finish() {
        let mut p = phase(["5S", "3S 3D", "KS", "4S 4D"], 0);
        play(&mut p, 0, "5S").unwrap();
        p.pass(seat(1)).unwrap();
        let outcome = play(&mut p, 2, "KS").unwrap();

        let PlayOutcome::DealOver {
            result,
            final_trick,
        } = outcome
        else {
            panic!("expected DealOver, got {outcome:?}");
        };
        assert_eq!(result.order, vec![seat(0), seat(2)]);
        assert_eq!(result.kind(), FinishKind::OneTwo);
        assert_eq!(
            final_trick,
            vec![
                played(0, "5S", single(Rank::Five)),
                passed(1),
                played(2, "KS", single(Rank::King)),
            ]
        );
        // The losers still hold their cards.
        assert_eq!(p.hands, ["", "3S 3D", "", "4S 4D"].map(hand));
    }

    /// 1-3, ending on a lead: 0 and 1 go out, then 2 wins a trick and goes
    /// out leading the next one. Seat 3 still holds cards and is 4th.
    #[test]
    fn one_three_finish() {
        let mut p = phase(["5S", "6S", "8S 3D", "7S 7D"], 0);
        play(&mut p, 0, "5S").unwrap();
        play(&mut p, 1, "6S").unwrap();
        assert_eq!(p.finish_order, vec![seat(0), seat(1)]);
        assert_eq!(p.turn, seat(2));
        play(&mut p, 2, "8S").unwrap();
        assert_eq!(p.turn, seat(3));
        p.pass(seat(3)).unwrap();
        // Seat 3 was the only other active seat, so the trick is over.
        assert_eq!(p.last_trick.as_ref().unwrap().winner, seat(2));
        assert_eq!(p.turn, seat(2));

        let outcome = play(&mut p, 2, "3D").unwrap();
        let PlayOutcome::DealOver {
            result,
            final_trick,
        } = outcome
        else {
            panic!("expected DealOver, got {outcome:?}");
        };
        assert_eq!(result.order, vec![seat(0), seat(1), seat(2), seat(3)]);
        assert_eq!(result.kind(), FinishKind::OneThree);
        assert_eq!(result.fourth(), Some(seat(3)));
        assert_eq!(p.hands, ["", "", "", "7S 7D"].map(hand));
        assert_eq!(final_trick, vec![played(2, "3D", single(Rank::Three))]);
    }

    /// 1-4, ending mid-trick with the partner already out: 0 then 1 go out,
    /// 2 passes, and 3 (partner of 1) goes out on the same trick.
    #[test]
    fn one_four_finish_mid_trick() {
        let mut p = phase(["5S", "6S", "4S 4D", "7S"], 0);
        play(&mut p, 0, "5S").unwrap();
        play(&mut p, 1, "6S").unwrap();
        p.pass(seat(2)).unwrap();
        let outcome = play(&mut p, 3, "7S").unwrap();

        let PlayOutcome::DealOver {
            result,
            final_trick,
        } = outcome
        else {
            panic!("expected DealOver, got {outcome:?}");
        };
        assert_eq!(result.order, vec![seat(0), seat(1), seat(3), seat(2)]);
        assert_eq!(result.kind(), FinishKind::OneFour);
        assert_eq!(
            final_trick,
            vec![
                played(0, "5S", single(Rank::Five)),
                played(1, "6S", single(Rank::Six)),
                passed(2),
                played(3, "7S", single(Rank::Seven)),
            ]
        );
        assert_eq!(p.hands, ["", "", "4S 4D", ""].map(hand));
        // The finish order and the trick were moved out.
        assert!(p.finish_order.is_empty());
        assert_eq!(p.trick, Trick::new());
    }

    #[test]
    fn declared_reading() {
        // Naming the (only) reading works like not naming it.
        let mut p = phase(HANDS, 0);
        let nine = single(Rank::Nine);
        assert_eq!(
            p.play(seat(0), &cards("9S"), Some(&nine)),
            Ok(PlayOutcome::Continues)
        );
        assert_eq!(p.trick.best.as_ref().unwrap().1.combo, nine);

        // Naming a reading the cards can't make.
        let mut p = phase(HANDS, 0);
        assert_rejected(
            &mut p,
            |p| p.play(seat(0), &cards("9S"), Some(&single(Rank::Ten))),
            ActionError::InvalidDeclaration,
        );
        assert_rejected(
            &mut p,
            |p| {
                p.play(
                    seat(0),
                    &cards("9S 9D"),
                    Some(&Combo::Pair(Face::Rank(Rank::Eight))),
                )
            },
            ActionError::InvalidDeclaration,
        );
    }

    // --- Taking back a play (GAME_RULES.md house rule #10) ----------------

    /// `before`, as a take back by `seat` should leave it: exactly as it was,
    /// except that nothing is left to take back and `seat` is marked.
    fn taken_back(before: &PlayPhase, seat: SeatId) -> PlayPhase {
        let mut expected = before.clone();
        expected.undo = None;
        expected.took_back = Some(seat);
        expected
    }

    #[test]
    fn take_back_restores_the_state_exactly() {
        // A completed trick behind it, then seat 0 leads 5♠, seats 1 and 2
        // pass, and seat 3 plays K♠ and takes it back.
        let mut p = phase(["5S 9D 3C", "3S KD 4C", "8S 6S 4D", "KS 4S 7D"], 0);
        play(&mut p, 0, "3C").unwrap();
        for s in [1, 2, 3] {
            p.pass(seat(s)).unwrap();
        }
        assert!(p.last_trick.is_some());
        play(&mut p, 0, "5S").unwrap();
        p.pass(seat(1)).unwrap();
        p.pass(seat(2)).unwrap();
        let before = p.clone();

        play(&mut p, 3, "KS").unwrap();
        assert_eq!(p.turn, seat(0));
        assert_eq!(p.trick.passed, [false; 4], "the play reset the passes");
        assert_eq!(p.take_back(seat(3)), Ok(PlayOutcome::Continues));

        assert_eq!(p, taken_back(&before, seat(3)));
        // Spelled out: the hand, trick, best play, passes and turn.
        assert_eq!(p.hands[3], hand("KS 4S 7D"));
        assert_eq!(
            p.trick.entries,
            vec![played(0, "5S", single(Rank::Five)), passed(1), passed(2)]
        );
        assert_eq!(p.trick.best.as_ref().unwrap().0, seat(0));
        assert_eq!(p.trick.passed, [false, true, true, false]);
        assert_eq!(p.turn, seat(3));

        // The taken-back play is gone: seat 3 may pass instead, which ends
        // the trick for seat 0.
        assert_eq!(p.pass(seat(3)), Ok(PlayOutcome::Continues));
        assert_eq!(p.took_back, None, "a pass clears the notice");
        assert_eq!(p.last_trick.as_ref().unwrap().winner, seat(0));
    }

    /// Taking back the lead of a trick leaves an empty trick and the same
    /// leader, who can't pass.
    #[test]
    fn take_back_a_lead() {
        let mut p = phase(HANDS, 1);
        let before = p.clone();
        play(&mut p, 1, "7S 7D").unwrap();
        p.take_back(seat(1)).unwrap();
        assert_eq!(p, taken_back(&before, seat(1)));
        assert_eq!(p.trick, Trick::new());
        assert_rejected(
            &mut p,
            |p| p.pass(seat(1)),
            ActionError::CannotPassWhenLeading,
        );
        // Something else may be led instead; that clears the notice.
        play(&mut p, 1, "3S").unwrap();
        assert_eq!(p.took_back, None);
        assert!(p.can_take_back(seat(1)));
    }

    /// A play that went out (without ending the deal) is undone too: the
    /// seat leaves the finish order and is active again.
    #[test]
    fn take_back_after_going_out_restores_the_finish_order() {
        let mut p = phase(["5S", "6S", "8S 3D", "7S 7D"], 0);
        play(&mut p, 0, "5S").unwrap();
        let before = p.clone();
        play(&mut p, 1, "6S").unwrap();
        assert_eq!(p.finish_order, vec![seat(0), seat(1)]);

        p.take_back(seat(1)).unwrap();
        assert_eq!(p, taken_back(&before, seat(1)));
        assert_eq!(p.finish_order, vec![seat(0)]);
        assert_eq!(p.hands[1], hand("6S"));
        assert_eq!(p.turn, seat(1));
    }

    /// Taking back the lead that follows a finished trick (here also going
    /// out) leaves `last_trick` as it was.
    #[test]
    fn take_back_keeps_the_last_trick() {
        let mut p = phase(["5S 3D", "9S 4D", "4S 6S", "7S 8S"], 0);
        play(&mut p, 0, "5S").unwrap();
        for s in [1, 2, 3] {
            p.pass(seat(s)).unwrap();
        }
        let last = p.last_trick.clone();
        assert!(last.is_some());
        play(&mut p, 0, "3D").unwrap();
        assert_eq!(p.finish_order, vec![seat(0)]);
        p.take_back(seat(0)).unwrap();
        assert_eq!(p.last_trick, last);
        assert!(p.finish_order.is_empty());
    }

    #[test]
    fn a_deal_ending_play_leaves_nothing_to_take_back() {
        let mut p = phase(["5S", "3S 3D", "KS", "4S 4D"], 0);
        play(&mut p, 0, "5S").unwrap();
        p.pass(seat(1)).unwrap();
        assert!(matches!(
            play(&mut p, 2, "KS"),
            Ok(PlayOutcome::DealOver { .. })
        ));
        assert_eq!(p.undo, None);
        assert_eq!(p.take_back(seat(2)), Err(ActionError::NothingToTakeBack));
    }

    #[test]
    fn nothing_to_take_back() {
        // Before any play.
        let mut p = phase(HANDS, 0);
        assert_rejected(
            &mut p,
            |p| p.take_back(seat(0)),
            ActionError::NothingToTakeBack,
        );

        // Another seat asks.
        play(&mut p, 0, "9S").unwrap();
        for s in [1, 2, 3] {
            assert_rejected(
                &mut p,
                |p| p.take_back(seat(s)),
                ActionError::NothingToTakeBack,
            );
        }
        assert!(p.can_take_back(seat(0)));

        // A rejected play by the next player doesn't count as acting.
        assert_rejected(
            &mut p,
            |p| play(p, 1, "3S"),
            ActionError::DoesNotBeatCurrent,
        );
        assert!(p.can_take_back(seat(0)));

        // The next player passed.
        p.pass(seat(1)).unwrap();
        assert_rejected(
            &mut p,
            |p| p.take_back(seat(0)),
            ActionError::NothingToTakeBack,
        );

        // The next player played.
        let mut p = phase(HANDS, 0);
        play(&mut p, 0, "5S").unwrap();
        play(&mut p, 1, "7S").unwrap();
        assert_rejected(
            &mut p,
            |p| p.take_back(seat(0)),
            ActionError::NothingToTakeBack,
        );
        assert!(p.can_take_back(seat(1)));
    }

    /// Being asked which reading you mean isn't acting yet: the previous
    /// player can still take back.
    #[test]
    fn a_needs_declaration_by_the_next_player_keeps_the_undo_point() {
        // Level 6, as in `following_offers_only_readings_that_beat`.
        let mut p = phase_at_six(
            ["5S", "6S 6D 7S 7D 8C 8C 3C", "6H 6H 8S 8D 9C 9S 3D", "5D"],
            1,
        );
        play(&mut p, 1, "6S 6D 7S 7D 8C 8C").unwrap();
        let before = p.clone();
        assert!(matches!(
            play(&mut p, 2, "6H 6H 8S 8D 9C 9S"),
            Ok(PlayOutcome::NeedsDeclaration { .. })
        ));
        assert_eq!(p, before);
        assert!(p.can_take_back(seat(1)));
    }

    #[test]
    fn only_one_step_back() {
        let mut p = phase(HANDS, 0);
        play(&mut p, 0, "5S").unwrap();
        let before = p.clone();
        play(&mut p, 1, "7S").unwrap();
        p.take_back(seat(1)).unwrap();
        assert_eq!(p, taken_back(&before, seat(1)));
        // Seat 1 acted after seat 0's play, so that play stays; and seat 1
        // has nothing more to take back.
        assert_eq!(p.take_back(seat(0)), Err(ActionError::NothingToTakeBack));
        assert_eq!(p.take_back(seat(1)), Err(ActionError::NothingToTakeBack));
    }

    // --- Ambiguous plays (step 5 with several readings) --------------------
    //
    // These run at level 6, so 6♥ is a wildcard and some sets have several
    // readings (GAME_RULES.md "Resolving wildcard plays").

    /// Like `phase`, but at level 6.
    fn phase_at_six(hands: [&str; 4], leader: u8) -> PlayPhase {
        PlayPhase::new(
            hands.map(hand),
            Level(Rank::Six),
            seat(leader),
            DealStart::AntiTribute {
                leader: seat(leader),
            },
        )
    }

    fn full_house(triple: Rank) -> Combo {
        Combo::FullHouse { triple }
    }

    fn five_eights() -> Combo {
        Combo::Bomb(Bomb::OfAKind {
            size: 5,
            rank: Rank::Eight,
        })
    }

    /// The combo of the play currently on top of the trick.
    fn best_combo(p: &PlayPhase) -> Combo {
        p.trick.best.as_ref().unwrap().1.combo
    }

    const AMBIGUOUS_LEAD: [&str; 4] = ["8S 8H 8D 6H 6H 3C", "KS KH KD 2S 2D 4C", "5S", "5D"];

    #[test]
    fn ambiguous_lead_needs_declaration_and_changes_nothing() {
        let mut p = phase_at_six(AMBIGUOUS_LEAD, 0);
        let before = p.clone();
        let outcome = play(&mut p, 0, "8S 8H 8D 6H 6H");
        assert_eq!(
            outcome,
            Ok(PlayOutcome::NeedsDeclaration {
                options: readings(&cards("8S 8H 8D 6H 6H"), Level(Rank::Six)),
            })
        );
        // Both readings are offered: the full house and the bomb.
        let Ok(PlayOutcome::NeedsDeclaration { options }) = outcome else {
            unreachable!()
        };
        let combos: Vec<Combo> = options.iter().map(|play| play.combo).collect();
        assert_eq!(combos, vec![full_house(Rank::Eight), five_eights()]);
        assert_eq!(p, before, "NeedsDeclaration must leave the phase unchanged");
    }

    #[test]
    fn declared_picks_one_of_several_readings() {
        let mut p = phase_at_six(AMBIGUOUS_LEAD, 0);
        assert_eq!(
            p.play(seat(0), &cards("8S 8H 8D 6H 6H"), Some(&five_eights())),
            Ok(PlayOutcome::Continues)
        );
        let (by, best) = p.trick.best.clone().unwrap();
        assert_eq!(by, seat(0));
        assert_eq!(best.combo, five_eights());
        assert_eq!(best.wildcard_as, vec![Rank::Eight, Rank::Eight]);
        assert_eq!(p.hands[0], hand("3C"));
        assert_eq!(p.turn, seat(1));

        let mut p = phase_at_six(AMBIGUOUS_LEAD, 0);
        let fh = full_house(Rank::Eight);
        assert_eq!(
            p.play(seat(0), &cards("8S 8H 8D 6H 6H"), Some(&fh)),
            Ok(PlayOutcome::Continues)
        );
        assert_eq!(best_combo(&p), fh);
        assert!(p.trick.best.as_ref().unwrap().1.wildcard_as.is_empty());
    }

    // Seat 1 leads a full house of Kings; seat 2 holds 8,8,8,6♥,6♥.
    const ON_KINGS: [&str; 4] = ["5S", "KS KH KD 2S 2D 4C", "8S 8H 8D 6H 6H 3C", "5D"];

    #[test]
    fn following_uses_the_only_reading_that_beats() {
        let mut p = phase_at_six(ON_KINGS, 1);
        assert_eq!(
            play(&mut p, 1, "KS KH KD 2S 2D"),
            Ok(PlayOutcome::Continues)
        );
        // The full house of 8s doesn't beat Kings; the bomb does, so it's
        // used without asking.
        assert_eq!(
            play(&mut p, 2, "8S 8H 8D 6H 6H"),
            Ok(PlayOutcome::Continues)
        );
        assert_eq!(best_combo(&p), five_eights());
    }

    #[test]
    fn declared_real_reading_that_does_not_beat() {
        let mut p = phase_at_six(ON_KINGS, 1);
        assert_eq!(
            play(&mut p, 1, "KS KH KD 2S 2D"),
            Ok(PlayOutcome::Continues)
        );
        // FullHouse{Eight} is a real reading of these cards, it just loses
        // to Kings — even though another reading (the bomb) would beat.
        assert_rejected(
            &mut p,
            |p| {
                p.play(
                    seat(2),
                    &cards("8S 8H 8D 6H 6H"),
                    Some(&full_house(Rank::Eight)),
                )
            },
            ActionError::DoesNotBeatCurrent,
        );
    }

    #[test]
    fn following_offers_only_readings_that_beat() {
        // Level 6: 6♥,6♥,8,8,9,9 is Tube{Nine}, Tube{Ten} or Plate{Nine}.
        // On Tube{Eight} both tubes beat and the plate can't, so the player
        // chooses between the two tubes.
        let mut p = phase_at_six(
            ["5S", "6S 6D 7S 7D 8C 8C 3C", "6H 6H 8S 8D 9C 9S 3D", "5D"],
            1,
        );
        assert_eq!(
            play(&mut p, 1, "6S 6D 7S 7D 8C 8C"),
            Ok(PlayOutcome::Continues)
        );
        let before = p.clone();
        let Ok(PlayOutcome::NeedsDeclaration { options }) = play(&mut p, 2, "6H 6H 8S 8D 9C 9S")
        else {
            panic!("expected NeedsDeclaration");
        };
        let combos: Vec<Combo> = options.iter().map(|play| play.combo).collect();
        assert_eq!(
            combos,
            vec![
                Combo::Tube { top: Rank::Nine },
                Combo::Tube { top: Rank::Ten }
            ]
        );
        assert_eq!(p, before);
    }

    #[test]
    fn following_with_one_beating_full_house_reading() {
        // Level 6: 3,3,6♥,5,5 is FullHouse{Three} or FullHouse{Five}. On a
        // full house of 4s only the 5s beat, so that one is used.
        let mut p = phase_at_six(["5S", "4S 4D 4C 7S 7D 2C", "3S 3D 6H 5C 5S 2D", "5D"], 1);
        assert_eq!(
            play(&mut p, 1, "4S 4D 4C 7S 7D"),
            Ok(PlayOutcome::Continues)
        );
        assert_eq!(
            play(&mut p, 2, "3S 3D 6H 5C 5S"),
            Ok(PlayOutcome::Continues)
        );
        let best = &p.trick.best.as_ref().unwrap().1;
        assert_eq!(best.combo, full_house(Rank::Five));
        assert_eq!(best.wildcard_as, vec![Rank::Five]);
    }
}
