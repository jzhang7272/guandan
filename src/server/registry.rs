//! Every open room, by invite code (LOBBY_FLOW_SPEC.md §5.1).
//!
//! The registry is only a phone book: it maps a `RoomCode` to the sending end
//! of that room's event channel. Each game's state still lives only in its own
//! Room task (the actor model of TECH_SPEC.md §5 is unchanged).

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use tokio::sync::mpsc;

use super::room::{Room, RoomEvent, RoomTimings};

/// The most rooms one server keeps open at once.
pub const MAX_ROOMS: usize = 100;

/// A room's invite code: 6 digits, e.g. `482193`. Leading zeros are allowed
/// (`012345` is a code), which is why a code is a string, not a number.
///
/// The only ways to get one are `parse` (a URL path) and `random`, so every
/// `RoomCode` is well-formed.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RoomCode(String);

impl RoomCode {
    /// Digits only: easy to read aloud and to type on a phone's number pad.
    pub const ALPHABET: &str = "0123456789";
    pub const LEN: usize = 6;

    /// Reads a code from a URL: accepted only if it is exactly `LEN` ASCII
    /// digits (other scripts' digits, such as `١٢٣٤٥٦`, are rejected). The
    /// home page strips the spaces people type before building the URL.
    pub fn parse(input: &str) -> Option<RoomCode> {
        let well_formed = input.len() == Self::LEN && input.bytes().all(|b| b.is_ascii_digit());
        well_formed.then(|| RoomCode(input.to_string()))
    }

