//! Dealing and deal results (TECH_SPEC.md §3.7).

use rand::Rng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

use super::card::{Card, Rank, SeatId, Suit, Team, full_deck};
use super::hand::PlayerHand;

/// Cards per player: 108 / 4.
const HAND_SIZE: usize = 27;

/// Shuffles `full_deck()` and deals 27 cards to each seat.
pub fn deal_hands(rng: &mut impl Rng) -> [PlayerHand; 4] {
    let mut deck = full_deck();
    deck.shuffle(rng);
    // After a uniform shuffle, handing out consecutive blocks of 27 is as
    // random as dealing one card at a time around the table.
    let mut blocks = deck.chunks(HAND_SIZE);
    std::array::from_fn(|_| {
        let block = blocks.next().expect("108 cards make exactly 4 hands");
        PlayerHand::new(block.to_vec())
    })
}

/// First deal of a match (GAME_RULES.md "Deal Start", engine equivalent): pick
/// one physical card uniformly from all cards except the 4 jokers and the two
/// 2♥. Returns the holder (who leads) and the card (shown to everyone).
///
/// Panics if no seat holds an eligible card (impossible after `deal_hands`).
pub fn pick_first_leader(hands: &[PlayerHand; 4], rng: &mut impl Rng) -> (SeatId, Card) {
    let two_of_hearts = Card::Standard {
        rank: Rank::Two,
        suit: Suit::Heart,
    };
    // One entry per physical card, so a seat holding both copies of a card
    // is twice as likely to be picked for it — that's what "uniform over
    // physical cards" means.
    let eligible: Vec<(SeatId, Card)> = SeatId::ALL
        .into_iter()
        .flat_map(|seat| hands[seat.index()].cards().iter().map(move |&c| (seat, c)))
        .filter(|&(_, card)| matches!(card, Card::Standard { .. }) && card != two_of_hearts)
        .collect();
    assert!(!eligible.is_empty(), "no card to reveal for the first lead");
    eligible[rng.random_range(0..eligible.len())]
}

/// The 1-2 / 1-3 / 1-4 finish types (GAME_RULES.md terminology).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[expect(
    clippy::enum_variant_names,
    reason = "keep the TECH_SPEC.md §3.7 names (1-2 / 1-3 / 1-4 finish types)"
)]
pub enum FinishKind {
    OneTwo,
    OneThree,
    OneFour,
}

impl FinishKind {
    /// Levels the winning team advances: 3 / 2 / 1.
    pub fn levels(self) -> u8 {
        match self {
            FinishKind::OneTwo => 3,
            FinishKind::OneThree => 2,
            FinishKind::OneFour => 1,
        }
    }
}

/// Finish places of a completed deal. `order` has 2 entries for a 1-2 finish
/// (the losing team is unranked — GAME_RULES.md "Deal End"), otherwise 4 (the
/// last entry is the player still holding cards).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DealResult {
    pub(crate) order: Vec<SeatId>,
}

impl DealResult {
    pub fn first(&self) -> SeatId {
        self.order[0]
    }

    /// `first().team()`.
    pub fn winning_team(&self) -> Team {
        self.first().team()
    }

    /// From where `first().partner()` appears in `order`.
    pub fn kind(&self) -> FinishKind {
        let partner = self.first().partner();
        match self.order.iter().position(|&seat| seat == partner) {
            Some(1) => FinishKind::OneTwo,
            Some(2) => FinishKind::OneThree,
            Some(3) => FinishKind::OneFour,
            _ => panic!("malformed deal result {:?}", self.order),
        }
    }

    /// `None` for 1-2.
    pub fn fourth(&self) -> Option<SeatId> {
        // A 1-2 order only has 2 entries, so there is no index 3.
        self.order.get(3).copied()
    }

    /// The other team's seats, in seat order.
    pub fn losers(&self) -> [SeatId; 2] {
        let losing_team = self.winning_team().other();
        let mut seats = SeatId::ALL
            .into_iter()
            .filter(|seat| seat.team() == losing_team);
        let (Some(a), Some(b)) = (seats.next(), seats.next()) else {
            unreachable!("every team has two seats")
        };
        [a, b]
    }
}

