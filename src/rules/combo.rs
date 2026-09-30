//! Combos and plays (TECH_SPEC.md §3.3).

use serde::{Deserialize, Serialize};

use super::card::{Card, Face, Rank};
use super::ranking::{Level, face_value, natural_value};

/// A declared reading: the combo type plus the rank it's compared by. It
/// stores no cards — once chosen, a play is compared purely by this
/// (GAME_RULES.md "Resolving wildcard plays", step 4).
///
/// Derived `Ord` is for deterministic sorting only; use `beats` for strength.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Combo {
    Single(Face),
    /// Joker pairs: both Small or both Big.
    Pair(Face),
    Triple(Rank),
    /// The pair isn't stored: it never affects strength (the pair-rank ≠
    /// triple-rank check lives in `readings`).
    FullHouse {
        triple: Rank,
    },
    /// `top` in Five..=Ace (Five = A-2-3-4-5).
    Straight {
        top: Rank,
    },
    /// 3 consecutive pairs; `top` in Three..=Ace.
    Tube {
        top: Rank,
    },
    /// 2 consecutive triples; `top` in Two..=Ace.
    Plate {
        top: Rank,
    },
    Bomb(Bomb),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Bomb {
    /// `size` 4..=10 (9 and 10 need wildcards).
    OfAKind { size: u8, rank: Rank },
    /// `top` in Five..=Ace; suit is irrelevant to strength.
    StraightFlush { top: Rank },
    /// 2 Small + 2 Big.
    Jokers,
}

/// A play as it sits on the table: the physical cards plus the reading chosen.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Play {
    pub(crate) cards: Vec<Card>,
    pub(crate) combo: Combo,
    /// Ranks the wildcards stand in for, sorted ascending — one entry per
    /// wildcard used as a substitute. A wildcard playing as itself (a
    /// level-rank card) is not listed. For display only; never used to compare.
    pub(crate) wildcard_as: Vec<Rank>,
}

/// Bomb strength tier: OfAKind 4 → 0, OfAKind 5 → 1, StraightFlush → 2,
/// OfAKind 6 → 3, 7 → 4, 8 → 5, 9 → 6, 10 → 7, Jokers → 8.
///
/// Every tier holds exactly one kind of bomb (one size of `OfAKind`, or all
/// straight flushes, or the joker bomb), so two bombs of the same tier are
/// always the same variant and can be compared by rank alone.
pub fn bomb_tier(bomb: &Bomb) -> u8 {
    match *bomb {
        Bomb::OfAKind { size: 4, .. } => 0,
        Bomb::OfAKind { size: 5, .. } => 1,
        // The straight flush slots in between 5- and 6-card bombs
        // (GAME_RULES.md "Bomb Hierarchy").
        Bomb::StraightFlush { .. } => 2,
        Bomb::OfAKind {
            size: size @ 6..=10,
            ..
        } => size - 3,
        Bomb::OfAKind { size, .. } => {
            unreachable!("OfAKind bomb size must be 4..=10, got {size}")
        }
        Bomb::Jokers => 8,
    }
}

/// Strictly-greater comparison; ties never beat (TECH_SPEC.md §3.3).
///
/// Non-sequential combos (singles, pairs, triples, full houses, same-rank
/// bombs) compare by `face_value`, where the level rank sits above Ace.
/// Sequential ones (straights, tubes, plates, straight flushes) compare by
/// `natural_value` of their top rank, where the level rank gets no boost.
pub fn beats(candidate: &Combo, current: &Combo, level: Level) -> bool {
    match (candidate, current) {
        // The joker bomb is the strongest play; nothing beats it. (Covered
        // by the tier comparison below too, but spelled out for clarity.)
        (_, Combo::Bomb(Bomb::Jokers)) => false,
        (Combo::Bomb(candidate), Combo::Bomb(current)) => bomb_beats(candidate, current, level),
        // Any bomb beats any non-bomb; a non-bomb never beats a bomb.
        (Combo::Bomb(_), _) => true,
        (_, Combo::Bomb(_)) => false,

        (Combo::Single(a), Combo::Single(b)) | (Combo::Pair(a), Combo::Pair(b)) => {
            face_value(*a, level) > face_value(*b, level)
        }
        (Combo::Triple(a), Combo::Triple(b))
        | (Combo::FullHouse { triple: a }, Combo::FullHouse { triple: b }) => {
            rank_face_value(*a, level) > rank_face_value(*b, level)
        }
        (Combo::Straight { top: a }, Combo::Straight { top: b })
        | (Combo::Tube { top: a }, Combo::Tube { top: b })
        | (Combo::Plate { top: a }, Combo::Plate { top: b }) => {
            natural_value(*a) > natural_value(*b)
        }

        // Different non-bomb types never beat each other (e.g. a tube can't
        // beat a plate, even though both are 6 cards).
        _ => false,
    }
}

