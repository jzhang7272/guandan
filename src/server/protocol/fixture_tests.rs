//! Wire-format fixtures (TECH_SPEC.md §4 and §8 "Wire-format fixtures";
//! TECH_TASKS.md Task 0b).
//!
//! Every message below is built as a Rust value with struct literals,
//! serialized with `serde_json::to_string_pretty`, and compared (as JSON
//! values) with `fixtures/<name>.json`. Run with `UPDATE_FIXTURES=1` to
//! (re)write the files instead. The client builds its screens against these
//! files (UI_SPEC.md §7 fixture mode), so they are the wire contract: a change
//! to a `rules/` type that alters the JSON fails here first.
//!
//! Most in-game fixtures follow the §4 story, built from four explicit dealt
//! hands (`DEALT`) whose union is checked to be the full deck: team B (seats
//! 1 & 3) won the first deal 1-2, so deal 2 is at level Five with a double
//! tribute; seat 1 goes out 1st (接风 to seat 3), then seat 3 goes out 2nd by
//! leading a Big Joker, so team B wins 1-2 again (Five → Eight). The other
//! fixtures are separate moments whose visible cards are checked generically
//! (`visible_cards_are_consistent`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde_json::Value;

use super::{
    ClientMessage, LobbyView, RedactedState, RejectCode, RoomView, SeatInfo, ServerMessage,
    SessionError,
};
use crate::rules::test_util::{cards, hand};
use crate::rules::{
    ActionError, Bomb, Card, CardCount, Combo, CompletedTrick, DealEnd, DealResult, DealStart,
    DealSummary, DutyView, Exchange, Face, Level, MatchView, PhaseView, Play, Progress, Rank,
    SeatId, Team, TributeTask, TrickEntry, face_value, full_deck, is_level_card, is_wildcard,
    natural_value, readings, resolve_deal,
};

// ---------------------------------------------------------------------------
// Small builders
// ---------------------------------------------------------------------------

fn seat(index: u8) -> SeatId {
    SeatId::new(index).unwrap()
}

/// Cards in hand order (sorted by `Card`'s derived `Ord`, as `PlayerHand`
/// keeps them and `view_for` sends them).
fn sorted(s: &str) -> Vec<Card> {
    hand(s).cards().to_vec()
}

fn card(s: &str) -> Card {
    let parsed = cards(s);
    assert_eq!(parsed.len(), 1, "expected exactly one card in {s:?}");
    parsed[0]
}

fn level(rank: Rank) -> Level {
    Level(rank)
}

fn play(s: &str, combo: Combo) -> Play {
    play_with_wildcards(s, combo, vec![])
}

fn play_with_wildcards(s: &str, combo: Combo, wildcard_as: Vec<Rank>) -> Play {
    Play {
        cards: sorted(s),
        combo,
        wildcard_as,
    }
}

fn played(index: u8, play: Play) -> TrickEntry {
    TrickEntry::Played {
        seat: seat(index),
        play,
    }
}

fn passed(index: u8) -> TrickEntry {
    TrickEntry::Passed { seat: seat(index) }
}

const NAMES: [&str; 4] = ["Josey", "Alex", "Sam", "Robin"];

/// All four seats taken by the story's players; `connected` per seat.
fn seats(connected: [bool; 4]) -> [SeatInfo; 4] {
    std::array::from_fn(|i| SeatInfo {
        display_name: Some(NAMES[i].to_string()),
        connected: connected[i],
    })
}

const ALL_CONNECTED: [bool; 4] = [true; 4];

fn state(your_seat: u8, seats: [SeatInfo; 4], room: RoomView) -> ServerMessage {
    ServerMessage::State(Box::new(RedactedState {
        your_seat: seat(your_seat),
        seats,
        room,
    }))
}

/// An in-game `State` with every seat taken.
fn in_game(your_seat: u8, connected: [bool; 4], view: MatchView) -> ServerMessage {
    state(your_seat, seats(connected), RoomView::InGame(view))
}

/// Card counts as `view_for` redacts them: the viewer's own is always exact;
/// others are exact only when ≤ 10 (GAME_RULES.md "hand-size visibility").
fn card_counts(sizes: [usize; 4], viewer: u8) -> [CardCount; 4] {
    std::array::from_fn(|i| {
        let n = u8::try_from(sizes[i]).unwrap();
        if i == usize::from(viewer) || n <= 10 {
            CardCount::Exact(n)
        } else {
            CardCount::MoreThanTen
        }
    })
}

fn progress(levels: [Rank; 2], declaring: Option<Team>, a_attempts: [u8; 2]) -> Progress {
    Progress {
        team_levels: [level(levels[0]), level(levels[1])],
        declaring,
        a_attempts,
    }
}

fn match_view(p: &Progress, card_counts: [CardCount; 4], phase: PhaseView) -> MatchView {
    MatchView {
        team_levels: p.team_levels,
        a_attempts: p.a_attempts,
        declaring: p.declaring,
        card_counts,
        phase,
    }
}

// ---------------------------------------------------------------------------
// Multiset helpers (test-only; `PlayerHand::remove_all` is Task 3's)
// ---------------------------------------------------------------------------

fn minus(from: &[Card], take: &[Card]) -> Vec<Card> {
    let mut rest = from.to_vec();
    for c in take {
        let at = rest
            .iter()
            .position(|x| x == c)
            .unwrap_or_else(|| panic!("{c:?} is not in {from:?}"));
        rest.remove(at);
    }
    rest
}

fn plus(to: &[Card], add: &[Card]) -> Vec<Card> {
    let mut all = to.to_vec();
    all.extend_from_slice(add);
    all.sort();
    all
}

fn is_subset(small: &[Card], big: &[Card]) -> bool {
    let mut rest = big.to_vec();
    small
        .iter()
        .all(|c| match rest.iter().position(|x| x == c) {
            Some(at) => {
                rest.remove(at);
                true
            }
            None => false,
        })
}

// ---------------------------------------------------------------------------
// Independent re-statement of the tribute option rules (TECH_SPEC.md §3.9),
// used only to check the hand-written options in the fixtures.
// ---------------------------------------------------------------------------

fn expected_tribute_options(hand: &[Card], lvl: Level) -> Vec<Card> {
    let candidates: Vec<Card> = hand
        .iter()
        .copied()
        .filter(|&c| !is_wildcard(c, lvl))
        .collect();
    let max = candidates
        .iter()
        .map(|c| face_value(c.face(), lvl))
        .max()
        .unwrap();
    let set: BTreeSet<Card> = candidates
        .into_iter()
        .filter(|c| face_value(c.face(), lvl) == max)
        .collect();
    set.into_iter().collect()
}

