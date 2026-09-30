//! Per-seat views (TECH_SPEC.md §3.12): what one player is allowed to see.
//! Redaction is a rule (hand-size visibility, tribute reveal timing), so it
//! lives here rather than in the server.

use serde::{Deserialize, Serialize};

use super::card::{Card, SeatId, Team};
use super::deal::DealStart;
use super::match_::{GameState, Match};
use super::ranking::Level;
use super::tribute::TributePhase;
use super::trick::{CompletedTrick, TrickEntry};

/// Other seats: `Exact` only when ≤ 10; the viewer's own: always `Exact`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CardCount {
    Exact(u8),
    MoreThanTen,
}

/// Builds what `viewer` may see of `m`: their own hand, everyone's card
/// counts (redacted above 10), and the public parts of the current phase.
pub fn view_for(m: &Match, viewer: SeatId) -> MatchView {
    // Each phase keeps the hands somewhere different; take them along with
    // the phase view so the counts always match the hands shown.
    let (hands, phase) = match &m.state {
        GameState::Tribute(t) => (&t.hands, tribute_view(t, viewer)),
        GameState::Playing(p) => (
            &p.hands,
            PhaseView::Playing {
                your_hand: p.hands[viewer.index()].cards().to_vec(),
                level: p.level,
                turn: p.turn,
                trick: p.trick.entries.clone(),
                last_trick: p.last_trick.clone(),
                finish_order: p.finish_order.clone(),
                deal_start: p.deal_start.clone(),
                can_take_back: p.can_take_back(viewer),
                took_back: p.took_back,
            },
        ),
    };

    MatchView {
        team_levels: m.progress.team_levels,
        a_attempts: m.progress.a_attempts,
        declaring: m.progress.declaring,
        card_counts: SeatId::ALL.map(|seat| card_count(hands[seat.index()].len(), seat == viewer)),
        phase,
    }
}

/// GAME_RULES.md "hand-size visibility": others see an exact count only once
/// it is 10 or fewer; you always know your own.
fn card_count(len: usize, is_viewer: bool) -> CardCount {
    if is_viewer || len <= 10 {
        // A hand never exceeds 27 cards, so this always fits.
        CardCount::Exact(len as u8)
    } else {
        CardCount::MoreThanTen
    }
}

