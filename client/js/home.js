// Home page (LOBBY_FLOW_SPEC §6.2), the "no such game" / "game has ended"
// pages (§6.1), and the pure invite-code helpers used by net.js's routing
// and lobby.js's invite line.
//
// Server actions: createRoom() (POST /api/rooms), rememberName() and a page
// navigation to /{code}; net.js does the room check and the WebSocket there.
//
// Local UI state: store.homeName / store.homeCode (the two inputs, kept so a
// re-render never loses what was typed), store.homeError ({ field, key, params }: a string key, not text, so a
// language switch translates an error already on screen), store.homeBusy.
import { el } from "./dom.js";
import { store, update } from "./store.js";
import { createRoom, rememberName } from "./net.js";
import { t } from "./i18n.js";

// Matches the server's limit (TECH_SPEC §6); the server still validates.
const NAME_MAX = 20;

// Invite codes (LOBBY_FLOW_SPEC §5.1): 6 digits, e.g. "482193". Leading
// zeros count ("012345"), so a code is always a string, never a number.
export const CODE_LENGTH = 6;
const CODE_RE = new RegExp(`^[0-9]{${CODE_LENGTH}}$`);

// ---------- pure helpers (checked in tests.html) ----------

// " 482193 " → "482193"; anything that isn't exactly 6 ASCII digits after
// trimming (like the server's parser) → null.
export function parseRoomCode(text) {
  const code = String(text ?? "").trim();
  return CODE_RE.test(code) ? code : null;
}