fn expected_return_options(hand: &[Card], received: Card, lvl: Level) -> Vec<Card> {
    let pool = minus(hand, &[received]);
    let eligible: BTreeSet<Card> = pool
        .iter()
        .copied()
        .filter(|&c| match c {
            Card::Standard { rank, .. } => !is_level_card(c, lvl) && natural_value(rank) <= 10,
            Card::Joker(_) => false,
        })
        .collect();
    if !eligible.is_empty() {
        return eligible.into_iter().collect();
    }
    let min = pool
        .iter()
        .map(|c| face_value(c.face(), lvl))
        .min()
        .unwrap();
    let set: BTreeSet<Card> = pool
        .into_iter()
        .filter(|c| face_value(c.face(), lvl) == min)
        .collect();
    set.into_iter().collect()
}

// ---------------------------------------------------------------------------
// The §4 story: deal 2 of a match, level Five, team B declaring.
// ---------------------------------------------------------------------------

/// The four hands as dealt for deal 2. Seat 0's highest non-wildcard is a
/// Small Joker; seat 2 holds no jokers, so its highest is a level card
/// (5♠/5♣). The Big Jokers are with seats 1 and 3, so no anti-tribute.
const DEALT: [&str; 4] = [
    "2H 2H 2D 3S 3D 4S 4H 5S 5D 6S 6C 7S 7S 7C 8S 8H 8C TS TD JD JC QS KS KH AS AC SJ",
    "2S 2S 3C 4C 4C 5D 5C 6H 6D 7H 7H 8D 8D 9S 9C 9C TS JH JC QS QD KS KH KC AS AD BJ",
    "2C 3S 3H 3D 5S 5C 6C 7D 9H 9H TH TH TD TC JS JS JD QH QH QC QC KD KD KC AH AH AC",
    "2D 2C 3H 3C 4S 4H 4D 4D 5H 5H 6S 6H 6D 7D 7C 8S 8H 8C 9S 9D 9D TC JH QD AD SJ BJ",
];

// Double tribute: seat 0 → seat 1 (Small Joker, the higher tribute), seat 2 →
// seat 3 (5♣). Returns: seat 1 → seat 0 (3♣), seat 3 → seat 2 (4♦).
const TRIBUTE_0: &str = "SJ";
const TRIBUTE_2: &str = "5C";
const RETURN_1: &str = "3C";
const RETURN_3: &str = "4D";

/// §4 playing moment (M1): seat 0 has 8 cards, seat 2 has 19.
const AT_PLAY: [&str; 4] = [
    "3S 5S 7S 7S KS KH AS AC",
    "6H 6D 7H 7H 8D 8D KH KC AS AD SJ BJ",
    "9H 9H TH TH TD TC JS JS JD QH QH QC QC KD KD KC AH AH AC",
    "5H 5H 5C 6S 6H 6D 8S 8H 8C 9S 9D 9D TC JH QD AD SJ BJ",
];

/// Later (M2): seat 1 just went out on a winning tube; seat 3 leads (接风)
/// holding 9,9,9 and both wildcards.
const AT_JIEFENG: [&str; 4] = [
    "3S 5S 7S 7S KS KH AS AC",
    "",
    "9H 9H TH TH TD TC JS JS JD QH QH QC QC KC AH AH AC",
    "5H 5H 9S 9D 9D BJ",
];

/// Deal end (M3): seat 3 bombed with 9,9,9,5♥,5♥, then led its last card.
const AT_DEAL_END: [&str; 4] = [
    "3S 5S KS KH AS AC",
    "",
    "9H 9H TH TH TD TC JS JS JD QH QH QC QC KC AH AH AC",
    "",
];

const AMBIGUOUS: &str = "9S 9D 9D 5H 5H";

/// `client_classify` / `classified`: the client's per-tab request counter.
const CLASSIFY_ID: u32 = 7;

fn hands(strs: [&str; 4]) -> [Vec<Card>; 4] {
    strs.map(sorted)
}

fn sizes(h: &[Vec<Card>; 4]) -> [usize; 4] {
    std::array::from_fn(|i| h[i].len())
}

fn dealt() -> [Vec<Card>; 4] {
    hands(DEALT)
}

/// After both tributes moved (they move together once all are paid).
fn after_tributes() -> [Vec<Card>; 4] {
    let d = dealt();
    [
        minus(&d[0], &cards(TRIBUTE_0)),
        plus(&d[1], &cards(TRIBUTE_0)),
        minus(&d[2], &cards(TRIBUTE_2)),
        plus(&d[3], &cards(TRIBUTE_2)),
    ]
}

/// After both returns moved: the hands the play phase starts with.
fn after_exchange() -> [Vec<Card>; 4] {
    let t = after_tributes();
    [
        plus(&t[0], &cards(RETURN_1)),
        minus(&t[1], &cards(RETURN_1)),
        plus(&t[2], &cards(RETURN_3)),
        minus(&t[3], &cards(RETURN_3)),
    ]
}

fn five() -> Level {
    level(Rank::Five)
}

/// Before deal 2: team B won deal 1 with a 1-2 (Two + 3 → Five).
fn deal2_progress() -> Progress {
    progress([Rank::Two, Rank::Five], Some(Team::B), [0, 0])
}

fn deal2_start() -> DealStart {
    DealStart::Tribute {
        exchanges: vec![
            Exchange {
                payer: seat(0),
                receiver: seat(1),
                tribute: card(TRIBUTE_0),
                returned: card(RETURN_1),
            },
            Exchange {
                payer: seat(2),
                receiver: seat(3),
                tribute: card(TRIBUTE_2),
                returned: card(RETURN_3),
            },
        ],
        leader: seat(0),
    }
}

/// M1 `last_trick`: seat 0 led a straight, seat 1 beat it, everyone passed.
fn m1_last_trick() -> CompletedTrick {
    CompletedTrick {
        entries: vec![
            played(
                0,
                play("4S 5D 6S 7C 8H", Combo::Straight { top: Rank::Eight }),
            ),
            played(
                1,
                play("9C TS JH QD KS", Combo::Straight { top: Rank::King }),
            ),
            passed(2),
            passed(3),
            passed(0),
        ],
        winner: seat(1),
        next_leader: seat(1),
    }
}

fn pair_of_nines() -> Play {
    play("9S 9C", Combo::Pair(Face::Rank(Rank::Nine)))
}

/// M1 current trick: seat 1 led a pair of 9s, seat 2 passed; seat 3 to act.
fn m1_trick() -> Vec<TrickEntry> {
    vec![played(1, pair_of_nines()), passed(2)]
}

