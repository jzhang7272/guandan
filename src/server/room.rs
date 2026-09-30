//! The Room actor (TECH_SPEC.md §5, §5.1, §6; LOBBY_FLOW_SPEC.md §4): a
//! single tokio task that owns all of one game's mutable state and processes
//! `RoomEvent`s one at a time. The `Registry` runs one Room per invite code.
//!
//! The flow: lobby → (all 4 ready) → one deal → lobby → … Every deal ends back
//! in the lobby, where the `Table` shows what the next deal starts from.
//! A room with nobody seated closes after a while (LOBBY_FLOW_SPEC.md §5.3).

use std::collections::HashMap;
use std::time::Duration;

use rand::Rng;
use rand::rngs::StdRng;
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::protocol::{
    ClientMessage, LobbyView, RedactedState, RejectCode, RoomView, SeatInfo, ServerMessage,
    SessionError,
};
use crate::rules::{
    Action, ActionError, ActionOutcome, Card, DealEnd, DealResult, Level, Match, Progress, SeatId,
    Team, deal_hands, readings, view_for,
};

/// Longest allowed display name, in characters, after trimming (§6).
const MAX_NAME_CHARS: usize = 20;

/// Assigned by `ws.rs` per accepted WebSocket (an `AtomicU64` counter).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ConnectionId(pub u64);

#[derive(Debug)]
pub enum RoomEvent {
    Connected {
        id: ConnectionId,
        outgoing: mpsc::UnboundedSender<ServerMessage>,
    },
    Disconnected {
        id: ConnectionId,
    },
    Message {
        id: ConnectionId,
        message: ClientMessage,
    },
}

pub struct Room {
    seats: [SeatSlot; 4],
    connections: HashMap<ConnectionId, mpsc::UnboundedSender<ServerMessage>>,
    /// Only after a successful `Join`. `SeatSlot.connection` and this map are
    /// two indexes of the same fact: change them only via `attach` / `detach`.
    connection_seat: HashMap<ConnectionId, SeatId>,
    /// The match between deals: what the next deal starts from.
    table: Table,
    phase: RoomPhase,
    /// When the room closes: armed whenever no seat has a live connection
    /// (including right after creation), cleared by any successful `Join`
    /// (LOBBY_FLOW_SPEC.md §5.3).
    close_at: Option<Instant>,
    rng: StdRng,
    timings: RoomTimings,
}

/// Injectable so tests can use short values (with `tokio::time::pause()`).
#[derive(Clone, Copy, Debug)]
pub struct RoomTimings {
    /// How long an empty room waits for its players while a match is in
    /// progress. prod: `ABANDON_AFTER_MINS` env var, default 360 (6 hours).
    pub abandon_after: Duration,
    /// How long an empty room with no match in progress stays open. prod:
    /// 10 minutes.
    pub idle_close_after: Duration,
}

/// The match progress *between* deals (LOBBY_FLOW_SPEC.md §4.1). The lobby
/// shows and edits it; a deal reads it when it starts and replaces it when
/// it ends. A reset deal leaves it untouched.
#[derive(Clone, Debug, Default)]
pub struct Table {
    /// Levels, declaring team and A attempts the next deal starts from.
    progress: Progress,
    /// `Some` → the next deal has tribute, so seats are locked.
    last_result: Option<DealResult>,
    /// The most recent finished deal, shown in the lobby.
    last_deal: Option<DealEnd>,
}

impl Table {
    /// Tribute depends on who finished where last deal, so once a deal of
    /// this match has been played nobody may change seats.
    fn seats_locked(&self) -> bool {
        self.last_result.is_some()
    }
}

#[derive(Debug)]
pub enum RoomPhase {
    Lobby(Lobby),
    /// Boxed because a `Match` is far bigger than a `Lobby` (clippy::large_enum_variant).
    InDeal(Box<Match>),
}

#[derive(Clone, Debug, Default)]
pub struct Lobby {
    ready: [bool; 4],
}

#[derive(Clone, Debug, Default)]
pub struct SeatSlot {
    display_name: Option<String>,
    session_token: Option<String>,
    /// `Some` = connected.
    connection: Option<ConnectionId>,
}

impl Room {
    /// `Registry::create`: a seed from the registry's rng, and its timings.
    pub fn new(rng: StdRng, timings: RoomTimings) -> Self {
        Room {
            seats: Default::default(),
            connections: HashMap::new(),
            connection_seat: HashMap::new(),
            table: Table::default(),
            phase: RoomPhase::Lobby(Lobby::default()),
            close_at: None,
            rng,
            timings,
        }
    }

    /// Processes events one at a time until the room closes: its close timer
    /// fires, or every sender is dropped. Returning drops the Room, and with
    /// it every connection's sender, so their ws tasks close their sockets.
    pub async fn run(mut self, mut events: mpsc::UnboundedReceiver<RoomEvent>) {
        // A new room has nobody seated yet.
        self.arm_close_timer_if_empty();
        loop {
            tokio::select! {
                event = events.recv() => match event {
                    Some(event) => self.handle(event),
                    None => break,
                },
                _ = sleep_until_opt(self.close_at) => {
                    tracing::info!("nobody came back: closing the room");
                    break;
                }
            }
        }
    }

    /// Test-only: a Room in the middle of deal `m`, which was started from
    /// `table`. Seats get these names and fresh tokens but no connections;
    /// `run` arms the close timer (with `abandon_after`) as it starts.
    #[cfg(test)]
    pub fn new_in_deal(
        rng: StdRng,
        timings: RoomTimings,
        names: [&str; 4],
        table: Table,
        m: Match,
    ) -> Self {
        let mut room = Room::new(rng, timings);
        for (seat, name) in SeatId::ALL.into_iter().zip(names) {
            let token = room.new_token();
            let slot = &mut room.seats[seat.index()];
            slot.display_name = Some(name.to_string());
            slot.session_token = Some(token);
        }
        room.table = table;
        room.phase = RoomPhase::InDeal(Box::new(m));
        room
    }

    fn handle(&mut self, event: RoomEvent) {
        match event {
            RoomEvent::Connected { id, outgoing } => {
                self.connections.insert(id, outgoing);
            }
            RoomEvent::Disconnected { id } => self.on_disconnected(id),
            RoomEvent::Message { id, message } => self.on_message(id, message),
        }
    }

    fn on_message(&mut self, id: ConnectionId, message: ClientMessage) {
        // A connection we no longer know (e.g. one just kicked, whose socket
        // hasn't closed yet) has nowhere to send a reply to: ignore it.
        if !self.connections.contains_key(&id) {
            return;
        }
        let Some(seat) = self.connection_seat.get(&id).copied() else {
            match message {
                ClientMessage::Join {
                    display_name,
                    reconnect_token,
                } => self.join(id, &display_name, reconnect_token.as_deref()),
                _ => self.reject(id, SessionError::NotJoined),
            }
            return;
        };

        match message {
            ClientMessage::Join { .. } => self.reject(id, SessionError::AlreadyJoined),
            ClientMessage::SetReady { ready } => self.set_ready(id, seat, ready),
            ClientMessage::ChooseSeat { seat: target } => self.choose_seat(id, seat, target),
            ClientMessage::UpdateSettings {
                team_levels,
                declaring,
            } => self.update_settings(id, team_levels, declaring),
            ClientMessage::NewMatch => self.new_match(id),
            ClientMessage::ResetDeal => self.reset_deal(id),
            ClientMessage::Classify { request_id, cards } => self.classify(id, request_id, &cards),
            ClientMessage::Play { .. }
            | ClientMessage::Pass
            | ClientMessage::TakeBack
            | ClientMessage::PayTribute { .. }
            | ClientMessage::ReturnTribute { .. } => self.game_action(id, seat, message),
        }
    }

    /// A match is worth protecting from disconnects while a deal is on, and
    /// between its deals (LOBBY_FLOW_SPEC.md §4.1).
    fn match_in_progress(&self) -> bool {
        matches!(self.phase, RoomPhase::InDeal(_)) || self.table.seats_locked()
    }

    // ---- Sessions (§6) ----

    fn join(&mut self, id: ConnectionId, display_name: &str, reconnect_token: Option<&str>) {
        let name = display_name.trim();
        if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
            self.reject(id, SessionError::InvalidName);
            return;
        }

        // The steps of §6, in order; the first that applies wins. An unknown
        // or stale token is not an error: it just falls through.
        let token_seat = reconnect_token.and_then(|token| self.seat_with_token(token));
        let seat = if let Some(seat) = token_seat {
            // 1. Token match: take the new name unless another seat has it.
            if self.seat_named(name).is_none_or(|holder| holder == seat) {
                self.seats[seat.index()].display_name = Some(name.to_string());
            }
            self.reclaim(seat, id);
            seat
        } else if let Some(seat) = self.seat_named(name) {
            if self.seats[seat.index()].connection.is_some() {
                // 3. A live player can only be displaced by their own token.
                self.reject(id, SessionError::NameTaken);
                return;
            }
            // 2. Name match on a disconnected seat: a new token, so a device
            // still holding the old one can't take the seat back.
            self.seats[seat.index()].session_token = Some(self.new_token());
            self.reclaim(seat, id);
            seat
        } else {
            // 4. A new seat: the lowest one nobody holds.
            let free = SeatId::ALL
                .into_iter()
                .find(|seat| self.seats[seat.index()].session_token.is_none());
            let Some(seat) = free else {
                self.reject(id, SessionError::NoSeatsAvailable);
                return;
            };
            let token = self.new_token();
            let slot = &mut self.seats[seat.index()];
            slot.display_name = Some(name.to_string());
            slot.session_token = Some(token);
            self.attach(seat, id);
            seat
        };

        let session_token = self.seats[seat.index()]
            .session_token
            .clone()
            .expect("a joined seat always has a token");
        self.send(
            id,
            ServerMessage::Joined {
                seat,
                session_token,
            },
        );

