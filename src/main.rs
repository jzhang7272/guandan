mod client_page;
mod rules;
mod server;

use std::sync::Arc;
use std::time::Duration;

use rand::SeedableRng;
use rand::rngs::StdRng;

use server::registry::{MAX_ROOMS, Registry};
use server::room::RoomTimings;

/// An empty room with no match in progress closes after this long
/// (LOBBY_FLOW_SPEC.md §5.3).
const IDLE_CLOSE_AFTER: Duration = Duration::from_secs(10 * 60);

/// Reads a numeric env var, or `default` when it is unset. A set but invalid
/// value stops startup instead of being silently ignored.
fn env_or<T: std::str::FromStr>(name: &str, default: T) -> T {
    match std::env::var(name) {
        Ok(value) => value
            .parse()
            .unwrap_or_else(|_| panic!("{name} must be a non-negative number, got {value:?}")),
        Err(_) => default,
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let port: u16 = env_or("PORT", 8080);
    let abandon_after_mins: u64 = env_or("ABANDON_AFTER_MINS", 360);
    let timings = RoomTimings {
        abandon_after: Duration::from_secs(abandon_after_mins * 60),
        idle_close_after: IDLE_CLOSE_AFTER,
    };

    // No rooms yet: each `POST /api/rooms` opens one.
    let registry = Arc::new(Registry::new(StdRng::from_os_rng(), timings, MAX_ROOMS));

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .unwrap_or_else(|err| panic!("can't listen on port {port}: {err}"));
    tracing::info!("serving on http://localhost:{port}");
    axum::serve(listener, server::router(registry))
        .await
        .expect("server error");
}
