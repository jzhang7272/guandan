//! Networking/IO: a thin wrapper around `rules` (TECH_SPEC.md §4–§6), with
//! one Room per invite code (LOBBY_FLOW_SPEC.md §5).

pub mod protocol;
pub mod registry;
pub mod room;
pub mod ws;

use std::sync::Arc;

use axum::Json;
use axum::Router;
use axum::extract::{Path, RawQuery, State};
use axum::http::StatusCode;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use serde_json::json;

use crate::client_page::CLIENT_FILES;
use registry::{Registry, RegistryFull, RoomCode};

/// Builds the axum `Router` for every endpoint (LOBBY_FLOW_SPEC.md §5.2): one
/// route per row of `client_page::CLIENT_FILES`, the room API, the game page
/// at `/{code}`, and each room's WebSocket at `/{code}/ws`. Shared by
/// `main.rs` and the tests.
///
/// The static routes are exact paths, which axum always prefers over a
/// `{code}` capture (and none of them is a valid code anyway).
pub fn router(registry: Arc<Registry>) -> Router {
    let mut router = Router::new();
    for &(path, content_type, body) in CLIENT_FILES {
        // Both are `'static` (a `&str` and the file's bytes), so the handler
        // can hand them out on every request without copying the file.
        router = router.route(
            path,
            get(move || async move { static_file(content_type, body) }),
        );
    }
    // The room handlers need the registry; they get it as router state.
    // `with_state` turns this into the `Router<()>` that `axum::serve`
    // accepts.
    router
        .route("/api/rooms", post(create_room))
        .route("/api/rooms/{code}", get(room_exists))
        .route("/{code}", get(game_page))
        .route("/{code}/", get(game_page_with_slash))
        .route("/{code}/ws", get(ws::upgrade))
        .with_state(registry)
}

/// A client file. `no-cache` = the browser may keep a copy but must check
/// back before using it, so a rebuilt server's client code is picked up on
/// the next page load instead of an old cached copy.
fn static_file(content_type: &'static str, body: &'static [u8]) -> Response {
    (
        [(CONTENT_TYPE, content_type), (CACHE_CONTROL, "no-cache")],
        body,
    )
        .into_response()
}

/// `index.html`, which is also the page for every room: the client reads the
/// code from the path.
fn index_html() -> Response {
    let &(_, content_type, body) = CLIENT_FILES
        .iter()
        .find(|(path, _, _)| *path == "/")
        .expect("CLIENT_FILES has a row for /");
    static_file(content_type, body)
}

/// A redirect to the room's canonical address `/{code}`, keeping the query
/// string (e.g. `?name=A` for solo testing, `?debug=1`).
fn redirect_to_room(code: &RoomCode, query: Option<String>) -> Response {
    let location = match query {
        Some(query) => format!("/{code}?{query}"),
        None => format!("/{code}"),
    };
    Redirect::temporary(&location).into_response()
}

/// `GET /{code}`: the game page, whether or not the room exists (the client
/// asks `/api/rooms/{code}` for that).
async fn game_page(Path(code): Path<String>) -> Response {
    match RoomCode::parse(&code) {
        None => StatusCode::NOT_FOUND.into_response(),
        Some(_) => index_html(),
    }
}

/// `GET /{code}/`: redirects to `/{code}`, because from `/{code}/` the page's
/// relative asset paths (`style.css`, `js/main.js`) would resolve under the
/// code instead of under `/`.
async fn game_page_with_slash(Path(code): Path<String>, RawQuery(query): RawQuery) -> Response {
    match RoomCode::parse(&code) {
        None => StatusCode::NOT_FOUND.into_response(),
        Some(parsed) => redirect_to_room(&parsed, query),
    }
}

/// `POST /api/rooms`: opens a room. `201 {"code": "482193"}`, or
/// `503 {"error": "TooManyRooms"}` when the server is full.
async fn create_room(State(registry): State<Arc<Registry>>) -> Response {
    match registry.create() {
        Ok(code) => (StatusCode::CREATED, Json(json!({ "code": code.as_str() }))).into_response(),
        Err(RegistryFull) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "TooManyRooms" })),
        )
            .into_response(),
    }
}