/// M2 `last_trick`: seat 1 led its last six cards as a tube; nobody beat it,
/// so the lead passes to seat 1's partner, seat 3 (接风).
fn m2_last_trick() -> CompletedTrick {
    CompletedTrick {
        entries: vec![
            played(
                1,
                play("6H 6D 7H 7H 8D 8D", Combo::Tube { top: Rank::Eight }),
            ),
            passed(2),
            passed(3),
            passed(0),
        ],
        winner: seat(1),
        next_leader: seat(3),
    }
}

fn m3_final_trick() -> Vec<TrickEntry> {
    vec![played(3, play("BJ", Combo::Single(Face::BigJoker)))]
}

fn ambiguous_options() -> Vec<Play> {
    vec![
        play("9S 9D 9D 5H 5H", Combo::FullHouse { triple: Rank::Nine }),
        play_with_wildcards(
            "9S 9D 9D 5H 5H",
            Combo::Bomb(Bomb::OfAKind {
                size: 5,
                rank: Rank::Nine,
            }),
            vec![Rank::Nine, Rank::Nine],
        ),
    ]
}

fn five_bomb() -> Combo {
    Combo::Bomb(Bomb::OfAKind {
        size: 5,
        rank: Rank::Nine,
    })
}

// --- deal 2 tribute phase ---

/// Seat 0 has paid (hidden until all are paid); seat 2 hasn't yet.
fn duties_one_paid() -> Vec<DutyView> {
    vec![
        DutyView {
            payer: seat(0),
            receiver: seat(1),
            paid: true,
            tribute: None,
            returned: false,
        },
        DutyView {
            payer: seat(2),
            receiver: seat(3),
            paid: false,
            tribute: None,
            returned: false,
        },
    ]
}

/// §4 moment: both paid (so both tributes are public); seat 3 has returned
/// blind; seat 1 still has to return.
fn duties_all_paid() -> Vec<DutyView> {
    vec![
        DutyView {
            payer: seat(0),
            receiver: seat(1),
            paid: true,
            tribute: Some(card(TRIBUTE_0)),
            returned: false,
        },
        DutyView {
            payer: seat(2),
            receiver: seat(3),
            paid: true,
            tribute: Some(card(TRIBUTE_2)),
            returned: true,
        },
    ]
}

/// Seat 2's options: its non-wildcard level cards.
const TRIBUTE_OPTIONS_2: &str = "5S 5C";
/// Seat 1's return options: every non-joker, non-level card of rank ≤ 10.
const RETURN_OPTIONS_1: &str = "2S 3C 4C 6H 6D 7H 8D 9S 9C TS";

fn tribute_state(
    viewer: u8,
    h: &[Vec<Card>; 4],
    duties: Vec<DutyView>,
    task: Option<TributeTask>,
    connected: [bool; 4],
) -> ServerMessage {
    in_game(
        viewer,
        connected,
        match_view(
            &deal2_progress(),
            card_counts(sizes(h), viewer),
            PhaseView::Tribute {
                your_hand: h[usize::from(viewer)].clone(),
                level: five(),
                duties,
                your_task: task,
            },
        ),
    )
}

/// §4: seen by seat 1, which must return a card. Seat 3 (Robin) is offline.
fn state_tribute() -> ServerMessage {
    tribute_state(
        1,
        &after_tributes(),
        duties_all_paid(),
        Some(TributeTask::ReturnTribute {
            options: sorted(RETURN_OPTIONS_1),
        }),
        [true, true, true, false],
    )
}

fn state_tribute_pay() -> ServerMessage {
    tribute_state(
        2,
        &dealt(),
        duties_one_paid(),
        Some(TributeTask::PayTribute {
            options: sorted(TRIBUTE_OPTIONS_2),
        }),
        ALL_CONNECTED,
    )
}

fn state_tribute_waiting() -> ServerMessage {
    tribute_state(0, &dealt(), duties_one_paid(), None, ALL_CONNECTED)
}

// --- deal 2 play phase ---

fn deal2_playing(
    viewer: u8,
    h: &[Vec<Card>; 4],
    turn: u8,
    trick: Vec<TrickEntry>,
    last_trick: CompletedTrick,
    finish_order: Vec<SeatId>,
) -> ServerMessage {
    in_game(
        viewer,
        ALL_CONNECTED,
        match_view(
            &deal2_progress(),
            card_counts(sizes(h), viewer),
            PhaseView::Playing {
                your_hand: h[usize::from(viewer)].clone(),
                level: five(),
                turn: seat(turn),
                trick,
                last_trick: Some(last_trick),
                finish_order,
                deal_start: deal2_start(),
                can_take_back: false,
                took_back: None,
            },
        ),
    )
}

/// §4: seen by seat 2, seat 3's turn.
fn state_playing() -> ServerMessage {
    deal2_playing(2, &hands(AT_PLAY), 3, m1_trick(), m1_last_trick(), vec![])
}

/// The same moment seen by seat 3, whose turn it is.
fn state_playing_your_turn() -> ServerMessage {
    deal2_playing(3, &hands(AT_PLAY), 3, m1_trick(), m1_last_trick(), vec![])
}

/// Seat 3 leads after 接风 (seat 1 went out on the winning play).
fn state_playing_lead_jiefeng() -> ServerMessage {
    deal2_playing(
        3,
        &hands(AT_JIEFENG),
        3,
        vec![],
        m2_last_trick(),
        vec![seat(1)],
    )
}

// ---------------------------------------------------------------------------
// Other moments (not part of the deal-2 story)
// ---------------------------------------------------------------------------

/// Deal 3 of the same match (level Eight): payers 0 and 2 hold one Big Joker
/// each, so tribute is cancelled and seat 1 (last deal's 1st place) leads.
const ANTI_TRIBUTE_HAND_0: &str =
    "2S 2H 3D 3C 4D 5S 5H 6C 6D 7S 7C 8S 8D 9H 9C TS TD JH JC QS QD KS KH AS AH AC BJ";

fn state_playing_anti_tribute() -> ServerMessage {
    let p = progress([Rank::Two, Rank::Eight], Some(Team::B), [0, 0]);
    in_game(
        0,
        ALL_CONNECTED,
        match_view(
            &p,
            card_counts([27, 24, 27, 27], 0),
            PhaseView::Playing {
                your_hand: sorted(ANTI_TRIBUTE_HAND_0),
                level: level(Rank::Eight),
                turn: seat(2),
                trick: vec![played(1, play("4S 4H 4C", Combo::Triple(Rank::Four)))],
                last_trick: None,
                finish_order: vec![],
                deal_start: DealStart::AntiTribute { leader: seat(1) },
                can_take_back: false,
                took_back: None,
            },
        ),
    )
}

/// The first deal of a match (level Two, no declaring team): 8♦ was turned
/// up and seat 2 holds it, so seat 2 led. Seen by seat 0, whose turn it is.
const FIRST_DEAL_HAND_0: &str =
    "2S 2D 3H 3C 4S 4D 5H 5C 6S 7D 7C 8S 8H 9C 9D TS TD JH QS QD KC KC AS AH AD SJ BJ";

