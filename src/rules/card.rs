//! Cards, seats, and teams (TECH_SPEC.md §3.1).
//!
//! Derived `Ord` on these types is only for deterministic sorting. It is NOT
//! game strength — use `face_value` / `natural_value` / `beats` for that.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Suit {
    Spade,
    Heart,
    Diamond,
    Club,
}

impl Suit {
    pub const ALL: [Suit; 4] = [Suit::Spade, Suit::Heart, Suit::Diamond, Suit::Club];
}

/// Declared in natural order, so derived `Ord` is natural order (Two < ... < Ace).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Rank {
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Ten,
    Jack,
    Queen,
    King,
    Ace,
}

impl Rank {
    /// Every rank in natural order.
    pub const ALL: [Rank; 13] = [
        Rank::Two,
        Rank::Three,
        Rank::Four,
        Rank::Five,
        Rank::Six,
        Rank::Seven,
        Rank::Eight,
        Rank::Nine,
        Rank::Ten,
        Rank::Jack,
        Rank::Queen,
        Rank::King,
        Rank::Ace,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum JokerColor {
    Small,
    Big,
}

/// One physical card. With two decks, duplicate `Card` values are expected —
/// a hand is a multiset (`Vec<Card>`), not a set. Two physical 6♠ are
/// interchangeable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Card {
    Standard { rank: Rank, suit: Suit },
    Joker(JokerColor),
}

/// A card ignoring suit — what non-sequential combos are ranked by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Face {
    Rank(Rank),
    SmallJoker,
    BigJoker,
}

impl Card {
    pub fn face(self) -> Face {
        match self {
            Card::Standard { rank, .. } => Face::Rank(rank),
            Card::Joker(JokerColor::Small) => Face::SmallJoker,
            Card::Joker(JokerColor::Big) => Face::BigJoker,
        }
    }
}

/// The 108-card double deck: 2 × 52 standard cards + 2 Small + 2 Big jokers.
pub fn full_deck() -> Vec<Card> {
    let mut deck = Vec::with_capacity(108);
    for _ in 0..2 {
        for suit in Suit::ALL {
            for rank in Rank::ALL {
                deck.push(Card::Standard { rank, suit });
            }
        }
        deck.push(Card::Joker(JokerColor::Small));
        deck.push(Card::Joker(JokerColor::Big));
    }
    deck
}

/// A seat at the table, 0..=3. Turn order is 0 → 1 → 2 → 3 → 0; partners sit
/// opposite (0 & 2, 1 & 3).
///
/// The field is private so the only ways to get a `SeatId` are `SeatId::new`
/// (which checks the range) and `SeatId::ALL` — the compiler guarantees the
/// 0..=3 invariant, so `index()` is always a safe array index.
///
/// On the wire it's a bare number (`0`); deserializing goes through
/// `TryFrom<u8>`, so an out-of-range seat from a client is a parse error
/// rather than a panic later.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct SeatId(u8);

impl SeatId {
    pub const ALL: [SeatId; 4] = [SeatId(0), SeatId(1), SeatId(2), SeatId(3)];

    /// `None` unless `index` is 0..=3.
    pub fn new(index: u8) -> Option<SeatId> {
        if index < 4 { Some(SeatId(index)) } else { None }
    }

    /// Seats 0 and 2 are team A; seats 1 and 3 are team B.
    pub fn team(self) -> Team {
        match self.0 {
            0 | 2 => Team::A,
            _ => Team::B,
        }
    }

    /// The seat opposite: `(self + 2) % 4`.
    pub fn partner(self) -> SeatId {
        SeatId((self.0 + 2) % 4)
    }

    /// The next player to act (turn order): `(self + 1) % 4`.
    pub fn next(self) -> SeatId {
        SeatId((self.0 + 1) % 4)
    }

    /// The player who acts just before this one: `(self + 3) % 4`.
    pub fn prev(self) -> SeatId {
        SeatId((self.0 + 3) % 4)
    }

    /// For indexing `[T; 4]` arrays.
    pub fn index(self) -> usize {
        usize::from(self.0)
    }
}

impl TryFrom<u8> for SeatId {
    type Error = String;

    fn try_from(index: u8) -> Result<Self, Self::Error> {
        SeatId::new(index).ok_or_else(|| format!("seat {index} is out of range 0..=3"))
    }
}