/// How the current deal began — shown to all players for the whole deal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DealStart {
    FirstDeal {
        revealed: Card,
        leader: SeatId,
    },
    AntiTribute {
        leader: SeatId,
    },
    Tribute {
        exchanges: Vec<Exchange>,
        leader: SeatId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exchange {
    pub(crate) payer: SeatId,
    pub(crate) receiver: SeatId,
    pub(crate) tribute: Card,
    pub(crate) returned: Card,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_util::{cards, hand};
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn seat(index: u8) -> SeatId {
        SeatId::new(index).unwrap()
    }

    fn result(order: &[u8]) -> DealResult {
        DealResult {
            order: order.iter().map(|&i| seat(i)).collect(),
        }
    }

    #[test]
    fn deal_hands_deals_the_whole_deck_27_each() {
        let hands = deal_hands(&mut StdRng::seed_from_u64(1));
        for h in &hands {
            assert_eq!(h.len(), 27);
            assert!(h.cards().is_sorted());
        }
        let mut dealt: Vec<Card> = hands.iter().flat_map(|h| h.cards().to_vec()).collect();
        dealt.sort();
        let mut deck = full_deck();
        deck.sort();
        assert_eq!(dealt, deck);
    }

    #[test]
    fn deal_hands_is_deterministic_for_a_seed() {
        let a = deal_hands(&mut StdRng::seed_from_u64(42));
        let b = deal_hands(&mut StdRng::seed_from_u64(42));
        assert_eq!(a, b);
        // And actually shuffles: another seed gives another deal.
        let c = deal_hands(&mut StdRng::seed_from_u64(43));
        assert_ne!(a, c);
    }

    #[test]
    fn pick_first_leader_never_picks_a_joker_or_two_of_hearts() {
        let two_of_hearts = cards("2H")[0];
        for seed in 0..500 {
            let mut rng = StdRng::seed_from_u64(seed);
            let hands = deal_hands(&mut rng);
            let (leader, card) = pick_first_leader(&hands, &mut rng);
            assert!(
                matches!(card, Card::Standard { .. }),
                "seed {seed}: {card:?}"
            );
            assert_ne!(card, two_of_hearts, "seed {seed}");
            // The leader really holds the revealed card.
            assert!(hands[leader.index()].count(card) > 0, "seed {seed}");
        }
    }

    #[test]
    fn pick_first_leader_can_pick_every_seat() {
        let mut seen = [false; 4];
        for seed in 0..200 {
            let mut rng = StdRng::seed_from_u64(seed);
            let hands = deal_hands(&mut rng);
            seen[pick_first_leader(&hands, &mut rng).0.index()] = true;
        }
        assert_eq!(seen, [true; 4]);
    }

    #[test]
    fn pick_first_leader_returns_the_holder_of_the_only_eligible_card() {
        // Only seat 2 holds a card that may be revealed.
        let hands = [
            hand("SJ BJ 2H"),
            hand("SJ 2H"),
            hand("BJ 5S"),
            PlayerHand::default(),
        ];
        for seed in 0..20 {
            let picked = pick_first_leader(&hands, &mut StdRng::seed_from_u64(seed));
            assert_eq!(picked, (seat(2), cards("5S")[0]));
        }
    }

    #[test]
    fn finish_kind_levels() {
        assert_eq!(FinishKind::OneTwo.levels(), 3);
        assert_eq!(FinishKind::OneThree.levels(), 2);
        assert_eq!(FinishKind::OneFour.levels(), 1);
    }

    #[test]
    fn one_two_finish() {
        // Seats 1 and 3 go out first and second; team A is unranked.
        let r = result(&[1, 3]);
        assert_eq!(r.first(), seat(1));
        assert_eq!(r.winning_team(), Team::B);
        assert_eq!(r.kind(), FinishKind::OneTwo);
        assert_eq!(r.fourth(), None);
        assert_eq!(r.losers(), [seat(0), seat(2)]);
    }

    #[test]
    fn one_three_finish() {
        let r = result(&[0, 1, 2, 3]);
        assert_eq!(r.first(), seat(0));
        assert_eq!(r.winning_team(), Team::A);
        assert_eq!(r.kind(), FinishKind::OneThree);
        assert_eq!(r.fourth(), Some(seat(3)));
        assert_eq!(r.losers(), [seat(1), seat(3)]);

        let r = result(&[3, 0, 1, 2]);
        assert_eq!(r.winning_team(), Team::B);
        assert_eq!(r.kind(), FinishKind::OneThree);
        assert_eq!(r.fourth(), Some(seat(2)));
        assert_eq!(r.losers(), [seat(0), seat(2)]);
    }

    #[test]
    fn one_four_finish_fourth_is_the_winners_partner() {
        let r = result(&[2, 1, 3, 0]);
        assert_eq!(r.first(), seat(2));
        assert_eq!(r.winning_team(), Team::A);
        assert_eq!(r.kind(), FinishKind::OneFour);
        assert_eq!(r.fourth(), Some(seat(0)));
        assert_eq!(r.fourth(), Some(r.first().partner()));
        assert_eq!(r.losers(), [seat(1), seat(3)]);

        let r = result(&[1, 2, 0, 3]);
        assert_eq!(r.winning_team(), Team::B);
        assert_eq!(r.kind(), FinishKind::OneFour);
        assert_eq!(r.fourth(), Some(seat(3)));
        assert_eq!(r.losers(), [seat(0), seat(2)]);
    }
}
