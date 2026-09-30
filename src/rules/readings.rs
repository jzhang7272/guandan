//! Every legal reading of a set of cards (TECH_SPEC.md §3.4).
//!
//! Algorithm — template matching: every combo is a fixed multiset of "slots"
//! (a face, plus a suit for straight flushes). We generate every candidate
//! template for the number of cards played and keep those the cards can fill.
//!
//! Filling a template (§3.4): the ordinary cards each take a slot they match
//! exactly; then each wildcard (heart level card) takes one of the slots
//! left over — either standing for itself, or substituting for any other
//! non-joker card (GAME_RULES.md "The Wildcard").

use std::collections::BTreeMap;

use super::card::{Card, Face, Rank, Suit};
use super::combo::{Bomb, Combo, Play};
use super::ranking::{Level, is_wildcard};

/// Every legal reading of exactly these cards, at most one `Play` per distinct
/// `Combo`, sorted by `Combo`'s derived `Ord`. Empty = not a legal combo.
pub fn readings(cards: &[Card], level: Level) -> Vec<Play> {
    // Split once: the filling rules treat the two groups differently.
    let (wildcards, others): (Vec<Card>, Vec<Card>) =
        cards.iter().partition(|&&card| is_wildcard(card, level));

    // Best filling found so far for each combo. A `BTreeMap` both dedupes
    // and gives the deterministic output order (Combo's derived Ord).
    let mut best: BTreeMap<Combo, Filling> = BTreeMap::new();

    for template in templates(cards.len()) {
        if matches!(template.combo, Combo::Straight { .. }) && all_same_suit(cards) {
            // Step 4: five same-suit physical cards are always a straight
            // flush, never a plain straight (GAME_RULES.md "Declared suit").
            continue;
        }
        let Some(filling) = fill(&template.slots, &others, &wildcards, level) else {
            continue;
        };
        // Several templates can give the same combo (a full house with
        // different pairs, a straight flush in different suits). Keep the
        // filling with the fewest substituting wildcards, then the smallest
        // `wildcard_as` (§3.4) — the derived `Ord` on `Filling` compares
        // exactly that, in that order.
        best.entry(template.combo)
            .and_modify(|kept| {
                if filling < *kept {
                    *kept = filling.clone();
                }
            })
            .or_insert(filling);
    }

    // Report the cards in hand order (Card's derived Ord), whatever order the
    // client sent them in, so every Play on the wire looks the same.
    let mut sorted = cards.to_vec();
    sorted.sort();
    best.into_iter()
        .map(|(combo, filling)| Play {
            cards: sorted.clone(),
            combo,
            wildcard_as: filling.wildcard_as,
        })
        .collect()
}

/// One position in a template. `suit` is `Some` only for straight-flush
/// slots; every other slot accepts any suit of its face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Slot {
    face: Face,
    suit: Option<Suit>,
}

impl Slot {
    fn any_suit(rank: Rank) -> Slot {
        Slot {
            face: Face::Rank(rank),
            suit: None,
        }
    }

    /// Does this physical card fit this slot exactly (step 1)?
    fn matches(self, card: Card) -> bool {
        match (card, self.suit) {
            (Card::Standard { suit, .. }, Some(slot_suit)) => {
                card.face() == self.face && suit == slot_suit
            }
            _ => card.face() == self.face && self.suit.is_none(),
        }
    }
}

/// A candidate combo and the cards it needs.
struct Template {
    combo: Combo,
    slots: Vec<Slot>,
}

