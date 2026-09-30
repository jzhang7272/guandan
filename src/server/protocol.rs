//! The WebSocket wire format (TECH_SPEC.md §4). JSON text frames; top-level
//! messages are internally tagged with `"type"`. `rules/` types are reused
//! directly rather than mirrored.

use serde::{Deserialize, Serialize};

use crate::rules::{
    ActionError, Card, Combo, DealEnd, Level, MatchView, Play, Progress, SeatId, Team,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    Join {
        display_name: String,
        reconnect_token: Option<String>,
    },
    Play {
        cards: Vec<Card>,
        declared: Option<Combo>,
    },
    Pass,
    /// Take back your own play or pass while nobody has acted since
    /// (GAME_PAGE_V3_SPEC.md §1). Broadcast as a State, or `Rejected` with
    /// `NothingToTakeBack`.
    TakeBack,
    PayTribute {
        card: Card,
    },
    ReturnTribute {
        card: Card,
    },
    // Lobby only (§5.1):
    SetReady {
        ready: bool,
    },
    /// Move to an empty seat — this is how partners are picked. Only while
    /// seats are unlocked (no match in progress).
    ChooseSeat {
        seat: SeatId,
    },
    /// Anyone may edit the table settings (LOBBY_FLOW_SPEC.md §4.2); any
    /// change un-readies everyone. A-attempt counters aren't editable.
    UpdateSettings {
        team_levels: [Level; 2],
        declaring: Option<Team>,
    },
    /// Throw the match away: levels back to 2, seats unlocked.
    NewMatch,
    // In a deal only:
    /// Abandon this deal and go back to the lobby as it was before it.
    ResetDeal,
    /// What could these cards be played as? Read-only: the cards needn't be
    /// in your hand and it needn't be your turn (HAND_LAYOUT_SPEC.md §3.5).
    Classify {
        request_id: u32,
        cards: Vec<Card>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    Joined {
        seat: SeatId,
        session_token: String,
    },
    /// A newtype variant holding a struct: with internal tagging, serde puts
    /// `"type": "State"` alongside the struct's own fields. Boxed because it is
    /// far bigger than the other variants (clippy::large_enum_variant); `Box`
    /// is invisible on the wire.
    State(Box<RedactedState>),
    /// Reply to an ambiguous `Play`; the client resends with `declared`.
    ChooseReading {
        cards: Vec<Card>,
        options: Vec<Play>,
    },
    /// `message` = the error's `Display` text.
    Rejected {
        code: RejectCode,
        message: String,
    },
    /// Your seat was reclaimed by another connection; don't auto-reconnect.
    Kicked,
    /// Reply to `Classify`, to that connection only; `request_id` is echoed.
    /// Empty `readings` = not a legal combo, or no deal in progress.
    Classified {
        request_id: u32,
        readings: Vec<Play>,
    },
}

/// Both variants serialize as a bare string, e.g. `"NotYourTurn"`. The two
/// enums' variant names don't overlap, so deserializing is unambiguous.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RejectCode {
    Action(ActionError),
    Session(SessionError),
}

