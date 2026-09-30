// Routing and the room check (LOBBY_FLOW_SPEC §6.1), POST /api/rooms (§6.2),
// WebSocket, reconnect, token/name storage (UI_SPEC §5, §6), fixture mode
// (§7), message log, game notices (GAME_PAGE_V3_SPEC §1, §3). The only
// writer of store.conn / joined / state / reading / toast / notice / name /
// namePrefill / nameError / log / lastLevel / fixture / page / roomCode /
// roomStatus.

import { store, update } from "./store.js";
// hand.js imports send() from here too; the cycle is fine because neither
// module calls the other while it's being loaded.
import { onNewState, onClassified, dealKeyOf } from "./hand.js";
import { closeResetConfirm } from "./table.js";
import { routeFromPath, parseRoomCode } from "./home.js";
import { returnsLine, seatName } from "./format.js";
import { t } from "./i18n.js";

// sessionStorage (per tab), one token per room: "token:XYZ234" (§6.1).
const tokenKey = (code) => `token:${code}`;
const NAME_KEY = "guandan.name";         // sessionStorage (per tab)
const LAST_NAME_KEY = "guandan.lastName"; // localStorage (form prefill only)
// sessionStorage (per tab): the dealKey whose returns notice was shown, so a
// deal shows it once.
const RETURNS_SHOWN_KEY = "guandan.returnsShown";

const BACKOFF_MS = [500, 1000, 2000, 4000]; // then every 5s
const BACKOFF_MAX_MS = 5000;
const TOAST_MS = 4000;
const TAKE_BACK_NOTICE_MS = 3500; // "Josey took back their play"
const RETURNS_NOTICE_MS = 6000;   // "Returns: Alex → Josey 3♣, …" (§3: about 6s)
const LOG_MAX = 20;

const params = new URLSearchParams(location.search);
const fixtureParam = params.get("fixture");
const fixtureMode = fixtureParam !== null;

let ws = null;
let attempt = 0;
let reconnectTimer = null;
// Set once the room check passed: from then on a name (join) opens the socket.
let roomChecked = false;

// ---------- storage (never throws: private windows can block storage) ----------
// Fixture mode neither reads nor writes storage, so fixtures render the same
// every time and never clobber a real session's token/name.

function getItem(storage, key) {
  if (fixtureMode) return null;
  try { return storage.getItem(key); } catch { return null; }
}
function setItem(storage, key, value) {
  if (fixtureMode) return;
  try {
    if (value === null || value === undefined) storage.removeItem(key);
    else storage.setItem(key, value);
  } catch { /* ignore */ }
}
const session = () => window.sessionStorage;
const local = () => window.localStorage;

// ---------- name sources (§6): ?name= → sessionStorage → localStorage prefill → ask ----------

function initName() {
  const urlName = params.get("name");
  const sessName = getItem(session(), NAME_KEY);
  const lastName = getItem(local(), LAST_NAME_KEY);
  store.name = (urlName && urlName.trim()) || sessName || null;
  store.namePrefill = store.name || lastName || "";
  store.nameError = null;
}

// ---------- log ----------

function log(dir, msg) {
  store.log.push({ dir, time: Date.now(), msg });
  if (store.log.length > LOG_MAX) store.log.splice(0, store.log.length - LOG_MAX);
}

// ---------- public API ----------

// Called once by main.js on load. Picks the page from the path (§6.1):
// Home opens nothing; a room page checks the code, then (once a name is
// known) opens the WebSocket. Fixture mode ignores the path.
export function connect() {
  initName();
  if (fixtureMode) {
    store.page = "room";
    // Optional ?code= shows the lobby's invite line in fixture mode.
    store.roomCode = parseRoomCode(params.get("code"));
    store.roomStatus = "open";
    loadFixtures();
    return;
  }
  const route = routeFromPath(location.pathname);
  store.page = route.page;
  if (route.page === "home") {
    store.conn = "idle";
    update();
    return;
  }
  if (!route.code) {
    // One path segment that can't be an invite code: no need to ask.
    store.roomCode = route.raw.toUpperCase();
    store.roomStatus = "missing";
    store.conn = "idle";
    update();
    return;
  }
  store.roomCode = route.code;
  checkRoom();
}