/// Every candidate template for `len` cards (the §3.4 table).
fn templates(len: usize) -> Vec<Template> {
    let mut out = Vec::new();
    match len {
        1 => {
            for face in all_faces() {
                out.push(same_face(Combo::Single(face), face, 1));
            }
        }
        2 => {
            // Joker pairs come from the joker faces here: two Small or two
            // Big. A Small + Big "pair" fits no template.
            for face in all_faces() {
                out.push(same_face(Combo::Pair(face), face, 2));
            }
        }
        3 => {
            for rank in Rank::ALL {
                out.push(same_face(Combo::Triple(rank), Face::Rank(rank), 3));
            }
        }
        4 => {
            out.extend(of_a_kind(4));
            let small = Slot {
                face: Face::SmallJoker,
                suit: None,
            };
            let big = Slot {
                face: Face::BigJoker,
                suit: None,
            };
            out.push(Template {
                combo: Combo::Bomb(Bomb::Jokers),
                slots: vec![small, small, big, big],
            });
        }
        5 => {
            // Full house: the pair must be a different face from the triple
            // (five of one rank is only ever a bomb). Joker pairs allowed.
            for triple in Rank::ALL {
                for pair in all_faces() {
                    if pair == Face::Rank(triple) {
                        continue;
                    }
                    let mut slots = vec![Slot::any_suit(triple); 3];
                    slots.extend(
                        [Slot {
                            face: pair,
                            suit: None,
                        }; 2],
                    );
                    out.push(Template {
                        combo: Combo::FullHouse { triple },
                        slots,
                    });
                }
            }
            for (top, ranks) in runs(5) {
                out.push(Template {
                    combo: Combo::Straight { top },
                    slots: ranks.iter().map(|&rank| Slot::any_suit(rank)).collect(),
                });
                for suit in Suit::ALL {
                    out.push(Template {
                        combo: Combo::Bomb(Bomb::StraightFlush { top }),
                        slots: ranks
                            .iter()
                            .map(|&rank| Slot {
                                face: Face::Rank(rank),
                                suit: Some(suit),
                            })
                            .collect(),
                    });
                }
            }
            out.extend(of_a_kind(5));
        }
        6 => {
            for (top, ranks) in runs(3) {
                out.push(Template {
                    combo: Combo::Tube { top },
                    slots: ranks
                        .iter()
                        .flat_map(|&rank| [Slot::any_suit(rank); 2])
                        .collect(),
                });
            }
            for (top, ranks) in runs(2) {
                out.push(Template {
                    combo: Combo::Plate { top },
                    slots: ranks
                        .iter()
                        .flat_map(|&rank| [Slot::any_suit(rank); 3])
                        .collect(),
                });
            }
            out.extend(of_a_kind(6));
        }
        7..=10 => out.extend(of_a_kind(len as u8)),
        _ => {}
    }
    out
}

/// The 13 ranks, then the two jokers.
fn all_faces() -> impl Iterator<Item = Face> {
    Rank::ALL
        .into_iter()
        .map(Face::Rank)
        .chain([Face::SmallJoker, Face::BigJoker])
}

/// `count` slots of one face, any suit.
fn same_face(combo: Combo, face: Face, count: usize) -> Template {
    Template {
        combo,
        slots: vec![Slot { face, suit: None }; count],
    }
}

/// `OfAKind { size, r }` for every rank.
fn of_a_kind(size: u8) -> impl Iterator<Item = Template> {
    Rank::ALL.into_iter().map(move |rank| {
        same_face(
            Combo::Bomb(Bomb::OfAKind { size, rank }),
            Face::Rank(rank),
            usize::from(size),
        )
    })
}

/// Every run of `length` consecutive ranks, as `(top, ranks)`. The Ace can
/// sit at either end (A-2-3… or …K-A) but never both, so there's no wrap.
/// An Ace-low run's top is its highest *non-Ace* rank (A-2-3-4-5 → Five).
fn runs(length: usize) -> Vec<(Rank, Vec<Rank>)> {
    // Ace, then Two..=Ace: every run is a window of this line.
    let line: Vec<Rank> = std::iter::once(Rank::Ace).chain(Rank::ALL).collect();
    line.windows(length)
        .map(|window| (window[length - 1], window.to_vec()))
        .collect()
}

/// How the wildcards were used in one successful filling of a template.
///
/// Field order matters: the derived `Ord` compares `substitutions` first,
/// then `wildcard_as`, which is exactly the §3.4 dedupe preference.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Filling {
    /// How many wildcards substituted (didn't stand for themselves). Not
    /// always `wildcard_as.len()`: a substitution for a level-rank slot (the
    /// 6♠ of a spade straight flush at level 6) isn't listed there.
    substitutions: usize,
    /// Ranks of the substituted slots, level rank excluded, sorted (step 5).
    wildcard_as: Vec<Rank>,
}

