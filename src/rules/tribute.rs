//! Tribute and return (TECH_SPEC.md §3.9, GAME_RULES.md "Tribute & Anti-Tribute").

use serde::{Deserialize, Serialize};

use super::card::{Card, JokerColor, Rank, SeatId};
use super::deal::{DealResult, Exchange, FinishKind};
use super::error::ActionError;
use super::hand::PlayerHand;
use super::ranking::{Level, face_value, is_level_card, is_wildcard, natural_value};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TributePlan {
    /// 抗贡: the previous deal's 1st-place player leads.
    AntiTribute { leader: SeatId },
    /// 1 duty (single) or 2 (double).
    Tribute(Vec<Duty>),
}

/// Invariant: `duties[0].receiver` is ALWAYS the previous deal's 1st-place
/// player. `TributePhase` relies on this to pick the leader.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Duty {
    pub(crate) payer: SeatId,
    pub(crate) receiver: SeatId,
}

/// `hands` are as dealt; `level` is the NEW deal's level. Never called for the
/// first deal of a match.
pub fn plan_tribute(previous: &DealResult, hands: &[PlayerHand; 4], level: Level) -> TributePlan {
    let first = previous.first();

    // Tribute follows finish place, not team: in a 1-4 the payer is the
    // winner's own partner.
    let payers: Vec<SeatId> = match previous.kind() {
        FinishKind::OneTwo => previous.losers().to_vec(),
        FinishKind::OneThree | FinishKind::OneFour => {
            let fourth = previous
                .fourth()
                .expect("a 1-3 / 1-4 result always has a 4th place");
            vec![fourth]
        }
    };

    // 抗贡: the payers (together, in a double tribute) hold both Big Jokers.
    // In a single tribute only the one payer's hand counts, so a partner
    // holding the other Big Joker doesn't cancel anything.
    let big_jokers: usize = payers
        .iter()
        .map(|payer| hands[payer.index()].count(Card::Joker(JokerColor::Big)))
        .sum();
    if big_jokers == 2 {
        return TributePlan::AntiTribute { leader: first };
    }

    let duties = match payers[..] {
        [payer] => vec![Duty {
            payer,
            receiver: first,
        }],
        [a, b] => {
            // In a 1-2 finish, 2nd place is the winner's partner.
            let second = first.partner();
            let rank_a = tribute_rank(&hands[a.index()], level);
            let rank_b = tribute_rank(&hands[b.index()], level);
            let (to_first, to_second) = if rank_a > rank_b {
                (a, b)
            } else if rank_b > rank_a {
                (b, a)
            } else if a.prev() == first {
                // Equal ranks: each payer pays the winning-team player who
                // acts just before them, so the payer seated right after 1st
                // place pays 1st place.
                (a, b)
            } else {
                (b, a)
            };
            // The 1st-place receiver's duty goes first (the `Duty` invariant).
            vec![
                Duty {
                    payer: to_first,
                    receiver: first,
                },
                Duty {
                    payer: to_second,
                    receiver: second,
                },
            ]
        }
        _ => unreachable!("a tribute has one or two payers"),
    };
    TributePlan::Tribute(duties)
}