/// `GET /api/rooms/{code}`: `200 {}` if the room is open, else `404`.
async fn room_exists(Path(code): Path<String>, State(registry): State<Arc<Registry>>) -> Response {
    let open = RoomCode::parse(&code).is_some_and(|code| registry.get(&code).is_some());
    if open {
        Json(json!({})).into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::SocketAddr;
    use std::time::Duration;

    use rand::SeedableRng;
    use rand::rngs::StdRng;

    use super::*;
    use crate::server::room::RoomTimings;

    fn timings() -> RoomTimings {
        RoomTimings {
            abandon_after: Duration::from_secs(60),
            idle_close_after: Duration::from_secs(60),
        }
    }

    /// Serves `router` on an ephemeral port, with room for `max_rooms` rooms.
    async fn spawn_server_with(max_rooms: usize) -> SocketAddr {
        let registry = Arc::new(Registry::new(
            StdRng::seed_from_u64(7),
            timings(),
            max_rooms,
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router(registry)).await.unwrap() });
        addr
    }

    async fn spawn_server() -> SocketAddr {
        spawn_server_with(registry::MAX_ROOMS).await
    }

    /// A bare HTTP/1.1 GET over a plain TCP socket, returning the raw response
    /// (status line, headers, body). Uses std's blocking `TcpStream` on the
    /// blocking pool so the test needs no HTTP-client dependency.
    async fn http_get(addr: SocketAddr, path: &str) -> String {
        String::from_utf8(http_get_bytes(addr, path).await).unwrap()
    }

    /// Same, as raw bytes (for binary files like the icon).
    async fn http_get_bytes(addr: SocketAddr, path: &str) -> Vec<u8> {
        http_request(addr, "GET", path).await
    }

    async fn http_post(addr: SocketAddr, path: &str) -> String {
        String::from_utf8(http_request(addr, "POST", path).await).unwrap()
    }

    async fn http_request(addr: SocketAddr, method: &str, path: &str) -> Vec<u8> {
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        tokio::task::spawn_blocking(move || {
            let mut stream = std::net::TcpStream::connect(addr).unwrap();
            stream.write_all(request.as_bytes()).unwrap();
            let mut response = Vec::new();
            stream.read_to_end(&mut response).unwrap();
            response
        })
        .await
        .unwrap()
    }

    /// The value of header `name` in a raw response (names are
    /// case-insensitive).
    fn header<'a>(response: &'a str, name: &str) -> Option<&'a str> {
        let (head, _) = response.split_once("\r\n\r\n")?;
        head.lines().skip(1).find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name).then(|| value.trim())
        })
    }

    /// The status code of a raw response (`HTTP/1.1 200 OK` → 200).
    fn status(response: &str) -> u16 {
        response[9..12].parse().unwrap()
    }

    fn json_body(response: &str) -> serde_json::Value {
        let (_, body) = response.split_once("\r\n\r\n").unwrap();
        serde_json::from_str(body).unwrap()
    }

    /// `POST /api/rooms`, returning the new code.
    async fn create_room(addr: SocketAddr) -> String {
        let response = http_post(addr, "/api/rooms").await;
        assert_eq!(status(&response), 201, "{response}");
        json_body(&response)["code"].as_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn serves_js_as_javascript() {
        let addr = spawn_server().await;
        let response = http_get(addr, "/js/main.js").await;
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        let content_type = header(&response, "content-type").unwrap();
        assert!(
            content_type.starts_with("text/javascript"),
            "{content_type}"
        );
        assert!(response.ends_with(include_str!("../../client/js/main.js")));
        // Browsers must revalidate, or a rebuilt server's JS is ignored.
        assert_eq!(header(&response, "cache-control"), Some("no-cache"));
    }

    #[tokio::test]
    async fn serves_index_html_at_root() {
        let addr = spawn_server().await;
        let response = http_get(addr, "/").await;
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        let content_type = header(&response, "content-type").unwrap();
        assert!(content_type.starts_with("text/html"), "{content_type}");
        assert!(response.ends_with(include_str!("../../client/index.html")));
    }

    #[tokio::test]
    async fn serves_the_icon_as_png() {
        let addr = spawn_server().await;
        let response = http_get_bytes(addr, "/assets/guandan.png").await;
        let icon = include_bytes!("../../client/assets/guandan.png");
        assert!(response.ends_with(icon));
        let head = String::from_utf8_lossy(&response[..response.len() - icon.len()]);
        assert!(head.starts_with("HTTP/1.1 200"), "{head}");
        assert_eq!(header(&head, "content-type"), Some("image/png"));
    }

    #[tokio::test]
    async fn tests_page_is_not_served() {
        let addr = spawn_server().await;
        let response = http_get(addr, "/tests.html").await;
        assert!(response.starts_with("HTTP/1.1 404"), "{response}");
    }

    #[tokio::test]
    async fn the_old_ws_route_is_gone() {
        let addr = spawn_server().await;
        // `ws` isn't a code, so `/{code}` answers 404.
        assert_eq!(status(&http_get(addr, "/ws").await), 404);
    }

    #[tokio::test]
    async fn serves_index_html_at_any_code() {
        let addr = spawn_server().await;
        // The page is served whether or not the room exists: the client asks
        // `/api/rooms/{code}` for that.
        let code = create_room(addr).await;
        assert_ne!(code, "000000");
        for path in [format!("/{code}"), "/000000".to_string()] {
            let response = http_get(addr, &path).await;
            assert_eq!(status(&response), 200, "{path}: {response}");
            let content_type = header(&response, "content-type").unwrap();
            assert!(content_type.starts_with("text/html"), "{content_type}");
            assert_eq!(header(&response, "cache-control"), Some("no-cache"));
            assert!(response.ends_with(include_str!("../../client/index.html")));
        }
    }

    #[tokio::test]
    async fn a_code_page_with_a_query_is_served_as_is() {
        let addr = spawn_server().await;
        // `?name=` links load the page directly, with no redirect.
        let response = http_get(addr, "/482193?name=Cara&debug=1").await;
        assert_eq!(status(&response), 200, "{response}");
        assert!(response.ends_with(include_str!("../../client/index.html")));
    }

    #[tokio::test]
    async fn a_trailing_slash_redirects_to_the_code() {
        let addr = spawn_server().await;
        let response = http_get(addr, "/482193/").await;
        assert_eq!(status(&response), 307, "{response}");
        assert_eq!(header(&response, "location"), Some("/482193"));
        // The query string survives, so `?name=` links keep working.
        let response = http_get(addr, "/012345/?name=Cara&debug=1").await;
        assert_eq!(status(&response), 307, "{response}");
        assert_eq!(
            header(&response, "location"),
            Some("/012345?name=Cara&debug=1")
        );
    }

    #[tokio::test]
    async fn a_path_that_is_not_a_code_is_a_404() {
        let addr = spawn_server().await;
        for path in [
            "/48219",
            "/4821930",
            "/48219a",
            "/ABCDEF",
            "/482%20193",
            "/%20482193",
            "/482-19",
            "/%D9%A1%D9%A2%D9%A3%D9%A4%D9%A5%D9%A6", // ١٢٣٤٥٦
            "/style.css.map",
            "/48219a/",
            "/hello",
            "/favicon.ico",
            "/nope/",
        ] {
            let response = http_get(addr, path).await;
            assert_eq!(status(&response), 404, "{path}: {response}");
        }
    }

    /// `index.html` is also served at `/{code}`, where its relative URLs
    /// resolve against `/`: each must be a served file.
    #[test]
    fn index_html_asset_paths_resolve_from_a_code_page() {
        let html = include_str!("../../client/index.html");
        let mut found = 0;
        for attribute in ["href=\"", "src=\""] {
            for rest in html.split(attribute).skip(1) {
                let url = &rest[..rest.find('"').unwrap()];
                // `./` or `../` would depend on more than the page's directory.
                assert!(!url.starts_with('.'), "{url}");
                let path = if url.starts_with('/') {
                    url.to_string()
                } else {
                    format!("/{url}")
                };
                assert!(
                    CLIENT_FILES.iter().any(|(p, _, _)| *p == path),
                    "index.html references {url}, which isn't served at {path}"
                );
                found += 1;
            }
        }
        assert!(found >= 3, "found only {found} URLs in index.html");
    }

    #[tokio::test]
    async fn post_api_rooms_creates_a_room() {
        let addr = spawn_server().await;
        let response = http_post(addr, "/api/rooms").await;
        assert_eq!(status(&response), 201, "{response}");
        let content_type = header(&response, "content-type").unwrap();
        assert!(
            content_type.starts_with("application/json"),
            "{content_type}"
        );
        let code = json_body(&response)["code"].as_str().unwrap().to_string();
        assert!(RoomCode::parse(&code).is_some_and(|parsed| parsed.as_str() == code));

        let response = http_get(addr, &format!("/api/rooms/{code}")).await;
        assert_eq!(status(&response), 200, "{response}");
        assert_eq!(json_body(&response), json!({}));
    }

    #[tokio::test]
    async fn get_api_rooms_for_an_unknown_code_is_a_404() {
        let addr = spawn_server().await;
        for code in ["482193", "48219", "not-a-code"] {
            let response = http_get(addr, &format!("/api/rooms/{code}")).await;
            assert_eq!(status(&response), 404, "{code}: {response}");
        }
    }

    #[tokio::test]
    async fn post_api_rooms_at_the_cap_is_a_503() {
        let addr = spawn_server_with(2).await;
        create_room(addr).await;
        create_room(addr).await;
        let response = http_post(addr, "/api/rooms").await;
        assert_eq!(status(&response), 503, "{response}");
        assert_eq!(json_body(&response), json!({ "error": "TooManyRooms" }));
    }

    #[tokio::test]
    async fn ws_for_an_unknown_code_is_a_404() {
        let addr = spawn_server().await;
        // Plain GETs: the code is checked before the upgrade.
        for path in ["/482193/ws", "/48219/ws", "/not-a-code/ws"] {
            let response = http_get(addr, path).await;
            assert_eq!(status(&response), 404, "{path}: {response}");
        }
    }
}