/// Try to fill `slots` with `others` (non-wildcards) plus `wildcards`.
/// Returns how the wildcards were used, or `None` if the cards don't fit.
fn fill(slots: &[Slot], others: &[Card], wildcards: &[Card], level: Level) -> Option<Filling> {
    if slots.len() != others.len() + wildcards.len() {
        return None;
    }
    // Step 1: every non-wildcard takes a distinct slot it matches exactly.
    // Greedy is enough: within a template, all slots a card matches are
    // identical (same face, and same suit for straight flushes).
    let mut open: Vec<Slot> = slots.to_vec();
    for &card in others {
        let index = open.iter().position(|slot| slot.matches(card))?;
        open.swap_remove(index);
    }
    // What's left must be filled by exactly the wildcards. Step 3: with no
    // real card in the set, there's nothing for a wildcard to "complete", so
    // it may only stand for itself.
    let may_substitute = !others.is_empty();
    place_wildcards(&open, wildcards, level, may_substitute)
}

/// Fill the `open` slots (exactly one per wildcard) with the wildcards
/// (§3.4 steps 2, 3 and 5).
///
/// Both wildcards are the same physical card (the heart level card), so it
/// doesn't matter which wildcard goes into which slot: every assignment
/// gives the same result, and we can simply walk the open slots. For each
/// one, the wildcard stands for itself when it matches the slot as a plain
/// card (always preferred — it isn't a substitution); otherwise it
/// substitutes, which is allowed for any non-joker slot.
fn place_wildcards(
    open: &[Slot],
    wildcards: &[Card],
    level: Level,
    may_substitute: bool,
) -> Option<Filling> {
    debug_assert_eq!(open.len(), wildcards.len());
    let mut filling = Filling {
        substitutions: 0,
        wildcard_as: Vec::new(),
    };
    for (slot, &wildcard) in open.iter().zip(wildcards) {
        if slot.matches(wildcard) {
            continue; // standing for itself
        }
        // A wildcard can never stand in for a joker (GAME_RULES.md).
        let Face::Rank(rank) = slot.face else {
            return None;
        };
        if !may_substitute {
            return None;
        }
        filling.substitutions += 1;
        // Step 5: a substitute for the level rank (another suit's level
        // card) is still "a level card" on the table, so it isn't listed.
        if rank != level.0 {
            filling.wildcard_as.push(rank);
        }
    }
    filling.wildcard_as.sort();
    Some(filling)
}