// POST /api/rooms (§5.2) for Home's Create game. Resolves to { code } on
// 201, { full: true } on 503 (too many rooms), or { error } otherwise.
// Never throws.
export async function createRoom() {
  if (fixtureMode) {
    log("out (fixture: not sent)", { POST: "/api/rooms" });
    update();
    return { error: "fixture mode" };
  }
  try {
    const res = await fetch("/api/rooms", { method: "POST" });
    if (res.status === 503) return { full: true };
    if (!res.ok) return { error: `HTTP ${res.status}` };
    const code = parseRoomCode((await res.json())?.code);
    return code ? { code } : { error: "bad response" };
  } catch (e) {
    return { error: String(e) };
  }
}

// Home: remember the name for the room page we're about to load (it reads
// sessionStorage, so the player isn't asked again), and as the prefill.
export function rememberName(name) {
  const trimmed = String(name ?? "").trim();
  if (!trimmed) return;
  setItem(session(), NAME_KEY, trimmed);
  setItem(local(), LAST_NAME_KEY, trimmed);
}

// Send a ClientMessage. In fixture mode nothing is sent; the would-be message
// is logged to the debug panel instead.
export function send(msg) {
  if (fixtureMode) {
    log("out (fixture: not sent)", msg);
  } else if (ws && ws.readyState === WebSocket.OPEN) {
    ws.send(JSON.stringify(msg));
    log("out", msg);
  } else {
    log("out (dropped: not connected)", msg);
    showToast(t("net.notConnected"));
  }
  update();
}

// Submit a display name (name form). Sends Join now if the socket is open;
// otherwise the Join goes out as soon as it opens.
export function join(name) {
  const trimmed = String(name ?? "").trim();
  store.name = trimmed;
  store.namePrefill = trimmed;
  store.nameError = null;
  if (fixtureMode || (ws && ws.readyState === WebSocket.OPEN)) sendJoin();
  // The first name on a room page: the socket waited for it (§6.1).
  else if (roomChecked && !ws && !reconnectTimer && store.conn !== "kicked") openSocket();
  update();
}

// "Take it back" after Kicked (§3.8): reconnect and Join with the stored
// token, which kicks the other connection.
export function reclaim() {
  if (fixtureMode) {
    log("out (fixture: not sent)", joinMessage());
    store.conn = "open";
    update();
    return;
  }
  attempt = 0;
  clearTimeout(reconnectTimer);
  reconnectTimer = null;
  openSocket();
}

// Fixture mode only: step through ?fixture=a,b,c (used by the debug panel).
// Next (+1) applies the next message on top of the current store, exactly as
// if it had just arrived (so local UI state such as the selection carries
// over — e.g. "Rejected keeps the selection" can be checked). Prev (-1)
// rewinds: a fresh store with messages 0..index-1 replayed in order.
export function stepFixture(delta) {
  const f = store.fixture;
  if (!f || f.messages.length === 0) return;
  const next = Math.max(0, Math.min(f.messages.length - 1, f.index + delta));
  if (next === f.index) return;
  if (next === f.index + 1) {
    f.index = next;
    handleMessage(f.messages[next].msg);
  } else {
    replayFixture(next);
  }
}

// ---------- the room check (§6.1) ----------

// GET /api/rooms/{code} → "ok" (200), "missing" (404), or "unreachable"
// (network error, or any other status). Never throws.
async function fetchRoomStatus(code) {
  try {
    const res = await fetch(`/api/rooms/${encodeURIComponent(code)}`, { cache: "no-store" });
    if (res.ok) return "ok";
    return res.status === 404 ? "missing" : "unreachable";
  } catch {
    return "unreachable";
  }
}