/// Bomb vs bomb: a higher tier wins; within a tier, compare by rank.
fn bomb_beats(candidate: &Bomb, current: &Bomb, level: Level) -> bool {
    let (candidate_tier, current_tier) = (bomb_tier(candidate), bomb_tier(current));
    if candidate_tier != current_tier {
        return candidate_tier > current_tier;
    }
    match (candidate, current) {
        // Same tier means same size, so only the rank matters.
        (Bomb::OfAKind { rank: a, .. }, Bomb::OfAKind { rank: b, .. }) => {
            rank_face_value(*a, level) > rank_face_value(*b, level)
        }
        // Suit never matters, so equal tops are a tie.
        (Bomb::StraightFlush { top: a }, Bomb::StraightFlush { top: b }) => {
            natural_value(*a) > natural_value(*b)
        }
        // Jokers vs Jokers (a tie), or mismatched variants, which can't
        // share a tier.
        _ => false,
    }
}

/// `face_value` of a plain rank.
fn rank_face_value(rank: Rank, level: Level) -> u8 {
    face_value(Face::Rank(rank), level)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_util::cards;

    const LEVEL_TWO: Level = Level(Rank::Two);
    const LEVEL_SIX: Level = Level(Rank::Six);

    fn of_a_kind(size: u8, rank: Rank) -> Combo {
        Combo::Bomb(Bomb::OfAKind { size, rank })
    }

    fn straight_flush(top: Rank) -> Combo {
        Combo::Bomb(Bomb::StraightFlush { top })
    }

    /// Every legal straight / straight-flush top, low → high (Five = A-2-3-4-5).
    const RUN_TOPS: [Rank; 10] = [
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

    /// Asserts `higher` beats `lower` and not the other way round.
    fn assert_strictly_above(higher: &Combo, lower: &Combo, level: Level) {
        assert!(
            beats(higher, lower, level),
            "{higher:?} should beat {lower:?}"
        );
        assert!(
            !beats(lower, higher, level),
            "{lower:?} should not beat {higher:?}"
        );
    }

    /// Asserts neither combo beats the other.
    fn assert_tie(a: &Combo, b: &Combo, level: Level) {
        assert!(!beats(a, b, level), "{a:?} should not beat {b:?}");
        assert!(!beats(b, a, level), "{b:?} should not beat {a:?}");
    }

    /// Asserts every combo in `order` (low → high) strictly beats every earlier one.
    fn assert_ascending(order: &[Combo], level: Level) {
        for (i, higher) in order.iter().enumerate() {
            for lower in &order[..i] {
                assert_strictly_above(higher, lower, level);
            }
        }
    }

    // ---- bomb_tier / bomb hierarchy ----

    #[test]
    fn bomb_tiers_match_the_spec_table() {
        let of = |size| Bomb::OfAKind {
            size,
            rank: Rank::Nine,
        };
        assert_eq!(bomb_tier(&of(4)), 0);
        assert_eq!(bomb_tier(&of(5)), 1);
        assert_eq!(bomb_tier(&Bomb::StraightFlush { top: Rank::Ace }), 2);
        assert_eq!(bomb_tier(&of(6)), 3);
        assert_eq!(bomb_tier(&of(7)), 4);
        assert_eq!(bomb_tier(&of(8)), 5);
        assert_eq!(bomb_tier(&of(9)), 6);
        assert_eq!(bomb_tier(&of(10)), 7);
        assert_eq!(bomb_tier(&Bomb::Jokers), 8);
    }

    #[test]
    fn full_bomb_order_with_straight_flush_between_five_and_six() {
        // GAME_RULES.md "Bomb Hierarchy", lowest to highest. Ranks are chosen
        // against the grain (Aces on the smallest bomb, the weakest straight
        // flush over a 5-bomb, …) so only the tier can decide.
        let order = [
            of_a_kind(4, Rank::Ace),
            of_a_kind(5, Rank::Two),
            straight_flush(Rank::Five),
            of_a_kind(6, Rank::Three),
            of_a_kind(7, Rank::Three),
            of_a_kind(8, Rank::Three),
            of_a_kind(9, Rank::Three),
            of_a_kind(10, Rank::Three),
            Combo::Bomb(Bomb::Jokers),
        ];
        assert_ascending(&order, LEVEL_TWO);

        // The straight flush sits strictly between the 5- and 6-card bombs
        // whatever their ranks, including level-rank bombs.
        assert_strictly_above(
            &straight_flush(Rank::Five),
            &of_a_kind(5, Rank::Six),
            LEVEL_SIX,
        );
        assert_strictly_above(
            &of_a_kind(6, Rank::Two),
            &straight_flush(Rank::Ace),
            LEVEL_SIX,
        );
    }

    #[test]
    fn more_cards_always_beat_fewer() {
        // "Among numbered bombs, more cards always beats fewer cards": even
        // the weakest rank beats the level rank one size down.
        for size in 4..10 {
            assert_strictly_above(
                &of_a_kind(size + 1, Rank::Two),
                &of_a_kind(size, Rank::Six),
                LEVEL_SIX,
            );
        }
    }

    #[test]
    fn same_size_bombs_compare_by_face_value() {
        // "Level-rank bombs rank higher than non-level ranks of the same size."
        assert_strictly_above(
            &of_a_kind(4, Rank::Six),
            &of_a_kind(4, Rank::Ace),
            LEVEL_SIX,
        );
        assert_strictly_above(
            &of_a_kind(8, Rank::Six),
            &of_a_kind(8, Rank::Ace),
            LEVEL_SIX,
        );
        assert_strictly_above(
            &of_a_kind(5, Rank::Seven),
            &of_a_kind(5, Rank::Five),
            LEVEL_SIX,
        );
        // At level 2, a bomb of 2s beats a bomb of Aces of the same size.
        assert_strictly_above(
            &of_a_kind(4, Rank::Two),
            &of_a_kind(4, Rank::Ace),
            LEVEL_TWO,
        );
    }

    #[test]
    fn straight_flushes_compare_by_natural_top() {
        // A-2-3-4-5 (top Five) is the lowest straight flush; T-J-Q-K-A the highest.
        let order: Vec<Combo> = RUN_TOPS.into_iter().map(straight_flush).collect();
        assert_ascending(&order, LEVEL_SIX);

        // The level rank gets no boost as a straight flush top.
        assert_strictly_above(
            &straight_flush(Rank::Seven),
            &straight_flush(Rank::Six),
            LEVEL_SIX,
        );
    }

    #[test]
    fn bomb_ties_never_beat() {
        // Two quadruple 9 bombs.
        assert_tie(
            &of_a_kind(4, Rank::Nine),
            &of_a_kind(4, Rank::Nine),
            LEVEL_TWO,
        );
        assert_tie(
            &of_a_kind(10, Rank::Seven),
            &of_a_kind(10, Rank::Seven),
            LEVEL_SIX,
        );
        assert_tie(
            &of_a_kind(8, Rank::Six),
            &of_a_kind(8, Rank::Six),
            LEVEL_SIX,
        );
        // Two straight flushes with the same top (in any suits — suit isn't stored).
        assert_tie(
            &straight_flush(Rank::Nine),
            &straight_flush(Rank::Nine),
            LEVEL_TWO,
        );
        assert_tie(
            &straight_flush(Rank::Five),
            &straight_flush(Rank::Five),
            LEVEL_TWO,
        );
        // Two joker bombs can't exist in one deal, but the rule still holds.
        assert_tie(
            &Combo::Bomb(Bomb::Jokers),
            &Combo::Bomb(Bomb::Jokers),
            LEVEL_TWO,
        );
    }

    #[test]
    fn joker_bomb_is_unbeatable() {
        let jokers = Combo::Bomb(Bomb::Jokers);
        let challengers = [
            of_a_kind(10, Rank::Ace),
            of_a_kind(8, Rank::Six),
            straight_flush(Rank::Ace),
            Combo::Single(Face::BigJoker),
            Combo::Pair(Face::BigJoker),
            Combo::Plate { top: Rank::Ace },
        ];
        for challenger in &challengers {
            assert_strictly_above(&jokers, challenger, LEVEL_SIX);
        }
    }

    #[test]
    fn any_bomb_beats_any_non_bomb() {
        let non_bombs = [
            Combo::Single(Face::BigJoker),
            Combo::Pair(Face::BigJoker),
            Combo::Triple(Rank::Six),
            Combo::FullHouse { triple: Rank::Six },
            Combo::Straight { top: Rank::Ace },
            Combo::Tube { top: Rank::Ace },
            Combo::Plate { top: Rank::Ace },
        ];
        for non_bomb in &non_bombs {
            assert_strictly_above(&of_a_kind(4, Rank::Two), non_bomb, LEVEL_SIX);
            assert_strictly_above(&straight_flush(Rank::Five), non_bomb, LEVEL_SIX);
        }
    }

    // ---- non-bombs ----

    #[test]
    fn different_non_bomb_types_never_beat_each_other() {
        // A pair must be beaten by a higher pair, a full house by another
        // full house, etc. — however strong the other type looks.
        let combos = [
            Combo::Single(Face::BigJoker),
            Combo::Pair(Face::Rank(Rank::Three)),
            Combo::Triple(Rank::Ace),
            Combo::FullHouse { triple: Rank::Two },
            Combo::Straight { top: Rank::Ace },
            Combo::Tube { top: Rank::Four },
            Combo::Plate { top: Rank::Ace },
        ];
        for (i, a) in combos.iter().enumerate() {
            for b in &combos[i + 1..] {
                assert_tie(a, b, LEVEL_SIX);
            }
        }
    }

    #[test]
    fn tube_and_plate_never_beat_each_other() {
        // Both are 6 cards, but they're different combo types.
        for tube_top in [Rank::Three, Rank::Ace] {
            for plate_top in [Rank::Two, Rank::Ace] {
                assert_tie(
                    &Combo::Tube { top: tube_top },
                    &Combo::Plate { top: plate_top },
                    LEVEL_TWO,
                );
            }
        }
    }

    #[test]
    fn singles_follow_the_level_order() {
        // GAME_RULES.md worked example at level 6:
        // 2 < 3 < 4 < 5 < 7 < 8 < 9 < 10 < J < Q < K < A < 6 < SJ < BJ.
        let order: Vec<Combo> = cards("2S 3S 4S 5S 7S 8S 9S TS JS QS KS AS 6S SJ BJ")
            .into_iter()
            .map(|card| Combo::Single(card.face()))
            .collect();
        assert_ascending(&order, LEVEL_SIX);
    }

    #[test]
    fn level_card_single_beats_ace_but_not_jokers() {
        let six = Combo::Single(Face::Rank(Rank::Six));
        assert_strictly_above(&six, &Combo::Single(Face::Rank(Rank::Ace)), LEVEL_SIX);
        assert_strictly_above(&Combo::Single(Face::SmallJoker), &six, LEVEL_SIX);
        // At another level, 6 is back below 7.
        assert_strictly_above(&Combo::Single(Face::Rank(Rank::Seven)), &six, LEVEL_TWO);
    }

    #[test]
    fn level_cards_of_any_suit_tie() {
        // "6♥6♠ ranks the same as 6♠6♠ or 6♥6♥" — suit never adds strength.
        let [six_h, six_s, six_d] = cards("6H 6S 6D")[..] else {
            unreachable!()
        };
        assert_tie(
            &Combo::Pair(six_h.face()),
            &Combo::Pair(six_s.face()),
            LEVEL_SIX,
        );
        assert_tie(
            &Combo::Single(six_h.face()),
            &Combo::Single(six_d.face()),
            LEVEL_SIX,
        );
    }

    #[test]
    fn pairs_compare_by_face_value_including_joker_pairs() {
        let order = [
            Combo::Pair(Face::Rank(Rank::Two)),
            Combo::Pair(Face::Rank(Rank::Ace)),
            Combo::Pair(Face::Rank(Rank::Six)),
            Combo::Pair(Face::SmallJoker),
            Combo::Pair(Face::BigJoker),
        ];
        assert_ascending(&order, LEVEL_SIX);
    }

    #[test]
    fn triples_and_full_houses_compare_by_face_value_of_the_triple() {
        assert_strictly_above(
            &Combo::Triple(Rank::Six),
            &Combo::Triple(Rank::Ace),
            LEVEL_SIX,
        );
        assert_strictly_above(
            &Combo::Triple(Rank::Seven),
            &Combo::Triple(Rank::Five),
            LEVEL_SIX,
        );
        assert_strictly_above(
            &Combo::FullHouse { triple: Rank::Six },
            &Combo::FullHouse { triple: Rank::Ace },
            LEVEL_SIX,
        );
        assert_strictly_above(
            &Combo::FullHouse {
                triple: Rank::Three,
            },
            &Combo::FullHouse { triple: Rank::Two },
            LEVEL_SIX,
        );
        // At level 2, a full house of 2s is the strongest full house.
        assert_strictly_above(
            &Combo::FullHouse { triple: Rank::Two },
            &Combo::FullHouse { triple: Rank::Ace },
            LEVEL_TWO,
        );
    }

    #[test]
    fn straights_compare_by_natural_top() {
        // At level 6 a straight topped by the level rank gets no boost:
        // 2-3-4-5-6 does not beat 3-4-5-6-7.
        assert!(!beats(
            &Combo::Straight { top: Rank::Six },
            &Combo::Straight { top: Rank::Seven },
            LEVEL_SIX,
        ));
        assert_strictly_above(
            &Combo::Straight { top: Rank::Seven },
            &Combo::Straight { top: Rank::Six },
            LEVEL_SIX,
        );
        // A-2-3-4-5 (top Five) is the lowest straight; 10-J-Q-K-A the highest,
        // at any level (including level A).
        let order: Vec<Combo> = RUN_TOPS
            .into_iter()
            .map(|top| Combo::Straight { top })
            .collect();
        assert_ascending(&order, LEVEL_SIX);
        assert_ascending(&order, Level(Rank::Ace));
    }

    #[test]
    fn tubes_compare_by_natural_top() {
        // A,A,2,2,3,3 (top Three) is the lowest; Q,Q,K,K,A,A the highest.
        let order: Vec<Combo> = Rank::ALL[1..]
            .iter()
            .map(|&top| Combo::Tube { top })
            .collect();
        assert_ascending(&order, LEVEL_SIX);
        // Level cards inside a tube get no boost: 4,4,5,5,6,6 < 5,5,6,6,7,7.
        assert_strictly_above(
            &Combo::Tube { top: Rank::Seven },
            &Combo::Tube { top: Rank::Six },
            LEVEL_SIX,
        );
    }

    #[test]
    fn plates_compare_by_natural_top() {
        // A,A,A,2,2,2 (top Two) is the lowest; K,K,K,A,A,A the highest.
        let order: Vec<Combo> = Rank::ALL.iter().map(|&top| Combo::Plate { top }).collect();
        assert_ascending(&order, LEVEL_SIX);
        // At level 2, the plate topped by 2 is still the lowest.
        assert_ascending(&order, LEVEL_TWO);
    }

    #[test]
    fn non_bomb_ties_never_beat() {
        let combos = [
            Combo::Single(Face::Rank(Rank::Nine)),
            Combo::Single(Face::BigJoker),
            Combo::Pair(Face::SmallJoker),
            Combo::Triple(Rank::Six),
            Combo::FullHouse {
                triple: Rank::Eight,
            },
            Combo::Straight { top: Rank::Five },
            Combo::Tube { top: Rank::Ace },
            Combo::Plate { top: Rank::Two },
        ];
        for combo in &combos {
            assert_tie(combo, combo, LEVEL_SIX);
        }
    }
}
