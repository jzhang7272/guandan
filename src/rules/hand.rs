//! A player's hand (TECH_SPEC.md §3.5).

use serde::{Deserialize, Serialize};

use super::card::Card;
use super::error::ActionError;

/// The cards a player holds: a multiset (two decks, so duplicates are normal).
///
/// Kept sorted by `Card`'s derived `Ord` so output is deterministic; that's
/// not game strength, and clients re-sort for display. The field is private
/// so the sorted invariant can't be broken from outside — build one with
/// `new` (or `Default` for an empty hand).
///
/// Deserializing goes through `From<Vec<Card>>`, which sorts, so a hand read
/// from JSON keeps the invariant too.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Vec<Card>")]
pub struct PlayerHand(Vec<Card>);

impl PlayerHand {
    /// Sorts the cards. The only way to build a non-empty hand.
    pub fn new(mut cards: Vec<Card>) -> Self {
        cards.sort();
        PlayerHand(cards)
    }

    pub fn cards(&self) -> &[Card] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// How many physical copies of `card` the hand holds (0, 1, or 2).
    pub fn count(&self, card: Card) -> usize {
        self.0.iter().filter(|&&c| c == card).count()
    }

    /// Multiset containment: `[6♠, 6♠]` needs two 6♠.
    pub fn contains_all(&self, cards: &[Card]) -> bool {
        // Every card must be held at least as many times as it's asked for.
        // Hands hold at most 27 cards, so counting per card is cheap enough.
        cards.iter().all(|&card| {
            let wanted = cards.iter().filter(|&&c| c == card).count();
            self.count(card) >= wanted
        })
    }

    /// Inserts `card` at its sorted position, keeping the invariant.
    pub fn add(&mut self, card: Card) {
        let index = self.0.partition_point(|&c| c <= card);
        self.0.insert(index, card);
    }

    /// Atomic: removes all of `cards` or none (`Err(CardsNotInHand)`).
    pub fn remove_all(&mut self, cards: &[Card]) -> Result<(), ActionError> {
        // Work on a copy so that a card missing halfway through leaves the
        // hand untouched. Removing from a sorted Vec keeps it sorted.
        let mut remaining = self.0.clone();
        for card in cards {
            let index = remaining
                .iter()
                .position(|c| c == card)
                .ok_or(ActionError::CardsNotInHand)?;
            remaining.remove(index);
        }
        self.0 = remaining;
        Ok(())
    }
}

impl From<Vec<Card>> for PlayerHand {
    fn from(cards: Vec<Card>) -> Self {
        PlayerHand::new(cards)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_util::{cards, hand};

    #[test]
    fn new_sorts() {
        let h = PlayerHand::new(cards("BJ 5S 2C 5S SJ"));
        assert_eq!(h.cards(), cards("2C 5S 5S SJ BJ"));
    }

    #[test]
    fn len_and_is_empty() {
        let h = hand("5S 5S 6H");
        assert_eq!(h.len(), 3);
        assert!(!h.is_empty());

        let empty = PlayerHand::new(Vec::new());
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
    }

    #[test]
    fn default_is_empty() {
        assert!(PlayerHand::default().is_empty());
        assert_eq!(PlayerHand::default(), PlayerHand::new(Vec::new()));
    }

    #[test]
    fn count_counts_physical_copies() {
        let h = hand("5S 5S 5H BJ");
        let [five_spade, five_heart, big_joker, six_spade] = cards("5S 5H BJ 6S")[..] else {
            unreachable!()
        };
        assert_eq!(h.count(five_spade), 2);
        assert_eq!(h.count(five_heart), 1);
        assert_eq!(h.count(big_joker), 1);
        assert_eq!(h.count(six_spade), 0);
    }

    #[test]
    fn serializes_as_a_plain_list_and_deserializes_sorted() {
        let h = hand("SJ 3C");
        assert_eq!(
            serde_json::to_string(&h).unwrap(),
            r#"[{"Standard":{"rank":"Three","suit":"Club"}},{"Joker":"Small"}]"#
        );
        let unsorted = r#"[{"Joker":"Small"},{"Standard":{"rank":"Three","suit":"Club"}}]"#;
        let back: PlayerHand = serde_json::from_str(unsorted).unwrap();
        assert_eq!(back, h);
    }

    #[test]
    fn contains_all_is_multiset_containment() {
        let h = hand("6S 6S 7H BJ");
        assert!(h.contains_all(&cards("6S")));
        assert!(h.contains_all(&cards("6S 6S")));
        assert!(h.contains_all(&cards("BJ 6S 7H 6S")));
        assert!(h.contains_all(&[]));
        assert!(!h.contains_all(&cards("6S 6S 6S")));
        assert!(!h.contains_all(&cards("7H 7H")));
        assert!(!h.contains_all(&cards("SJ")));
        // Same rank in another suit is a different card.
        assert!(!h.contains_all(&cards("6H")));

        // One 6♠ doesn't cover a pair of 6♠.
        assert!(!hand("6S 7H").contains_all(&cards("6S 6S")));
    }

    #[test]
    fn add_keeps_the_hand_sorted() {
        let mut h = hand("3C 9S BJ");
        for card in cards("SJ 2D 9S KH") {
            h.add(card);
            assert!(h.cards().is_sorted());
        }
        assert_eq!(h, hand("3C 9S BJ SJ 2D 9S KH"));
        assert_eq!(h.count(cards("9S")[0]), 2);

        let mut empty = PlayerHand::default();
        empty.add(cards("5S")[0]);
        assert_eq!(empty, hand("5S"));
    }

    #[test]
    fn remove_all_removes_one_copy_per_listed_card() {
        let mut h = hand("6S 6S 6S 7H BJ");
        assert_eq!(h.remove_all(&cards("6S 6S BJ")), Ok(()));
        assert_eq!(h, hand("6S 7H"));

        assert_eq!(h.remove_all(&[]), Ok(()));
        assert_eq!(h, hand("6S 7H"));

        assert_eq!(h.remove_all(&cards("7H 6S")), Ok(()));
        assert!(h.is_empty());
    }

    #[test]
    fn remove_all_is_atomic() {
        let original = hand("6S 7H 8D");

        // Too many copies: the first 6♠ would come out, the second can't.
        let mut h = original.clone();
        assert_eq!(
            h.remove_all(&cards("6S 7H 6S")),
            Err(ActionError::CardsNotInHand)
        );
        assert_eq!(h, original);

        // A card that isn't held at all, listed after ones that are.
        let mut h = original.clone();
        assert_eq!(
            h.remove_all(&cards("6S 7H 8D BJ")),
            Err(ActionError::CardsNotInHand)
        );
        assert_eq!(h, original);
    }
}