fn state_playing_first_deal() -> ServerMessage {
    first_deal_playing(
        0,
        sorted(FIRST_DEAL_HAND_0),
        [27, 27, 25, 25],
        0,
        first_deal_trick(),
        false,
        None,
    )
}

/// The first deal's trick so far: seat 2 led a pair of 6s, seat 3 beat it
/// with a pair of Jacks.
fn first_deal_trick() -> Vec<TrickEntry> {
    vec![
        played(2, play("6H 6D", Combo::Pair(Face::Rank(Rank::Six)))),
        played(3, play("JS JC", Combo::Pair(Face::Rank(Rank::Jack)))),
    ]
}

/// Seat 3's hand in the first deal, after playing JS JC (25 cards).
const FIRST_DEAL_HAND_3: &str =
    "2C 3S 3D 4H 4C 5S 5D 6C 7S 7H 8D 8C 9S 9H TH TC JD QH QC KS KH KD AC AC SJ";

/// A moment of the first deal (level Two, 8♦ turned up, seat 2 led).
fn first_deal_playing(
    viewer: u8,
    your_hand: Vec<Card>,
    sizes: [usize; 4],
    turn: u8,
    trick: Vec<TrickEntry>,
    can_take_back: bool,
    took_back: Option<SeatId>,
) -> ServerMessage {
    let p = progress([Rank::Two, Rank::Two], None, [0, 0]);
    in_game(
        viewer,
        ALL_CONNECTED,
        match_view(
            &p,
            card_counts(sizes, viewer),
            PhaseView::Playing {
                your_hand,
                level: level(Rank::Two),
                turn: seat(turn),
                trick,
                last_trick: None,
                finish_order: vec![],
                deal_start: DealStart::FirstDeal {
                    revealed: card("8D"),
                    leader: seat(2),
                },
                can_take_back,
                took_back,
            },
        ),
    )
}

/// The first deal's moment seen by seat 3, which just played the pair of
/// Jacks: nobody has acted since, so seat 3 may take it back.
fn state_playing_can_take_back() -> ServerMessage {
    first_deal_playing(
        3,
        sorted(FIRST_DEAL_HAND_3),
        [27, 27, 25, 25],
        0,
        first_deal_trick(),
        true,
        None,
    )
}

/// Seat 3 took the pair of Jacks back: the trick is just seat 2's pair of
/// 6s, it's seat 3's turn again with 27 cards, and everyone is told who took
/// back (`took_back: 3`). Seen by seat 0.
fn state_playing_took_back() -> ServerMessage {
    let mut trick = first_deal_trick();
    trick.pop();
    first_deal_playing(
        0,
        sorted(FIRST_DEAL_HAND_0),
        [27, 27, 25, 27],
        3,
        trick,
        false,
        Some(seat(3)),
    )
}

/// A first deal (level Two, so 2♥ is the wildcard) whose second trick
/// escalates through every bomb size the deck allows alongside a 10-card
/// bomb: 4 → 5 → straight flush → 6 → 7 → 8 → 10 (8 Kings + both 2♥) →
/// jokers. `last_trick` is a plate over a plate. Seen by seat 2, whose turn
/// it is (it can only pass).
const ALL_BOMBS_HAND_2: &str = "2S 2D 3D 3C 4D 4H 5S 7S 7H 7D 9C TC JC QC AH AD";

fn of_a_kind(size: u8, rank: Rank) -> Combo {
    Combo::Bomb(Bomb::OfAKind { size, rank })
}

fn state_playing_all_bombs() -> ServerMessage {
    let p = progress([Rank::Two, Rank::Two], None, [0, 0]);
    in_game(
        2,
        ALL_CONNECTED,
        match_view(
            &p,
            card_counts([6, 10, 16, 14], 2),
            PhaseView::Playing {
                your_hand: sorted(ALL_BOMBS_HAND_2),
                level: level(Rank::Two),
                turn: seat(2),
                trick: vec![
                    played(1, play("AS", Combo::Single(Face::Rank(Rank::Ace)))),
                    played(2, play("5S 5H 5D 5D", of_a_kind(4, Rank::Five))),
                    played(3, play("6S 6S 6H 6H 6D", of_a_kind(5, Rank::Six))),
                    played(
                        0,
                        play(
                            "3C 4C 5C 6C 7C",
                            Combo::Bomb(Bomb::StraightFlush { top: Rank::Seven }),
                        ),
                    ),
                    played(1, play("JS JS JH JH JD JD", of_a_kind(6, Rank::Jack))),
                    played(2, play("QS QS QH QH QD QD QC", of_a_kind(7, Rank::Queen))),
                    played(
                        3,
                        play("8S 8S 8H 8H 8D 8D 8C 8C", of_a_kind(8, Rank::Eight)),
                    ),
                    played(
                        0,
                        play_with_wildcards(
                            "KS KS KH KH KD KD KC KC 2H 2H",
                            of_a_kind(10, Rank::King),
                            vec![Rank::King, Rank::King],
                        ),
                    ),
                    played(1, play("SJ SJ BJ BJ", Combo::Bomb(Bomb::Jokers))),
                ],
                last_trick: Some(CompletedTrick {
                    entries: vec![
                        played(
                            0,
                            play("3S 3S 3H 4S 4S 4H", Combo::Plate { top: Rank::Four }),
                        ),
                        played(
                            1,
                            play("9S 9H 9D TS TH TD", Combo::Plate { top: Rank::Ten }),
                        ),
                        passed(2),
                        passed(3),
                        passed(0),
                    ],
                    winner: seat(1),
                    next_leader: seat(1),
                }),
                finish_order: vec![],
                deal_start: DealStart::FirstDeal {
                    revealed: card("4S"),
                    leader: seat(0),
                },
                can_take_back: false,
                took_back: None,
            },
        ),
    )
}

// --- lobby ---

fn lobby_seats() -> [SeatInfo; 4] {
    let mut s = seats([true, true, true, false]);
    s[3].display_name = None;
    s
}

/// A fresh lobby (no match yet): three seated, seats 0 and 2 ready.
fn lobby(your_seat: u8) -> ServerMessage {
    state(
        your_seat,
        lobby_seats(),
        RoomView::Lobby(LobbyView {
            ready: [true, false, true, false],
            progress: Progress::default(),
            seats_locked: false,
            last_deal: None,
        }),
    )
}

