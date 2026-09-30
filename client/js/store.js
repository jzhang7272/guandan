// The one shared store (UI_SPEC §2.3) and change notification.
//
// net.js is the only writer of: conn, joined, state, reading, toast, notice, name,
// namePrefill, nameError, log, lastLevel, fixture, page, roomCode, roomStatus.
// hand.js is the only writer of: handIds, nextCardId, groups, dealKey, nextRequestId.
// Everything else is local UI state that handlers may mutate before calling
// update().

export const store = {
  conn: "connecting",      // "connecting" | "open" | "reconnecting" | "kicked" | "idle" (no socket wanted: Home, name form, game gone)
  joined: null,            // { seat, session_token } from Joined
  state: null,             // latest State message (RedactedState, incl. "type")
  selected: new Set(),     // indices into your_hand (NOT card values: hands have duplicates)
  reading: null,           // { cards, options } from ChooseReading, while the picker is open
  toast: null,             // { text, until } — last Rejected message (until = ms timestamp)
  notice: null,            // { text, until } — game notice on the felt (take back, returns)
  showLastTrick: false,
  confirmNewMatch: false,   // lobby.js: the inline "Start over?" confirm is open
  lastLevel: null,         // last seen deal level ("Five"); play.js's reading picker falls back to it

  name: null,              // current display name; null => show the name form
  namePrefill: "",         // prefill for the name form (last used / last rejected name)
  nameError: null,         // { code, message } of the last NameTaken / InvalidName / NoSeatsAvailable rejection (shown via net.js rejectedText, so it follows the language)
  log: [],                 // last 20 messages in/out: { dir, time, msg }

  // Hand layout (HAND_LAYOUT_SPEC §6), maintained by hand.js.
  handIds: [],             // local card id per your_hand index ("c0", "c1", …), stable across States
  nextCardId: 0,           // counter for fresh card ids
  groups: [],              // [{ groupId, cardIds, bomb: bool|null, readings, requestId }], creation order
  dealKey: null,           // identifies the current deal (hand.js dealKeyOf); a change resets groups
  nextRequestId: 1,        // per-tab Classify request_id counter

  debugOpen: false,       // debug panel visible (≡ button or ?debug=1)
  fixture: null,           // fixture mode only: { names, messages, index, error }

  // Rooms and invite codes (LOBBY_FLOW_SPEC §6.1), set by net.js from the path.
  page: null,              // "home" | "room" (fixture mode: "room")
  roomCode: null,          // "482193" on a room page (fixture mode: ?code=, else null)
  roomStatus: null,        // "checking" | "open" | "missing" (no such game) | "ended" (room closed)

  // Home (home.js local UI state, LOBBY_FLOW_SPEC §6.2).
  homeName: null,          // the name input as typed (null → namePrefill)
  homeCode: null,          // the invite-code input as typed
  homeError: null,         // { field: "name" | "create" | "code", text }
  homeBusy: false,         // Create game request in flight

  inviteCopied: null,      // lobby.js: "ok" | "manual" after Copy link (see copyInvite)
};

const listeners = [];
let scheduled = false;

function flush() {
  if (!scheduled) return;
  scheduled = false;
  for (const fn of listeners) fn();
}

// Schedule one render on the next animation frame (several calls in a row →
// one render). Background tabs don't run rAF, so a short timeout backs it up —
// otherwise the tab-title turn marker would never update in hidden tabs.
export function update() {
  if (scheduled) return;
  scheduled = true;
  if (typeof requestAnimationFrame === "function") requestAnimationFrame(flush);
  setTimeout(flush, 50);
}

export function onUpdate(fn) {
  listeners.push(fn);
}