/// True if every card is a standard card of one shared suit (step 4).
fn all_same_suit(cards: &[Card]) -> bool {
    let mut suits = cards.iter().map(|card| match card {
        Card::Standard { suit, .. } => Some(*suit),
        Card::Joker(_) => None,
    });
    let Some(Some(first)) = suits.next() else {
        return false;
    };
    suits.all(|suit| suit == Some(first))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_util::cards;

    fn level(rank: Rank) -> Level {
        Level(rank)
    }

    /// Just the combos, in output order.
    fn combos(s: &str, level_rank: Rank) -> Vec<Combo> {
        readings(&cards(s), level(level_rank))
            .into_iter()
            .map(|play| play.combo)
            .collect()
    }

    /// The single reading of `s` at level Two (asserts there is exactly one).
    fn only(s: &str) -> Combo {
        only_at(s, Rank::Two)
    }

    fn only_at(s: &str, level_rank: Rank) -> Combo {
        let found = combos(s, level_rank);
        assert_eq!(found.len(), 1, "{s} at {level_rank:?}: {found:?}");
        found[0]
    }

    fn illegal(s: &str) {
        let found = combos(s, Rank::Two);
        assert!(found.is_empty(), "{s} should be illegal: {found:?}");
    }

    fn bomb(size: u8, rank: Rank) -> Combo {
        Combo::Bomb(Bomb::OfAKind { size, rank })
    }

    fn straight_flush(top: Rank) -> Combo {
        Combo::Bomb(Bomb::StraightFlush { top })
    }

    // --- Every row of the template table ----------------------------------

    #[test]
    fn empty_set_has_no_reading() {
        assert!(readings(&[], level(Rank::Two)).is_empty());
    }

    #[test]
    fn singles() {
        assert_eq!(only("7S"), Combo::Single(Face::Rank(Rank::Seven)));
        assert_eq!(only("SJ"), Combo::Single(Face::SmallJoker));
        assert_eq!(only("BJ"), Combo::Single(Face::BigJoker));
    }

    #[test]
    fn pairs() {
        assert_eq!(only("7S 7H"), Combo::Pair(Face::Rank(Rank::Seven)));
        assert_eq!(only("7S 7S"), Combo::Pair(Face::Rank(Rank::Seven)));
        assert_eq!(only("SJ SJ"), Combo::Pair(Face::SmallJoker));
        assert_eq!(only("BJ BJ"), Combo::Pair(Face::BigJoker));
        illegal("7S 8S");
        illegal("SJ BJ");
    }

    #[test]
    fn triples() {
        assert_eq!(only("QS QH QD"), Combo::Triple(Rank::Queen));
        illegal("QS QH KD");
        illegal("SJ SJ BJ");
    }

    #[test]
    fn four_cards() {
        assert_eq!(only("9S 9H 9D 9C"), bomb(4, Rank::Nine));
        assert_eq!(only("SJ SJ BJ BJ"), Combo::Bomb(Bomb::Jokers));
        // Two pairs, triple + single, a 4-card run.
        illegal("5S 5H 6S 6H");
        illegal("5S 5H 5D 6S");
        illegal("5S 6H 7D 8S");
    }

    #[test]
    fn joker_bomb_needs_exactly_two_of_each() {
        illegal("SJ SJ SJ BJ");
        illegal("SJ BJ BJ BJ");
    }

    #[test]
    fn full_houses() {
        assert_eq!(
            only("8S 8H 8D 3S 3C"),
            Combo::FullHouse {
                triple: Rank::Eight
            }
        );
        // The pair can be a joker pair.
        assert_eq!(
            only("8S 8H 8D BJ BJ"),
            Combo::FullHouse {
                triple: Rank::Eight
            }
        );
        assert_eq!(
            only("8S SJ 8H SJ 8D"),
            Combo::FullHouse {
                triple: Rank::Eight
            }
        );
        // A mixed joker "pair" isn't a pair.
        illegal("8S 8H 8D SJ BJ");
        // Triple + two singles.
        illegal("8S 8H 8D 3S 4C");
    }

    #[test]
    fn five_of_a_kind_is_a_bomb_not_a_full_house() {
        assert_eq!(only("8S 8H 8D 8C 8S"), bomb(5, Rank::Eight));
    }

    #[test]
    fn straights() {
        assert_eq!(only("5S 6H 7D 8C 9S"), Combo::Straight { top: Rank::Nine });
        assert_eq!(only("9S 7H 5D 8C 6S"), Combo::Straight { top: Rank::Nine });
    }

    #[test]
    fn straight_flushes() {
        assert_eq!(only("5S 6S 7S 8S 9S"), straight_flush(Rank::Nine));
        assert_eq!(only("TH JH QH KH AH"), straight_flush(Rank::Ace));
    }

    #[test]
    fn all_same_suit_run_is_never_a_straight() {
        // Only the straight flush, at every level.
        for rank in Rank::ALL {
            assert_eq!(
                combos("5D 6D 7D 8D 9D", rank),
                vec![straight_flush(Rank::Nine)]
            );
        }
        // One off-suit card makes it a straight, not a straight flush.
        assert_eq!(only("5D 6D 7D 8D 9C"), Combo::Straight { top: Rank::Nine });
    }

    #[test]
    fn six_cards() {
        assert_eq!(only("4S 4H 5S 5D 6C 6C"), Combo::Tube { top: Rank::Six });
        assert_eq!(only("JS JH JD QS QD QC"), Combo::Plate { top: Rank::Queen });
        assert_eq!(only("3S 3H 3D 3C 3S 3H"), bomb(6, Rank::Three));
        // A 6-card run and two consecutive pairs + two more.
        illegal("4S 5H 6D 7C 8S 9S");
        illegal("4S 4H 5S 5D 7C 7C");
        // Plate needs consecutive triples.
        illegal("JS JH JD KS KD KC");
    }

    #[test]
    fn bombs_of_seven_and_eight() {
        assert_eq!(only("KS KS KH KH KD KD KC"), bomb(7, Rank::King));
        assert_eq!(only("KS KS KH KH KD KD KC KC"), bomb(8, Rank::King));
        illegal("KS KS KH KH KD KD KC QC");
    }

    #[test]
    fn nine_and_ten_cards_without_wildcards_are_illegal() {
        // 9- and 10-card bombs need substituting wildcards; with only 8
        // physical copies of a rank, nothing else fits these sizes.
        illegal("KS KS KH KH KD KD KC KC AS");
        illegal("KS KS KH KH KD KD KC KC AS AS");
    }

    #[test]
    fn eleven_or_more_cards_are_never_legal() {
        illegal("KS KS KH KH KD KD KC KC AS AS AS");
        illegal("2S 3S 4S 5S 6S 7S 8S 9S TS JS QS KS");
    }

    #[test]
    fn illegal_two_and_three_card_sets() {
        illegal("SJ BJ");
        illegal("5S 5H 6D");
    }

    // --- Runs: A-low / A-high, no wrap, level cards -----------------------

    #[test]
    fn ace_low_and_ace_high_runs() {
        assert_eq!(only("AS 2H 3D 4C 5S"), Combo::Straight { top: Rank::Five });
        assert_eq!(only("TS JH QD KC AS"), Combo::Straight { top: Rank::Ace });
        assert_eq!(only("AC 2C 3C 4C 5C"), straight_flush(Rank::Five));
        assert_eq!(only("TC JC QC KC AC"), straight_flush(Rank::Ace));

        assert_eq!(only("AS AH 2S 2D 3C 3C"), Combo::Tube { top: Rank::Three });
        assert_eq!(only("QS QH KS KD AC AC"), Combo::Tube { top: Rank::Ace });

        assert_eq!(only("AS AH AD 2S 2D 2C"), Combo::Plate { top: Rank::Two });
        assert_eq!(only("KS KH KD AS AD AC"), Combo::Plate { top: Rank::Ace });
    }

    #[test]
    fn runs_do_not_wrap() {
        illegal("QS KH AD 2C 3S");
        illegal("KS AH 2D 3C 4S");
        illegal("KS KH AS AD 2C 2C");
        illegal("KS KH KD 2S 2D 2C");
    }

    #[test]
    fn jokers_are_never_in_runs() {
        illegal("SJ 2H 3D 4C 5S");
        illegal("TS JH QD KC SJ");
    }

    #[test]
    fn level_cards_sit_at_their_natural_position_in_runs() {
        // Level 6: 6s link 5 and 7 like any other rank; the top is natural.
        assert_eq!(
            only_at("4S 4H 5S 5D 6C 6C", Rank::Six),
            Combo::Tube { top: Rank::Six }
        );
        assert_eq!(
            only_at("5S 6C 7D 8C 9S", Rank::Six),
            Combo::Straight { top: Rank::Nine }
        );
        assert_eq!(
            only_at("6S 6D 6C 7S 7D 7C", Rank::Six),
            Combo::Plate { top: Rank::Seven }
        );
        assert_eq!(
            only_at("2S 3S 4S 5S 6S", Rank::Six),
            straight_flush(Rank::Six)
        );
        // Level A: the Ace still works low and high.
        assert_eq!(
            only_at("AS 2H 3D 4C 5S", Rank::Ace),
            Combo::Straight { top: Rank::Five }
        );
        assert_eq!(
            only_at("TS JH QD KC AS", Rank::Ace),
            Combo::Straight { top: Rank::Ace }
        );
    }

    // --- Wildcards (§3.4 steps 2, 3, 5) -------------------------------------

    /// `(combo, wildcard_as)` for every reading, in output order.
    fn wild(s: &str, level_rank: Rank) -> Vec<(Combo, Vec<Rank>)> {
        readings(&cards(s), level(level_rank))
            .into_iter()
            .map(|play| (play.combo, play.wildcard_as))
            .collect()
    }

    fn full_house(triple: Rank) -> Combo {
        Combo::FullHouse { triple }
    }

    fn pair(rank: Rank) -> Combo {
        Combo::Pair(Face::Rank(rank))
    }

    #[test]
    fn triple_plus_two_wildcards_is_a_full_house_or_a_bomb() {
        // §3.4 / GAME_RULES.md "Full house": level 6, 8,8,8,6♥,6♥ is exactly
        // the full house (wildcards as themselves: a pair of level cards)
        // and the quintuple bomb (both wildcards as 8s). The full house
        // whose pair is two substituted 10s (say) collapses into the first,
        // since the dedupe keeps the filling with fewer substitutions.
        let found = wild("8S 8H 8D 6H 6H", Rank::Six);
        assert_eq!(
            found,
            vec![
                (full_house(Rank::Eight), vec![]),
                (bomb(5, Rank::Eight), vec![Rank::Eight, Rank::Eight]),
            ]
        );
        // Output order is Combo's derived Ord.
        assert!(found[0].0 < found[1].0);
    }

    #[test]
    fn choose_reading_fixture_example() {
        // fixtures/choose_reading.json: level Five, 9,9,9,5♥,5♥.
        let plays = readings(&cards("9S 9D 9D 5H 5H"), level(Rank::Five));
        assert_eq!(
            plays,
            vec![
                Play {
                    cards: cards("5H 5H 9S 9D 9D"),
                    combo: full_house(Rank::Nine),
                    wildcard_as: vec![],
                },
                Play {
                    cards: cards("5H 5H 9S 9D 9D"),
                    combo: bomb(5, Rank::Nine),
                    wildcard_as: vec![Rank::Nine, Rank::Nine],
                },
            ]
        );
    }

    #[test]
    fn wildcard_completes_either_pair_of_a_full_house() {
        // Level 6: 3,3,6♥,5,5 — the wildcard makes the 3s or the 5s a triple.
        assert_eq!(
            wild("3S 3D 6H 5C 5S", Rank::Six),
            vec![
                (full_house(Rank::Three), vec![Rank::Three]),
                (full_house(Rank::Five), vec![Rank::Five]),
            ]
        );
    }

    #[test]
    fn wildcard_at_either_end_of_a_straight() {
        // GAME_RULES.md: 5,6,7,8 + wildcard (level J) → W as 4 or as 9.
        assert_eq!(
            wild("5S 6H 7D 8C JH", Rank::Jack),
            vec![
                (Combo::Straight { top: Rank::Eight }, vec![Rank::Four]),
                (Combo::Straight { top: Rank::Nine }, vec![Rank::Nine]),
            ]
        );
    }

    #[test]
    fn wildcard_suit_chooses_straight_or_straight_flush() {
        // Level J: 4♠5♠7♠8♠ + W — W as a non-♠ 6 (straight) or as 6♠.
        assert_eq!(
            wild("4S 5S 7S 8S JH", Rank::Jack),
            vec![
                (Combo::Straight { top: Rank::Eight }, vec![Rank::Six]),
                (straight_flush(Rank::Eight), vec![Rank::Six]),
            ]
        );
        // Level 6: the gap is the level rank itself. As the 6♥ it stands for
        // itself (straight); as the 6♠ it substitutes for the level card of
        // another suit (straight flush) — and a level-rank slot is never
        // listed in `wildcard_as`.
        assert_eq!(
            wild("4S 5S 6H 7S 8S", Rank::Six),
            vec![
                (Combo::Straight { top: Rank::Eight }, vec![]),
                (straight_flush(Rank::Eight), vec![]),
            ]
        );
    }

    #[test]
    fn wildcard_extends_a_heart_run_only_into_a_straight_flush() {
        // All five physical cards are hearts (the wildcard counts as its
        // printed suit), so no reading is a plain straight (step 4).
        assert_eq!(
            wild("5H 6H 7H 8H JH", Rank::Jack),
            vec![
                (straight_flush(Rank::Eight), vec![Rank::Four]),
                (straight_flush(Rank::Nine), vec![Rank::Nine]),
            ]
        );
    }

    #[test]
    fn wildcard_completes_a_four_bomb() {
        // GAME_RULES.md: 4,4,4 + wildcard → a quadruple bomb of 4s.
        assert_eq!(
            wild("4S 4D 4C 6H", Rank::Six),
            vec![(bomb(4, Rank::Four), vec![Rank::Four])]
        );
    }

    #[test]
    fn pagat_tube_and_plate_example() {
        // GAME_RULES.md "Resolving wildcard plays": level 4, 4♥,4♥,8,8,9,9.
        assert_eq!(
            wild("4H 4H 8S 8D 9C 9S", Rank::Four),
            vec![
                (
                    Combo::Tube { top: Rank::Nine },
                    vec![Rank::Seven, Rank::Seven]
                ),
                (Combo::Tube { top: Rank::Ten }, vec![Rank::Ten, Rank::Ten]),
                (
                    Combo::Plate { top: Rank::Nine },
                    vec![Rank::Eight, Rank::Nine]
                ),
            ]
        );
    }

    #[test]
    fn two_wildcards_fill_two_different_ranks_in_a_tube() {
        assert_eq!(
            wild("2H 2H 5S 5D 6S 7C", Rank::Two),
            vec![(
                Combo::Tube { top: Rank::Seven },
                vec![Rank::Six, Rank::Seven]
            )]
        );
    }

    #[test]
    fn one_wildcard_substitutes_while_the_other_is_itself() {
        // Level 6, tube 5-6-7: one wildcard is a 6 as itself, the other
        // fills the missing 7.
        assert_eq!(
            wild("5S 5D 6H 6H 6C 7C", Rank::Six),
            vec![(Combo::Tube { top: Rank::Seven }, vec![Rank::Seven])]
        );
    }

    #[test]
    fn wildcard_makes_a_pair_with_any_rank() {
        assert_eq!(
            wild("7S 6H", Rank::Six),
            vec![(pair(Rank::Seven), vec![Rank::Seven])]
        );
        // With another level card it's simply a pair of level cards.
        assert_eq!(wild("6S 6H", Rank::Six), vec![(pair(Rank::Six), vec![])]);
    }

    #[test]
    fn lone_wildcard_and_wildcard_pair_cannot_substitute() {
        // Step 3: only readings as themselves — a level-rank single / pair.
        assert_eq!(
            wild("6H", Rank::Six),
            vec![(Combo::Single(Face::Rank(Rank::Six)), vec![])]
        );
        assert_eq!(wild("6H 6H", Rank::Six), vec![(pair(Rank::Six), vec![])]);
    }

    #[test]
    fn wildcards_never_fill_joker_slots() {
        // Not a joker pair, a joker bomb, or a joker pair in a full house.
        assert!(combos("SJ 6H", Rank::Six).is_empty());
        assert!(combos("BJ 6H", Rank::Six).is_empty());
        assert!(combos("SJ SJ BJ 6H", Rank::Six).is_empty());
        assert!(combos("SJ BJ BJ 6H", Rank::Six).is_empty());
        assert!(combos("SJ SJ 6H 6H", Rank::Six).is_empty());
        assert!(combos("8S 8D 8C BJ 6H", Rank::Six).is_empty());
        // A real joker pair next to a wildcard-completed triple is fine.
        assert_eq!(
            wild("8S 8D 6H SJ SJ", Rank::Six),
            vec![(full_house(Rank::Eight), vec![Rank::Eight])]
        );
    }

    #[test]
    fn nine_and_ten_card_bombs_need_wildcards() {
        assert_eq!(
            wild("7S 7S 7H 7H 7D 7D 7C 7C 6H", Rank::Six),
            vec![(bomb(9, Rank::Seven), vec![Rank::Seven])]
        );
        assert_eq!(
            wild("7S 7S 7H 7H 7D 7D 7C 7C 6H 6H", Rank::Six),
            vec![(bomb(10, Rank::Seven), vec![Rank::Seven, Rank::Seven])]
        );
    }

    #[test]
    fn level_rank_bomb_maxes_at_eight() {
        // All eight physical level cards (both wildcards as themselves).
        assert_eq!(
            wild("6S 6S 6H 6H 6D 6D 6C 6C", Rank::Six),
            vec![(bomb(8, Rank::Six), vec![])]
        );
        // There's no ninth level card, and the wildcards are already used.
        assert!(combos("6S 6S 6H 6H 6D 6D 6C 6C 7S", Rank::Six).is_empty());
    }

    #[test]
    fn wildcard_as_itself_in_runs() {
        // Mixed suits: a straight through the natural-position 6.
        assert_eq!(
            wild("5S 6H 7D 8C 9S", Rank::Six),
            vec![(Combo::Straight { top: Rank::Nine }, vec![])]
        );
        // All hearts: as itself it's the 6♥ slot of a heart straight flush.
        assert_eq!(
            only_at("5H 6H 7H 8H 9H", Rank::Six),
            straight_flush(Rank::Nine)
        );
        // Tube through the 6s.
        assert_eq!(
            wild("5S 5D 6H 6H 7C 7C", Rank::Six),
            vec![(Combo::Tube { top: Rank::Seven }, vec![])]
        );
        // One wildcard can't fill two gaps.
        assert!(combos("5S 6H 7D 8C TS", Rank::Six).is_empty());
    }

    #[test]
    fn a_heart_is_only_a_wildcard_at_its_own_level() {
        // At level Two, 6♥ is an ordinary card; at level Six, it's the wildcard.
        assert_eq!(only("6H 6S"), Combo::Pair(Face::Rank(Rank::Six)));
        assert_eq!(
            only_at("6H 6S", Rank::Six),
            Combo::Pair(Face::Rank(Rank::Six))
        );
    }

    // --- Output shape -----------------------------------------------------

    #[test]
    fn play_keeps_the_physical_cards_in_hand_order() {
        let set = cards("9S 7H 5D 8C 6S");
        let plays = readings(&set, level(Rank::Two));
        assert_eq!(plays.len(), 1);
        assert_eq!(plays[0].cards, cards("5D 6S 7H 8C 9S"));
        assert!(plays[0].wildcard_as.is_empty());
    }

    #[test]
    fn output_is_deduped_and_sorted_by_combo() {
        // Many full-house templates (triple of 8s + every other pair) could
        // take 8,8,8 plus a wildcard pair; only one full house comes out.
        let plays = readings(&cards("8S 8H 8D 6H 6H"), level(Rank::Six));
        let full_houses = plays
            .iter()
            .filter(|play| matches!(play.combo, Combo::FullHouse { .. }))
            .count();
        assert_eq!(full_houses, 1);

        // The result doesn't depend on input order.
        for s in ["5S 6S 7S 8S 9S", "9S 8S 7S 6S 5S", "7S 5S 9S 6S 8S"] {
            assert_eq!(combos(s, Rank::Two), vec![straight_flush(Rank::Nine)]);
        }
    }

    #[test]
    fn run_windows() {
        let straights = runs(5);
        assert_eq!(straights.len(), 10);
        assert_eq!(straights[0].0, Rank::Five);
        assert_eq!(
            straights[0].1,
            vec![Rank::Ace, Rank::Two, Rank::Three, Rank::Four, Rank::Five]
        );
        assert_eq!(straights[9].0, Rank::Ace);
        assert_eq!(runs(3).len(), 12);
        assert_eq!(runs(3)[0].0, Rank::Three);
        assert_eq!(runs(2).len(), 13);
        assert_eq!(runs(2)[0].0, Rank::Two);
    }

    #[test]
    fn five_card_template_count_matches_the_spec() {
        // §3.4: 182 full houses + 10 straights + 40 straight flushes + 13 bombs.
        assert_eq!(templates(5).len(), 182 + 10 + 40 + 13);
        assert_eq!(templates(6).len(), 12 + 13 + 13);
        assert_eq!(templates(4).len(), 13 + 1);
        assert_eq!(templates(1).len(), 15);
        assert!(templates(11).is_empty());
        assert!(templates(0).is_empty());
    }
}
