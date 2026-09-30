//! The browser client, embedded into the binary so the server ships as a
//! single file (TECH_SPEC.md §1). `server::router` serves one route per row.
//!
//! Adding a client file = adding one row here.

/// `(URL path, content type, file contents)` for every served file under `client/`.
/// Contents are raw bytes (`include_bytes!`) so images can be served too.
///
/// `.js` files must be `text/javascript`: browsers refuse to load an ES module
/// served with any other script type.
pub const CLIENT_FILES: &[(&str, &str, &[u8])] = &[
    (
        "/",
        "text/html; charset=utf-8",
        include_bytes!("../client/index.html"),
    ),
    (
        "/style.css",
        "text/css; charset=utf-8",
        include_bytes!("../client/style.css"),
    ),
    (
        "/js/main.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../client/js/main.js"),
    ),
    (
        "/js/store.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../client/js/store.js"),
    ),
    (
        "/js/dom.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../client/js/dom.js"),
    ),
    (
        "/js/net.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../client/js/net.js"),
    ),
    (
        "/js/debug.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../client/js/debug.js"),
    ),
    (
        "/js/format.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../client/js/format.js"),
    ),
    (
        "/js/cards.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../client/js/cards.js"),
    ),
    (
        "/js/lobby.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../client/js/lobby.js"),
    ),
    (
        "/js/table.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../client/js/table.js"),
    ),
    (
        "/js/play.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../client/js/play.js"),
    ),
    (
        "/js/tribute.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../client/js/tribute.js"),
    ),
    (
        "/js/results.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../client/js/results.js"),
    ),
    (
        "/js/hand.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../client/js/hand.js"),
    ),
    (
        "/js/home.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../client/js/home.js"),
    ),
    (
        "/js/i18n.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../client/js/i18n.js"),
    ),
    (
        "/js/strings_en.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../client/js/strings_en.js"),
    ),
    (
        "/js/strings_zh.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../client/js/strings_zh.js"),
    ),
    // Tab / home-screen icon (a 192×192 copy of assets/guandan-full.png,
    // which is kept in the repo as the source image but not served).
    (
        "/assets/guandan.png",
        "image/png",
        include_bytes!("../client/assets/guandan.png"),
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_files_are_served_as_javascript() {
        for (path, content_type, _) in CLIENT_FILES {
            if path.ends_with(".js") {
                assert!(
                    content_type.starts_with("text/javascript"),
                    "{path} has content type {content_type}"
                );
            }
        }
    }

    // A new client/js file that isn't in the table is a 404 in the real
    // server (the fixture-mode static server would still serve it, hiding
    // the bug), so check the directory against the table.
    #[test]
    fn every_client_js_file_is_served() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/client/js");
        for entry in std::fs::read_dir(dir).unwrap() {
            let name = entry.unwrap().file_name().into_string().unwrap();
            if name.ends_with(".js") {
                let path = format!("/js/{name}");
                assert!(
                    CLIENT_FILES.iter().any(|(p, _, _)| *p == path),
                    "client/js/{name} is not in CLIENT_FILES"
                );
            }
        }
    }

    #[test]
    fn paths_are_unique() {
        let mut paths: Vec<&str> = CLIENT_FILES.iter().map(|(path, _, _)| *path).collect();
        paths.sort();
        paths.dedup();
        assert_eq!(paths.len(), CLIENT_FILES.len());
    }
}