// Before the first connection: does the room exist? 404 → "No game with
// code …". Server unreachable → retry with the reconnect backoff.
async function checkRoom() {
  store.roomStatus = "checking";
  store.conn = attempt === 0 ? "connecting" : "reconnecting";
  update();
  const status = await fetchRoomStatus(store.roomCode);
  if (status === "missing") {
    store.roomStatus = "missing";
    store.conn = "idle";
    update();
    return;
  }
  if (status === "unreachable") {
    scheduleRetry(checkRoom);
    return;
  }
  roomChecked = true;
  store.roomStatus = "open";
  if (store.name) {
    openSocket();
  } else {
    // The name form; join() opens the socket.
    store.conn = "idle";
    update();
  }
}

// After a socket closes: if the room is gone (404) the game has ended — no
// reconnect loop. Otherwise (room still there, or server unreachable)
// reconnect with backoff as before.
async function onSocketLost() {
  store.conn = "reconnecting";
  update();
  const status = await fetchRoomStatus(store.roomCode);
  if (store.conn === "kicked" || ws) return; // superseded meanwhile (e.g. Take it back)
  if (status === "missing") {
    store.roomStatus = "ended";
    store.conn = "idle";
    update();
    return;
  }
  scheduleReconnect();
}

// ---------- WebSocket ----------

function wsUrl() {
  const proto = location.protocol === "https:" ? "wss:" : "ws:";
  return `${proto}//${location.host}/${encodeURIComponent(store.roomCode)}/ws`;
}

function joinMessage() {
  return {
    type: "Join",
    display_name: store.name,
    reconnect_token: (store.roomCode && getItem(session(), tokenKey(store.roomCode))) || null,
  };
}

function sendJoin() {
  if (!store.name) return;
  send(joinMessage());
}

function openSocket() {
  if (ws) {
    const old = ws;
    ws = null; // detach first so its close event is ignored
    try { old.close(); } catch { /* ignore */ }
  }
  store.conn = attempt === 0 ? "connecting" : "reconnecting";
  update();

  let socket;
  try {
    socket = new WebSocket(wsUrl());
  } catch (e) {
    log("error", { error: String(e) });
    onSocketLost();
    return;
  }
  ws = socket;

  socket.addEventListener("open", () => {
    if (ws !== socket) return;
    attempt = 0;
    store.conn = "open";
    sendJoin();
    update();
  });

  socket.addEventListener("message", (ev) => {
    if (ws !== socket) return;
    let msg;
    try {
      msg = JSON.parse(ev.data);
    } catch {
      log("in (unparseable)", { raw: String(ev.data) });
      update();
      return;
    }
    handleMessage(msg);
    if (store.conn === "kicked") {
      ws = null;
      try { socket.close(); } catch { /* ignore */ }
    }
  });

  socket.addEventListener("close", () => {
    if (ws !== socket) return; // stale socket, or we closed it on purpose
    ws = null;
    if (store.conn === "kicked") return; // no automatic reconnect after Kicked
    onSocketLost();
  });
}

function scheduleReconnect() {
  scheduleRetry(openSocket);
}

// Run `fn` after the next backoff delay (0.5s, 1s, 2s, 4s, then every 5s).
function scheduleRetry(fn) {
  store.conn = "reconnecting";
  const delay = attempt < BACKOFF_MS.length ? BACKOFF_MS[attempt] : BACKOFF_MAX_MS;
  attempt++;
  clearTimeout(reconnectTimer);
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    if (store.conn !== "kicked") fn();
  }, delay);
  update();
}

// ---------- ServerMessage handling ----------

function yourHand(state) {
  // your_hand from any in-game phase, or null (lobby / no state).
  const phase = state?.room?.InGame?.phase;
  if (!phase) return null;
  const inner = Object.values(phase)[0];
  return inner?.your_hand ?? null;
}

