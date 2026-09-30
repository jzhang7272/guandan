//! The per-connection WebSocket task (TECH_SPEC.md §5): upgrade, assign a
//! `ConnectionId`, send `RoomEvent::Connected`, then one `tokio::select!` loop
//! between the socket and the Room's outgoing channel.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use tokio::sync::mpsc;

use super::protocol::{ClientMessage, ServerMessage};
use super::registry::{Registry, RoomCode};
use super::room::{ConnectionId, RoomEvent};

/// Source of `ConnectionId`s: each number is handed out once per process.
static NEXT_CONNECTION_ID: AtomicU64 = AtomicU64::new(0);

/// `GET /{code}/ws`: accept the upgrade into that room and run the
/// connection in its own task. An unknown code is a 404 (checked first, so a
/// plain GET for a closed room gets a 404 too).
pub async fn upgrade(
    Path(code): Path<String>,
    State(registry): State<Arc<Registry>>,
    ws: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Response {
    let Some(events) = RoomCode::parse(&code).and_then(|code| registry.get(&code)) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match ws {
        Ok(ws) => ws.on_upgrade(move |socket| run_connection(socket, events)),
        // Not a WebSocket request.
        Err(rejection) => rejection.into_response(),
    }
}

/// Runs one connection until either the browser or the Room goes away.
///
/// A single `select!` loop owns the socket, so there is no `socket.split()`
/// and no second task: whichever branch fires uses the socket in its body.
async fn run_connection(mut socket: WebSocket, events: mpsc::UnboundedSender<RoomEvent>) {
    // Relaxed is enough: we only need uniqueness, not ordering with other memory.
    let id = ConnectionId(NEXT_CONNECTION_ID.fetch_add(1, Ordering::Relaxed));
    // Unbounded so the Room never waits on a slow browser (§5).
    let (outgoing, mut from_room) = mpsc::unbounded_channel::<ServerMessage>();
    tracing::info!(id = id.0, "connection opened");

    if events.send(RoomEvent::Connected { id, outgoing }).is_err() {
        // The room closed between the lookup and now; nothing to serve.
        return;
    }

    loop {
        tokio::select! {
            frame = socket.recv() => match frame {
                Some(Ok(Message::Text(text))) => {
                    match serde_json::from_str::<ClientMessage>(&text) {
                        Ok(message) => {
                            if events.send(RoomEvent::Message { id, message }).is_err() {
                                break;
                            }
                        }
                        // A malformed frame is a client bug, not a reason to
                        // drop the connection.
                        Err(err) => tracing::warn!(id = id.0, %err, "dropping unparsable frame"),
                    }
                }
                // The browser closed the socket, or the connection broke.
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                // Binary / ping / pong: not part of the protocol (axum
                // answers pings by itself).
                Some(Ok(_)) => {}
            },
            message = from_room.recv() => match message {
                Some(message) => {
                    let json = match serde_json::to_string(&message) {
                        Ok(json) => json,
                        Err(err) => {
                            tracing::error!(id = id.0, %err, "failed to serialize a message");
                            continue;
                        }
                    };
                    if socket.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                // The Room dropped our sender (after `Kicked`, or because the
                // room closed). Send a Close frame so the browser sees a clean
                // close, not a drop.
                None => {
                    let _ = socket.send(Message::Close(None)).await;
                    break;
                }
            },
        }
    }

    tracing::info!(id = id.0, "connection closed");
    // If the Room is gone there is nobody left to tell, so ignore the error.
    let _ = events.send(RoomEvent::Disconnected { id });
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures_util::{SinkExt, StreamExt};
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use tokio_tungstenite::tungstenite;

    use super::*;
    use crate::server::registry::MAX_ROOMS;
    use crate::server::room::RoomTimings;

    /// The one end-to-end smoke test (TECH_SPEC.md §8): a real server on an
    /// ephemeral port and a real WebSocket client; `POST /api/rooms`, then
    /// `Join` on `/{code}/ws` → `Joined` + `State`. It proves the plumbing
    /// only; Room behavior is tested in `room.rs`.
    #[tokio::test]
    async fn join_over_a_real_websocket() {
        let timings = RoomTimings {
            abandon_after: Duration::from_secs(60),
            idle_close_after: Duration::from_secs(60),
        };
        let registry = Arc::new(Registry::new(StdRng::seed_from_u64(7), timings, MAX_ROOMS));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = crate::server::router(registry);
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let code = create_room(addr).await;
        let (mut client, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/{code}/ws"))
            .await
            .unwrap();
        let join = ClientMessage::Join {
            display_name: "Josey".to_string(),
            reconnect_token: None,
        };
        let join = serde_json::to_string(&join).unwrap();
        client.send(tungstenite::Message::text(join)).await.unwrap();

        let joined = next_message(&mut client).await;
        assert!(matches!(joined, ServerMessage::Joined { .. }), "{joined:?}");
        let state = next_message(&mut client).await;
        assert!(matches!(state, ServerMessage::State(_)), "{state:?}");

        // An unknown code refuses the upgrade with a 404.
        assert_ne!(code, "482193");
        let unknown = tokio_tungstenite::connect_async(format!("ws://{addr}/482193/ws")).await;
        match unknown {
            Err(tungstenite::Error::Http(response)) => assert_eq!(response.status(), 404),
            other => panic!("expected a 404, got {other:?}"),
        }
    }

    /// When a room closes, its sockets get a Close frame (the contract the
    /// client's "This game has ended" relies on, LOBBY_FLOW_SPEC.md §5.3).
    #[tokio::test]
    async fn a_closing_room_closes_its_sockets() {
        let timings = RoomTimings {
            abandon_after: Duration::from_secs(60),
            idle_close_after: Duration::from_millis(200),
        };
        let registry = Arc::new(Registry::new(StdRng::seed_from_u64(7), timings, MAX_ROOMS));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = crate::server::router(Arc::clone(&registry));
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let code = create_room(addr).await;
        // Connected but never joined, so the idle timer keeps running.
        let (mut client, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/{code}/ws"))
            .await
            .unwrap();
        let frame = tokio::time::timeout(Duration::from_secs(5), client.next())
            .await
            .expect("timed out waiting for the room to close");
        assert!(
            matches!(frame, Some(Ok(tungstenite::Message::Close(_))) | None),
            "{frame:?}"
        );
        let code = RoomCode::parse(&code).unwrap();
        assert!(registry.get(&code).is_none());
    }

    /// `POST /api/rooms` over a bare TCP socket; returns the new code.
    async fn create_room(addr: std::net::SocketAddr) -> String {
        use std::io::{Read, Write};
        let request = format!(
            "POST /api/rooms HTTP/1.1\r\nHost: {addr}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        let response = tokio::task::spawn_blocking(move || {
            let mut stream = std::net::TcpStream::connect(addr).unwrap();
            stream.write_all(request.as_bytes()).unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            response
        })
        .await
        .unwrap();
        assert!(response.starts_with("HTTP/1.1 201"), "{response}");
        let (_, body) = response.split_once("\r\n\r\n").unwrap();
        let body: serde_json::Value = serde_json::from_str(body).unwrap();
        body["code"].as_str().unwrap().to_string()
    }

    /// The next server message, with a timeout so a broken server fails the
    /// test instead of hanging it.
    async fn next_message<S>(client: &mut S) -> ServerMessage
    where
        S: StreamExt<Item = Result<tungstenite::Message, tungstenite::Error>> + Unpin,
    {
        let frame = tokio::time::timeout(Duration::from_secs(5), client.next())
            .await
            .expect("timed out waiting for the server")
            .expect("the server closed the socket")
            .unwrap();
        serde_json::from_str(frame.to_text().unwrap()).unwrap()
    }
}