        // Joining changes the lobby's membership, so this player isn't ready
        // yet (others keep their flags). It can't start the match either.
        if let RoomPhase::Lobby(lobby) = &mut self.phase {
            lobby.ready[seat.index()] = false;
        }
        self.close_at = None;
        self.broadcast();
    }

    /// Gives `seat` to connection `id`, kicking any other live connection
    /// that holds it.
    fn reclaim(&mut self, seat: SeatId, id: ConnectionId) {
        if let Some(old) = self.seats[seat.index()].connection {
            self.send(old, ServerMessage::Kicked);
            self.detach(old);
            // Dropping the sender makes the old ws task close its socket; its
            // later `Disconnected` finds no entry and is ignored.
            self.connections.remove(&old);
        }
        self.attach(seat, id);
    }

    fn on_disconnected(&mut self, id: ConnectionId) {
        self.connections.remove(&id);
        let Some(seat) = self.detach(id) else {
            // Never joined (or already kicked): nothing else to do.
            return;
        };

        if let RoomPhase::Lobby(lobby) = &mut self.phase {
            // An absent player is never ready, so they can't hold up (or
            // trigger) the start by accident.
            lobby.ready[seat.index()] = false;
            if !self.table.seats_locked() {
                // No match to protect: free the seat so an absent player
                // can't block the start (§5.1).
                self.seats[seat.index()] = SeatSlot::default();
            }
        }
        // Otherwise (mid-deal, or between the deals of a match) keep name and
        // token so the player can come back and reclaim the seat (§6).

        if self.connection_seat.is_empty() {
            // Everyone has gone: nobody is left to tell.
            self.arm_close_timer_if_empty();
        } else {
            self.broadcast();
        }
    }

    /// With no seated connection, the room closes after a while (§5.3):
    /// `abandon_after` if there's a match to come back to, else
    /// `idle_close_after`. An already armed timer is left alone.
    fn arm_close_timer_if_empty(&mut self) {
        if !self.connection_seat.is_empty() || self.close_at.is_some() {
            return;
        }
        let wait = if self.match_in_progress() {
            self.timings.abandon_after
        } else {
            self.timings.idle_close_after
        };
        self.close_at = Some(Instant::now() + wait);
    }

    /// The seat whose token is `token`, if any.
    fn seat_with_token(&self, token: &str) -> Option<SeatId> {
        SeatId::ALL
            .into_iter()
            .find(|seat| self.seats[seat.index()].session_token.as_deref() == Some(token))
    }

    /// The seat holding `name`, compared case-insensitively (`name` is
    /// already trimmed, and stored names are too).
    fn seat_named(&self, name: &str) -> Option<SeatId> {
        let wanted = name.to_lowercase();
        SeatId::ALL.into_iter().find(|seat| {
            self.seats[seat.index()]
                .display_name
                .as_ref()
                .is_some_and(|held| held.to_lowercase() == wanted)
        })
    }

    /// 16 random bytes from the Room's rng, hex-encoded.
    fn new_token(&mut self) -> String {
        let bytes: [u8; 16] = self.rng.random();
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// `SeatSlot.connection` and `connection_seat` are two indexes of the same
    /// fact; these two helpers are the only code that changes either.
    fn attach(&mut self, seat: SeatId, id: ConnectionId) {
        debug_assert!(self.seats[seat.index()].connection.is_none());
        self.seats[seat.index()].connection = Some(id);
        self.connection_seat.insert(id, seat);
    }

    /// Unlinks `id` from its seat, returning the seat it held (if any).
    fn detach(&mut self, id: ConnectionId) -> Option<SeatId> {
        let seat = self.connection_seat.remove(&id)?;
        self.seats[seat.index()].connection = None;
        Some(seat)
    }

    /// Frees every seat with no live connection. Used whenever the lobby is
    /// (or becomes) unlocked: lobby seats only exist while connected (§5.1).
    fn free_disconnected_seats(&mut self) {
        for slot in &mut self.seats {
            if slot.connection.is_none() {
                *slot = SeatSlot::default();
            }
        }
    }

    // ---- Lobby (§5.1, LOBBY_FLOW_SPEC.md §4.3) ----

    fn set_ready(&mut self, id: ConnectionId, seat: SeatId, ready: bool) {
        let RoomPhase::Lobby(lobby) = &mut self.phase else {
            self.reject(id, SessionError::NotInLobby);
            return;
        };
        lobby.ready[seat.index()] = ready;
        self.maybe_start();
        self.broadcast();
    }

    fn choose_seat(&mut self, id: ConnectionId, from: SeatId, target: SeatId) {
        if !matches!(self.phase, RoomPhase::Lobby(_)) {
            self.reject(id, SessionError::NotInLobby);
            return;
        }
        if self.table.seats_locked() {
            self.reject(id, SessionError::SeatsLocked);
            return;
        }
        // "Occupied" = the seat has a token, which includes your own seat.
        if self.seats[target.index()].session_token.is_some() {
            self.reject(id, SessionError::SeatTaken);
            return;
        }

        // Move the whole slot (name, token); the connection goes through
        // detach/attach so the two indexes stay in step.
        self.detach(id);
        let slot = std::mem::take(&mut self.seats[from.index()]);
        self.seats[target.index()] = slot;
        self.attach(target, id);

        if let RoomPhase::Lobby(lobby) = &mut self.phase {
            lobby.ready[from.index()] = false;
            lobby.ready[target.index()] = false;
        }
        self.maybe_start();
        self.broadcast();
    }

    /// Anyone may change the levels and the declaring team (GAME_RULES.md
    /// house rule #9). Any change un-readies everyone, so nobody is dealt in
    /// under settings they didn't see.
    fn update_settings(
        &mut self,
        id: ConnectionId,
        team_levels: [Level; 2],
        declaring: Option<Team>,
    ) {
        let RoomPhase::Lobby(lobby) = &mut self.phase else {
            self.reject(id, SessionError::NotInLobby);
            return;
        };
        // After a deal there is always a winner, and the next deal's tribute
        // needs a declaring team.
        if declaring.is_none() && self.table.seats_locked() {
            self.reject(id, SessionError::InvalidSettings);
            return;
        }
        self.table.progress = self.table.progress.with_settings(team_levels, declaring);
        lobby.ready = [false; 4];
        self.broadcast();
    }

    /// Throws the match away: a fresh table (levels back to Two, seats
    /// unlocked). Harmless before a match, so it's allowed any time in the
    /// lobby.
    fn new_match(&mut self, id: ConnectionId) {
        let RoomPhase::Lobby(lobby) = &mut self.phase else {
            self.reject(id, SessionError::NotInLobby);
            return;
        };
        lobby.ready = [false; 4];
        self.table = Table::default();
        self.free_disconnected_seats();
        self.broadcast();
    }

    /// Starts a deal once all 4 seats are filled and ready.
    fn maybe_start(&mut self) {
        let RoomPhase::Lobby(lobby) = &self.phase else {
            return;
        };
        let all_seated = self.seats.iter().all(|slot| slot.session_token.is_some());
        if all_seated && lobby.ready.iter().all(|&ready| ready) {
            tracing::info!("all 4 players are ready: starting a deal");
            let hands = deal_hands(&mut self.rng);
            let m = Match::from_deal(
                self.table.progress.clone(),
                self.table.last_result.clone(),
                hands,
                &mut self.rng,
            );
            self.phase = RoomPhase::InDeal(Box::new(m));
        }
    }

    // ---- Deals ----

    fn game_action(&mut self, id: ConnectionId, seat: SeatId, message: ClientMessage) {
        let RoomPhase::InDeal(m) = &mut self.phase else {
            self.reject(id, SessionError::NotInDeal);
            return;
        };
        // Kept for `ChooseReading`, which echoes the cards back to the player.
        let mut played = Vec::new();
        let action = match message {
            ClientMessage::Play { cards, declared } => {
                played = cards.clone();
                Action::Play { cards, declared }
            }
            ClientMessage::Pass => Action::Pass,
            ClientMessage::TakeBack => Action::TakeBack,
            ClientMessage::PayTribute { card } => Action::PayTribute { card },
            ClientMessage::ReturnTribute { card } => Action::ReturnTribute { card },
            ClientMessage::Join { .. }
            | ClientMessage::SetReady { .. }
            | ClientMessage::ChooseSeat { .. }
            | ClientMessage::UpdateSettings { .. }
            | ClientMessage::NewMatch
            | ClientMessage::ResetDeal
            | ClientMessage::Classify { .. } => {
                unreachable!("on_message only passes game actions here")
            }
        };

        match m.apply(seat, action) {
            Ok(ActionOutcome::Applied) => self.broadcast(),
            Ok(ActionOutcome::NeedsDeclaration { options }) => {
                // Card lists on the wire are in hand order (a display order,
                // not game strength).
                played.sort();
                self.send(
                    id,
                    ServerMessage::ChooseReading {
                        cards: played,
                        options,
                    },
                );
            }
            Ok(ActionOutcome::DealOver(end)) => self.end_deal(end),
            Err(error) => self.reject_action(id, error),
        }
    }

    /// Every deal ends in the lobby, with the table updated for the next one
    /// (nobody ready yet).
    fn end_deal(&mut self, end: DealEnd) {
        if end.summary.match_winner.is_some() {
            // A fresh table, but the lobby still shows the winning deal. The
            // seats unlock, so the absent ones are freed.
            tracing::info!("the match is over");
            self.table = Table {
                progress: Progress::default(),
                last_result: None,
                last_deal: Some(end),
            };
            self.phase = RoomPhase::Lobby(Lobby::default());
            self.free_disconnected_seats();
        } else {
            self.table = Table {
                progress: end.summary.after.clone(),
                last_result: Some(end.summary.result.clone()),
                last_deal: Some(end),
            };
            self.phase = RoomPhase::Lobby(Lobby::default());
        }
        self.broadcast();
    }

    /// Abandons the deal: back to the lobby as it was before the deal, with
    /// the table untouched, so re-dealing uses the same tribute payers and
    /// receivers.
    fn reset_deal(&mut self, id: ConnectionId) {
        if !matches!(self.phase, RoomPhase::InDeal(_)) {
            self.reject(id, SessionError::NotInDeal);
            return;
        }
        tracing::info!("the deal was reset");
        self.phase = RoomPhase::Lobby(Lobby::default());
        // Resetting a match's first deal lands in an unlocked lobby, where
        // seats only exist while connected (§5.1).
        if !self.table.seats_locked() {
            self.free_disconnected_seats();
        }
        self.broadcast();
    }

    /// Answers only the asker with every reading of `cards` at the current
    /// deal's level (HAND_LAYOUT_SPEC.md §3.5). Read-only: the cards aren't
    /// checked against the hand, turn order doesn't matter, and nothing is
    /// broadcast. In the lobby there is no level to read at, so the answer is
    /// empty.
    fn classify(&self, id: ConnectionId, request_id: u32, cards: &[Card]) {
        let found = match &self.phase {
            RoomPhase::Lobby(_) => Vec::new(),
            RoomPhase::InDeal(m) => readings(cards, m.deal_level()),
        };
        self.send(
            id,
            ServerMessage::Classified {
                request_id,
                readings: found,
            },
        );
    }

    // ---- Sending ----

    /// Sends to one connection. A failed send means its ws task has already
    /// exited; its `Disconnected` event will clean up, so ignore the error.
    fn send(&self, id: ConnectionId, message: ServerMessage) {
        if let Some(outgoing) = self.connections.get(&id) {
            let _ = outgoing.send(message);
        }
    }

    /// A game rule refused the action.
    fn reject_action(&self, id: ConnectionId, error: ActionError) {
        self.send(
            id,
            ServerMessage::Rejected {
                code: RejectCode::Action(error),
                message: error.to_string(),
            },
        );
    }

    fn reject(&self, id: ConnectionId, error: SessionError) {
        self.send(
            id,
            ServerMessage::Rejected {
                code: RejectCode::Session(error),
                message: error.to_string(),
            },
        );
    }

    /// Sends every joined connection its own full `State`. Connections that
    /// haven't joined receive nothing.
    fn broadcast(&self) {
        for (&id, &seat) in &self.connection_seat {
            self.send(id, self.state_for(seat));
        }
    }

    fn state_for(&self, your_seat: SeatId) -> ServerMessage {
        let seats = self.seats.clone().map(|slot| SeatInfo {
            display_name: slot.display_name,
            connected: slot.connection.is_some(),
        });
        let room = match &self.phase {
            RoomPhase::Lobby(lobby) => RoomView::Lobby(LobbyView {
                ready: lobby.ready,
                progress: self.table.progress.clone(),
                seats_locked: self.table.seats_locked(),
                last_deal: self.table.last_deal.clone(),
            }),
            RoomPhase::InDeal(m) => RoomView::InGame(view_for(m, your_seat)),
        };
        ServerMessage::State(Box::new(RedactedState {
            your_seat,
            seats,
            room,
        }))
    }
}