function showToast(text) {
  const until = Date.now() + TOAST_MS;
  store.toast = { text, until };
  setTimeout(() => {
    if (store.toast && store.toast.until <= Date.now()) {
      store.toast = null;
      update();
    }
  }, TOAST_MS + 20);
}

// A Rejected message's text, by its code in the current language
// (LANGUAGE_SPEC §4.3). A code with no string (t() → "?error.X?") falls
// back to the server's English message.
export function rejectedText(msg) {
  const key = `error.${msg.code}`;
  const text = t(key);
  if (text !== `?${key}?`) return text;
  return msg.message || String(msg.code);
}

// A game notice on the felt (table.js draws store.notice): neutral, not an
// error, and it goes away by itself. A newer notice replaces an older one.
function showNotice(text, ms) {
  const until = Date.now() + ms;
  store.notice = { text, until };
  setTimeout(() => {
    if (store.notice && store.notice.until <= Date.now()) {
      store.notice = null;
      update();
    }
  }, ms + 20);
}

function playingOf(state) {
  return state?.room?.InGame?.phase?.Playing ?? null;
}

// The notices a State brings, decided on the State transition (prev → next),
// never on a re-render, so each shows once:
//   - took_back newly set: "<name> took back their play", to everyone (§1).
//     A later State that still carries the same took_back (e.g. someone
//     reconnects) doesn't repeat it;
//   - Tribute → Playing after a tribute: the returns, to everyone (§3); once
//     per deal (remembered by dealKey), so it isn't shown again later.
function stateNotices(prev, next) {
  const p = playingOf(next);
  if (!p) return;
  const tb = p.took_back;
  if (Number.isInteger(tb) && playingOf(prev)?.took_back !== tb) {
    showNotice(t("net.tookBack", { name: seatName(next.seats, tb) }), TAKE_BACK_NOTICE_MS);
  }
  const fromTribute = Boolean(prev?.room?.InGame?.phase?.Tribute);
  const returns = returnsLine(p.deal_start, next.seats);
  if (fromTribute && returns) {
    const key = dealKeyOf(next);
    if (key === null || getItem(session(), RETURNS_SHOWN_KEY) !== key) {
      setItem(session(), RETURNS_SHOWN_KEY, key);
      showNotice(returns, RETURNS_NOTICE_MS);
    }
  }
}

