// Entry point: connect(), the §3 render dispatch (plus Home and the
// no-such-game pages, LOBBY_FLOW_SPEC §6.1), overlays (connection banner,
// toast, modal, debug panel), the EN | 中文 language toggle and <html lang>
// (LANGUAGE_SPEC §2, §3), and the tab-title turn marker (§3.4).
import { el } from "./dom.js";
import { store, update, onUpdate } from "./store.js";
import { connect } from "./net.js";
import { renderDebug } from "./debug.js";
import { renderNameForm, renderLobby, renderKicked } from "./lobby.js";
import { renderHome, renderRoomGone } from "./home.js";
import { renderGame } from "./table.js";
import { renderReadingPicker } from "./play.js";
import { initLang, setLang, getLang, langTag, t } from "./i18n.js";

const app = document.getElementById("app");

// §3 dispatch: exactly one screen. A deal is only ever Tribute or Playing;
// when it ends (or is reset) the server sends a lobby State, so the lobby
// is also the between-deals screen (LOBBY_FLOW_SPEC §6.4).
function renderScreen() {
  // Path "/" → Home; "/{code}" → check the room, then the name form, then
  // the game (net.js sets page / roomStatus; fixture mode is always a room).
  if (store.page === "home") return renderHome();
  if (store.roomStatus === "missing" || store.roomStatus === "ended") return renderRoomGone();
  if (store.page === null || store.roomStatus === "checking") {
    return el("div", { class: "waiting" }, store.roomCode ? t("main.lookingFor", { code: store.roomCode }) : t("main.loading"));
  }
  if (!store.name) return renderNameForm();
  const room = store.state?.room;
  if (!room) {
    return el("div", { class: "waiting" },
      store.conn === "open" ? t("main.joiningAs", { name: store.name }) : t("main.connecting"));
  }
  if ("Lobby" in room) return renderLobby();
  const phase = room.InGame?.phase;
  if (phase && ("Tribute" in phase || "Playing" in phase)) return renderGame();
  // Developer-only, like the fixture load error below: stays English.
  return el("div", { class: "waiting" }, "Unknown room state (see debug panel).");
}

function renderBanner() {
  if (store.fixture?.error) return el("div", { class: "conn-banner" }, `Fixture load error: ${store.fixture.error}`);
  if (store.conn === "connecting") return el("div", { class: "conn-banner" }, t("main.connecting"));
  if (store.conn === "reconnecting") return el("div", { class: "conn-banner" }, t("main.reconnecting"));
  return null;
}

function renderToast() {
  const toast = store.toast;
  if (!toast || toast.until <= Date.now()) return null;
  return el("div", { class: "toast", role: "status" }, toast.text);
}

// The language toggle (LANGUAGE_SPEC §2): a fixed pill in the top-left
// corner of every page, always reading "EN | 中文" so anyone can find their
// own language. Each half is a real button (40px tap target); the visible
// segment is the span inside it, drawn smaller. Switching only re-renders:
// nothing goes to the server. The ids keep focus on the toggle (captureFocus).
function renderLangToggle() {
  const current = getLang();
  const half = (lang, tag, label) => el("button", {
    type: "button",
    id: `lang-${lang}`,
    class: `lang-btn${lang === current ? " is-current" : ""}`,
    lang: tag,
    "aria-pressed": String(lang === current),
    onClick: () => { keptInputs = snapshotInputs(); setLang(lang); update(); },
  }, el("span", { class: "lang-seg" }, label));
  return el("div", { class: "lang-toggle", role: "group", "aria-label": t("lang.groupAria") },
    half("en", "en", t("lang.en")),
    half("zh", "zh-Hans", t("lang.zh")));
}

function overlay(node, extraClass) {
  return el("div", { class: `overlay ${extraClass || ""}`.trim() },
    el("div", { class: "overlay-box" }, node));
}

// Your move: your turn to play, or a tribute to pay/return. A kicked tab
// never claims the turn — the tab that took the seat over has it.
function isYourTurn() {
  if (store.conn === "kicked") return false;
  const s = store.state;
  const phase = s?.room?.InGame?.phase;
  if (phase?.Playing) return phase.Playing.turn === s.your_seat;
  return !!phase?.Tribute?.your_task;
}

// Keep a focused input's value / caret across the full re-render (e.g. the
// name form while a reconnect or toast triggers an update). Needs an `id`.
function captureFocus() {
  const a = document.activeElement;
  if (!a || !a.id || !app.contains(a)) return null;
  const isText = a.tagName === "INPUT" || a.tagName === "TEXTAREA";
  return {
    id: a.id,
    value: isText ? a.value : null,
    start: isText ? a.selectionStart : null,
    end: isText ? a.selectionEnd : null,
  };
}

// Typed-in text inputs aren't all kept in the store (the name form's input
// only survives a re-render while focused, via captureFocus), and clicking
// the toggle takes focus away from them. So the toggle snapshots every text
// input's value by id and the next render puts them back (§2: switching
// keeps inputs being typed in).
let keptInputs = null;

function snapshotInputs() {
  const values = new Map();
  for (const node of app.querySelectorAll("input[id], textarea[id]")) {
    if (node.tagName === "TEXTAREA" || node.type === "text") values.set(node.id, node.value);
  }
  return values;
}

function restoreInputs(values) {
  for (const [id, value] of values) {
    const node = document.getElementById(id);
    if (node && app.contains(node)) node.value = value;
  }
}

function restoreFocus(saved) {
  if (!saved) return;
  const node = document.getElementById(saved.id);
  if (!node) return;
  if (saved.value !== null && "value" in node) {
    node.value = saved.value;
    try { node.setSelectionRange(saved.start, saved.end); } catch { /* not a text input */ }
  }
  node.focus();
}

function render() {
  const saved = captureFocus();

  const nodes = [
    renderBanner(),
    el("main", { class: "screen" }, renderScreen()),
    store.reading ? overlay(renderReadingPicker(), "reading-overlay") : null,
    store.conn === "kicked" ? overlay(renderKicked(), "kicked-overlay") : null,
    renderToast(),
    renderLangToggle(),
    el("button", {
      class: "debug-toggle",
      title: t("main.debugTitle"),
      "aria-label": t("main.debugAria"),
      onClick: () => { store.debugOpen = !store.debugOpen; update(); },
    }, "≡"),
    store.debugOpen ? renderDebug() : null,
  ].filter(Boolean);

  app.replaceChildren(...nodes);
  if (keptInputs) { restoreInputs(keptInputs); keptInputs = null; }
  document.body.classList.toggle("debug-open", store.debugOpen);
  document.documentElement.lang = langTag();
  document.title = isYourTurn() ? t("main.titleYourTurn") : t("app.name");

  restoreFocus(saved);
}

initLang(); // §3: ?lang= → saved choice → browser language; before any render
store.debugOpen = new URLSearchParams(location.search).get("debug") === "1";
onUpdate(render);
connect(); // sets store.page first, so the first render shows the right page
render();