/// An unset timer is a future that never fires (see TECH_SPEC.md §5 for why
/// this beats a `select!` precondition).
async fn sleep_until_opt(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::test_util::{cards, hand};
    use crate::rules::{
        Bomb, Card, CardCount, Combo, DealStart, GameState, MatchView, PhaseView, Play, Rank,
        TrickEntry, full_deck,
    };
    use rand::SeedableRng;
    use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
    use tokio::task::JoinHandle;

    fn seat(index: u8) -> SeatId {
        SeatId::new(index).unwrap()
    }

    fn timings() -> RoomTimings {
        RoomTimings {
            abandon_after: Duration::from_secs(60),
            idle_close_after: Duration::from_secs(10),
        }
    }

    /// Lets the Room process everything queued. Tests run with the clock
    /// paused, and a paused clock only jumps forward once every task is idle,
    /// so this returns after the Room is blocked waiting for its next event.
    async fn settle() {
        tokio::time::sleep(Duration::from_millis(1)).await;
    }

    /// A fake WebSocket connection: its id and the receiving end of the
    /// channel the Room sends to.
    struct Client {
        id: ConnectionId,
        rx: UnboundedReceiver<ServerMessage>,
        /// The latest `State` seen by `drain`.
        latest_state: Option<RedactedState>,
    }

    impl Client {
        /// Everything received since the last drain.
        fn drain(&mut self) -> Vec<ServerMessage> {
            let mut messages = Vec::new();
            while let Ok(message) = self.rx.try_recv() {
                if let ServerMessage::State(state) = &message {
                    self.latest_state = Some((**state).clone());
                }
                messages.push(message);
            }
            messages
        }

        /// The latest `State` received so far (drains the rest).
        fn last_state(&mut self) -> RedactedState {
            self.drain();
            self.latest_state.clone().expect("expected a State message")
        }
    }

    /// A running Room plus the sending end of its event channel.
    struct Harness {
        events: UnboundedSender<RoomEvent>,
        room: JoinHandle<()>,
        next_id: u64,
    }

    impl Harness {
        fn start() -> Self {
            Self::start_with(Room::new(StdRng::seed_from_u64(7), timings()))
        }

        fn start_with(room: Room) -> Self {
            let (events, rx) = mpsc::unbounded_channel();
            Harness {
                events,
                room: tokio::spawn(room.run(rx)),
                next_id: 0,
            }
        }

        fn connect(&mut self) -> Client {
            self.next_id += 1;
            let id = ConnectionId(self.next_id);
            let (outgoing, rx) = mpsc::unbounded_channel::<ServerMessage>();
            self.events
                .send(RoomEvent::Connected { id, outgoing })
                .unwrap();
            Client {
                id,
                rx,
                latest_state: None,
            }
        }

        fn send(&self, client: &Client, message: ClientMessage) {
            let id = client.id;
            self.events
                .send(RoomEvent::Message { id, message })
                .unwrap();
        }

        fn disconnect(&self, client: &Client) {
            let id = client.id;
            self.events.send(RoomEvent::Disconnected { id }).unwrap();
        }

        /// Sends `message` from `client`, lets the Room handle it, and returns
        /// what `client` received.
        async fn request(&self, client: &mut Client, message: ClientMessage) -> Vec<ServerMessage> {
            self.send(client, message);
            settle().await;
            client.drain()
        }

        /// Sends `Join` on `client`, checks for `Joined` + `State`, and returns
        /// the seat and token. Leaves that `State` unread.
        async fn join_as(
            &self,
            client: &mut Client,
            name: &str,
            token: Option<&str>,
        ) -> (SeatId, String) {
            let replies = self.request(client, join_message(name, token)).await;
            let [
                ServerMessage::Joined {
                    seat,
                    session_token,
                },
                ServerMessage::State(state),
            ] = replies.as_slice()
            else {
                panic!("expected Joined + State, got {replies:?}");
            };
            assert_eq!(state.your_seat, *seat);
            (*seat, session_token.clone())
        }

        /// A new connection that joins as `name`.
        async fn join(&mut self, name: &str) -> (Client, SeatId, String) {
            let mut client = self.connect();
            let (seat, token) = self.join_as(&mut client, name, None).await;
            (client, seat, token)
        }

        /// Ends the Room (by dropping the sender) and checks it didn't panic.
        async fn finish(self) {
            drop(self.events);
            self.room.await.expect("the Room task panicked");
        }

        /// Whether `Room::run` has returned (the room closed by itself: the
        /// Harness still holds a sender).
        async fn is_closed(&self) -> bool {
            settle().await;
            self.room.is_finished()
        }
    }

    fn join_message(name: &str, token: Option<&str>) -> ClientMessage {
        ClientMessage::Join {
            display_name: name.to_string(),
            reconnect_token: token.map(str::to_string),
        }
    }

    fn ready(value: bool) -> ClientMessage {
        ClientMessage::SetReady { ready: value }
    }

    fn rejected(code: SessionError) -> ServerMessage {
        ServerMessage::Rejected {
            code: RejectCode::Session(code),
            message: code.to_string(),
        }
    }

    fn lobby(state: &RedactedState) -> &LobbyView {
        match &state.room {
            RoomView::Lobby(lobby) => lobby,
            other => panic!("expected the lobby, got {other:?}"),
        }
    }

    fn lobby_ready(state: &RedactedState) -> [bool; 4] {
        lobby(state).ready
    }

    /// The lobby of a room with no match going: levels 2/2, nobody
    /// declaring, seats unlocked, no previous deal.
    fn fresh_lobby(ready: [bool; 4]) -> RoomView {
        RoomView::Lobby(LobbyView {
            ready,
            progress: Progress::default(),
            seats_locked: false,
            last_deal: None,
        })
    }

    fn update_settings(levels: [Rank; 2], declaring: Option<Team>) -> ClientMessage {
        ClientMessage::UpdateSettings {
            team_levels: levels.map(Level),
            declaring,
        }
    }

    fn names(state: &RedactedState) -> [Option<&str>; 4] {
        [0, 1, 2, 3].map(|i| state.seats[i].display_name.as_deref())
    }

    #[tokio::test(start_paused = true)]
    async fn four_joins_fill_seats_in_order_and_stay_in_the_lobby() {
        let mut h = Harness::start();
        let mut clients = Vec::new();
        let mut tokens = Vec::new();
        for (i, name) in ["Josey", "Alex", "Sam", "Robin"].into_iter().enumerate() {
            let (client, joined_seat, token) = h.join(name).await;
            assert_eq!(joined_seat, seat(i as u8));
            assert_eq!(token.len(), 32);
            assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(!tokens.contains(&token));
            tokens.push(token);
            clients.push(client);
        }

        for (i, client) in clients.iter_mut().enumerate() {
            let state = client.last_state();
            assert_eq!(state.your_seat, seat(i as u8));
            assert_eq!(
                names(&state),
                [Some("Josey"), Some("Alex"), Some("Sam"), Some("Robin")]
            );
            assert!(state.seats.iter().all(|info| info.connected));
            assert_eq!(state.room, fresh_lobby([false; 4]));
        }
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn four_set_ready_starts_the_first_deal() {
        let mut h = Harness::start();
        let mut clients = Vec::new();
        for name in ["A", "B", "C", "D"] {
            clients.push(h.join(name).await.0);
        }
        for client in &clients[..3] {
            h.send(client, ready(true));
        }
        settle().await;
        for client in &mut clients {
            assert_eq!(lobby_ready(&client.last_state()), [true, true, true, false]);
        }

        // The last ready click starts the match.
        h.send(&clients[3], ready(true));
        settle().await;
        let mut hands = Vec::new();
        for (i, client) in clients.iter_mut().enumerate() {
            let state = client.last_state();
            assert_eq!(state.your_seat, seat(i as u8));
            let view = match_view(&state);
            assert_eq!(view.card_counts[i], CardCount::Exact(27));
            let PhaseView::Playing {
                your_hand,
                level,
                deal_start,
                ..
            } = &view.phase
            else {
                panic!("expected Playing, got {:?}", view.phase);
            };
            assert_eq!(your_hand.len(), 27);
            assert_eq!(*level, Level(Rank::Two));
            assert!(matches!(deal_start, DealStart::FirstDeal { .. }));
            hands.extend(your_hand.iter().copied());
        }
        // Four different hands that together make up both decks.
        hands.sort();
        let mut deck = full_deck();
        deck.sort();
        assert_eq!(hands, deck);
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn set_ready_is_broadcast_to_every_seat() {
        let mut h = Harness::start();
        let (mut a, _, _) = h.join("A").await;
        let (mut b, _, _) = h.join("B").await;
        h.send(&b, ready(true));
        settle().await;
        assert_eq!(lobby_ready(&a.last_state()), [false, true, false, false]);
        assert_eq!(lobby_ready(&b.last_state()), [false, true, false, false]);
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn choose_seat_moves_to_an_empty_seat_and_clears_only_that_players_ready() {
        let mut h = Harness::start();
        let (mut a, _, token_a) = h.join("A").await;
        let (mut b, _, _) = h.join("B").await;
        h.send(&a, ready(true));
        h.send(&b, ready(true));
        h.send(&a, ClientMessage::ChooseSeat { seat: seat(3) });
        settle().await;

        let state = a.last_state();
        assert_eq!(state.your_seat, seat(3));
        assert_eq!(names(&state), [None, Some("B"), None, Some("A")]);
        assert!(!state.seats[0].connected);
        assert!(state.seats[3].connected);
        assert_eq!(lobby_ready(&state), [false, true, false, false]);
        assert_eq!(b.last_state().your_seat, seat(1));

        // The token moved with the seat: reclaiming with it lands on seat 3.
        let mut a2 = h.connect();
        let reclaimed = h.join_as(&mut a2, "A", Some(&token_a)).await;
        assert_eq!(reclaimed, (seat(3), token_a));

        // And seat 0 is really free again.
        let (_c, c_seat, _) = h.join("C").await;
        assert_eq!(c_seat, seat(0));
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn choose_seat_to_an_occupied_seat_is_rejected() {
        let mut h = Harness::start();
        let (mut a, _, _) = h.join("A").await;
        let (_b, _, _) = h.join("B").await;
        a.drain();
        let replies = h
            .request(&mut a, ClientMessage::ChooseSeat { seat: seat(1) })
            .await;
        assert_eq!(replies, vec![rejected(SessionError::SeatTaken)]);
        // Your own seat counts as occupied too.
        let replies = h
            .request(&mut a, ClientMessage::ChooseSeat { seat: seat(0) })
            .await;
        assert_eq!(replies, vec![rejected(SessionError::SeatTaken)]);
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn disconnect_in_the_lobby_frees_the_seat_and_keeps_others_ready() {
        let mut h = Harness::start();
        let (mut a, _, _) = h.join("A").await;
        let (b, _, _) = h.join("B").await;
        let (c, _, _) = h.join("C").await;
        for client in [&a, &b, &c] {
            h.send(client, ready(true));
        }
        h.disconnect(&b);
        settle().await;

        let state = a.last_state();
        assert_eq!(names(&state), [Some("A"), None, Some("C"), None]);
        assert!(!state.seats[1].connected);
        assert_eq!(lobby_ready(&state), [true, false, true, false]);

        // B's name is free again.
        let (_b2, b2_seat, _) = h.join("b").await;
        assert_eq!(b2_seat, seat(1));
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_refresh_in_the_lobby_rejoins_into_the_lowest_empty_seat() {
        let mut h = Harness::start();
        let (_a, _, _) = h.join("A").await;
        let (b, _, token_b) = h.join("B").await;
        let (_c, _, _) = h.join("C").await;
        h.send(&b, ClientMessage::ChooseSeat { seat: seat(3) });
        h.disconnect(&b);
        settle().await;

        // The old token is stale (the seat was freed): it falls through to a
        // fresh seat, the lowest empty one, with a new token.
        let mut b2 = h.connect();
        let (b2_seat, token) = h.join_as(&mut b2, "B", Some(&token_b)).await;
        assert_eq!(b2_seat, seat(1));
        assert_ne!(token, token_b);
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_join_clears_only_the_joiners_ready_flag() {
        let mut h = Harness::start();
        let (mut a, _, _) = h.join("A").await;
        let (b, _, _) = h.join("B").await;
        h.send(&a, ready(true));
        h.send(&b, ready(true));
        settle().await;
        let (_c, _, _) = h.join("C").await;
        assert_eq!(lobby_ready(&a.last_state()), [true, true, false, false]);
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn reconnect_with_token_keeps_the_seat_and_kicks_the_old_connection() {
        let mut h = Harness::start();
        let (mut old, _, token) = h.join("Josey").await;
        let (mut other, _, _) = h.join("Alex").await;
        h.send(&old, ready(true));
        settle().await;
        old.drain();

        let mut new = h.connect();
        let reclaimed = h.join_as(&mut new, "Josey", Some(&token)).await;
        assert_eq!(reclaimed, (seat(0), token));

        // The old connection is told, then its channel closes.
        assert_eq!(old.rx.recv().await, Some(ServerMessage::Kicked));
        assert_eq!(old.rx.recv().await, None);

        let state = other.last_state();
        assert_eq!(names(&state), [Some("Josey"), Some("Alex"), None, None]);
        assert!(state.seats[0].connected);
        // Rejoining counts as joining: that player's ready flag is cleared.
        assert_eq!(lobby_ready(&state), [false; 4]);

        // The kicked socket's later Disconnected is ignored: the seat stays.
        h.disconnect(&old);
        settle().await;
        assert!(other.drain().is_empty());
        let replies = h.request(&mut new, ready(true)).await;
        let [ServerMessage::State(state)] = replies.as_slice() else {
            panic!("expected one State, got {replies:?}");
        };
        assert_eq!(state.your_seat, seat(0));
        assert!(state.seats[0].connected);
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn reconnect_with_token_updates_the_name_unless_another_seat_has_it() {
        let mut h = Harness::start();
        let (_a, _, token_a) = h.join("Josey").await;
        let (mut b, _, _) = h.join("Alex").await;

        let mut a2 = h.connect();
        h.join_as(&mut a2, "Jo", Some(&token_a)).await;
        assert_eq!(names(&b.last_state())[0], Some("Jo"));

        let mut a3 = h.connect();
        h.join_as(&mut a3, "ALEX", Some(&token_a)).await;
        assert_eq!(names(&b.last_state())[0], Some("Jo"));
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_connected_seats_name_is_taken_case_insensitively() {
        let mut h = Harness::start();
        let (_a, _, _) = h.join("Josey").await;
        let mut other = h.connect();
        for name in ["Josey", "  josey  ", "JOSEY"] {
            let replies = h.request(&mut other, join_message(name, None)).await;
            assert_eq!(replies, vec![rejected(SessionError::NameTaken)]);
        }
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn names_are_trimmed_and_limited_to_20_chars() {
        let mut h = Harness::start();
        let mut client = h.connect();
        for bad in ["", "   ", "abcdefghijklmnopqrstu"] {
            let replies = h.request(&mut client, join_message(bad, None)).await;
            assert_eq!(replies, vec![rejected(SessionError::InvalidName)]);
        }

        // 20 characters (not bytes) once trimmed is fine.
        let name = "ééééééééééééééééééé!";
        h.join_as(&mut client, &format!("  {name}  "), None).await;
        let (mut other, _, _) = h.join("B").await;
        assert_eq!(names(&other.last_state())[0], Some(name));
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn an_unknown_token_falls_through_to_a_fresh_seat() {
        let mut h = Harness::start();
        let (_a, _, _) = h.join("A").await;
        let mut b = h.connect();
        let (b_seat, token) = h.join_as(&mut b, "B", Some("not-a-real-token")).await;
        assert_eq!(b_seat, seat(1));
        assert_ne!(token, "not-a-real-token");
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_fifth_player_gets_no_seat() {
        let mut h = Harness::start();
        let mut clients = Vec::new();
        for name in ["A", "B", "C", "D"] {
            clients.push(h.join(name).await.0);
        }
        let mut fifth = h.connect();
        let replies = h.request(&mut fifth, join_message("E", None)).await;
        assert_eq!(replies, vec![rejected(SessionError::NoSeatsAvailable)]);
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn messages_before_join_are_rejected_and_unjoined_connections_get_no_state() {
        let mut h = Harness::start();
        let mut lurker = h.connect();
        let before_join = [
            ClientMessage::Pass,
            ClientMessage::TakeBack,
            ready(true),
            ClientMessage::ChooseSeat { seat: seat(2) },
            update_settings([Rank::Two, Rank::Two], None),
            ClientMessage::NewMatch,
            ClientMessage::ResetDeal,
            classify(1, "9S 9S"),
        ];
        for message in before_join {
            let replies = h.request(&mut lurker, message).await;
            assert_eq!(replies, vec![rejected(SessionError::NotJoined)]);
        }

        let (_a, _, _) = h.join("A").await;
        assert!(lurker.drain().is_empty());
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_second_join_is_rejected() {
        let mut h = Harness::start();
        let (mut a, _, _) = h.join("A").await;
        let replies = h.request(&mut a, join_message("Other", None)).await;
        assert_eq!(replies, vec![rejected(SessionError::AlreadyJoined)]);
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn game_actions_in_the_lobby_are_rejected() {
        let mut h = Harness::start();
        let (mut a, _, _) = h.join("A").await;
        let card = cards("5S")[0];
        let actions = [
            ClientMessage::Play {
                cards: vec![card],
                declared: None,
            },
            ClientMessage::Pass,
            ClientMessage::TakeBack,
            ClientMessage::PayTribute { card },
            ClientMessage::ReturnTribute { card },
        ];
        for action in actions {
            let replies = h.request(&mut a, action).await;
            assert_eq!(replies, vec![rejected(SessionError::NotInDeal)]);
        }
        let replies = h.request(&mut a, ClientMessage::ResetDeal).await;
        assert_eq!(replies, vec![rejected(SessionError::NotInDeal)]);
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn everyone_leaving_the_lobby_frees_the_seats_but_keeps_the_settings() {
        let mut h = Harness::start();
        let mut lurker = h.connect();
        let (a, _, token_a) = h.join("A").await;
        let (b, _, _) = h.join("B").await;
        h.send(&b, ClientMessage::ChooseSeat { seat: seat(2) });
        h.send(&a, update_settings([Rank::King, Rank::Ace], Some(Team::B)));
        h.disconnect(&a);
        h.disconnect(&b);
        settle().await;

        // The never-joined connection was kept and finds empty seats; the old
        // token matches nothing (the seat was freed).
        let joined = h.join_as(&mut lurker, "B", Some(&token_a)).await;
        assert_eq!(joined.0, seat(0));
        assert_ne!(joined.1, token_a);
        let (mut other, _, _) = h.join("C").await;
        let state = other.last_state();
        assert_eq!(names(&state), [Some("B"), Some("C"), None, None]);
        // It's the same room, so the lobby settings are still there.
        assert_eq!(
            lobby(&state).progress,
            Progress::default().with_settings([Level(Rank::King), Level(Rank::Ace)], Some(Team::B))
        );
        h.finish().await;
    }

    // ---- Room lifetime (LOBBY_FLOW_SPEC.md §5.3) ----

    #[tokio::test(start_paused = true)]
    async fn a_new_room_nobody_joins_closes_after_idle_close_after() {
        let mut h = Harness::start();
        // A connection that never joins doesn't keep the room open.
        let mut lurker = h.connect();
        tokio::time::sleep(timings().idle_close_after - Duration::from_secs(1)).await;
        assert!(!h.is_closed().await);
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(h.is_closed().await);
        // The Room dropped the lurker's sender, so its ws task would close
        // the socket.
        assert_eq!(lurker.rx.recv().await, None);
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_join_clears_the_close_timer() {
        let mut h = Harness::start();
        tokio::time::sleep(timings().idle_close_after - Duration::from_secs(1)).await;
        let (_a, _, _) = h.join("A").await;
        tokio::time::sleep(timings().abandon_after * 10).await;
        assert!(!h.is_closed().await);
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_rejected_join_does_not_clear_the_close_timer() {
        let mut h = Harness::start();
        let mut client = h.connect();
        let replies = h.request(&mut client, join_message("", None)).await;
        assert_eq!(replies, vec![rejected(SessionError::InvalidName)]);
        tokio::time::sleep(timings().idle_close_after).await;
        assert!(h.is_closed().await);
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn everyone_leaving_the_lobby_closes_the_room_after_idle_close_after() {
        let mut h = Harness::start();
        let (a, _, _) = h.join("A").await;
        let (b, _, _) = h.join("B").await;
        // Time spent with players seated doesn't count.
        tokio::time::sleep(timings().abandon_after * 2).await;
        h.disconnect(&a);
        settle().await;
        // One player left: still open.
        tokio::time::sleep(timings().abandon_after * 2).await;
        assert!(!h.is_closed().await);

        h.disconnect(&b);
        tokio::time::sleep(timings().idle_close_after - Duration::from_secs(1)).await;
        assert!(!h.is_closed().await);
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(h.is_closed().await);
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_new_room_mid_deal_waits_abandon_after() {
        // `new_in_deal` has seats but no connections, and a deal in progress.
        let h = Harness::start_with(in_deal_room(almost_over(Rank::Five)));
        tokio::time::sleep(timings().abandon_after - Duration::from_secs(1)).await;
        assert!(!h.is_closed().await);
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(h.is_closed().await);
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn an_unjoined_disconnect_changes_nothing() {
        let mut h = Harness::start();
        let (mut a, _, _) = h.join("A").await;
        let lurker = h.connect();
        h.disconnect(&lurker);
        settle().await;
        assert!(a.drain().is_empty());
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn sleep_until_opt_never_fires_when_unset() {
        let unset = tokio::time::timeout(Duration::from_secs(3600), sleep_until_opt(None)).await;
        assert!(unset.is_err());

        let start = Instant::now();
        sleep_until_opt(Some(start + Duration::from_secs(5))).await;
        assert_eq!(Instant::now() - start, Duration::from_secs(5));
    }

    // ---- In a match ----

    const NAMES: [&str; 4] = ["A", "B", "C", "D"];

    fn match_view(state: &RedactedState) -> &MatchView {
        match &state.room {
            RoomView::InGame(view) => view,
            other => panic!("expected a match, got {other:?}"),
        }
    }

    /// The viewer's own hand, in any phase.
    fn your_hand(state: &RedactedState) -> Vec<Card> {
        match &match_view(state).phase {
            PhaseView::Tribute { your_hand, .. } | PhaseView::Playing { your_hand, .. } => {
                your_hand.clone()
            }
        }
    }

    fn turn(state: &RedactedState) -> SeatId {
        match &match_view(state).phase {
            PhaseView::Playing { turn, .. } => *turn,
            other => panic!("expected Playing, got {other:?}"),
        }
    }

    fn play(c: &str) -> ClientMessage {
        ClientMessage::Play {
            cards: cards(c),
            declared: None,
        }
    }

    fn classify(request_id: u32, c: &str) -> ClientMessage {
        ClientMessage::Classify {
            request_id,
            cards: cards(c),
        }
    }

    fn classified(request_id: u32, readings: Vec<Play>) -> ServerMessage {
        ServerMessage::Classified {
            request_id,
            readings,
        }
    }

    fn action_rejected(error: ActionError) -> ServerMessage {
        ServerMessage::Rejected {
            code: RejectCode::Action(error),
            message: error.to_string(),
        }
    }

    /// A table where team A (at `level_a`) won the previous deal 1-2 and
    /// declares, with team B at Two.
    fn table_after_a_won(level_a: Rank) -> Table {
        Table {
            progress: Progress {
                team_levels: [Level(level_a), Level(Rank::Two)],
                declaring: Some(Team::A),
                a_attempts: [0, 0],
            },
            last_result: Some(DealResult {
                order: vec![seat(0), seat(2)],
            }),
            last_deal: None,
        }
    }

    /// A deal three actions from its end: seat 0 plays 3♠ (and is out), seat
    /// 1 passes, seat 2 plays 4♠ (and is out) → team A goes out 1-2. Team A
    /// declares at `level_a`; at Ace that 1-2 wins the match.
    ///
    /// Seats 1 and 3 hold both Big Jokers, so the previous deal's losers
    /// refuse tribute (抗贡) and seat 0, last deal's winner, leads.
    fn almost_over(level_a: Rank) -> (Table, Match) {
        let table = table_after_a_won(level_a);
        let m = Match::from_deal(
            table.progress.clone(),
            table.last_result.clone(),
            ["3S", "BJ", "4S", "BJ"].map(hand),
            &mut StdRng::seed_from_u64(7),
        );
        (table, m)
    }

    fn in_deal_room((table, m): (Table, Match)) -> Room {
        Room::new_in_deal(StdRng::seed_from_u64(7), timings(), NAMES, table, m)
    }

    /// A deal started from `table_after_a_won(level_a)` (for rooms whose
    /// deal isn't `almost_over`'s).
    fn in_deal_room_at(level_a: Rank, m: Match) -> Room {
        in_deal_room((table_after_a_won(level_a), m))
    }

    impl Harness {
        /// Connects and joins all four players by name (reclaiming the seats
        /// `new_in_deal` made), then drains everything they received. Returns
        /// the clients and their session tokens, by seat.
        async fn join_all(&mut self) -> (Vec<Client>, Vec<String>) {
            let mut clients = Vec::new();
            let mut tokens = Vec::new();
            for (i, name) in NAMES.into_iter().enumerate() {
                let (client, joined_seat, token) = self.join(name).await;
                assert_eq!(joined_seat, seat(i as u8));
                clients.push(client);
                tokens.push(token);
            }
            for client in &mut clients {
                client.drain();
            }
            (clients, tokens)
        }

        /// Plays `almost_over`'s three actions and checks each was accepted.
        async fn finish_the_deal(&self, clients: &mut [Client]) {
            for (i, message) in [(0, play("3S")), (1, ClientMessage::Pass), (2, play("4S"))] {
                self.send(&clients[i], message);
                settle().await;
                // Each accepted action is broadcast, so every seat's inbox
                // holds only States (a Rejected would show up here).
                for client in clients.iter_mut() {
                    let messages = client.drain();
                    assert!(
                        matches!(messages.as_slice(), [ServerMessage::State(_)]),
                        "after seat {i}'s action: {messages:?}"
                    );
                }
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn new_in_deal_seats_are_reclaimed_by_name() {
        let mut h = Harness::start_with(in_deal_room(almost_over(Rank::Five)));
        let (mut clients, _tokens) = h.join_all().await;
        let state = clients[1].last_state();
        assert_eq!(names(&state), NAMES.map(Some));
        assert!(state.seats.iter().all(|info| info.connected));
        assert_eq!(your_hand(&state), cards("BJ"));
        assert_eq!(turn(&state), seat(0));
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn play_and_pass_are_broadcast_to_every_seat() {
        let mut h = Harness::start();
        let mut clients = Vec::new();
        for name in NAMES {
            clients.push(h.join(name).await.0);
        }
        for client in &clients {
            h.send(client, ready(true));
        }
        settle().await;

        let leader = turn(&clients[0].last_state());
        let lead = &mut clients[leader.index()];
        let card = your_hand(&lead.last_state())[0];
        h.send(
            lead,
            ClientMessage::Play {
                cards: vec![card],
                declared: None,
            },
        );
        settle().await;

        let next = seat((leader.index() as u8 + 1) % 4);
        for client in &mut clients {
            let state = client.last_state();
            let PhaseView::Playing { trick, turn, .. } = &match_view(&state).phase else {
                panic!("expected Playing");
            };
            assert_eq!(*turn, next);
            assert_eq!(trick.len(), 1);
            // Only the leader sees their own exact count (26).
            let expected = if state.your_seat == leader {
                CardCount::Exact(26)
            } else {
                CardCount::MoreThanTen
            };
            assert_eq!(match_view(&state).card_counts[leader.index()], expected);
        }
        assert_eq!(your_hand(&clients[leader.index()].last_state()).len(), 26);

        h.send(&clients[next.index()], ClientMessage::Pass);
        settle().await;
        for client in &mut clients {
            let state = client.last_state();
            let PhaseView::Playing { trick, turn, .. } = &match_view(&state).phase else {
                panic!("expected Playing");
            };
            assert_eq!(*turn, seat((next.index() as u8 + 1) % 4));
            assert_eq!(trick.len(), 2);
        }
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_rejected_play_goes_only_to_that_seat() {
        let mut h = Harness::start_with(in_deal_room(almost_over(Rank::Five)));
        let (mut clients, _tokens) = h.join_all().await;

        // Not seat 1's turn (seat 0 leads).
        let replies = h.request(&mut clients[1], play("BJ")).await;
        assert_eq!(replies, vec![action_rejected(ActionError::NotYourTurn)]);
        // Not in seat 0's hand.
        let replies = h.request(&mut clients[0], play("5S")).await;
        assert_eq!(replies, vec![action_rejected(ActionError::CardsNotInHand)]);
        // Tribute is over.
        let card = cards("3S")[0];
        let replies = h
            .request(&mut clients[0], ClientMessage::PayTribute { card })
            .await;
        assert_eq!(replies, vec![action_rejected(ActionError::WrongPhase)]);

        for client in &mut clients {
            assert!(client.drain().is_empty());
        }
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn take_back_is_broadcast_and_a_rejected_one_goes_only_to_that_seat() {
        let mut h = Harness::start_with(in_deal_room(almost_over(Rank::Five)));
        let (mut clients, _tokens) = h.join_all().await;

        // Nobody has played yet.
        let replies = h.request(&mut clients[0], ClientMessage::TakeBack).await;
        assert_eq!(
            replies,
            vec![action_rejected(ActionError::NothingToTakeBack)]
        );

        // Seat 0 plays its last card (out, but the deal goes on); only seat 0
        // is offered the take back.
        h.send(&clients[0], play("3S"));
        settle().await;
        for (i, client) in clients.iter_mut().enumerate() {
            let state = client.last_state();
            let PhaseView::Playing { can_take_back, .. } = &match_view(&state).phase else {
                panic!("expected Playing");
            };
            assert_eq!(*can_take_back, i == 0, "seat {i}");
        }

        // Seat 1 can't take back seat 0's play: only seat 1 hears about it.
        let replies = h.request(&mut clients[1], ClientMessage::TakeBack).await;
        assert_eq!(
            replies,
            vec![action_rejected(ActionError::NothingToTakeBack)]
        );
        for client in &mut clients {
            assert!(client.drain().is_empty());
        }

        // Seat 0 takes it back: everyone sees the state from before, marked
        // as taken back by seat 0.
        h.send(&clients[0], ClientMessage::TakeBack);
        settle().await;
        for (i, client) in clients.iter_mut().enumerate() {
            let messages = client.drain();
            let [ServerMessage::State(state)] = messages.as_slice() else {
                panic!("seat {i}: expected one State, got {messages:?}");
            };
            let PhaseView::Playing {
                turn,
                trick,
                finish_order,
                can_take_back,
                took_back,
                ..
            } = &match_view(state).phase
            else {
                panic!("expected Playing");
            };
            assert_eq!(*turn, seat(0));
            assert!(trick.is_empty());
            assert!(finish_order.is_empty());
            assert!(!can_take_back);
            assert_eq!(*took_back, Some(seat(0)));
            assert_eq!(match_view(state).card_counts[0], CardCount::Exact(1));
        }
        assert_eq!(your_hand(&clients[0].last_state()), cards("3S"));
        h.finish().await;
    }

    /// Playing at level Six (team A declares), so 6♥ is the wildcard. The
    /// losers hold both Big Jokers (抗贡), so seat 0 leads straight away.
    fn wildcard_match() -> Match {
        let progress = Progress {
            team_levels: [Level(Rank::Six), Level(Rank::Two)],
            declaring: Some(Team::A),
            a_attempts: [0, 0],
        };
        Match::from_deal(
            progress,
            Some(DealResult {
                order: vec![seat(0), seat(2)],
            }),
            ["8S 8H 8D 6H 6H 3C", "BJ 4C", "5S", "BJ 7C"].map(hand),
            &mut StdRng::seed_from_u64(7),
        )
    }

    #[tokio::test(start_paused = true)]
    async fn an_ambiguous_play_asks_only_that_seat_then_a_declared_resend_applies() {
        let mut h = Harness::start_with(in_deal_room_at(Rank::Six, wildcard_match()));
        let (mut clients, _tokens) = h.join_all().await;

        // 8,8,8,6♥,6♥ (sent out of order): a full house of 8s or a
        // quintuple bomb of 8s. Only seat 0 hears about it, with the cards
        // in hand order.
        let replies = h.request(&mut clients[0], play("6H 8S 6H 8D 8H")).await;
        let in_hand_order = cards("6H 6H 8S 8H 8D");
        let full_house = Combo::FullHouse {
            triple: Rank::Eight,
        };
        assert_eq!(
            replies,
            vec![ServerMessage::ChooseReading {
                cards: in_hand_order.clone(),
                options: vec![
                    Play {
                        cards: in_hand_order.clone(),
                        combo: full_house,
                        wildcard_as: vec![],
                    },
                    Play {
                        cards: in_hand_order.clone(),
                        combo: Combo::Bomb(Bomb::OfAKind {
                            size: 5,
                            rank: Rank::Eight,
                        }),
                        wildcard_as: vec![Rank::Eight, Rank::Eight],
                    },
                ],
            }]
        );
        for client in &mut clients[1..] {
            assert!(client.drain().is_empty());
        }

        // Resending with a declaration applies it and is broadcast.
        h.send(
            &clients[0],
            ClientMessage::Play {
                cards: in_hand_order.clone(),
                declared: Some(full_house),
            },
        );
        settle().await;
        for client in &mut clients {
            let messages = client.drain();
            let [ServerMessage::State(state)] = messages.as_slice() else {
                panic!("expected one State, got {messages:?}");
            };
            let PhaseView::Playing { trick, turn, .. } = &match_view(state).phase else {
                panic!("expected Playing");
            };
            assert_eq!(*turn, seat(1));
            assert_eq!(
                trick.as_slice(),
                [TrickEntry::Played {
                    seat: seat(0),
                    play: Play {
                        cards: in_hand_order.clone(),
                        combo: full_house,
                        wildcard_as: vec![],
                    },
                }]
            );
        }
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn classify_answers_only_the_asker_and_changes_nothing() {
        let mut h = Harness::start_with(in_deal_room_at(Rank::Six, wildcard_match()));
        let (mut clients, _tokens) = h.join_all().await;

        // Seat 1 asks, out of turn, about cards it doesn't hold (seat 0 has
        // them): 8,8,8 + both 6♥ wildcards, sent out of order. Both readings
        // come back, in hand order, with the request id echoed.
        let replies = h
            .request(&mut clients[1], classify(42, "6H 8S 6H 8D 8H"))
            .await;
        let in_hand_order = cards("6H 6H 8S 8H 8D");
        let full_house = Play {
            cards: in_hand_order.clone(),
            combo: Combo::FullHouse {
                triple: Rank::Eight,
            },
            wildcard_as: vec![],
        };
        let wildcard_bomb = Play {
            cards: in_hand_order,
            combo: Combo::Bomb(Bomb::OfAKind {
                size: 5,
                rank: Rank::Eight,
            }),
            wildcard_as: vec![Rank::Eight, Rank::Eight],
        };
        assert_eq!(
            replies,
            vec![classified(42, vec![full_house, wildcard_bomb])]
        );

        // A plain bomb: one reading.
        let replies = h.request(&mut clients[1], classify(7, "8C 8S 8H 8D")).await;
        let bomb = Play {
            cards: cards("8S 8H 8D 8C"),
            combo: Combo::Bomb(Bomb::OfAKind {
                size: 4,
                rank: Rank::Eight,
            }),
            wildcard_as: vec![],
        };
        assert_eq!(replies, vec![classified(7, vec![bomb])]);

        // Not a combo: empty readings, not a Rejected.
        let replies = h.request(&mut clients[2], classify(8, "3C 4C")).await;
        assert_eq!(replies, vec![classified(8, vec![])]);

        // Nobody else heard anything, and no State went out.
        for client in &mut clients {
            assert!(client.drain().is_empty());
        }
        // Nothing changed: still seat 0's turn, with its whole hand.
        let replies = h.request(&mut clients[1], ClientMessage::Pass).await;
        assert_eq!(replies, vec![action_rejected(ActionError::NotYourTurn)]);
        let replies = h.request(&mut clients[0], play("3C")).await;
        let [ServerMessage::State(state)] = replies.as_slice() else {
            panic!("expected one State, got {replies:?}");
        };
        assert_eq!(your_hand(state), cards("6H 6H 8S 8H 8D"));
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn classify_during_tribute_uses_the_deal_level() {
        // Team A went out 1-2 at Five: seats 1 and 3 each owe a tribute, so
        // the deal starts in Tribute at level Five (5♥ is the wildcard).
        let progress = Progress {
            team_levels: [Level(Rank::Five), Level(Rank::Two)],
            declaring: Some(Team::A),
            a_attempts: [0, 0],
        };
        let m = Match::from_deal(
            progress,
            Some(DealResult {
                order: vec![seat(0), seat(2)],
            }),
            ["3S", "4C", "4S", "7C"].map(hand),
            &mut StdRng::seed_from_u64(7),
        );
        assert!(matches!(m.state(), GameState::Tribute(_)));
        let mut h = Harness::start_with(in_deal_room_at(Rank::Five, m));
        let (mut clients, _tokens) = h.join_all().await;

        let replies = h
            .request(&mut clients[3], classify(3, "9S 9D 9D 5H 5H"))
            .await;
        let expected = readings(&cards("9S 9D 9D 5H 5H"), Level(Rank::Five));
        assert_eq!(expected.len(), 2, "full house or a wildcard bomb");
        assert_eq!(replies, vec![classified(3, expected)]);
        for client in &mut clients {
            assert!(client.drain().is_empty());
        }
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn classify_outside_a_deal_is_empty() {
        // In the lobby.
        let mut h = Harness::start();
        let (mut a, _, _) = h.join("A").await;
        let (mut b, _, _) = h.join("B").await;
        a.drain();
        b.drain();
        let replies = h.request(&mut a, classify(5, "9S 9S")).await;
        assert_eq!(replies, vec![classified(5, vec![])]);
        assert!(b.drain().is_empty());
        h.finish().await;

        // Back in the lobby after a deal.
        let mut h = Harness::start_with(in_deal_room(almost_over(Rank::Five)));
        let (mut clients, _tokens) = h.join_all().await;
        h.finish_the_deal(&mut clients).await;
        let replies = h.request(&mut clients[1], classify(6, "BJ")).await;
        assert_eq!(replies, vec![classified(6, vec![])]);
        for client in &mut clients {
            assert!(client.drain().is_empty());
        }
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn lobby_messages_during_a_match_are_rejected() {
        let mut h = Harness::start_with(in_deal_room(almost_over(Rank::Five)));
        let (mut clients, _tokens) = h.join_all().await;
        let a = &mut clients[0];
        for message in [ready(true), ready(false)] {
            let replies = h.request(a, message).await;
            assert_eq!(replies, vec![rejected(SessionError::NotInLobby)]);
        }
        let lobby_only = [
            ClientMessage::ChooseSeat { seat: seat(1) },
            update_settings([Rank::Two, Rank::Two], Some(Team::A)),
            ClientMessage::NewMatch,
        ];
        for message in lobby_only {
            let replies = h.request(a, message).await;
            assert_eq!(replies, vec![rejected(SessionError::NotInLobby)]);
        }
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn reconnect_with_token_mid_match_keeps_the_seat_and_hand() {
        let mut h = Harness::start_with(in_deal_room(almost_over(Rank::Five)));
        let (mut clients, tokens) = h.join_all().await;
        h.send(&clients[0], play("3S"));
        settle().await;
        let before = clients[1].last_state();

        // Seat 1 opens a second tab with its token (a refresh looks the same
        // to the server if the old socket hasn't closed yet).
        let token = tokens[1].clone();
        let mut b2 = h.connect();
        let reclaimed = h.join_as(&mut b2, "B", Some(&token)).await;
        assert_eq!(reclaimed, (seat(1), token));
        assert_eq!(clients[1].rx.recv().await, Some(ServerMessage::Kicked));
        assert_eq!(clients[1].rx.recv().await, None);

        let after = b2.last_state();
        assert_eq!(after.your_seat, seat(1));
        assert_eq!(after.room, before.room);
        assert!(after.seats.iter().all(|info| info.connected));

        // The game carries on from the new connection.
        let replies = h.request(&mut b2, ClientMessage::Pass).await;
        let [ServerMessage::State(state)] = replies.as_slice() else {
            panic!("expected one State, got {replies:?}");
        };
        assert_eq!(turn(state), seat(2));
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_disconnected_seat_is_reclaimed_by_name_with_a_new_token() {
        let mut h = Harness::start_with(in_deal_room(almost_over(Rank::Five)));
        let (mut clients, tokens) = h.join_all().await;
        let before = clients[2].last_state();
        let old_token = tokens[2].clone();

        // Losing a player mid-match keeps the seat; the others see it.
        h.disconnect(&clients[2]);
        settle().await;
        let state = clients[0].last_state();
        assert_eq!(names(&state), NAMES.map(Some));
        assert!(!state.seats[2].connected);

        // Back on another device, with no token: matched by name (any case).
        let mut c2 = h.connect();
        let (c2_seat, new_token) = h.join_as(&mut c2, " c ", None).await;
        assert_eq!(c2_seat, seat(2));
        assert_ne!(new_token, old_token);
        let after = c2.last_state();
        assert_eq!(after.room, before.room);
        assert_eq!(names(&after)[2], Some("C"));
        assert!(clients[0].last_state().seats[2].connected);

        // The old token no longer matches anything: it falls through, and
        // with every seat held a new name gets no seat.
        let mut stale = h.connect();
        let replies = h
            .request(&mut stale, join_message("Zed", Some(&old_token)))
            .await;
        assert_eq!(replies, vec![rejected(SessionError::NoSeatsAvailable)]);
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn everyone_leaving_mid_match_keeps_it_until_they_rejoin() {
        let mut h = Harness::start_with(in_deal_room(almost_over(Rank::Five)));
        let (mut clients, _) = h.join_all().await;
        h.send(&clients[0], play("3S"));
        settle().await;
        let before = clients[0].last_state();
        let before_b = clients[1].last_state();
        for client in &clients {
            h.disconnect(client);
        }
        settle().await;

        // Just before the grace period ends, A comes back to the same match.
        tokio::time::sleep(timings().abandon_after - Duration::from_secs(1)).await;
        let mut a2 = h.connect();
        let (a2_seat, _) = h.join_as(&mut a2, "A", None).await;
        assert_eq!(a2_seat, seat(0));
        let state = a2.last_state();
        assert_eq!(state.room, before.room);
        assert_eq!(turn(&state), seat(1));

        // The rejoin disarmed the timer: long after it would have fired, the
        // match is still there for the others to come back to.
        tokio::time::sleep(timings().abandon_after * 2).await;
        let mut b2 = h.connect();
        h.join_as(&mut b2, "B", None).await;
        assert_eq!(b2.last_state().room, before_b.room);
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn an_abandoned_match_closes_the_room_when_the_timer_fires() {
        let mut h = Harness::start_with(in_deal_room(almost_over(Rank::Five)));
        let (clients, _tokens) = h.join_all().await;
        let mut lurker = h.connect();
        for client in &clients {
            h.disconnect(client);
        }
        settle().await;

        // A match is in progress, so `idle_close_after` doesn't apply.
        tokio::time::sleep(timings().idle_close_after * 2).await;
        assert!(!h.is_closed().await);

        tokio::time::sleep(timings().abandon_after).await;
        assert!(h.is_closed().await);
        assert_eq!(lurker.rx.recv().await, None);
        h.finish().await;
    }

    // ---- The lobby flow (LOBBY_FLOW_SPEC.md §4.3) ----

    /// Four fresh players seated A–D in a new room.
    async fn four_in_a_fresh_lobby(h: &mut Harness) -> Vec<Client> {
        let mut clients = Vec::new();
        for name in NAMES {
            clients.push(h.join(name).await.0);
        }
        settle().await;
        for client in &mut clients {
            client.drain();
        }
        clients
    }

    /// Every client sends SetReady (the last one starts the deal).
    async fn all_ready(h: &Harness, clients: &mut [Client]) {
        for client in clients.iter() {
            h.send(client, ready(true));
        }
        settle().await;
    }

    /// The lobby `almost_over(Rank::Five)` ends in: team A won 1-2, Five →
    /// Eight, and still declares.
    fn after_five_to_eight() -> Progress {
        Progress {
            team_levels: [Level(Rank::Eight), Level(Rank::Two)],
            declaring: Some(Team::A),
            a_attempts: [0, 0],
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_deal_starts_from_the_table_settings() {
        let mut h = Harness::start();
        let mut clients = four_in_a_fresh_lobby(&mut h).await;
        // Team A at Ace and declaring: the first deal is played at Ace, but
        // it is still a first deal (no tribute, turned-up card).
        h.send(
            &clients[1],
            update_settings([Rank::Ace, Rank::Four], Some(Team::A)),
        );
        all_ready(&h, &mut clients).await;

        for client in &mut clients {
            let state = client.last_state();
            let view = match_view(&state);
            assert_eq!(view.team_levels, [Level(Rank::Ace), Level(Rank::Four)]);
            assert_eq!(view.declaring, Some(Team::A));
            let PhaseView::Playing {
                level, deal_start, ..
            } = &view.phase
            else {
                panic!("expected Playing, got {:?}", view.phase);
            };
            assert_eq!(*level, Level(Rank::Ace));
            assert!(matches!(deal_start, DealStart::FirstDeal { .. }));
        }
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_finished_deal_returns_to_a_locked_lobby_with_the_table_updated() {
        let mut h = Harness::start_with(in_deal_room(almost_over(Rank::Five)));
        let (mut clients, _tokens) = h.join_all().await;
        h.finish_the_deal(&mut clients).await;

        for client in &mut clients {
            let state = client.last_state();
            let lobby = lobby(&state);
            assert_eq!(lobby.ready, [false; 4]);
            assert_eq!(lobby.progress, after_five_to_eight());
            assert!(lobby.seats_locked);
            let end = lobby.last_deal.as_ref().expect("the deal is shown");
            assert_eq!(end.summary.result.order, vec![seat(0), seat(2)]);
            assert_eq!(end.summary.before, table_after_a_won(Rank::Five).progress);
            assert_eq!(end.summary.after, after_five_to_eight());
            assert_eq!(end.summary.match_winner, None);
            assert_eq!(end.final_trick.len(), 3, "3♠, pass, 4♠");
        }

        // Everyone readies up → the next deal, at team A's new level, with
        // tribute from last deal's finish (unless it's cancelled by 抗贡).
        all_ready(&h, &mut clients).await;
        for (i, client) in clients.iter_mut().enumerate() {
            let state = client.last_state();
            let view = match_view(&state);
            assert_eq!(view.card_counts[i], CardCount::Exact(27));
            assert_eq!(view.team_levels, [Level(Rank::Eight), Level(Rank::Two)]);
            match &view.phase {
                PhaseView::Tribute { level, .. } => assert_eq!(*level, Level(Rank::Eight)),
                PhaseView::Playing {
                    level, deal_start, ..
                } => {
                    assert_eq!(*level, Level(Rank::Eight));
                    assert!(matches!(deal_start, DealStart::AntiTribute { .. }));
                }
            }
        }
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_won_match_returns_to_a_fresh_unlocked_lobby_that_shows_the_win() {
        let mut h = Harness::start_with(in_deal_room(almost_over(Rank::Ace)));
        let (mut clients, _tokens) = h.join_all().await;
        // D leaves before the end; the last three actions don't need D.
        let d = clients.pop().unwrap();
        h.disconnect(&d);
        settle().await;
        for client in &mut clients {
            client.drain();
        }
        h.finish_the_deal(&mut clients).await;

        for (i, client) in clients.iter_mut().enumerate() {
            let state = client.last_state();
            assert_eq!(state.your_seat, seat(i as u8));
            // D's seat was freed: the lobby is unlocked again.
            assert_eq!(names(&state), [Some("A"), Some("B"), Some("C"), None]);
            let lobby = lobby(&state);
            assert_eq!(lobby.ready, [false; 4]);
            assert_eq!(lobby.progress, Progress::default());
            assert!(!lobby.seats_locked);
            let end = lobby.last_deal.as_ref().expect("the winning deal is shown");
            assert_eq!(end.summary.match_winner, Some(Team::A));
        }

        // A newcomer can take the free seat, and the next match starts
        // fresh: a first deal at Two.
        let (e, e_seat, _) = h.join("E").await;
        assert_eq!(e_seat, seat(3));
        clients.push(e);
        all_ready(&h, &mut clients).await;
        let state = clients[0].last_state();
        let view = match_view(&state);
        assert_eq!(view.team_levels, [Level(Rank::Two), Level(Rank::Two)]);
        assert_eq!(view.declaring, None);
        assert!(matches!(
            view.phase,
            PhaseView::Playing {
                deal_start: DealStart::FirstDeal { .. },
                ..
            }
        ));
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn update_settings_applies_and_unreadies_everyone() {
        let mut h = Harness::start();
        let (mut a, _, _) = h.join("A").await;
        let (mut b, _, _) = h.join("B").await;
        h.send(&a, ready(true));
        h.send(&b, ready(true));
        settle().await;
        assert_eq!(lobby_ready(&a.last_state()), [true, true, false, false]);

        h.send(&b, update_settings([Rank::Ace, Rank::King], Some(Team::B)));
        settle().await;
        for client in [&mut a, &mut b] {
            let state = client.last_state();
            let lobby = lobby(&state);
            assert_eq!(lobby.ready, [false; 4]);
            assert_eq!(
                lobby.progress,
                Progress {
                    team_levels: [Level(Rank::Ace), Level(Rank::King)],
                    declaring: Some(Team::B),
                    a_attempts: [0, 0],
                }
            );
            assert!(!lobby.seats_locked);
        }

        // Before a match, "nobody declaring" is fine.
        h.send(&a, update_settings([Rank::Ace, Rank::King], None));
        settle().await;
        assert_eq!(lobby(&b.last_state()).progress.declaring, None);
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn between_deals_a_team_must_be_declaring() {
        let mut h = Harness::start_with(in_deal_room(almost_over(Rank::Five)));
        let (mut clients, _tokens) = h.join_all().await;
        h.finish_the_deal(&mut clients).await;
        h.send(&clients[1], ready(true));
        settle().await;
        for client in &mut clients {
            client.drain();
        }

        let replies = h
            .request(
                &mut clients[0],
                update_settings([Rank::Eight, Rank::Two], None),
            )
            .await;
        assert_eq!(replies, vec![rejected(SessionError::InvalidSettings)]);
        // Nothing changed, and nobody else heard anything.
        for client in &mut clients[1..] {
            assert!(client.drain().is_empty());
        }

        // Handing the lead to team B is allowed.
        h.send(
            &clients[0],
            update_settings([Rank::Eight, Rank::Two], Some(Team::B)),
        );
        settle().await;
        let state = clients[2].last_state();
        let lobby = lobby(&state);
        assert_eq!(lobby.progress.declaring, Some(Team::B));
        assert_eq!(lobby.ready, [false; 4]);
        assert!(lobby.seats_locked);
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn reset_deal_goes_back_to_the_lobby_with_the_table_untouched() {
        let mut h = Harness::start_with(in_deal_room(almost_over(Rank::Five)));
        let (mut clients, _tokens) = h.join_all().await;
        h.send(&clients[0], play("3S"));
        settle().await;

        // Anyone seated may reset, not just the player to act.
        h.send(&clients[3], ClientMessage::ResetDeal);
        settle().await;
        let before = table_after_a_won(Rank::Five);
        for client in &mut clients {
            let state = client.last_state();
            assert_eq!(names(&state), NAMES.map(Some));
            assert_eq!(
                state.room,
                RoomView::Lobby(LobbyView {
                    ready: [false; 4],
                    progress: before.progress.clone(),
                    seats_locked: true,
                    last_deal: None,
                })
            );
        }

        // No deal to reset any more.
        let replies = h.request(&mut clients[0], ClientMessage::ResetDeal).await;
        assert_eq!(replies, vec![rejected(SessionError::NotInDeal)]);

        // A re-deal starts from the same table: level Five, and tribute from
        // the same previous result.
        all_ready(&h, &mut clients).await;
        let state = clients[0].last_state();
        let view = match_view(&state);
        assert_eq!(view.card_counts[0], CardCount::Exact(27));
        match &view.phase {
            PhaseView::Tribute { level, duties, .. } => {
                assert_eq!(*level, Level(Rank::Five));
                let payers: Vec<SeatId> = duties.iter().map(|d| d.payer).collect();
                assert!(payers.contains(&seat(1)) && payers.contains(&seat(3)));
            }
            PhaseView::Playing {
                level, deal_start, ..
            } => {
                assert_eq!(*level, Level(Rank::Five));
                assert!(matches!(deal_start, DealStart::AntiTribute { .. }));
            }
        }
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn resetting_a_first_deal_frees_the_seats_of_absent_players() {
        let mut h = Harness::start();
        let mut clients = four_in_a_fresh_lobby(&mut h).await;
        all_ready(&h, &mut clients).await;
        let d = clients.pop().unwrap();
        h.disconnect(&d);
        settle().await;

        h.send(&clients[0], ClientMessage::ResetDeal);
        settle().await;
        let state = clients[1].last_state();
        assert_eq!(names(&state), [Some("A"), Some("B"), Some("C"), None]);
        assert_eq!(state.room, fresh_lobby([false; 4]));
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn new_match_starts_over_and_unlocks_the_seats() {
        let mut h = Harness::start_with(in_deal_room(almost_over(Rank::Five)));
        let (mut clients, _tokens) = h.join_all().await;
        h.finish_the_deal(&mut clients).await;
        let d = clients.pop().unwrap();
        h.disconnect(&d);
        h.send(&clients[0], ready(true));
        settle().await;

        h.send(&clients[2], ClientMessage::NewMatch);
        settle().await;
        for client in &mut clients {
            let state = client.last_state();
            // D was away, so D's seat is freed.
            assert_eq!(names(&state), [Some("A"), Some("B"), Some("C"), None]);
            assert_eq!(state.room, fresh_lobby([false; 4]));
        }

        // Seats can change again.
        h.send(&clients[0], ClientMessage::ChooseSeat { seat: seat(3) });
        settle().await;
        assert_eq!(clients[0].last_state().your_seat, seat(3));
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn seats_are_locked_between_deals() {
        let mut h = Harness::start_with(in_deal_room(almost_over(Rank::Five)));
        let (mut clients, _tokens) = h.join_all().await;
        h.finish_the_deal(&mut clients).await;
        let d = clients.pop().unwrap();
        h.disconnect(&d);
        settle().await;
        clients[0].drain();

        // Even D's seat, whose player is away, can't be taken.
        let replies = h
            .request(&mut clients[0], ClientMessage::ChooseSeat { seat: seat(3) })
            .await;
        assert_eq!(replies, vec![rejected(SessionError::SeatsLocked)]);
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_disconnect_between_deals_keeps_the_seat_for_its_player() {
        let mut h = Harness::start_with(in_deal_room(almost_over(Rank::Five)));
        let (mut clients, _tokens) = h.join_all().await;
        h.finish_the_deal(&mut clients).await;
        h.send(&clients[1], ready(true));
        h.send(&clients[2], ready(true));
        settle().await;

        h.disconnect(&clients[2]);
        settle().await;
        let state = clients[0].last_state();
        assert_eq!(names(&state), NAMES.map(Some));
        assert!(!state.seats[2].connected);
        // An absent player isn't ready; the others keep their flags.
        assert_eq!(lobby_ready(&state), [false, true, false, false]);
        assert!(lobby(&state).seats_locked);

        // A newcomer can't take the seat, but C can reclaim it by name.
        let mut e = h.connect();
        let replies = h.request(&mut e, join_message("E", None)).await;
        assert_eq!(replies, vec![rejected(SessionError::NoSeatsAvailable)]);
        let mut c2 = h.connect();
        let (c2_seat, _) = h.join_as(&mut c2, "c", None).await;
        assert_eq!(c2_seat, seat(2));
        let state = c2.last_state();
        assert_eq!(lobby(&state).progress, after_five_to_eight());
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn everyone_leaving_between_deals_starts_the_abandon_timer() {
        let mut h = Harness::start_with(in_deal_room(almost_over(Rank::Five)));
        let (mut clients, _tokens) = h.join_all().await;
        h.finish_the_deal(&mut clients).await;
        for client in &clients {
            h.disconnect(client);
        }
        settle().await;

        // Within the grace period the match is still there (seats are locked,
        // so it's `abandon_after`, not `idle_close_after`).
        tokio::time::sleep(timings().abandon_after - Duration::from_secs(1)).await;
        let mut a2 = h.connect();
        h.join_as(&mut a2, "A", None).await;
        let state = a2.last_state();
        assert_eq!(names(&state), NAMES.map(Some));
        assert_eq!(lobby(&state).progress, after_five_to_eight());
        assert!(lobby(&state).seats_locked);

        // Everyone leaves again and stays away: the room closes.
        h.disconnect(&a2);
        tokio::time::sleep(timings().abandon_after - Duration::from_secs(1)).await;
        assert!(!h.is_closed().await);
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(h.is_closed().await);
        h.finish().await;
    }

    #[tokio::test(start_paused = true)]
    async fn everyone_leaving_after_a_won_match_closes_after_idle_close_after() {
        let mut h = Harness::start_with(in_deal_room(almost_over(Rank::Ace)));
        let (mut clients, _tokens) = h.join_all().await;
        h.finish_the_deal(&mut clients).await;
        // The match is over and the lobby unlocked: nothing to protect.
        assert!(!lobby(&clients[0].last_state()).seats_locked);
        for client in &clients {
            h.disconnect(client);
        }
        tokio::time::sleep(timings().idle_close_after + Duration::from_secs(1)).await;
        assert!(h.is_closed().await);
        h.finish().await;
    }
}