function handleMessage(msg) {
  if (!msg || typeof msg !== "object" || Array.isArray(msg)) {
    log("in (ignored: not a message)", msg);
    update();
    return;
  }
  log("in", msg);

  switch (msg.type) {
    case "Joined": {
      store.joined = { seat: msg.seat, session_token: msg.session_token };
      if (store.roomCode) setItem(session(), tokenKey(store.roomCode), msg.session_token);
      if (store.name) {
        setItem(session(), NAME_KEY, store.name);
        setItem(local(), LAST_NAME_KEY, store.name);
      }
      store.nameError = null;
      if (store.conn !== "kicked") store.conn = "open";
      break;
    }

    case "State": {
      const prev = store.state;
      const prevHand = JSON.stringify(yourHand(prev));
      const nextHand = JSON.stringify(yourHand(msg));
      // Selection reset rule (§2.3): your_hand changed → clear selection and
      // close the reading picker. (Rejected never clears it.)
      if (prevHand !== nextHand) {
        store.selected.clear();
        store.reading = null;
      }
      store.state = msg;

      const phase = msg.room?.InGame?.phase;
      const inner = phase ? Object.values(phase)[0] : null;
      if (inner && inner.level) store.lastLevel = inner.level;

      // Keep the name in sync with the server (a token reclaim can keep the
      // old name, TECH_SPEC §6). In fixture mode this also supplies a name so
      // a State fixture doesn't stop at the name form.
      const serverName = msg.seats?.[msg.your_seat]?.display_name;
      if (serverName && (fixtureMode || store.joined)) {
        store.name = serverName;
        if (!fixtureMode) setItem(session(), NAME_KEY, serverName);
      } else if (fixtureMode && !store.name) {
        store.name = "(fixture)";
      }

      // Leaving a deal (every deal now ends in the lobby, LOBBY_FLOW_SPEC
      // §6.1, and Reset deal lands there too): drop the deal's local UI
      // state so none of it carries into the next deal.
      if (!msg.room?.InGame) {
        store.showLastTrick = false;
        store.notice = null;
        closeResetConfirm();
      } else {
        // And entering a deal closes the lobby's "Start over?" confirm.
        store.confirmNewMatch = false;
      }

      // Hand layout (HAND_LAYOUT_SPEC §3.3, §3.4): re-match card ids to the
      // new hand, drop played cards from groups. Entering the lobby (no
      // Tribute/Playing phase) resets ids and groups and clears the saved
      // copy; so does the first State of a new deal, even one with the same
      // dealKey as the deal before (a re-deal after Reset deal).
      // hand.js reads store.state, so this runs after it's set.
      onNewState(prev, msg);
      stateNotices(prev, msg);
      break;
    }

    case "Classified":
      // Reply to a group's Classify (§3.5); stale replies are ignored there.
      onClassified(msg);
      break;

    case "ChooseReading":
      store.reading = { cards: msg.cards, options: msg.options };
      break;

    case "Rejected":
      // A rejected Join: back to the name form (§3.1), prefilled with the
      // rejected name. Room full counts too — otherwise the tab would sit on
      // "Joining…" forever; from the form the player can retry, e.g. under a
      // disconnected player's name.
      if (msg.code === "NameTaken" || msg.code === "InvalidName" || msg.code === "NoSeatsAvailable") {
        store.nameError = { code: msg.code, message: msg.message };
        store.namePrefill = store.name || store.namePrefill;
        store.name = null;
        setItem(session(), NAME_KEY, null);
      } else {
        showToast(rejectedText(msg));
      }
      break;

    case "Kicked":
      store.conn = "kicked";
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
      break;

    default:
      // e.g. a ClientMessage fixture, or an unknown variant: shown in the log only.
      store.log[store.log.length - 1].dir = "in (ignored)";
      break;
  }
  update();
}

// ---------- fixture mode (§7) ----------

function fixtureNames() {
  return fixtureParam
    .split(",")
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

async function loadFixtures() {
  store.conn = "open";
  const names = fixtureNames();
  store.fixture = { names, messages: [], index: -1, error: null };
  update();

  const messages = [];
  const errors = [];
  for (const name of names) {
    if (!/^[A-Za-z0-9_.-]+$/.test(name) || name.includes("..")) {
      errors.push(`bad fixture name "${name}"`);
      continue;
    }
    // Relative to the page (client/index.html) → <repo>/fixtures/<name>.json
    const url = new URL(`../fixtures/${name}.json`, document.baseURI);
    try {
      const res = await fetch(url);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const data = await res.json();
      // One message per file, or an array of messages (a flow).
      const list = Array.isArray(data) ? data : [data];
      list.forEach((msg, i) =>
        messages.push({ label: list.length > 1 ? `${name}[${i}]` : name, msg }));
    } catch (e) {
      errors.push(`${name}: ${e.message || e}`);
    }
  }

  store.fixture.messages = messages;
  store.fixture.error = errors.length ? errors.join("; ") : null;
  if (messages.length > 0) replayFixture(0);
  else update();
}

function replayFixture(index) {
  const f = store.fixture;
  // Fresh store (keep display prefs and the loaded fixture list).
  store.conn = "open";
  store.joined = null;
  store.state = null;
  store.selected = new Set();
  store.reading = null;
  store.toast = null;
  store.notice = null;
  store.showLastTrick = false;
  store.lastLevel = null;
  store.log = [];
  initName();
  f.index = index;
  for (let i = 0; i <= index; i++) handleMessage(f.messages[i].msg);
  update();
}