/// The highest `face_value` among the hand's non-wildcard cards (a wildcard
/// may never be given as tribute). `None` only for a hand with no such card,
/// which a dealt 27-card hand can't be.
fn tribute_rank(hand: &PlayerHand, level: Level) -> Option<u8> {
    hand.cards()
        .iter()
        .filter(|&&card| !is_wildcard(card, level))
        .map(|card| face_value(card.face(), level))
        .max()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TributePhase {
    pub(crate) hands: [PlayerHand; 4],
    pub(crate) level: Level,
    pub(crate) duties: Vec<DutyState>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DutyState {
    pub(crate) duty: Duty,
    /// Set when paid.
    pub(crate) tribute: Option<Card>,
    /// Set when returned.
    pub(crate) returned: Option<Card>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TributeOutcome {
    Continues,
    /// All returns in: `Match` builds the `PlayPhase` from these. The hands are
    /// moved out with `std::mem::take`, leaving the `TributePhase` empty —
    /// `Match` replaces it immediately.
    Complete {
        hands: [PlayerHand; 4],
        leader: SeatId,
        exchanges: Vec<Exchange>,
    },
}

impl TributePhase {
    pub fn new(hands: [PlayerHand; 4], level: Level, duties: Vec<Duty>) -> Self {
        let duties = duties
            .into_iter()
            .map(|duty| DutyState {
                duty,
                tribute: None,
                returned: None,
            })
            .collect();
        TributePhase {
            hands,
            level,
            duties,
        }
    }

    pub fn all_paid(&self) -> bool {
        self.duties.iter().all(|state| state.tribute.is_some())
    }

    fn all_returned(&self) -> bool {
        self.duties.iter().all(|state| state.returned.is_some())
    }

    /// Index into `duties` of the duty `seat` pays, if any.
    fn payer_duty(&self, seat: SeatId) -> Option<usize> {
        self.duties
            .iter()
            .position(|state| state.duty.payer == seat)
    }

    /// Index into `duties` of the duty `seat` receives, if any.
    fn receiver_duty(&self, seat: SeatId) -> Option<usize> {
        self.duties
            .iter()
            .position(|state| state.duty.receiver == seat)
    }

    /// Every distinct non-wildcard card whose `face_value` equals the payer's
    /// max. Empty if not a payer or already paid.
    pub fn tribute_options(&self, seat: SeatId) -> Vec<Card> {
        let Some(index) = self.payer_duty(seat) else {
            return Vec::new();
        };
        if self.duties[index].tribute.is_some() {
            return Vec::new();
        }
        // No card moves until every tribute is paid, so this is the hand as
        // dealt.
        let hand = &self.hands[seat.index()];
        let Some(max) = tribute_rank(hand, self.level) else {
            return Vec::new();
        };
        // Only the suit is a choice: at level 6, 6♠ and 6♦ tie with each other
        // (but never with the 6♥ wildcard).
        let mut options: Vec<Card> = hand
            .cards()
            .iter()
            .copied()
            .filter(|&card| {
                !is_wildcard(card, self.level) && face_value(card.face(), self.level) == max
            })
            .collect();
        // The hand is sorted, so duplicate copies are adjacent.
        options.dedup();
        options
    }

    /// Cards the receiver may return (only once `all_paid`). See TECH_SPEC.md §3.9.
    /// Empty if not a receiver, not all paid yet, or already returned.
    pub fn return_options(&self, seat: SeatId) -> Vec<Card> {
        let Some(index) = self.receiver_duty(seat) else {
            return Vec::new();
        };
        let state = &self.duties[index];
        if !self.all_paid() || state.returned.is_some() {
            return Vec::new();
        }
        let tribute = state.tribute.expect("all_paid");

        // The receiver may not hand back the physical card they just got, but
        // may return another copy of it. Cards are only tracked as a multiset,
        // so take exactly one copy of the tribute out of the pool.
        let mut pool: Vec<Card> = self.hands[seat.index()].cards().to_vec();
        if let Some(position) = pool.iter().position(|&card| card == tribute) {
            pool.remove(position);
        }

        // "10 or below": never a joker, and never a level card even when its
        // natural rank is ≤ 10 (GAME_RULES.md Interpretation #1).
        let is_eligible = |card: Card| match card {
            Card::Standard { rank, .. } => {
                !is_level_card(card, self.level) && natural_value(rank) <= natural_value(Rank::Ten)
            }
            Card::Joker(_) => false,
        };
        let mut options: Vec<Card> = pool
            .iter()
            .copied()
            .filter(|&card| is_eligible(card))
            .collect();

        if options.is_empty() {
            // Fallback: the lowest-ranked card(s), in the non-sequential order.
            let lowest = pool
                .iter()
                .map(|card| face_value(card.face(), self.level))
                .min();
            options = pool
                .iter()
                .copied()
                .filter(|card| Some(face_value(card.face(), self.level)) == lowest)
                .collect();
        }
        options.sort();
        options.dedup();
        options
    }

    /// Records `seat`'s tribute. Nothing moves until the last one is paid;
    /// then every tribute moves into its receiver's hand at once.
    pub fn pay(&mut self, seat: SeatId, card: Card) -> Result<TributeOutcome, ActionError> {
        let index = self.payer_duty(seat).ok_or(ActionError::NotATributePayer)?;
        if self.duties[index].tribute.is_some() {
            return Err(ActionError::AlreadyPaid);
        }
        if !self.tribute_options(seat).contains(&card) {
            return Err(ActionError::InvalidTributeCard);
        }
        self.duties[index].tribute = Some(card);

        if self.all_paid() {
            for state in &self.duties {
                let tribute = state.tribute.expect("all_paid");
                self.hands[state.duty.payer.index()]
                    .remove_all(&[tribute])
                    .expect("a tribute option is in the payer's hand");
                self.hands[state.duty.receiver.index()].add(tribute);
            }
        }
        Ok(TributeOutcome::Continues)
    }

    /// Records `seat`'s return card blind. When the last one arrives, every
    /// return moves into its payer's hand at once and the phase is complete.
    pub fn give_back(&mut self, seat: SeatId, card: Card) -> Result<TributeOutcome, ActionError> {
        let index = self
            .receiver_duty(seat)
            .ok_or(ActionError::NotATributeReceiver)?;
        if !self.all_paid() {
            return Err(ActionError::TributeNotComplete);
        }
        if self.duties[index].returned.is_some() {
            return Err(ActionError::AlreadyReturned);
        }
        if !self.return_options(seat).contains(&card) {
            return Err(ActionError::InvalidReturnCard);
        }
        self.duties[index].returned = Some(card);

        if !self.all_returned() {
            return Ok(TributeOutcome::Continues);
        }

        let mut exchanges = Vec::with_capacity(self.duties.len());
        for state in &self.duties {
            let (Some(tribute), Some(returned)) = (state.tribute, state.returned) else {
                unreachable!("every duty is paid and returned")
            };
            let Duty { payer, receiver } = state.duty;
            self.hands[receiver.index()]
                .remove_all(&[returned])
                .expect("a return option is in the receiver's hand");
            self.hands[payer.index()].add(returned);
            exchanges.push(Exchange {
                payer,
                receiver,
                tribute,
                returned,
            });
        }
        Ok(TributeOutcome::Complete {
            hands: std::mem::take(&mut self.hands),
            // The `Duty` invariant: duties[0] is the payer to 1st place.
            leader: self.duties[0].duty.payer,
            exchanges,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_util::{cards, hand};

    fn seat(index: u8) -> SeatId {
        SeatId::new(index).unwrap()
    }

    fn card(s: &str) -> Card {
        cards(s)[0]
    }

    fn level(rank: Rank) -> Level {
        Level(rank)
    }

    fn hands(a: &str, b: &str, c: &str, d: &str) -> [PlayerHand; 4] {
        [hand(a), hand(b), hand(c), hand(d)]
    }

    fn result(order: &[u8]) -> DealResult {
        DealResult {
            order: order.iter().map(|&i| seat(i)).collect(),
        }
    }

    fn duty(payer: u8, receiver: u8) -> Duty {
        Duty {
            payer: seat(payer),
            receiver: seat(receiver),
        }
    }

    fn sorted(s: &str) -> Vec<Card> {
        let mut v = cards(s);
        v.sort();
        v
    }

    // ---- plan_tribute ----

    #[test]
    fn one_three_fourth_place_pays_first_place() {
        let h = hands("3C", "4C", "5C", "AS");
        let plan = plan_tribute(&result(&[0, 1, 2, 3]), &h, level(Rank::Two));
        assert_eq!(plan, TributePlan::Tribute(vec![duty(3, 0)]));
    }

    #[test]
    fn one_four_payer_is_the_winners_partner() {
        // Seat 0 wins, partner 2 is last: 2 pays their own partner.
        let h = hands("3C", "4C", "AS", "5C");
        let plan = plan_tribute(&result(&[0, 1, 3, 2]), &h, level(Rank::Two));
        assert_eq!(plan, TributePlan::Tribute(vec![duty(2, 0)]));
    }

    #[test]
    fn double_tribute_higher_pays_first_lower_pays_second() {
        // 1st = 1, 2nd = 3; losers 0 and 2.
        let h = hands("AS 3C", "4C", "KS 5C", "6C");
        let plan = plan_tribute(&result(&[1, 3]), &h, level(Rank::Two));
        assert_eq!(plan, TributePlan::Tribute(vec![duty(0, 1), duty(2, 3)]));

        let h = hands("KS 3C", "4C", "AS 5C", "6C");
        let plan = plan_tribute(&result(&[1, 3]), &h, level(Rank::Two));
        assert_eq!(plan, TributePlan::Tribute(vec![duty(2, 1), duty(0, 3)]));
    }

    #[test]
    fn double_tribute_rank_uses_the_new_deals_level() {
        // At level K, K beats A.
        let h = hands("KS 3C", "4C", "AS 5C", "6C");
        let plan = plan_tribute(&result(&[1, 3]), &h, level(Rank::King));
        assert_eq!(plan, TributePlan::Tribute(vec![duty(0, 1), duty(2, 3)]));
    }

    #[test]
    fn double_tribute_tie_each_pays_prev() {
        // 1st = 1: seat 2 acts right after 1, so 2 pays 1 (2.prev() == 1) and
        // 0 pays 3 (0.prev() == 3). The 1st-place duty comes first.
        let h = hands("AS 3C", "4C", "AD 5C", "6C");
        let plan = plan_tribute(&result(&[1, 3]), &h, level(Rank::Two));
        assert_eq!(plan, TributePlan::Tribute(vec![duty(2, 1), duty(0, 3)]));

        // 1st = 3: seat 0 acts right after 3, so 0 pays 3.
        let plan = plan_tribute(&result(&[3, 1]), &h, level(Rank::Two));
        assert_eq!(plan, TributePlan::Tribute(vec![duty(0, 3), duty(2, 1)]));

        // Winners in seats 0/2, losers 1/3: 1st = 0 → 1 pays 0, 3 pays 2.
        let h = hands("3C", "AS 4C", "5C", "AH 6C");
        let plan = plan_tribute(&result(&[0, 2]), &h, level(Rank::Two));
        assert_eq!(plan, TributePlan::Tribute(vec![duty(1, 0), duty(3, 2)]));
    }

    #[test]
    fn level_cards_of_other_suits_tie_in_double_tribute() {
        // At level 6, 6♠ and 6♦ are equal rank → the prev() rule.
        let h = hands("6S 3C", "4C", "6D 5C", "7C");
        let plan = plan_tribute(&result(&[1, 3]), &h, level(Rank::Six));
        assert_eq!(plan, TributePlan::Tribute(vec![duty(2, 1), duty(0, 3)]));
    }

    #[test]
    fn wildcard_does_not_count_toward_tribute_rank() {
        // Level 6: seat 0's 6♥ is a wildcard, so their rank is the 5.
        let h = hands("6H 5S", "4C", "7S 3C", "8C");
        let plan = plan_tribute(&result(&[1, 3]), &h, level(Rank::Six));
        assert_eq!(plan, TributePlan::Tribute(vec![duty(2, 1), duty(0, 3)]));

        // Seat 0 holds only 6♥ + A: their rank is A, which beats K.
        let h = hands("6H AS", "4C", "KS 3C", "8C");
        let plan = plan_tribute(&result(&[1, 3]), &h, level(Rank::Six));
        assert_eq!(plan, TributePlan::Tribute(vec![duty(0, 1), duty(2, 3)]));
    }

    #[test]
    fn anti_tribute_single_payer_holds_both_big_jokers() {
        let h = hands("3C", "4C", "5C", "BJ BJ 6C");
        let plan = plan_tribute(&result(&[0, 1, 2, 3]), &h, level(Rank::Two));
        assert_eq!(plan, TributePlan::AntiTribute { leader: seat(0) });

        // 1-4: the payer is the winner's partner; same rule.
        let h = hands("3C", "4C", "BJ BJ", "5C");
        let plan = plan_tribute(&result(&[0, 1, 3, 2]), &h, level(Rank::Two));
        assert_eq!(plan, TributePlan::AntiTribute { leader: seat(0) });
    }

    #[test]
    fn anti_tribute_single_partner_holding_one_does_not_count() {
        // Payer 3 has one Big Joker, their partner 1 has the other.
        let h = hands("3C", "BJ 4C", "5C", "BJ 6C");
        let plan = plan_tribute(&result(&[0, 1, 2, 3]), &h, level(Rank::Two));
        assert_eq!(plan, TributePlan::Tribute(vec![duty(3, 0)]));
    }

    #[test]
    fn anti_tribute_double_one_payer_holds_both() {
        let h = hands("BJ BJ", "4C", "5C", "6C");
        let plan = plan_tribute(&result(&[1, 3]), &h, level(Rank::Two));
        assert_eq!(plan, TributePlan::AntiTribute { leader: seat(1) });
    }

    #[test]
    fn anti_tribute_double_each_payer_holds_one() {
        let h = hands("BJ 3C", "4C", "BJ 5C", "6C");
        let plan = plan_tribute(&result(&[3, 1]), &h, level(Rank::Two));
        assert_eq!(plan, TributePlan::AntiTribute { leader: seat(3) });
    }

    #[test]
    fn no_anti_tribute_when_a_winner_holds_a_big_joker() {
        // Only one Big Joker among the payers; seat 0's beats seat 2's SJ.
        let h = hands("BJ 3C", "BJ 4C", "SJ 5C", "6C");
        let plan = plan_tribute(&result(&[1, 3]), &h, level(Rank::Two));
        assert_eq!(plan, TributePlan::Tribute(vec![duty(0, 1), duty(2, 3)]));
    }

    // ---- tribute_options ----

    fn single(payer_hand: &str, lvl: Rank) -> TributePhase {
        TributePhase::new(
            hands("3C", "4C", "5C", payer_hand),
            level(lvl),
            vec![duty(3, 0)],
        )
    }

    #[test]
    fn wildcard_is_never_a_tribute() {
        // Holder of 6♥ + 6♠ at level 6 must give the 6♠, not an A.
        let phase = single("6H 6S AS", Rank::Six);
        assert_eq!(phase.tribute_options(seat(3)), cards("6S"));

        // Holder of only 6♥ + A gives the A.
        let phase = single("6H AS 3D", Rank::Six);
        assert_eq!(phase.tribute_options(seat(3)), cards("AS"));

        let mut phase = phase;
        assert_eq!(
            phase.pay(seat(3), card("6H")),
            Err(ActionError::InvalidTributeCard)
        );
    }

    #[test]
    fn tribute_options_are_every_distinct_suit_of_the_max() {
        let phase = single("AS AS AD KS 6H", Rank::Six);
        assert_eq!(phase.tribute_options(seat(3)), sorted("AS AD"));

        let phase = single("6S 6D 6H AS", Rank::Six);
        assert_eq!(phase.tribute_options(seat(3)), sorted("6S 6D"));

        let phase = single("BJ SJ AS", Rank::Two);
        assert_eq!(phase.tribute_options(seat(3)), cards("BJ"));
    }

    #[test]
    fn tribute_options_empty_for_non_payer_or_after_paying() {
        let mut phase = single("AS 3D", Rank::Two);
        assert!(phase.tribute_options(seat(0)).is_empty());
        assert!(phase.tribute_options(seat(1)).is_empty());
        phase.pay(seat(3), card("AS")).unwrap();
        assert!(phase.tribute_options(seat(3)).is_empty());
    }

    // ---- pay / give_back ----

    #[test]
    fn pay_errors() {
        // Double tribute so the phase is still paying after one payment.
        let mut phase = TributePhase::new(
            hands("AS 3C", "4D JS", "KS 5C", "7D QS"),
            level(Rank::Two),
            vec![duty(0, 1), duty(2, 3)],
        );
        assert_eq!(
            phase.pay(seat(1), card("4D")),
            Err(ActionError::NotATributePayer)
        );
        assert_eq!(
            phase.pay(seat(0), card("3C")),
            Err(ActionError::InvalidTributeCard)
        );
        assert_eq!(
            phase.pay(seat(0), card("AD")),
            Err(ActionError::InvalidTributeCard)
        );
        assert_eq!(
            phase.pay(seat(0), card("AS")),
            Ok(TributeOutcome::Continues)
        );
        assert_eq!(
            phase.pay(seat(0), card("AS")),
            Err(ActionError::AlreadyPaid)
        );
    }

    #[test]
    fn double_tribute_reveal_and_move_timing() {
        let dealt = hands("AS 3C", "4D JS", "KS 5C", "7D QS");
        let plan = plan_tribute(&result(&[1, 3]), &dealt, level(Rank::Two));
        let TributePlan::Tribute(duties) = plan else {
            panic!("expected a tribute")
        };
        assert_eq!(duties, vec![duty(0, 1), duty(2, 3)]);
        let mut phase = TributePhase::new(dealt.clone(), level(Rank::Two), duties);

        // First payment is only recorded.
        assert_eq!(
            phase.pay(seat(0), card("AS")),
            Ok(TributeOutcome::Continues)
        );
        assert!(!phase.all_paid());
        assert_eq!(phase.hands, dealt);

        // Returns are rejected until every tribute is paid.
        assert!(phase.return_options(seat(1)).is_empty());
        assert_eq!(
            phase.give_back(seat(1), card("4D")),
            Err(ActionError::TributeNotComplete)
        );

        // The last payment moves both tributes at once.
        assert_eq!(
            phase.pay(seat(2), card("KS")),
            Ok(TributeOutcome::Continues)
        );
        assert!(phase.all_paid());
        assert_eq!(phase.hands, hands("3C", "4D JS AS", "5C", "7D QS KS"));

        assert_eq!(phase.return_options(seat(1)), cards("4D"));
        assert_eq!(phase.return_options(seat(3)), cards("7D"));
        assert!(phase.return_options(seat(0)).is_empty());

        assert_eq!(
            phase.give_back(seat(0), card("3C")),
            Err(ActionError::NotATributeReceiver)
        );
        assert_eq!(
            phase.give_back(seat(1), card("JS")),
            Err(ActionError::InvalidReturnCard)
        );
        assert_eq!(
            phase.give_back(seat(1), card("AS")),
            Err(ActionError::InvalidReturnCard)
        );

        // First return is recorded blind: nothing moves.
        let before = phase.hands.clone();
        assert_eq!(
            phase.give_back(seat(1), card("4D")),
            Ok(TributeOutcome::Continues)
        );
        assert_eq!(phase.hands, before);
        assert!(phase.return_options(seat(1)).is_empty());
        assert_eq!(
            phase.give_back(seat(1), card("4D")),
            Err(ActionError::AlreadyReturned)
        );

        // The last return completes the phase and moves both at once.
        let outcome = phase.give_back(seat(3), card("7D")).unwrap();
        assert_eq!(
            outcome,
            TributeOutcome::Complete {
                hands: hands("3C 4D", "JS AS", "5C 7D", "QS KS"),
                leader: seat(0),
                exchanges: vec![
                    Exchange {
                        payer: seat(0),
                        receiver: seat(1),
                        tribute: card("AS"),
                        returned: card("4D"),
                    },
                    Exchange {
                        payer: seat(2),
                        receiver: seat(3),
                        tribute: card("KS"),
                        returned: card("7D"),
                    },
                ],
            }
        );
    }

    /// Plays a whole tribute phase from `plan_tribute`, each payer giving
    /// their first option and each receiver returning their first option.
    fn run_to_completion(previous: &DealResult, dealt: [PlayerHand; 4], lvl: Rank) -> SeatId {
        let TributePlan::Tribute(duties) = plan_tribute(previous, &dealt, level(lvl)) else {
            panic!("expected a tribute")
        };
        let mut phase = TributePhase::new(dealt, level(lvl), duties.clone());
        for d in &duties {
            let option = phase.tribute_options(d.payer)[0];
            phase.pay(d.payer, option).unwrap();
        }
        let mut last = TributeOutcome::Continues;
        for d in &duties {
            let option = phase.return_options(d.receiver)[0];
            last = phase.give_back(d.receiver, option).unwrap();
        }
        match last {
            TributeOutcome::Complete { leader, .. } => leader,
            TributeOutcome::Continues => panic!("expected Complete"),
        }
    }

    #[test]
    fn single_tribute_leader_is_the_payer() {
        let dealt = hands("3C", "4C", "5C", "AS 6C");
        assert_eq!(
            run_to_completion(&result(&[0, 1, 2, 3]), dealt, Rank::Two),
            seat(3)
        );
        // 1-4: the winner's partner pays and leads.
        let dealt = hands("3C", "4C", "AS 6C", "5C");
        assert_eq!(
            run_to_completion(&result(&[0, 1, 3, 2]), dealt, Rank::Two),
            seat(2)
        );
    }

    #[test]
    fn double_tribute_leader_is_the_payer_to_first_place() {
        // Seat 2 has the higher card, so pays 1st place (1) and leads.
        let dealt = hands("KS 3C", "4C", "AS 5C", "6C");
        assert_eq!(
            run_to_completion(&result(&[1, 3]), dealt, Rank::Two),
            seat(2)
        );

        // Tie with 1st = 1: seat 2 (whose prev is 1) pays 1st place and leads.
        let dealt = hands("AS 3C", "4C", "AD 5C", "6C");
        assert_eq!(
            run_to_completion(&result(&[1, 3]), dealt.clone(), Rank::Two),
            seat(2)
        );
        // Tie with 1st = 3: seat 0 (whose prev is 3) leads.
        assert_eq!(
            run_to_completion(&result(&[3, 1]), dealt, Rank::Two),
            seat(0)
        );
    }

    // ---- return_options ----

    /// Single tribute 3 → 0, already paid, so the receiver can return.
    fn paid(receiver_hand: &str, payer_hand: &str, tribute: &str, lvl: Rank) -> TributePhase {
        let mut phase = TributePhase::new(
            hands(receiver_hand, "2C", "2D", payer_hand),
            level(lvl),
            vec![duty(3, 0)],
        );
        phase.pay(seat(3), card(tribute)).unwrap();
        phase
    }

    #[test]
    fn level_card_is_never_a_return() {
        // Level 6: 6♠/6♦ are ≤ 10 naturally but excluded; J is above 10.
        let phase = paid("6S 6D 9C JS", "AS 3C", "AS", Rank::Six);
        assert_eq!(phase.return_options(seat(0)), cards("9C"));
    }

    #[test]
    fn return_options_are_distinct_and_sorted() {
        let phase = paid("TS 3D 3D 9H QS BJ", "AS 3C", "AS", Rank::Two);
        assert_eq!(phase.return_options(seat(0)), sorted("TS 3D 9H"));
    }

    #[test]
    fn lowest_card_fallback_when_nothing_is_ten_or_below() {
        // Only J, J, Q, a level card, and the received A: the two jacks tie
        // for lowest.
        let phase = paid("JS JH QS 6S", "AS 3C", "AS", Rank::Six);
        assert_eq!(phase.return_options(seat(0)), sorted("JS JH"));

        // The fallback can land on a level card when it's the lowest left.
        let phase = paid("6S SJ", "AS 3C", "AS", Rank::Six);
        assert_eq!(phase.return_options(seat(0)), cards("6S"));
    }

    #[test]
    fn cannot_return_the_received_card() {
        // The tribute 5♠ is ≤ 10, but it's the card just received.
        let mut phase = paid("JS QS", "5S 3C", "5S", Rank::Two);
        assert_eq!(phase.return_options(seat(0)), cards("JS"));
        assert_eq!(
            phase.give_back(seat(0), card("5S")),
            Err(ActionError::InvalidReturnCard)
        );
    }

    #[test]
    fn can_return_another_copy_of_the_received_card() {
        let mut phase = paid("5S JS", "5S 3C", "5S", Rank::Two);
        assert_eq!(phase.return_options(seat(0)), cards("5S"));
        let outcome = phase.give_back(seat(0), card("5S")).unwrap();
        let TributeOutcome::Complete {
            hands: final_hands,
            leader,
            exchanges,
        } = outcome
        else {
            panic!("expected Complete")
        };
        assert_eq!(leader, seat(3));
        assert_eq!(final_hands[0], hand("5S JS"));
        assert_eq!(final_hands[3], hand("5S 3C"));
        assert_eq!(
            exchanges,
            vec![Exchange {
                payer: seat(3),
                receiver: seat(0),
                tribute: card("5S"),
                returned: card("5S"),
            }]
        );
        // The phase was emptied by the move.
        assert!(phase.hands.iter().all(PlayerHand::is_empty));
    }
}