    /// A random code (possibly one already in use; `Registry::create` checks).
    fn random(rng: &mut impl Rng) -> RoomCode {
        let alphabet = Self::ALPHABET.as_bytes();
        let code = (0..Self::LEN)
            .map(|_| char::from(alphabet[rng.random_range(0..alphabet.len())]))
            .collect();
        RoomCode(code)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RoomCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// `Registry::create` refused: `max_rooms` rooms are already open.
#[derive(Debug, PartialEq, Eq)]
pub struct RegistryFull;

pub struct Registry {
    /// Held only for a lookup, insert or remove, never across an `.await`
    /// (a `std` mutex held across an await could block the runtime).
    rooms: Mutex<HashMap<RoomCode, mpsc::UnboundedSender<RoomEvent>>>,
    /// For codes, and to seed each new Room's own rng.
    rng: Mutex<StdRng>,
    timings: RoomTimings,
    max_rooms: usize,
}

impl Registry {
    /// `main.rs`: `StdRng::from_os_rng()`, prod timings, `MAX_ROOMS`.
    pub fn new(rng: StdRng, timings: RoomTimings, max_rooms: usize) -> Self {
        Registry {
            rooms: Mutex::new(HashMap::new()),
            rng: Mutex::new(rng),
            timings,
            max_rooms,
        }
    }

    /// Opens a new room under an unused code and spawns its task. When the
    /// room closes (`Room::run` returns), its code is removed again.
    ///
    /// Takes `self: &Arc<Self>` because the spawned task keeps a handle to
    /// the registry, to remove the code at the end. Must be called from
    /// inside the tokio runtime (it uses `tokio::spawn`).
    pub fn create(self: &Arc<Self>) -> Result<RoomCode, RegistryFull> {
        let (events, room_events) = mpsc::unbounded_channel();
        let (code, room) = {
            let mut rooms = self.rooms.lock().expect("registry lock poisoned");
            if rooms.len() >= self.max_rooms {
                return Err(RegistryFull);
            }
            let mut rng = self.rng.lock().expect("registry rng lock poisoned");
            // With 10^6 = 1 million codes and at most `max_rooms` in use,
            // a random code is almost always free on the first try.
            let code = loop {
                let code = RoomCode::random(&mut *rng);
                if !rooms.contains_key(&code) {
                    break code;
                }
            };
            let room = Room::new(StdRng::from_rng(&mut *rng), self.timings);
            rooms.insert(code.clone(), events);
            (code, room)
        }; // Both locks are released here.

        tracing::info!(%code, "room created");
        let registry = Arc::clone(self);
        let task_code = code.clone();
        tokio::spawn(async move {
            room.run(room_events).await;
            registry.remove(&task_code);
            tracing::info!(code = %task_code, "room closed");
        });
        Ok(code)
    }

    /// The event sender of the room with this code, if it's open.
    pub fn get(&self, code: &RoomCode) -> Option<mpsc::UnboundedSender<RoomEvent>> {
        let rooms = self.rooms.lock().expect("registry lock poisoned");
        rooms.get(code).cloned()
    }

    fn remove(&self, code: &RoomCode) {
        let mut rooms = self.rooms.lock().expect("registry lock poisoned");
        rooms.remove(code);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::server::protocol::ServerMessage;
    use crate::server::room::ConnectionId;

    fn timings() -> RoomTimings {
        RoomTimings {
            abandon_after: Duration::from_secs(60),
            idle_close_after: Duration::from_secs(10),
        }
    }

    fn registry(max_rooms: usize) -> Arc<Registry> {
        Arc::new(Registry::new(
            StdRng::seed_from_u64(7),
            timings(),
            max_rooms,
        ))
    }

    #[test]
    fn random_codes_are_six_digits() {
        let mut rng = StdRng::seed_from_u64(7);
        for _ in 0..1000 {
            let code = RoomCode::random(&mut rng);
            assert_eq!(code.as_str().len(), RoomCode::LEN);
            assert!(code.as_str().chars().all(|c| c.is_ascii_digit()), "{code}");
            // Every generated code reads back as itself.
            assert_eq!(RoomCode::parse(code.as_str()), Some(code));
        }
    }

    #[test]
    fn the_alphabet_is_the_ten_digits() {
        assert_eq!(RoomCode::ALPHABET, "0123456789");
        assert_eq!(RoomCode::LEN, 6);
    }

    #[test]
    fn parse_accepts_six_digits() {
        let code = RoomCode::parse("482193").unwrap();
        assert_eq!(code.as_str(), "482193");
        assert_eq!(code.to_string(), "482193");
        // Leading zeros are part of the code.
        assert_eq!(RoomCode::parse("012345").unwrap().as_str(), "012345");
        assert!(RoomCode::parse("000000").is_some());
    }

    #[test]
    fn parse_rejects_bad_input() {
        for bad in [
            "",
            "48219",   // too short
            "4821930", // too long
            "48219a",  // a letter
            "ABCDEF",  // letters
            "482 19",  // space
            "482 193", // a space (the home page strips these; the server doesn't)
            " 482193", // not trimmed
            "482-19",  // punctuation
            "+48219",  // a sign
            "style.css",
            "١٢٣٤٥٦",       // Arabic-Indic digits
            "４８２１９３", // fullwidth digits
        ] {
            assert_eq!(RoomCode::parse(bad), None, "{bad:?}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn create_then_get_finds_a_live_room() {
        let registry = registry(10);
        let code = registry.create().unwrap();
        let events = registry.get(&code).expect("the new room is registered");

        // The sender really reaches a running Room: connect and join.
        let (outgoing, mut rx) = mpsc::unbounded_channel();
        let id = ConnectionId(1);
        events.send(RoomEvent::Connected { id, outgoing }).unwrap();
        let message = crate::server::protocol::ClientMessage::Join {
            display_name: "Josey".to_string(),
            reconnect_token: None,
        };
        events.send(RoomEvent::Message { id, message }).unwrap();
        tokio::time::sleep(Duration::from_millis(1)).await;
        assert!(matches!(rx.try_recv(), Ok(ServerMessage::Joined { .. })));

        let other = RoomCode::parse("482193").unwrap();
        assert_ne!(other, code);
        assert!(registry.get(&other).is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn codes_are_unique() {
        let registry = registry(100);
        let mut codes: Vec<RoomCode> = (0..100).map(|_| registry.create().unwrap()).collect();
        codes.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        codes.dedup();
        assert_eq!(codes.len(), 100);
    }

    #[tokio::test(start_paused = true)]
    async fn create_fails_at_max_rooms() {
        let registry = registry(2);
        registry.create().unwrap();
        registry.create().unwrap();
        assert_eq!(registry.create(), Err(RegistryFull));
    }

    #[tokio::test(start_paused = true)]
    async fn a_closed_room_is_removed_and_frees_its_slot() {
        let registry = registry(1);
        let code = registry.create().unwrap();
        assert_eq!(registry.create(), Err(RegistryFull));

        // Nobody ever joins, so the room closes after `idle_close_after`.
        tokio::time::sleep(timings().idle_close_after - Duration::from_secs(1)).await;
        assert!(registry.get(&code).is_some());
        tokio::time::sleep(Duration::from_secs(2)).await;
        assert!(registry.get(&code).is_none());

        // Its slot is free again.
        let new_code = registry.create().unwrap();
        assert!(registry.get(&new_code).is_some());
    }
}
