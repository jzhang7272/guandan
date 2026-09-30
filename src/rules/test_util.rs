//! Test-only fixture parsers shared by every test module (TECH_SPEC.md §8).
//! Use these instead of writing per-module variants.
//!
//! Format: space-separated tokens. A rank `2`–`9`, `T`, `J`, `Q`, `K`, `A`
//! followed by a suit `S`/`H`/`D`/`C` (e.g. `TS` = 10♠), or `SJ` / `BJ` for
//! the Small / Big Joker. Example: `cards("5S 5H 6H SJ BJ")`.

use super::card::{Card, JokerColor, Rank, Suit};
use super::hand::PlayerHand;

/// Parses a card list. Panics on a malformed token, naming it.
pub(crate) fn cards(s: &str) -> Vec<Card> {
    s.split_whitespace().map(parse_card).collect()
}

/// `PlayerHand::new(cards(s))`.
pub(crate) fn hand(s: &str) -> PlayerHand {
    PlayerHand::new(cards(s))
}

fn parse_card(token: &str) -> Card {
    // Jokers first: "SJ" would otherwise look like rank S, suit J.
    match token {
        "SJ" => return Card::Joker(JokerColor::Small),
        "BJ" => return Card::Joker(JokerColor::Big),
        _ => {}
    }

    let chars: Vec<char> = token.chars().collect();
    let [rank_char, suit_char] = chars[..] else {
        panic!("malformed card token {token:?}: expected 2 characters, like \"TS\" or \"SJ\"");
    };

    let rank = match rank_char {
        '2' => Rank::Two,
        '3' => Rank::Three,
        '4' => Rank::Four,
        '5' => Rank::Five,
        '6' => Rank::Six,
        '7' => Rank::Seven,
        '8' => Rank::Eight,
        '9' => Rank::Nine,
        'T' => Rank::Ten,
        'J' => Rank::Jack,
        'Q' => Rank::Queen,
        'K' => Rank::King,
        'A' => Rank::Ace,
        _ => panic!("malformed card token {token:?}: unknown rank {rank_char:?}"),
    };
    let suit = match suit_char {
        'S' => Suit::Spade,
        'H' => Suit::Heart,
        'D' => Suit::Diamond,
        'C' => Suit::Club,
        _ => panic!("malformed card token {token:?}: unknown suit {suit_char:?}"),
    };
    Card::Standard { rank, suit }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_cards_and_jokers() {
        assert_eq!(
            cards("5S TH AD 2C SJ BJ"),
            vec![
                Card::Standard {
                    rank: Rank::Five,
                    suit: Suit::Spade
                },
                Card::Standard {
                    rank: Rank::Ten,
                    suit: Suit::Heart
                },
                Card::Standard {
                    rank: Rank::Ace,
                    suit: Suit::Diamond
                },
                Card::Standard {
                    rank: Rank::Two,
                    suit: Suit::Club
                },
                Card::Joker(JokerColor::Small),
                Card::Joker(JokerColor::Big),
            ]
        );
    }

    #[test]
    fn keeps_duplicates_and_order() {
        assert_eq!(cards("6S 5S 6S").len(), 3);
        assert_eq!(cards("6S 5S")[0], cards("6S")[0]);
    }

    #[test]
    fn empty_string_is_no_cards() {
        assert!(cards("").is_empty());
        assert!(cards("   ").is_empty());
    }

    #[test]
    fn hand_is_sorted() {
        assert_eq!(hand("BJ 3S 2S").cards(), cards("2S 3S BJ"));
    }

    #[test]
    #[should_panic(expected = "malformed card token \"10S\"")]
    fn rejects_ten_written_as_10() {
        cards("10S");
    }

    #[test]
    #[should_panic(expected = "unknown rank")]
    fn rejects_bad_rank() {
        cards("1S");
    }

    #[test]
    #[should_panic(expected = "unknown suit")]
    fn rejects_bad_suit() {
        cards("5X");
    }

    #[test]
    #[should_panic(expected = "malformed card token")]
    fn rejects_bad_length() {
        cards("5");
    }
}