/// LOBBY_FLOW_SPEC.md §7: team A declared at Ace (its first A attempt) and
/// lost; team B won 1-3 (Alex 1st, Sam 2nd, Robin 3rd, Josey 4th), so team B
/// goes Six → Eight and declares, and team A stays at Ace with one failure.
/// Josey and Robin are ready for the next deal; seen by seat 0.
fn state_lobby_between_deals() -> ServerMessage {
    let summary = DealSummary {
        result: DealResult {
            order: vec![seat(1), seat(2), seat(3), seat(0)],
        },
        before: progress([Rank::Ace, Rank::Six], Some(Team::A), [0, 0]),
        after: progress([Rank::Ace, Rank::Eight], Some(Team::B), [1, 0]),
        dropped_to_two: None,
        match_winner: None,
    };
    state(
        0,
        seats(ALL_CONNECTED),
        RoomView::Lobby(LobbyView {
            ready: [true, false, false, true],
            progress: summary.after.clone(),
            seats_locked: true,
            last_deal: Some(DealEnd {
                summary,
                // Josey led a pair of 7s; Robin went out on a pair of Ks.
                final_trick: vec![
                    played(0, play("7S 7D", Combo::Pair(Face::Rank(Rank::Seven)))),
                    played(3, play("KH KC", Combo::Pair(Face::Rank(Rank::King)))),
                ],
            }),
        }),
    )
}

/// A failed A attempt: team A (declaring at Ace, 2 failures already) loses
/// 1-4, hits 3 failures and drops to Two; team B goes Nine → Ten and
/// declares. Robin has left (the seat is kept: seats are locked). Seen by
/// seat 0.
fn state_lobby_dropped_to_two() -> ServerMessage {
    let summary = DealSummary {
        result: DealResult {
            order: vec![seat(1), seat(0), seat(2), seat(3)],
        },
        before: progress([Rank::Ace, Rank::Nine], Some(Team::A), [2, 0]),
        after: progress([Rank::Two, Rank::Ten], Some(Team::B), [0, 0]),
        dropped_to_two: Some(Team::A),
        match_winner: None,
    };
    state(
        0,
        seats([true, true, true, false]),
        RoomView::Lobby(LobbyView {
            ready: [false; 4],
            progress: summary.after.clone(),
            seats_locked: true,
            last_deal: Some(DealEnd {
                summary,
                final_trick: vec![
                    played(3, play("9D", Combo::Single(Face::Rank(Rank::Nine)))),
                    played(2, play("QS", Combo::Single(Face::Rank(Rank::Queen)))),
                ],
            }),
        }),
    )
}

/// Team A wins the match at level A with a 1-3 (its second A attempt). The
/// table is fresh again (levels 2/2, seats unlocked) but the winning deal is
/// still shown, for the "Team A won the match!" banner. Seen by seat 3.
fn state_lobby_match_won() -> ServerMessage {
    let before = progress([Rank::Ace, Rank::Jack], Some(Team::A), [1, 0]);
    let summary = DealSummary {
        result: DealResult {
            order: vec![seat(0), seat(1), seat(2), seat(3)],
        },
        before: before.clone(),
        after: before,
        dropped_to_two: None,
        match_winner: Some(Team::A),
    };
    state(
        3,
        seats(ALL_CONNECTED),
        RoomView::Lobby(LobbyView {
            ready: [false; 4],
            progress: Progress::default(),
            seats_locked: false,
            last_deal: Some(DealEnd {
                summary,
                final_trick: vec![
                    played(3, play("9S", Combo::Single(Face::Rank(Rank::Nine)))),
                    played(2, play("BJ", Combo::Single(Face::BigJoker))),
                ],
            }),
        }),
    )
}

// ---------------------------------------------------------------------------
// The fixture list
// ---------------------------------------------------------------------------

enum Message {
    Client(ClientMessage),
    Server(ServerMessage),
}

impl Message {
    fn to_value(&self) -> Value {
        match self {
            Message::Client(m) => serde_json::to_value(m).unwrap(),
            Message::Server(m) => serde_json::to_value(m).unwrap(),
        }
    }

    fn to_pretty(&self) -> String {
        match self {
            Message::Client(m) => serde_json::to_string_pretty(m).unwrap(),
            Message::Server(m) => serde_json::to_string_pretty(m).unwrap(),
        }
    }

    /// Parses `json` as the same message type and checks it equals `self`.
    fn round_trips_from(&self, json: &str) -> Result<(), String> {
        let same = match self {
            Message::Client(m) => serde_json::from_str::<ClientMessage>(json)
                .map(|back| back == *m)
                .map_err(|e| e.to_string())?,
            Message::Server(m) => serde_json::from_str::<ServerMessage>(json)
                .map(|back| back == *m)
                .map_err(|e| e.to_string())?,
        };
        if same {
            Ok(())
        } else {
            Err("deserializes to a different value".to_string())
        }
    }
}

fn rejected(code: RejectCode, message: String) -> ServerMessage {
    ServerMessage::Rejected { code, message }
}