/// Server-level errors (sessions, lobby) — not game rules, so they live here
/// and never in `rules/`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, thiserror::Error)]
pub enum SessionError {
    #[error("Join the room first")]
    NotJoined,
    #[error("You have already joined")]
    AlreadyJoined,
    /// Empty after trimming, or longer than 20 chars.
    #[error("Names must be 1 to 20 characters")]
    InvalidName,
    /// A *connected* seat already uses this name.
    #[error("That name is already taken by a connected player")]
    NameTaken,
    #[error("All seats are taken")]
    NoSeatsAvailable,
    /// A game action (Play, Pass, …) or ResetDeal while in the lobby.
    #[error("There is no deal in progress")]
    NotInDeal,
    /// SetReady / ChooseSeat / UpdateSettings / NewMatch during a deal.
    #[error("That can only be done in the lobby")]
    NotInLobby,
    /// ChooseSeat to an occupied seat.
    #[error("That seat is taken")]
    SeatTaken,
    /// ChooseSeat between the deals of a match (tribute depends on who
    /// finished where, so seats can't change).
    #[error("Seats are locked during a match; start a new match to change seats")]
    SeatsLocked,
    /// UpdateSettings with no declaring team once a deal has been played.
    #[error("Once a deal has been played, a team must be declaring")]
    InvalidSettings,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactedState {
    pub your_seat: SeatId,
    pub seats: [SeatInfo; 4],
    pub room: RoomView,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatInfo {
    pub display_name: Option<String>,
    pub connected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoomView {
    Lobby(LobbyView),
    InGame(MatchView),
}

/// The lobby: before a match and between its deals (LOBBY_FLOW_SPEC.md §4.2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LobbyView {
    pub ready: [bool; 4],
    /// What the next deal starts from: `team_levels` and `declaring` are
    /// editable (UpdateSettings), `a_attempts` only shown.
    pub progress: Progress,
    /// True between the deals of a match: ChooseSeat is refused and
    /// `declaring` can't be None.
    pub seats_locked: bool,
    /// The most recent finished deal, if any. `summary.match_winner` set =
    /// that deal won the match (and `progress` is fresh again).
    pub last_deal: Option<DealEnd>,
}

#[cfg(test)]
mod fixture_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn seat(index: u8) -> SeatId {
        SeatId::new(index).unwrap()
    }

    #[test]
    fn session_error_messages_and_codes() {
        assert_eq!(SessionError::NotJoined.to_string(), "Join the room first");
        assert_eq!(
            serde_json::to_string(&SessionError::NameTaken).unwrap(),
            r#""NameTaken""#
        );
        let back: SessionError = serde_json::from_str(r#""SeatTaken""#).unwrap();
        assert_eq!(back, SessionError::SeatTaken);
    }

    #[test]
    fn reject_codes_are_bare_strings_both_ways() {
        let action = RejectCode::Action(ActionError::NotYourTurn);
        let session = RejectCode::Session(SessionError::NoSeatsAvailable);
        assert_eq!(serde_json::to_value(action).unwrap(), json!("NotYourTurn"));
        assert_eq!(
            serde_json::to_value(session).unwrap(),
            json!("NoSeatsAvailable")
        );

        let back: RejectCode = serde_json::from_str(r#""NotYourTurn""#).unwrap();
        assert_eq!(back, action);
        let back: RejectCode = serde_json::from_str(r#""NoSeatsAvailable""#).unwrap();
        assert_eq!(back, session);
    }

    #[test]
    fn rejected_matches_the_wire_example() {
        let error = ActionError::NotYourTurn;
        let message = ServerMessage::Rejected {
            code: RejectCode::Action(error),
            message: error.to_string(),
        };
        assert_eq!(
            serde_json::to_value(&message).unwrap(),
            json!({"type": "Rejected", "code": "NotYourTurn", "message": "It is not your turn"})
        );
    }

    #[test]
    fn state_puts_the_tag_next_to_the_struct_fields() {
        let empty_seat = SeatInfo {
            display_name: None,
            connected: false,
        };
        let message = ServerMessage::State(Box::new(RedactedState {
            your_seat: seat(0),
            seats: [
                SeatInfo {
                    display_name: Some("Josey".to_string()),
                    connected: true,
                },
                empty_seat.clone(),
                empty_seat.clone(),
                empty_seat,
            ],
            room: RoomView::Lobby(LobbyView {
                ready: [true, false, false, false],
                progress: Progress::default(),
                seats_locked: false,
                last_deal: None,
            }),
        }));

        let value = serde_json::to_value(&message).unwrap();
        assert_eq!(
            value,
            json!({
                "type": "State",
                "your_seat": 0,
                "seats": [
                    {"display_name": "Josey", "connected": true},
                    {"display_name": null, "connected": false},
                    {"display_name": null, "connected": false},
                    {"display_name": null, "connected": false}
                ],
                "room": {"Lobby": {
                    "ready": [true, false, false, false],
                    "progress": {
                        "team_levels": ["Two", "Two"],
                        "declaring": null,
                        "a_attempts": [0, 0]
                    },
                    "seats_locked": false,
                    "last_deal": null
                }}
            })
        );

        let back: ServerMessage = serde_json::from_value(value).unwrap();
        assert_eq!(back, message);
    }

    #[test]
    fn join_parses_from_the_wire_example() {
        let parsed: ClientMessage = serde_json::from_str(
            r#"{"type": "Join", "display_name": "Josey", "reconnect_token": null}"#,
        )
        .unwrap();
        assert_eq!(
            parsed,
            ClientMessage::Join {
                display_name: "Josey".to_string(),
                reconnect_token: None,
            }
        );
    }

    #[test]
    fn choose_seat_rejects_an_out_of_range_seat() {
        let result: Result<ClientMessage, _> =
            serde_json::from_str(r#"{"type": "ChooseSeat", "seat": 4}"#);
        assert!(result.is_err());
        let ok: Value = serde_json::to_value(ClientMessage::ChooseSeat { seat: seat(3) }).unwrap();
        assert_eq!(ok, json!({"type": "ChooseSeat", "seat": 3}));
    }

    #[test]
    fn fieldless_messages_are_just_the_tag() {
        assert_eq!(
            serde_json::to_value(ServerMessage::Kicked).unwrap(),
            json!({"type": "Kicked"})
        );
        let parsed: ClientMessage = serde_json::from_str(r#"{"type": "Pass"}"#).unwrap();
        assert_eq!(parsed, ClientMessage::Pass);
        let parsed: ClientMessage = serde_json::from_str(r#"{"type": "TakeBack"}"#).unwrap();
        assert_eq!(parsed, ClientMessage::TakeBack);
    }
}
