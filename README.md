# guandan

Play this game at https://daguandan.cc

A self-hostable [Guandan (掼蛋)](https://www.pagat.com/climbing/guan_dan.html)
server. Four friends open a link in a web browser (desktop or phone) and play
live. It's a single Rust binary with the web client built in: no database,
no separate frontend build, no external services.

- Full rules: dealing, tribute / return / anti-tribute, every combo type
  including wildcards (逢人配), the bomb hierarchy, 接风, level advancement,
  and the match-end condition (including "three failed attempts at A → back
  to 2").
- Several games at once: create a game to get a six-digit invite code (like
  `482193`) and share the link `http://<host>/482193` with your friends.
- A pre-game lobby where you pick seats (and so partners) and ready up.
- English or Simplified Chinese (简体中文): the **EN | 中文** toggle in the
  top-left corner of every page switches instantly, using real 掼蛋 terms
  (级牌, 进贡, 头游, 双下…). Each player picks their own language.
- Take back your last play or pass (悔牌) until the next player acts,
  unless it ended the deal (a play) or the trick (a pass).
- Automatic reconnection during a match: refresh a tab, or come back on
  another device under the same name, and you get your seat and hand back.
  (Before the first deal, leaving the lobby frees your seat.)
- State is in memory only. Restarting the server ends any game in progress.

## Requirements

- Rust 1.85 or newer (the crate uses the 2024 edition). Install from
  <https://rustup.rs>.

## Running

```sh
cargo run --release
```

Then open <http://localhost:8080> in a browser. It prints
`serving on http://localhost:8080` when it's ready.

One player creates a game on the home page and shares its invite link
(`http://localhost:8080/482193`, where `482193` is the game's code). Everyone
opens that link and enters a name, which takes the first free seat (**Sit
here** moves you to another empty one), then clicks **I'm ready**. The match
starts as soon as all four seats are ready. Partners sit opposite each other:
South & North (南北方) against East & West (东西方).

Each game lives in memory until nobody has been in it for a while (see
`ABANDON_AFTER_MINS` below); then it closes and its code stops working. A
server keeps at most 100 games open at once.

### Options (environment variables)

| Variable | Default | What it does |
|---|---|---|
| `PORT` | `8080` | Port to listen on (all interfaces, `0.0.0.0`). |
| `ABANDON_AFTER_MINS` | `360` | If every player disconnects mid-match (during a deal or between deals), the game is kept this many minutes so the table can come back and carry on. After that the game closes. A game with nobody in it and no match in progress (including a newly created one nobody joined) closes after 10 minutes. |

An invalid value for any of these stops the server at startup with an error.

Example:

```sh
PORT=9000 ABANDON_AFTER_MINS=60 cargo run --release
```

### URL options

| Parameter | Example | What it does |
|---|---|---|
| `?name=` | `/482193?name=foo` | Join with this name without typing it in. |
| `?lang=` | `/482193?lang=zh` | Show this page load in `zh` (Chinese) or `en` (English), without changing the saved choice. Otherwise the page uses the language last picked with the toggle, or on a first visit the browser's language. |
| `?debug=1` | `/482193?debug=1` | Open the debug panel (raw game state and the last 20 messages). The **≡** button in the top-right corner toggles it too. |

## Hosting for friends

The server listens on every interface, so anyone who can reach the port can
play:

- **Same network**: open `http://<your-computer's-LAN-IP>:8080`, create a
  game, and share its link.
- **Over the internet**: forward the port on your router to your machine, or
  run it on a VPS, and share `http://<public-IP-or-domain>:8080/<code>`.
- **HTTPS**: put it behind a reverse proxy (Caddy, nginx, …) that forwards
  WebSocket upgrades on `/<code>/ws` (every game's socket path ends in `/ws`).
  The client switches to `wss://` automatically when the page is served over
  HTTPS.

For a long-running deployment, build once and run the binary directly:

```sh
cargo build --release
PORT=8080 ./target/release/guandan
```

The client files are embedded in the binary, so `target/release/guandan` is
the only file you need to copy to another machine (built for that machine's
OS and architecture).

There are no accounts. Anyone with a game's link who knows a disconnected
player's name can take their seat, so it's meant for friends, not strangers.

## Playtesting alone (four tabs)

Create a game at <http://localhost:8080> to get a code (say `482193`), then
open four tabs, one per player:

```
http://localhost:8080/482193?name=A
http://localhost:8080/482193?name=B
http://localhost:8080/482193?name=C
http://localhost:8080/482193?name=D
```

Click **I'm ready** in each. The tab whose turn it is (to play, or to pay or
return a tribute) shows **▶ Your turn** in its title, so you know which tab to
switch to. Each tab keeps its own session, so once the match has started,
refreshing a tab keeps its seat, and closing a tab and reopening it with the
same `?name=` takes the seat back. In the lobby before the first deal, a tab
that leaves gives up its seat and rejoins in the first free one. The language
choice is shared by every tab of a browser; add `&lang=zh` or `&lang=en` to a
tab's link to mix languages.

## Development

```sh
cargo test                                   # all rules, server, and wire-format tests
cargo fmt --check                            # formatting
cargo clippy --all-targets -- -D warnings    # lints (must be clean)
```

### Project layout

```
src/
  rules/     Pure game logic: cards, ranking, combos, readings (incl. wildcards),
             tricks, tribute, levels, the match state machine, per-player views.
             No IO; unit-tested in isolation.
  server/    The WebSocket protocol, the Room (one game: lobby, sessions,
             reconnection, timers), the Registry (every open game, by invite
             code), and the axum router.
  client_page.rs  The client files embedded in the binary (CLIENT_FILES), one
             row per served file.
  main.rs    Reads the environment variables, builds the Registry, starts the
             server.
client/      The browser client: plain HTML + native ES modules, no build step.
             All player-visible text lives in js/strings_en.js and
             js/strings_zh.js (same keys), looked up with t() from js/i18n.js.
  assets/    Images: guandan.png (192px tab/home-screen icon, served) and
             guandan-full.png (the full-size source image, not served).
fixtures/    Golden JSON for every wire message (see fixtures/README.md).
```

The server is authoritative: the client contains no game rules. It sends
the cards you picked, and the server decides whether the play is legal. If a
wildcard makes a play ambiguous (for example a full house of 7s or of Ks), the
server asks that player which one they meant.

### Working on the client without a server (fixture mode)

`fixtures/` holds an example of every message the server sends. The client can
render them directly, which helps when building or checking a screen:

```sh
python3 -m http.server 8000     # from the repo root
```

- `http://localhost:8000/client/index.html?fixture=state_playing&debug=1`
  renders one fixture. Names are listed in `fixtures/README.md`.
- `?fixture=state_playing,rejected` loads several in order; step through them
  with **Prev / Next** in the debug panel.
- In fixture mode nothing is sent; button presses are logged to the debug
  panel instead.
- Add `&lang=zh` to see a screen in Chinese.
- `http://localhost:8000/client/tests.html` runs the client's formatting tests
  against every fixture, in both languages, and checks that the English and
  Chinese string tables have the same keys.

The fixtures are checked against real serialization by `cargo test`. If you
change a type that's sent over the wire, regenerate them with:

```sh
UPDATE_FIXTURES=1 cargo test
```