fn fixtures() -> Vec<(&'static str, Message)> {
    use Message::{Client, Server};
    vec![
        // --- client → server ---
        (
            "client_join",
            Client(ClientMessage::Join {
                display_name: "Josey".to_string(),
                reconnect_token: None,
            }),
        ),
        (
            "client_join_reconnect",
            Client(ClientMessage::Join {
                display_name: "Josey".to_string(),
                reconnect_token: Some("9f2b7a6e1c4d4f0aa5b6c7d8e9f0a1b2".to_string()),
            }),
        ),
        (
            "client_play",
            Client(ClientMessage::Play {
                cards: pair_of_nines().cards,
                declared: None,
            }),
        ),
        (
            "client_play_declared",
            Client(ClientMessage::Play {
                cards: sorted(AMBIGUOUS),
                declared: Some(five_bomb()),
            }),
        ),
        ("client_pass", Client(ClientMessage::Pass)),
        ("client_take_back", Client(ClientMessage::TakeBack)),
        (
            "client_pay_tribute",
            Client(ClientMessage::PayTribute {
                card: card(TRIBUTE_2),
            }),
        ),
        (
            "client_return_tribute",
            Client(ClientMessage::ReturnTribute {
                card: card(RETURN_1),
            }),
        ),
        (
            "client_set_ready",
            Client(ClientMessage::SetReady { ready: true }),
        ),
        (
            "client_choose_seat",
            Client(ClientMessage::ChooseSeat { seat: seat(3) }),
        ),
        (
            "client_update_settings",
            Client(ClientMessage::UpdateSettings {
                team_levels: [level(Rank::Ace), level(Rank::Eight)],
                declaring: Some(Team::B),
            }),
        ),
        ("client_new_match", Client(ClientMessage::NewMatch)),
        ("client_reset_deal", Client(ClientMessage::ResetDeal)),
        (
            "client_classify",
            Client(ClientMessage::Classify {
                request_id: CLASSIFY_ID,
                cards: sorted(AMBIGUOUS),
            }),
        ),
        // --- server → client ---
        (
            "joined",
            Server(ServerMessage::Joined {
                seat: seat(0),
                session_token: "9f2b7a6e1c4d4f0aa5b6c7d8e9f0a1b2".to_string(),
            }),
        ),
        ("state_lobby", Server(lobby(0))),
        ("state_lobby_not_ready", Server(lobby(1))),
        (
            "state_lobby_between_deals",
            Server(state_lobby_between_deals()),
        ),
        (
            "state_lobby_dropped_to_two",
            Server(state_lobby_dropped_to_two()),
        ),
        ("state_lobby_match_won", Server(state_lobby_match_won())),
        ("state_tribute", Server(state_tribute())),
        ("state_tribute_pay", Server(state_tribute_pay())),
        ("state_tribute_waiting", Server(state_tribute_waiting())),
        ("state_playing", Server(state_playing())),
        ("state_playing_your_turn", Server(state_playing_your_turn())),
        (
            "state_playing_lead_jiefeng",
            Server(state_playing_lead_jiefeng()),
        ),
        (
            "state_playing_first_deal",
            Server(state_playing_first_deal()),
        ),
        (
            "state_playing_anti_tribute",
            Server(state_playing_anti_tribute()),
        ),
        ("state_playing_all_bombs", Server(state_playing_all_bombs())),
        (
            "state_playing_can_take_back",
            Server(state_playing_can_take_back()),
        ),
        ("state_playing_took_back", Server(state_playing_took_back())),
        (
            "choose_reading",
            Server(ServerMessage::ChooseReading {
                cards: sorted(AMBIGUOUS),
                options: ambiguous_options(),
            }),
        ),
        (
            "rejected",
            Server(rejected(
                RejectCode::Action(ActionError::NotYourTurn),
                ActionError::NotYourTurn.to_string(),
            )),
        ),
        (
            "rejected_name_taken",
            Server(rejected(
                RejectCode::Session(SessionError::NameTaken),
                SessionError::NameTaken.to_string(),
            )),
        ),
        (
            "rejected_seats_locked",
            Server(rejected(
                RejectCode::Session(SessionError::SeatsLocked),
                SessionError::SeatsLocked.to_string(),
            )),
        ),
        ("kicked", Server(ServerMessage::Kicked)),
        (
            "classified",
            Server(ServerMessage::Classified {
                request_id: CLASSIFY_ID,
                readings: ambiguous_options(),
            }),
        ),
    ]
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn fixtures_match_serde_output() {
    let update = std::env::var("UPDATE_FIXTURES").is_ok_and(|v| v == "1");
    let dir = fixtures_dir();
    if update {
        std::fs::create_dir_all(&dir).unwrap();
    }

    let mut failures = Vec::new();
    for (name, message) in fixtures() {
        let path = dir.join(format!("{name}.json"));
        if update {
            std::fs::write(&path, message.to_pretty() + "\n").unwrap();
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            failures.push(format!("{name}: missing {}", path.display()));
            continue;
        };
        match serde_json::from_str::<Value>(&text) {
            Ok(on_disk) if on_disk == message.to_value() => {}
            Ok(_) => failures.push(format!("{name}: differs from serde output")),
            Err(e) => failures.push(format!("{name}: invalid JSON: {e}")),
        }
        if let Err(e) = message.round_trips_from(&text) {
            failures.push(format!("{name}: {e}"));
        }
    }
    assert!(
        failures.is_empty(),
        "fixture mismatches (rerun with UPDATE_FIXTURES=1 if the change is intended):\n{}",
        failures.join("\n")
    );
}

#[test]
fn fixture_names_are_unique_and_every_file_is_listed() {
    let names: Vec<&str> = fixtures().iter().map(|(name, _)| *name).collect();
    let unique: BTreeSet<&str> = names.iter().copied().collect();
    assert_eq!(unique.len(), names.len(), "duplicate fixture name");

    let readme = std::fs::read_to_string(fixtures_dir().join("README.md")).unwrap();
    for name in &names {
        assert!(
            readme.contains(&format!("`{name}.json`")),
            "fixtures/README.md doesn't describe {name}.json"
        );
    }

    // No stale files left behind by a renamed fixture.
    for entry in std::fs::read_dir(fixtures_dir()).unwrap() {
        let file = entry.unwrap().file_name().into_string().unwrap();
        if let Some(stem) = file.strip_suffix(".json") {
            assert!(unique.contains(stem), "fixtures/{file} has no builder");
        }
    }
}

/// Collects every object key and string value, to check variant coverage.
fn collect_names(value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::String(s) => {
            out.insert(s.clone());
        }
        Value::Array(items) => items.iter().for_each(|v| collect_names(v, out)),
        Value::Object(map) => {
            for (k, v) in map {
                out.insert(k.clone());
                collect_names(v, out);
            }
        }
        _ => {}
    }
}

#[test]
fn fixtures_cover_every_variant_the_client_sees() {
    let mut types = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut declared = (false, false);
    for (_, message) in fixtures() {
        let value = message.to_value();
        types.insert(value["type"].as_str().unwrap().to_string());
        collect_names(&value, &mut names);
        if let Message::Client(ClientMessage::Play { declared: d, .. }) = &message {
            if d.is_some() {
                declared.1 = true;
            } else {
                declared.0 = true;
            }
        }
    }
    assert_eq!(declared, (true, true), "Play with and without `declared`");

    let expected_types = [
        // ClientMessage
        "Join",
        "Play",
        "Pass",
        "TakeBack",
        "PayTribute",
        "ReturnTribute",
        "SetReady",
        "ChooseSeat",
        "UpdateSettings",
        "NewMatch",
        "ResetDeal",
        "Classify",
        // ServerMessage
        "Joined",
        "State",
        "ChooseReading",
        "Rejected",
        "Kicked",
        "Classified",
    ];
    for t in expected_types {
        assert!(types.contains(t), "no fixture with type {t}");
    }

    let expected_names = [
        // RoomView
        "Lobby",
        "InGame",
        // PhaseView
        "Tribute",
        "Playing",
        // LobbyView.last_deal
        "last_deal",
        "final_trick",
        // PhaseView::Playing take back (GAME_PAGE_V3_SPEC.md §1)
        "can_take_back",
        "took_back",
        // TributeTask
        "PayTribute",
        "ReturnTribute",
        // DealStart
        "FirstDeal",
        "AntiTribute",
        // Combo
        "Single",
        "Pair",
        "Triple",
        "FullHouse",
        "Straight",
        "Tube",
        "Plate",
        "Bomb",
        // Bomb
        "OfAKind",
        "StraightFlush",
        "Jokers",
        // CardCount, Face (SmallJoker has the same shape as BigJoker)
        "Exact",
        "MoreThanTen",
        "BigJoker",
        // RejectCode: one of each enum
        "NotYourTurn",
        "NameTaken",
    ];
    for n in expected_names {
        assert!(names.contains(n), "no fixture contains {n}");
    }
}