impl From<SeatId> for u8 {
    fn from(seat: SeatId) -> u8 {
        seat.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Team {
    A,
    B,
}

impl Team {
    /// For indexing per-team arrays: A → 0, B → 1.
    pub fn index(self) -> usize {
        match self {
            Team::A => 0,
            Team::B => 1,
        }
    }

    pub fn other(self) -> Team {
        match self {
            Team::A => Team::B,
            Team::B => Team::A,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seat(index: u8) -> SeatId {
        SeatId::new(index).unwrap()
    }

    #[test]
    fn full_deck_has_two_of_every_card() {
        let deck = full_deck();
        assert_eq!(deck.len(), 108);

        let count = |card: Card| deck.iter().filter(|&&c| c == card).count();
        for suit in Suit::ALL {
            for rank in Rank::ALL {
                assert_eq!(count(Card::Standard { rank, suit }), 2);
            }
        }
        assert_eq!(count(Card::Joker(JokerColor::Small)), 2);
        assert_eq!(count(Card::Joker(JokerColor::Big)), 2);
    }

    #[test]
    fn face_ignores_suit() {
        let spade = Card::Standard {
            rank: Rank::Six,
            suit: Suit::Spade,
        };
        let heart = Card::Standard {
            rank: Rank::Six,
            suit: Suit::Heart,
        };
        assert_eq!(spade.face(), Face::Rank(Rank::Six));
        assert_eq!(spade.face(), heart.face());
        assert_eq!(Card::Joker(JokerColor::Small).face(), Face::SmallJoker);
        assert_eq!(Card::Joker(JokerColor::Big).face(), Face::BigJoker);
    }

    #[test]
    fn seat_new_checks_range() {
        assert_eq!(SeatId::new(0), Some(SeatId::ALL[0]));
        assert_eq!(SeatId::new(3), Some(SeatId::ALL[3]));
        assert_eq!(SeatId::new(4), None);
        assert_eq!(SeatId::new(255), None);
    }

    #[test]
    fn seat_all_is_in_index_order() {
        for (i, s) in SeatId::ALL.into_iter().enumerate() {
            assert_eq!(s.index(), i);
        }
    }

    #[test]
    fn seat_teams_and_partners() {
        assert_eq!(seat(0).team(), Team::A);
        assert_eq!(seat(1).team(), Team::B);
        assert_eq!(seat(2).team(), Team::A);
        assert_eq!(seat(3).team(), Team::B);

        assert_eq!(seat(0).partner(), seat(2));
        assert_eq!(seat(1).partner(), seat(3));
        assert_eq!(seat(2).partner(), seat(0));
        assert_eq!(seat(3).partner(), seat(1));
        for s in SeatId::ALL {
            assert_eq!(s.partner().team(), s.team());
        }
    }

    #[test]
    fn seat_next_and_prev_wrap() {
        assert_eq!(seat(0).next(), seat(1));
        assert_eq!(seat(3).next(), seat(0));
        assert_eq!(seat(0).prev(), seat(3));
        assert_eq!(seat(2).prev(), seat(1));
        for s in SeatId::ALL {
            assert_eq!(s.next().prev(), s);
            // Neighbours are always on the other team.
            assert_ne!(s.next().team(), s.team());
        }
    }

    #[test]
    fn team_index_and_other() {
        assert_eq!(Team::A.index(), 0);
        assert_eq!(Team::B.index(), 1);
        assert_eq!(Team::A.other(), Team::B);
        assert_eq!(Team::B.other(), Team::A);
    }

    #[test]
    fn seat_serializes_as_a_bare_number() {
        assert_eq!(serde_json::to_string(&seat(2)).unwrap(), "2");
        let back: SeatId = serde_json::from_str("3").unwrap();
        assert_eq!(back, seat(3));
    }

    #[test]
    fn seat_deserialization_rejects_out_of_range() {
        assert!(serde_json::from_str::<SeatId>("4").is_err());
        assert!(serde_json::from_str::<SeatId>("-1").is_err());
        assert!(serde_json::from_str::<SeatId>("\"0\"").is_err());
    }

    #[test]
    fn card_json_shape() {
        // TECH_SPEC.md §4: data-carrying enums are externally tagged.
        let card = Card::Standard {
            rank: Rank::Three,
            suit: Suit::Club,
        };
        assert_eq!(
            serde_json::to_string(&card).unwrap(),
            r#"{"Standard":{"rank":"Three","suit":"Club"}}"#
        );
        assert_eq!(
            serde_json::to_string(&Card::Joker(JokerColor::Small)).unwrap(),
            r#"{"Joker":"Small"}"#
        );
        assert_eq!(
            serde_json::to_string(&Face::BigJoker).unwrap(),
            r#""BigJoker""#
        );
        assert_eq!(
            serde_json::to_string(&Face::Rank(Rank::Nine)).unwrap(),
            r#"{"Rank":"Nine"}"#
        );
        assert_eq!(serde_json::to_string(&Team::B).unwrap(), r#""B""#);
    }
}