/// GAME_RULES.md "Order of operations": tribute cards are revealed together
/// once every tribute is paid (so no payer sees another's choice first), and
/// return cards stay hidden until all are in — at which point the phase is
/// already `Playing` and they show up in `deal_start`.
fn tribute_view(t: &TributePhase, viewer: SeatId) -> PhaseView {
    let all_paid = t.all_paid();

    let duties = t
        .duties
        .iter()
        .map(|d| DutyView {
            payer: d.duty.payer,
            receiver: d.duty.receiver,
            paid: d.tribute.is_some(),
            tribute: if all_paid { d.tribute } else { None },
            returned: d.returned.is_some(),
        })
        .collect();

    // Only ask the tribute rules for options when the viewer really has
    // something to do; everyone else gets `None`.
    let owes_tribute = t
        .duties
        .iter()
        .any(|d| d.duty.payer == viewer && d.tribute.is_none());
    let owes_return = all_paid
        && t.duties
            .iter()
            .any(|d| d.duty.receiver == viewer && d.returned.is_none());
    let your_task = if owes_tribute {
        Some(TributeTask::PayTribute {
            options: t.tribute_options(viewer),
        })
    } else if owes_return {
        Some(TributeTask::ReturnTribute {
            options: t.return_options(viewer),
        })
    } else {
        None
    };

    PhaseView::Tribute {
        your_hand: t.hands[viewer.index()].cards().to_vec(),
        level: t.level,
        duties,
        your_task,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchView {
    pub(crate) team_levels: [Level; 2],
    pub(crate) a_attempts: [u8; 2],
    pub(crate) declaring: Option<Team>,
    /// The viewer's own count is always `Exact`.
    pub(crate) card_counts: [CardCount; 4],
    pub(crate) phase: PhaseView,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhaseView {
    Tribute {
        your_hand: Vec<Card>,
        level: Level,
        duties: Vec<DutyView>,
        /// What the viewer must do now, if anything.
        your_task: Option<TributeTask>,
    },
    Playing {
        your_hand: Vec<Card>,
        level: Level,
        turn: SeatId,
        trick: Vec<TrickEntry>,
        /// Shown until the next trick ends.
        last_trick: Option<CompletedTrick>,
        finish_order: Vec<SeatId>,
        /// Public: tribute/return cards are shown to all.
        deal_start: DealStart,
        /// True only for the viewer who may take back their play or pass right now
        /// (GAME_RULES.md house rule #10), so the client needs no rule logic.
        can_take_back: bool,
        /// The seat whose play or pass was just taken back (the same for everyone),
        /// until the next play or pass.
        took_back: Option<SeatId>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DutyView {
    pub(crate) payer: SeatId,
    pub(crate) receiver: SeatId,
    pub(crate) paid: bool,
    /// `None` until ALL tributes are paid, then public.
    pub(crate) tribute: Option<Card>,
    /// The return card itself is hidden until all returns are in.
    pub(crate) returned: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TributeTask {
    /// `= tribute_options(viewer)`.
    PayTribute { options: Vec<Card> },
    /// `= return_options(viewer)`.
    ReturnTribute { options: Vec<Card> },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::card::{Rank, Suit};
    use crate::rules::combo::{Combo, Play};
    use crate::rules::hand::PlayerHand;
    use crate::rules::level::Progress;
    use crate::rules::play_phase::{PlayPhase, UndoPoint};
    use crate::rules::test_util::{cards, hand};
    use crate::rules::tribute::{Duty, DutyState};
    use crate::rules::trick::Trick;

    fn seat(index: u8) -> SeatId {
        SeatId::new(index).unwrap()
    }

    fn one(s: &str) -> Card {
        cards(s)[0]
    }

    /// `n` copies of one card: the view only ever looks at other hands'
    /// sizes, so their contents don't matter.
    fn filler(card: Card, n: usize) -> PlayerHand {
        PlayerHand::new(vec![card; n])
    }

    /// Only Hearts / Diamonds / Clubs — never Spades or Jokers, which the
    /// tests below keep for the viewer's hand and the public cards. So a
    /// serialized view containing "Heart", "Diamond" or "Club" has leaked
    /// another player's hand.
    fn hidden(n: usize) -> PlayerHand {
        let suits = [Suit::Heart, Suit::Diamond, Suit::Club];
        let hidden_cards = (0..n)
            .map(|i| Card::Standard {
                rank: Rank::ALL[i % 13],
                suit: suits[i % 3],
            })
            .collect();
        PlayerHand::new(hidden_cards)
    }

    fn assert_no_hidden_cards(view: &MatchView) {
        let json = serde_json::to_string(view).unwrap();
        for suit in ["Heart", "Diamond", "Club"] {
            assert!(!json.contains(suit), "{suit} leaked into {json}");
        }
    }

    fn progress() -> Progress {
        Progress {
            team_levels: [Level(Rank::Seven), Level(Rank::Ace)],
            declaring: Some(Team::B),
            a_attempts: [0, 2],
        }
    }

    fn joker_single(who: u8, s: &str) -> TrickEntry {
        let card = one(s);
        TrickEntry::Played {
            seat: seat(who),
            play: Play {
                cards: vec![card],
                combo: Combo::Single(card.face()),
                wildcard_as: vec![],
            },
        }
    }

    fn playing(hands: [PlayerHand; 4]) -> Match {
        Match {
            progress: progress(),
            state: GameState::Playing(PlayPhase {
                hands,
                level: Level(Rank::Ace),
                turn: seat(2),
                trick: Trick {
                    entries: vec![joker_single(1, "BJ")],
                    best: None,
                    passed: [false; 4],
                },
                last_trick: Some(CompletedTrick {
                    entries: vec![joker_single(3, "SJ"), TrickEntry::Passed { seat: seat(0) }],
                    winner: seat(3),
                    next_leader: seat(3),
                }),
                finish_order: vec![seat(3)],
                deal_start: DealStart::FirstDeal {
                    revealed: one("7S"),
                    leader: seat(0),
                },
                undo: None,
                took_back: None,
            }),
        }
    }

    // --- Playing -----------------------------------------------------------

    #[test]
    fn playing_shows_only_the_viewers_hand() {
        let viewer_hand = hand("2S 3S 4S 5S 6S 7S 8S 9S TS JS QS KS AS 2S 3S");
        let m = playing([hidden(11), hidden(10), viewer_hand.clone(), hidden(0)]);

        let view = view_for(&m, seat(2));

        let PhaseView::Playing { your_hand, .. } = &view.phase else {
            panic!("expected Playing, got {:?}", view.phase);
        };
        assert_eq!(your_hand, viewer_hand.cards());
        assert_no_hidden_cards(&view);
    }

    #[test]
    fn others_counts_are_exact_only_at_ten_or_fewer() {
        let m = playing([hidden(27), hidden(11), hidden(10), hidden(0)]);

        // Seat 3 has gone out, so from there every other count is someone
        // else's.
        let view = view_for(&m, seat(3));

        assert_eq!(
            view.card_counts,
            [
                CardCount::MoreThanTen,
                CardCount::MoreThanTen,
                CardCount::Exact(10),
                CardCount::Exact(0),
            ]
        );
    }

    #[test]
    fn viewers_own_count_is_exact_even_above_ten() {
        let m = playing([hidden(27), hidden(11), hidden(10), hidden(0)]);

        assert_eq!(view_for(&m, seat(0)).card_counts[0], CardCount::Exact(27));
        assert_eq!(view_for(&m, seat(1)).card_counts[1], CardCount::Exact(11));
        // ...while each stays redacted for the other.
        assert_eq!(view_for(&m, seat(1)).card_counts[0], CardCount::MoreThanTen);
        assert_eq!(view_for(&m, seat(0)).card_counts[1], CardCount::MoreThanTen);
    }

    #[test]
    fn playing_copies_the_public_fields_through() {
        let m = playing([hidden(12), hidden(12), hidden(12), hidden(0)]);
        let GameState::Playing(p) = &m.state else {
            unreachable!()
        };

        let view = view_for(&m, seat(0));

        assert_eq!(
            view.phase,
            PhaseView::Playing {
                your_hand: hidden(12).cards().to_vec(),
                level: p.level,
                turn: p.turn,
                trick: p.trick.entries.clone(),
                last_trick: p.last_trick.clone(),
                finish_order: p.finish_order.clone(),
                deal_start: p.deal_start.clone(),
                can_take_back: false,
                took_back: None,
            }
        );
    }

    /// The view's take-back fields for `viewer`.
    fn take_back_fields(m: &Match, viewer: u8) -> (bool, Option<SeatId>) {
        match view_for(m, seat(viewer)).phase {
            PhaseView::Playing {
                can_take_back,
                took_back,
                ..
            } => (can_take_back, took_back),
            other => panic!("expected Playing, got {other:?}"),
        }
    }

    #[test]
    fn only_the_player_who_just_played_can_take_back_but_everyone_sees_it_taken() {
        let leader = seat(1);
        let mut m = Match {
            progress: progress(),
            state: GameState::Playing(PlayPhase::new(
                ["3S 5S", "4S 6S", "7S 8S", "9S TS"].map(hand),
                Level(Rank::Ace),
                leader,
                DealStart::AntiTribute { leader },
            )),
        };
        let GameState::Playing(p) = &mut m.state else {
            unreachable!()
        };
        p.play(leader, &cards("4S"), None).unwrap();

        for viewer in 0..4 {
            let expected = seat(viewer) == leader;
            assert_eq!(
                take_back_fields(&m, viewer),
                (expected, None),
                "viewer {viewer}"
            );
        }

        let GameState::Playing(p) = &mut m.state else {
            unreachable!()
        };
        p.take_back(leader).unwrap();
        for viewer in 0..4 {
            assert_eq!(
                take_back_fields(&m, viewer),
                (false, Some(leader)),
                "viewer {viewer}"
            );
        }
    }

    /// A pass can be taken back too (house rule #10): only the passer is
    /// offered it, and the player whose play it followed no longer is.
    #[test]
    fn only_the_player_who_just_passed_can_take_back() {
        let leader = seat(1);
        let mut m = Match {
            progress: progress(),
            state: GameState::Playing(PlayPhase::new(
                ["3S 5S", "4S 6S", "7S 8S", "9S TS"].map(hand),
                Level(Rank::Ace),
                leader,
                DealStart::AntiTribute { leader },
            )),
        };
        let GameState::Playing(p) = &mut m.state else {
            unreachable!()
        };
        p.play(leader, &cards("6S"), None).unwrap();
        p.pass(seat(2)).unwrap();

        for viewer in 0..4 {
            assert_eq!(
                take_back_fields(&m, viewer),
                (viewer == 2, None),
                "viewer {viewer}"
            );
        }

        let GameState::Playing(p) = &mut m.state else {
            unreachable!()
        };
        p.take_back(seat(2)).unwrap();
        for viewer in 0..4 {
            assert_eq!(
                take_back_fields(&m, viewer),
                (false, Some(seat(2))),
                "viewer {viewer}"
            );
        }
    }

    #[test]
    fn progress_fields_come_from_the_match() {
        let m = playing([hidden(12), hidden(12), hidden(12), hidden(0)]);

        let view = view_for(&m, seat(0));

        assert_eq!(view.team_levels, progress().team_levels);
        assert_eq!(view.a_attempts, progress().a_attempts);
        assert_eq!(view.declaring, progress().declaring);
    }

    // --- Tribute -----------------------------------------------------------

    fn tribute(duties: Vec<DutyState>, hands: [PlayerHand; 4]) -> Match {
        Match {
            progress: progress(),
            state: GameState::Tribute(TributePhase {
                hands,
                level: Level(Rank::Ace),
                duties,
            }),
        }
    }

    fn duty(payer: u8, receiver: u8, tribute: Option<&str>, returned: Option<&str>) -> DutyState {
        DutyState {
            duty: Duty {
                payer: seat(payer),
                receiver: seat(receiver),
            },
            tribute: tribute.map(one),
            returned: returned.map(one),
        }
    }

    fn duty_view(
        payer: u8,
        receiver: u8,
        paid: bool,
        tribute: Option<&str>,
        returned: bool,
    ) -> DutyView {
        DutyView {
            payer: seat(payer),
            receiver: seat(receiver),
            paid,
            tribute: tribute.map(one),
            returned,
        }
    }

    #[test]
    fn a_paid_tribute_stays_hidden_until_all_are_paid() {
        // Double tribute: seat 0 has paid the Big Joker, seat 2 hasn't.
        let m = tribute(
            vec![duty(0, 1, Some("BJ"), None), duty(2, 3, None, None)],
            [hidden(26), hidden(28), hidden(27), hidden(27)],
        );

        // Seat 0 has paid and receives nothing, so it has no task.
        let view = view_for(&m, seat(0));

        let PhaseView::Tribute {
            your_hand,
            level,
            duties,
            your_task,
        } = &view.phase
        else {
            panic!("expected Tribute, got {:?}", view.phase);
        };
        assert_eq!(your_hand, hidden(26).cards());
        assert_eq!(*level, Level(Rank::Ace));
        assert_eq!(
            duties,
            &vec![
                duty_view(0, 1, true, None, false),
                duty_view(2, 3, false, None, false),
            ]
        );
        assert_eq!(your_task, &None);
        assert!(!serde_json::to_string(&view).unwrap().contains("Big"));
        assert_eq!(
            view.card_counts,
            [
                CardCount::Exact(26),
                CardCount::MoreThanTen,
                CardCount::MoreThanTen,
                CardCount::MoreThanTen,
            ]
        );
    }

    #[test]
    fn an_unpaid_payer_is_asked_to_pay_with_the_tribute_options() {
        // Level Ace: the Kings are this payer's highest cards, in two suits.
        let m = tribute(
            vec![duty(0, 1, Some("BJ"), None), duty(2, 3, None, None)],
            [
                hidden(26),
                hidden(28),
                hand("3C 5D QD KS KH KH"),
                hidden(27),
            ],
        );
        let GameState::Tribute(t) = &m.state else {
            unreachable!()
        };

        let PhaseView::Tribute { your_task, .. } = view_for(&m, seat(2)).phase else {
            panic!("expected Tribute");
        };

        assert_eq!(
            your_task,
            Some(TributeTask::PayTribute {
                options: t.tribute_options(seat(2)),
            })
        );
        assert_eq!(
            your_task,
            Some(TributeTask::PayTribute {
                options: cards("KS KH"),
            })
        );
    }

    #[test]
    fn a_receiver_is_asked_to_return_once_all_are_paid() {
        // All paid, so seat 1 now holds the Big Joker it received; seat 3
        // has already returned.
        let m = tribute(
            vec![
                duty(0, 1, Some("BJ"), None),
                duty(2, 3, Some("AS"), Some("2S")),
            ],
            [
                hidden(26),
                hand("3C 5D 5D JH KS BJ"),
                hidden(26),
                hidden(28),
            ],
        );
        let GameState::Tribute(t) = &m.state else {
            unreachable!()
        };

        let PhaseView::Tribute { your_task, .. } = view_for(&m, seat(1)).phase else {
            panic!("expected Tribute");
        };

        assert_eq!(
            your_task,
            Some(TributeTask::ReturnTribute {
                options: t.return_options(seat(1)),
            })
        );
        // Level Ace: only the cards 10 or below.
        assert_eq!(
            your_task,
            Some(TributeTask::ReturnTribute {
                options: cards("3C 5D"),
            })
        );
    }

    #[test]
    fn a_receiver_has_no_task_before_all_tributes_are_paid() {
        let m = tribute(
            vec![duty(0, 1, Some("BJ"), None), duty(2, 3, None, None)],
            [hidden(26), hidden(28), hidden(27), hidden(27)],
        );

        for receiver in [1, 3] {
            let PhaseView::Tribute { your_task, .. } = view_for(&m, seat(receiver)).phase else {
                panic!("expected Tribute");
            };
            assert_eq!(your_task, None, "receiver {receiver}");
        }
    }

    #[test]
    fn seats_outside_a_single_tribute_see_it_hidden_then_revealed() {
        let unpaid = tribute(
            vec![duty(0, 1, None, None)],
            [hidden(27), hidden(27), hidden(27), hidden(27)],
        );
        let paid = tribute(
            vec![duty(0, 1, Some("AS"), None)],
            [hidden(26), hidden(28), hidden(27), hidden(27)],
        );

        for bystander in [2, 3] {
            let PhaseView::Tribute {
                duties, your_task, ..
            } = view_for(&unpaid, seat(bystander)).phase
            else {
                panic!("expected Tribute");
            };
            assert_eq!(duties, vec![duty_view(0, 1, false, None, false)]);
            assert_eq!(your_task, None);

            let PhaseView::Tribute {
                duties, your_task, ..
            } = view_for(&paid, seat(bystander)).phase
            else {
                panic!("expected Tribute");
            };
            assert_eq!(duties, vec![duty_view(0, 1, true, Some("AS"), false)]);
            assert_eq!(your_task, None);
        }
    }

    #[test]
    fn tributes_are_public_once_all_paid_but_return_cards_stay_hidden() {
        // All paid; seat 3 has returned 2S (blind), seat 1 hasn't returned.
        let m = tribute(
            vec![
                duty(0, 1, Some("BJ"), None),
                duty(2, 3, Some("AS"), Some("2S")),
            ],
            [hidden(26), hidden(28), hidden(26), hidden(28)],
        );

        // Payers have nothing left to do and seat 3 has returned, so none of
        // these viewers has a task.
        for viewer in [0, 2, 3] {
            let view = view_for(&m, seat(viewer));
            let PhaseView::Tribute {
                duties, your_task, ..
            } = &view.phase
            else {
                panic!("expected Tribute");
            };
            assert_eq!(
                duties,
                &vec![
                    duty_view(0, 1, true, Some("BJ"), false),
                    duty_view(2, 3, true, Some("AS"), true),
                ],
                "viewer {viewer}"
            );
            assert_eq!(your_task, &None, "viewer {viewer}");
            // No hand here holds a Spade and the only public one is the
            // tributed AS, so a 2S in the view could only be the return.
            let json = serde_json::to_string(&view).unwrap();
            let returned = serde_json::to_string(&one("2S")).unwrap();
            assert!(!json.contains(&returned), "viewer {viewer}: {json}");
        }
    }

    // --- Wire fixtures -----------------------------------------------------

    /// Rebuilds a `Match` that should produce `expected` for `viewer`: the
    /// viewer's hand and every public field come from the view; the hidden
    /// parts (other hands, unrevealed tribute and return cards) are made-up
    /// cards of the right number, which the view must not show anyway.
    fn match_behind(expected: &MatchView, viewer: SeatId) -> Match {
        let placeholder = one("2H");
        let hands_for = |your_hand: &[Card]| {
            SeatId::ALL.map(|seat| match expected.card_counts[seat.index()] {
                _ if seat == viewer => PlayerHand::new(your_hand.to_vec()),
                CardCount::Exact(n) => filler(placeholder, n.into()),
                CardCount::MoreThanTen => filler(placeholder, 20),
            })
        };

        let state = match &expected.phase {
            PhaseView::Tribute {
                your_hand,
                level,
                duties,
                ..
            } => GameState::Tribute(TributePhase {
                hands: hands_for(your_hand),
                level: *level,
                duties: duties
                    .iter()
                    .map(|d| DutyState {
                        duty: Duty {
                            payer: d.payer,
                            receiver: d.receiver,
                        },
                        tribute: d.tribute.or(d.paid.then_some(placeholder)),
                        returned: d.returned.then_some(placeholder),
                    })
                    .collect(),
            }),
            PhaseView::Playing {
                your_hand,
                level,
                turn,
                trick,
                last_trick,
                finish_order,
                deal_start,
                can_take_back,
                took_back,
            } => GameState::Playing(PlayPhase {
                hands: hands_for(your_hand),
                level: *level,
                turn: *turn,
                trick: Trick {
                    entries: trick.clone(),
                    // Not part of the view.
                    best: None,
                    passed: [false; 4],
                },
                last_trick: last_trick.clone(),
                finish_order: finish_order.clone(),
                deal_start: deal_start.clone(),
                // Only the undoing seat shows in the view; what it would
                // restore is private, so any stand-in will do.
                undo: can_take_back.then(|| {
                    Box::new(UndoPoint {
                        seat: viewer,
                        hand: PlayerHand::new(your_hand.clone()),
                        trick: Trick::new(),
                        turn: viewer,
                        finish_order: vec![],
                        last_trick: None,
                    })
                }),
                took_back: *took_back,
            }),
        };

        Match {
            progress: Progress {
                team_levels: expected.team_levels,
                declaring: expected.declaring,
                a_attempts: expected.a_attempts,
            },
            state,
        }
    }

    /// `view_for` on the state behind a `fixtures/state_*.json` gives exactly
    /// that fixture's `room.InGame`, as JSON.
    fn check_fixture(json: &str) {
        let message: serde_json::Value = serde_json::from_str(json).unwrap();
        let expected_json = &message["room"]["InGame"];
        let expected: MatchView = serde_json::from_value(expected_json.clone()).unwrap();
        let viewer: SeatId = serde_json::from_value(message["your_seat"].clone()).unwrap();

        let view = view_for(&match_behind(&expected, viewer), viewer);

        assert_eq!(&serde_json::to_value(&view).unwrap(), expected_json);
    }

    #[test]
    fn playing_fixtures_match_view_for() {
        check_fixture(include_str!("../../fixtures/state_playing.json"));
        check_fixture(include_str!("../../fixtures/state_playing_your_turn.json"));
        check_fixture(include_str!(
            "../../fixtures/state_playing_lead_jiefeng.json"
        ));
        check_fixture(include_str!(
            "../../fixtures/state_playing_anti_tribute.json"
        ));
        check_fixture(include_str!("../../fixtures/state_playing_first_deal.json"));
        check_fixture(include_str!("../../fixtures/state_playing_all_bombs.json"));
        check_fixture(include_str!(
            "../../fixtures/state_playing_can_take_back.json"
        ));
        check_fixture(include_str!("../../fixtures/state_playing_took_back.json"));
    }

    #[test]
    fn tribute_fixtures_match_view_for() {
        // The options depend only on the viewer's own hand, which the fixture
        // gives in full, so the real rules compute them from real cards.
        check_fixture(include_str!("../../fixtures/state_tribute_waiting.json"));
        check_fixture(include_str!("../../fixtures/state_tribute_pay.json"));
        check_fixture(include_str!("../../fixtures/state_tribute.json"));
    }
}
