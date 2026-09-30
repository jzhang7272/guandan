//! Level-aware ranking (TECH_SPEC.md §3.2, GAME_RULES.md "Deck & Card Ranking").
//!
//! There are two orderings, and every comparison must pick the right one:
//! - `face_value`: singles, pairs, triples, full houses, same-rank bombs, and
//!   tribute/return "highest"/"lowest". The level rank is elevated above Ace.
//! - `natural_value`: straights, tubes, plates, straight flushes. The level
//!   rank stays in its natural position.

use serde::{Deserialize, Serialize};

use super::card::{Card, Face, Rank, Suit};

/// The deal level: the rank that is elevated (and whose heart copies are the
/// wildcards) for this deal. On the wire it's just the rank, e.g. `"Six"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Level(pub(crate) Rank);

/// Strength in the NON-SEQUENTIAL order. Higher = stronger.
///
/// The 12 ranks other than the level rank map to 0..=11 in natural order;
/// the level rank (all suits, tied) is 12; Small Joker 13; Big Joker 14.
/// Suit never matters — a wildcard has the same value as any other level card.
pub fn face_value(face: Face, level: Level) -> u8 {
    match face {
        Face::SmallJoker => 13,
        Face::BigJoker => 14,
        Face::Rank(rank) if rank == level.0 => 12,
        Face::Rank(rank) => {
            // Position in natural order, Two = 0 … Ace = 12.
            let natural = natural_value(rank) - 2;
            // Ranks above the level rank slide down one to close the gap it
            // left when it moved to the top.
            if natural_value(rank) > natural_value(level.0) {
                natural - 1
            } else {
                natural
            }
        }
    }
}

/// Strength in the SEQUENTIAL order: Two = 2 … King = 13, Ace = 14. The level
/// rank is not elevated. (Ace-low runs never compare by the Ace: A-2-3-4-5
/// has top Five.)
pub fn natural_value(rank: Rank) -> u8 {
    match rank {
        Rank::Two => 2,
        Rank::Three => 3,
        Rank::Four => 4,
        Rank::Five => 5,
        Rank::Six => 6,
        Rank::Seven => 7,
        Rank::Eight => 8,
        Rank::Nine => 9,
        Rank::Ten => 10,
        Rank::Jack => 11,
        Rank::Queen => 12,
        Rank::King => 13,
        Rank::Ace => 14,
    }
}

/// The heart copy of the level rank (e.g. 6♥ at level 6).
pub fn is_wildcard(card: Card, level: Level) -> bool {
    matches!(card, Card::Standard { rank, suit: Suit::Heart } if rank == level.0)
}

/// Any suit's copy of the level rank.
pub fn is_level_card(card: Card, level: Level) -> bool {
    matches!(card, Card::Standard { rank, .. } if rank == level.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_util::cards;

    #[test]
    fn level_rank_all_suits_are_equal() {
        for level_rank in Rank::ALL {
            let level = Level(level_rank);
            for suit in Suit::ALL {
                let card = Card::Standard {
                    rank: level_rank,
                    suit,
                };
                assert_eq!(face_value(card.face(), level), 12);
            }
        }
    }

    #[test]
    fn values_are_0_to_14_with_no_gaps_at_every_level() {
        for level_rank in Rank::ALL {
            let level = Level(level_rank);
            let mut values: Vec<u8> = Rank::ALL
                .into_iter()
                .map(|rank| face_value(Face::Rank(rank), level))
                .collect();
            values.push(face_value(Face::SmallJoker, level));
            values.push(face_value(Face::BigJoker, level));
            values.sort();
            assert_eq!(
                values,
                (0..=14).collect::<Vec<u8>>(),
                "level {level_rank:?}"
            );
        }
    }

    #[test]
    fn worked_example_at_level_six() {
        // GAME_RULES.md: 2 < 3 < 4 < 5 < 7 < … < A < 6 < Small Joker < Big Joker.
        let level = Level(Rank::Six);
        let order = cards("2S 3S 4S 5S 7S 8S 9S TS JS QS KS AS 6S SJ BJ");
        let values: Vec<u8> = order.iter().map(|c| face_value(c.face(), level)).collect();
        assert_eq!(values, (0..=14).collect::<Vec<u8>>());
    }

    #[test]
    fn level_two_and_level_ace() {
        // At level Two the level rank leaves the bottom: Three becomes 0.
        let two = Level(Rank::Two);
        assert_eq!(face_value(Face::Rank(Rank::Three), two), 0);
        assert_eq!(face_value(Face::Rank(Rank::Ace), two), 11);
        assert_eq!(face_value(Face::Rank(Rank::Two), two), 12);

        // At level Ace nothing moves except that Ace is 12 either way.
        let ace = Level(Rank::Ace);
        assert_eq!(face_value(Face::Rank(Rank::Two), ace), 0);
        assert_eq!(face_value(Face::Rank(Rank::King), ace), 11);
        assert_eq!(face_value(Face::Rank(Rank::Ace), ace), 12);
    }

    #[test]
    fn natural_value_ignores_level() {
        let values: Vec<u8> = Rank::ALL.into_iter().map(natural_value).collect();
        assert_eq!(values, (2..=14).collect::<Vec<u8>>());
    }

    #[test]
    fn wildcard_is_only_the_heart_level_card() {
        let level = Level(Rank::Six);
        let [six_h, six_s, seven_h, small_joker] = cards("6H 6S 7H SJ")[..] else {
            unreachable!()
        };

        assert!(is_wildcard(six_h, level));
        assert!(!is_wildcard(six_s, level));
        assert!(!is_wildcard(seven_h, level));
        assert!(!is_wildcard(small_joker, level));

        assert!(is_level_card(six_h, level));
        assert!(is_level_card(six_s, level));
        assert!(!is_level_card(seven_h, level));
        assert!(!is_level_card(small_joker, level));
    }

    #[test]
    fn level_serializes_as_the_rank() {
        assert_eq!(
            serde_json::to_string(&Level(Rank::Six)).unwrap(),
            r#""Six""#
        );
        let back: Level = serde_json::from_str(r#""Five""#).unwrap();
        assert_eq!(back, Level(Rank::Five));
    }
}