/// The State fixture called `name`.
fn state_fixture(name: &str) -> RedactedState {
    let Message::Server(ServerMessage::State(s)) =
        fixtures().into_iter().find(|(n, _)| *n == name).unwrap().1
    else {
        panic!("{name} is not a State");
    };
    *s
}

#[test]
fn match_won_fixture_has_a_winner_and_a_tribute_state_has_no_task() {
    let s = state_fixture("state_lobby_match_won");
    let RoomView::Lobby(lobby) = &s.room else {
        panic!("not in the lobby")
    };
    let end = lobby.last_deal.as_ref().unwrap();
    assert_eq!(end.summary.match_winner, Some(Team::A));
    assert_eq!(lobby.progress, Progress::default());
    assert!(!lobby.seats_locked);

    let s = state_fixture("state_tribute_waiting");
    let RoomView::InGame(view) = &s.room else {
        panic!("not in game")
    };
    assert!(matches!(
        view.phase,
        PhaseView::Tribute {
            your_task: None,
            ..
        }
    ));
}

/// The take-back fixtures show what the client keys on: `can_take_back`
/// for the player who just played, and `took_back` for everyone afterwards;
/// the other Playing fixtures have neither. The take back puts seat 3's
/// pair of Jacks back into its hand.
#[test]
fn take_back_fixtures_show_the_flags() {
    let playing = |name: &str| {
        let s = state_fixture(name);
        let RoomView::InGame(view) = s.room else {
            panic!("{name}: not in game")
        };
        view.phase
    };
    let PhaseView::Playing {
        can_take_back,
        took_back,
        ..
    } = playing("state_playing_can_take_back")
    else {
        panic!("not Playing")
    };
    assert_eq!((can_take_back, took_back), (true, None));
    let PhaseView::Playing {
        can_take_back,
        took_back,
        ..
    } = playing("state_playing_took_back")
    else {
        panic!("not Playing")
    };
    assert_eq!((can_take_back, took_back), (false, Some(seat(3))));

    for name in [
        "state_playing",
        "state_playing_your_turn",
        "state_playing_lead_jiefeng",
        "state_playing_first_deal",
        "state_playing_anti_tribute",
        "state_playing_all_bombs",
    ] {
        let PhaseView::Playing {
            can_take_back,
            took_back,
            ..
        } = playing(name)
        else {
            panic!("{name}: not Playing")
        };
        assert_eq!((can_take_back, took_back), (false, None), "{name}");
    }

    // Seat 3's 25 cards plus the Jacks it took back are a 27-card hand that
    // fits in one deck alongside seat 0's hand and seat 2's 6s.
    let taken_back = plus(&sorted(FIRST_DEAL_HAND_3), &cards("JS JC"));
    assert_eq!(taken_back.len(), 27);
    let mut seen: BTreeMap<Card, usize> = BTreeMap::new();
    for c in taken_back
        .iter()
        .chain(&sorted(FIRST_DEAL_HAND_0))
        .chain(&cards("6H 6D"))
    {
        *seen.entry(*c).or_default() += 1;
    }
    assert!(seen.values().all(|&n| n <= 2), "a card 3 times");
}

/// Every lobby fixture is a table the Room could really hold: the summary is
/// what the rules make of `before` + `result`; seats are locked exactly when
/// the table holds a deal's result (a shown deal that didn't win the match),
/// and then `progress` is that deal's `after`; the final trick's cards are in
/// hand order and appear at most twice.
#[test]
fn lobby_fixtures_are_consistent() {
    let mut lobbies = 0;
    for (name, message) in fixtures() {
        let Message::Server(ServerMessage::State(s)) = message else {
            continue;
        };
        let RoomView::Lobby(lobby) = &s.room else {
            continue;
        };
        lobbies += 1;
        let Some(end) = &lobby.last_deal else {
            assert!(!lobby.seats_locked, "{name}: locked with no deal played");
            assert_eq!(lobby.progress, Progress::default(), "{name}");
            continue;
        };
        let summary = &end.summary;
        assert_eq!(
            resolve_deal(&summary.before, summary.result.clone()),
            *summary,
            "{name}: summary"
        );
        let won = summary.match_winner.is_some();
        assert_eq!(lobby.seats_locked, !won, "{name}: seats_locked");
        if won {
            assert_eq!(lobby.progress, Progress::default(), "{name}");
        } else {
            assert_eq!(lobby.progress, summary.after, "{name}");
            assert!(lobby.progress.declaring.is_some(), "{name}");
        }

        let mut seen: BTreeMap<Card, usize> = BTreeMap::new();
        for (_, play) in trick_plays(&end.final_trick) {
            assert!(play.cards.is_sorted(), "{name}: play cards not sorted");
            for c in &play.cards {
                *seen.entry(*c).or_default() += 1;
            }
        }
        assert!(seen.values().all(|&n| n <= 2), "{name}: a card 3 times");
    }
    assert!(lobbies >= 5);
}

fn trick_plays(entries: &[TrickEntry]) -> Vec<(SeatId, &Play)> {
    entries
        .iter()
        .filter_map(|e| match e {
            TrickEntry::Played { seat, play } => Some((*seat, play)),
            TrickEntry::Passed { .. } => None,
        })
        .collect()
}

fn cards_played_by(entries: &[TrickEntry], who: SeatId) -> Vec<Card> {
    trick_plays(entries)
        .into_iter()
        .filter(|(s, _)| *s == who)
        .flat_map(|(_, p)| p.cards.clone())
        .collect()
}