// What the player typed in Home's invite-code box → the code to try.
// Pasting the whole invite link works too (its last path segment is the
// code). Full-width digits (a Chinese keyboard's "４８２１９３") become
// ASCII ones, then everything that isn't a digit is dropped ("482 193",
// "482-193"). Not validated here: see parseRoomCode.
export function codeFromInput(text) {
  let s = String(text ?? "").trim();
  const q = s.search(/[?#]/);
  if (q >= 0) s = s.slice(0, q);
  s = s.replace(/\/+$/, "");
  const slash = s.lastIndexOf("/");
  if (slash >= 0) s = s.slice(slash + 1);
  return s
    .replace(/[\uFF10-\uFF19]/g, (d) => String.fromCharCode(d.charCodeAt(0) - 0xFF10 + 48))
    .replace(/[^0-9]+/g, "");
}

// location.pathname → which page to show (§6.1):
//   { page: "home" }                     "/" (and "/…/index.html": the static
//                                        fixture server, when not in fixture mode)
//   { page: "room", code: "482193" }     "/482193" or "/482193/"
//   { page: "room", code: null, raw }    one path segment that can't be a code
//                                        (shown as "No game with code …")
//   anything else                        home
export function routeFromPath(pathname) {
  const path = String(pathname ?? "/");
  if (path === "/" || path === "" || /\/index\.html$/.test(path)) return { page: "home" };
  const m = /^\/([^/]+)\/?$/.exec(path);
  if (!m) return { page: "home" };
  let raw;
  try { raw = decodeURIComponent(m[1]); } catch { raw = m[1]; }
  const code = parseRoomCode(raw);
  return code ? { page: "room", code } : { page: "room", code: null, raw };
}

// The link to share for a room: origin + "/" + code (§6.3).
export function inviteLink(origin, code) {
  return `${origin}/${code}`;
}

// ---------- Home (§6.2) ----------

const nameValue = () => store.homeName ?? store.namePrefill ?? "";

// Both buttons need a name: returns it, or sets the error and returns null.
function requireName() {
  const name = nameValue().trim();
  if (name) return name;
  store.homeError = { field: "name", key: "home.errNameFirst" };
  update();
  return null;
}

function goToRoom(code) {
  // Keep ?debug=1 across the navigation; the name travels in storage.
  const debug = new URLSearchParams(location.search).get("debug") === "1";
  location.assign(`/${encodeURIComponent(code)}${debug ? "?debug=1" : ""}`);
}

async function onCreate() {
  const name = requireName();
  if (!name || store.homeBusy) return;
  store.homeBusy = true;
  store.homeError = null;
  update();
  const result = await createRoom();
  if (result.code) {
    rememberName(name);
    goToRoom(result.code); // stays "busy" until the page unloads
    return;
  }
  store.homeBusy = false;
  store.homeError = {
    field: "create",
    key: result.full ? "home.errServerFull" : "home.errCreateFailed",
  };
  update();
}

function onJoin(ev) {
  ev.preventDefault();
  const name = requireName();
  if (!name) return;
  const raw = String(store.homeCode ?? "").trim();
  if (!raw) {
    store.homeError = { field: "code", key: "home.errCodeMissing" };
    update();
    return;
  }
  const code = parseRoomCode(codeFromInput(raw));
  if (!code) {
    // Quote what they typed, not the digits left after stripping.
    store.homeError = {
      field: "code",
      key: "home.errCodeInvalid",
      params: { typed: raw.slice(0, 20), length: CODE_LENGTH },
    };
    update();
    return;
  }
  rememberName(name);
  goToRoom(code);
}

export function renderHome() {
  const err = store.homeError;
  const errorFor = (field) =>
    err && err.field === field
      ? el("p", { id: `home-${field}-error`, class: "home-error", role: "alert" }, t(err.key, err.params))
      : null;
  const clearError = () => {
    if (store.homeError) {
      store.homeError = null;
      update();
    }
  };

  return el("div", { class: "home" },
    el("div", { class: "home-card" },
      el("h1", { class: "home-title" },
        // The tab icon, beside the name (decorative: the text says it).
        el("img", { class: "home-logo", src: "assets/guandan.png", alt: "", width: 40, height: 40 }),
        el("span", {}, t("app.name"))),

      el("label", { class: "home-label", for: "home-name" }, t("home.yourName")),
      el("input", {
        id: "home-name", // main.js keeps focus/value across re-renders by id
        type: "text",
        value: nameValue(),
        maxlength: NAME_MAX,
        autocomplete: "off",
        autocapitalize: "words",
        spellcheck: "false",
        autofocus: nameValue() ? null : true,
        "aria-invalid": err?.field === "name" ? "true" : null,
        "aria-describedby": err?.field === "name" ? "home-name-error" : null,
        onInput: (ev) => {
          store.homeName = ev.currentTarget.value;
          if (store.homeError?.field === "name") clearError();
        },
      }),
      errorFor("name"),

      el("button", {
        type: "button",
        class: "primary home-create",
        disabled: store.homeBusy,
        onClick: onCreate,
      }, store.homeBusy ? t("home.creating") : t("home.createGame")),
      errorFor("create"),

      el("div", { class: "home-or", role: "separator" }, el("span", {}, t("home.or"))),

      // novalidate: the input's pattern is only a keyboard hint; onJoin
      // cleans up "482 193" or a pasted link, which the pattern would block.
      el("form", { class: "home-join", onSubmit: onJoin, autocomplete: "off", novalidate: true },
        el("label", { class: "home-label", for: "home-code" }, t("home.inviteCode")),
        el("div", { class: "home-join-row" },
          el("input", {
            id: "home-code",
            type: "text",
            value: store.homeCode ?? "",
            placeholder: t("home.codePlaceholder"),
            // A number pad on phones (pattern is iOS's hint); type stays
            // "text" so a pasted "482 193" or invite link isn't refused.
            inputmode: "numeric",
            pattern: "[0-9]*",
            spellcheck: "false",
            "aria-invalid": err?.field === "code" ? "true" : null,
            "aria-describedby": err?.field === "code" ? "home-code-error" : null,
            onInput: (ev) => {
              store.homeCode = ev.currentTarget.value;
              if (store.homeError?.field === "code") clearError();
            },
          }),
          el("button", { type: "submit", disabled: store.homeBusy }, t("home.joinGame"))),
        errorFor("code"))));
}

// ---------- no such game / game ended (§6.1) ----------

// store.roomStatus "missing" (the code was never valid or is unknown) or
// "ended" (the room closed while we were in it).
export function renderRoomGone() {
  const ended = store.roomStatus === "ended";
  const shown = String(store.roomCode ?? "").slice(0, 20);
  return el("div", { class: "home" },
    el("div", { class: "home-card home-gone" },
      el("h1", { class: "home-title" }, ended ? t("gone.ended") : t("gone.noGame", { code: shown })),
      el("p", { class: "muted" },
        ended
          ? t("gone.endedHint")
          : t("gone.noGameHint")),
      el("button", {
        type: "button",
        class: "primary",
        onClick: () => location.assign("/"),
      }, t("gone.backHome"))));
}