/// Every in-game `State` fixture, generically: the viewer's own count is
/// exact and matches its hand; other exact counts are ≤ 10; no card appears
/// more than twice across the viewer's hand and every visible play; task
/// options come from the viewer's hand; the seat to act hasn't gone out.
#[test]
fn visible_cards_are_consistent() {
    for (name, message) in fixtures() {
        let Message::Server(ServerMessage::State(s)) = message else {
            continue;
        };
        let RoomView::InGame(view) = &s.room else {
            continue;
        };
        let viewer = s.your_seat.index();

        let (hand, entries): (&Vec<Card>, Vec<&TrickEntry>) = match &view.phase {
            PhaseView::Tribute {
                your_hand,
                your_task,
                ..
            } => {
                if let Some(
                    TributeTask::PayTribute { options } | TributeTask::ReturnTribute { options },
                ) = your_task
                {
                    assert!(is_subset(options, your_hand), "{name}: options not in hand");
                }
                (your_hand, vec![])
            }
            PhaseView::Playing {
                your_hand,
                turn,
                trick,
                last_trick,
                finish_order,
                can_take_back,
                took_back,
                ..
            } => {
                assert!(!finish_order.contains(turn), "{name}: turn is out");
                // Only the seat that made the trick's latest action (a play or
                // a mid-trick pass) can take it back; after a take back it's
                // that seat's turn. A trick-ending pass can't be taken back.
                if *can_take_back {
                    let viewers = matches!(
                        trick.last(),
                        Some(TrickEntry::Played { seat, .. } | TrickEntry::Passed { seat })
                            if *seat == s.your_seat
                    );
                    assert!(
                        viewers,
                        "{name}: can_take_back but the latest action isn't the viewer's"
                    );
                }
                if let Some(taker) = took_back {
                    assert_eq!(turn, taker, "{name}: took_back but not their turn");
                }
                let mut all: Vec<&TrickEntry> = trick.iter().collect();
                if let Some(last) = last_trick {
                    all.extend(last.entries.iter());
                }
                (your_hand, all)
            }
        };

        assert!(hand.is_sorted(), "{name}: your_hand not in hand order");
        assert_eq!(
            view.card_counts[viewer],
            CardCount::Exact(u8::try_from(hand.len()).unwrap()),
            "{name}: viewer's count"
        );
        for (i, count) in view.card_counts.iter().enumerate() {
            if let CardCount::Exact(n) = count {
                assert!(i == viewer || *n <= 10, "{name}: seat {i} count {n}");
            }
        }

        let mut seen: BTreeMap<Card, usize> = BTreeMap::new();
        let mut add = |c: &Card| *seen.entry(*c).or_default() += 1;
        hand.iter().for_each(&mut add);
        for entry in entries {
            if let TrickEntry::Played { play, .. } = entry {
                assert!(play.cards.is_sorted(), "{name}: play cards not sorted");
                play.cards.iter().for_each(&mut add);
            }
        }
        for (c, n) in seen {
            assert!(n <= 2, "{name}: {c:?} appears {n} times");
        }
    }
}

/// The deal-2 story, strictly: the dealt hands are exactly the deck, every
/// later hand is a sub-multiset of the one before, every play came from the
/// player's own hand, and the tribute/return options follow the rules.
#[test]
fn deal2_story_is_consistent() {
    let d = dealt();
    let mut deck = full_deck();
    deck.sort();
    let mut all: Vec<Card> = d.iter().flatten().copied().collect();
    all.sort();
    assert_eq!(all, deck, "dealt hands must be the full deck");
    assert!(d.iter().all(|h| h.len() == 27));

    // Tribute: each payer's options (by the rules) and what they paid.
    assert_eq!(expected_tribute_options(&d[0], five()), cards(TRIBUTE_0));
    assert_eq!(
        expected_tribute_options(&d[2], five()),
        sorted(TRIBUTE_OPTIONS_2)
    );
    assert!(sorted(TRIBUTE_OPTIONS_2).contains(&card(TRIBUTE_2)));
    // Seat 0's tribute is higher than seat 2's, so seat 0 pays 1st place.
    assert!(
        face_value(card(TRIBUTE_0).face(), five()) > face_value(card(TRIBUTE_2).face(), five())
    );
    // No anti-tribute: the payers don't hold both Big Jokers.
    let big = card("BJ");
    assert!(d[0].iter().chain(&d[2]).filter(|&&c| c == big).count() < 2);

    // Returns.
    let t = after_tributes();
    assert_eq!(
        expected_return_options(&t[1], card(TRIBUTE_0), five()),
        sorted(RETURN_OPTIONS_1)
    );
    assert!(sorted(RETURN_OPTIONS_1).contains(&card(RETURN_1)));
    assert!(expected_return_options(&t[3], card(TRIBUTE_2), five()).contains(&card(RETURN_3)));

    // M1: hands and the plays on the table.
    let x = after_exchange();
    let m1 = hands(AT_PLAY);
    let mut m1_entries = m1_last_trick().entries;
    m1_entries.extend(m1_trick());
    for s in SeatId::ALL {
        let i = s.index();
        assert!(is_subset(&m1[i], &x[i]), "M1 seat {i} hand");
        let gone = minus(&x[i], &m1[i]);
        assert!(
            is_subset(&cards_played_by(&m1_entries, s), &gone),
            "M1 seat {i} plays"
        );
    }
    assert_eq!(sizes(&m1)[0], 8);
    assert_eq!(sizes(&m1)[2], 19);
    assert!(sizes(&m1)[1] > 10 && sizes(&m1)[3] > 10);

    // M2: seat 1 went out with the tube; seat 3 holds the ambiguous cards.
    let m2 = hands(AT_JIEFENG);
    for s in SeatId::ALL {
        let i = s.index();
        assert!(is_subset(&m2[i], &m1[i]), "M2 seat {i} hand");
        let gone = minus(&m1[i], &m2[i]);
        assert!(
            is_subset(&cards_played_by(&m2_last_trick().entries, s), &gone),
            "M2 seat {i} plays"
        );
    }
    assert!(m2[1].is_empty());
    assert_eq!(
        cards_played_by(&m2_last_trick().entries, seat(1)).len(),
        6,
        "the tube is seat 1's last play"
    );
    assert!(is_subset(&cards(AMBIGUOUS), &m2[3]));

    // M3: seat 3 bombed, then led its last card.
    let m3 = hands(AT_DEAL_END);
    for s in SeatId::ALL {
        let i = s.index();
        assert!(is_subset(&m3[i], &m2[i]), "M3 seat {i} hand");
    }
    assert_eq!(
        minus(&m2[3], &cards(AMBIGUOUS)),
        cards_played_by(&m3_final_trick(), seat(3))
    );
    assert!(m3[1].is_empty() && m3[3].is_empty());
}

/// `classified` is what the Room would answer to `client_classify` during
/// deal 2 (level Five): the full house and the wildcard bomb.
#[test]
fn classified_fixture_is_the_rules_answer() {
    let find = |wanted: &str| {
        fixtures()
            .into_iter()
            .find(|(name, _)| *name == wanted)
            .unwrap()
            .1
    };
    let Message::Client(ClientMessage::Classify { request_id, cards }) = find("client_classify")
    else {
        panic!("client_classify is not a Classify");
    };
    let Message::Server(ServerMessage::Classified {
        request_id: echoed,
        readings: answer,
    }) = find("classified")
    else {
        panic!("classified is not a Classified");
    };
    assert_eq!(echoed, request_id);
    assert_eq!(answer, readings(&cards, five()));
    assert!(
        answer
            .iter()
            .any(|play| matches!(play.combo, Combo::Bomb(_))),
        "the example should show a bomb reading"
    );
}
