// Name form, lobby, kicked overlay (UI_SPEC §3.1, §3.2, §3.8; the lobby's
// table settings, last deal and New match per LOBBY_FLOW_SPEC §6.3; its
// look — team sections, Ready card, compass names — per LOBBY_LOOK_SPEC §1).
//
// Server actions: join(name) (name form), send ChooseSeat / SetReady /
// UpdateSettings / NewMatch (lobby), reclaim() ("Take it back"). main.js
// wraps renderKicked() in the overlay backdrop, so it returns just the box
// content.
//
// Local UI state: store.confirmNewMatch — the inline "Start over?" confirm
// is open; store.inviteCopied — the invite line's Copy link feedback.
import { el } from "./dom.js";
import { store, update } from "./store.js";
import { join, reclaim, send, rejectedText } from "./net.js";
import { rankLabel, seatDirection, seatName, teamName } from "./format.js";
import { renderLastDeal } from "./results.js";
import { inviteLink } from "./home.js";
import { t, list } from "./i18n.js";

// Matches the server's limit (TECH_SPEC §6); the server still validates.
const NAME_MAX = 20;

// Partners sit opposite: seats 0 & 2 are team A, 1 & 3 team B.
const teamOf = (seat) => (seat % 2 === 0 ? "A" : "B");

// Every level a team can be at, in order (wire Rank strings).
const LEVELS = [
  "Two", "Three", "Four", "Five", "Six", "Seven", "Eight",
  "Nine", "Ten", "Jack", "Queen", "King", "Ace",
];

// ---------- §3.1 name form ----------

export function renderNameForm() {
  const onSubmit = (ev) => {
    ev.preventDefault();
    const input = ev.currentTarget.elements.namedItem("name");
    const name = input.value.trim();
    if (!name) {
      input.focus();
      return;
    }
    join(name);
  };

  return el("div", { class: "name-form-wrap" },
    el("form", { class: "name-form", onSubmit, autocomplete: "off" },
      el("h1", {}, t("app.name")),
      el("label", { for: "name-input" }, t("nameForm.yourName")),
      el("div", { class: "name-form-row" },
        el("input", {
          id: "name-input", // main.js keeps focus/value across re-renders by id
          name: "name",
          type: "text",
          value: store.namePrefill || "",
          maxlength: NAME_MAX,
          required: true,
          // Reject whitespace-only names before they reach the server.
          pattern: ".*\\S.*",
          autofocus: true,
          autocapitalize: "words",
          spellcheck: "false",
          "aria-invalid": store.nameError ? "true" : null,
          "aria-describedby": store.nameError ? "name-error" : null,
        }),
        el("button", { type: "submit", class: "primary" }, t("nameForm.join"))),
      store.nameError
        ? el("p", { id: "name-error", class: "name-error", role: "alert" }, rejectedText(store.nameError))
        : null));
}

// ---------- §3.2 lobby ----------

export function renderLobby() {
  const s = store.state;
  const lobby = s?.room?.Lobby;
  if (!s || !lobby) return el("div", { class: "waiting" }, "No lobby state.");

  // Top to bottom (LOBBY_LOOK_SPEC §1, the mockup): Last deal, the two team
  // sections, the Ready card, Settings, New match….
  return el("div", { class: "lobby" },
    el("div", { class: "lobby-head" },
      el("h1", { class: "lobby-title" }, t("lobby.title")),
      renderInvite()),
    renderLastDeal(),
    el("div", { class: "lobby-teams" }, renderTeam("A", s, lobby), renderTeam("B", s, lobby)),
    renderReadyCard(s, lobby),
    renderSettings(lobby),
    // Only when there's a match to throw away (§6.3).
    lobby.seats_locked || lobby.last_deal ? renderNewMatch() : null);
}

// ---------- invite line (LOBBY_FLOW_SPEC §6.3) ----------

// "Invite: XYZ234 [Copy link]". No code (fixture mode without ?code=) → nothing.
function renderInvite() {
  const code = store.roomCode;
  if (!code) return null;
  const link = inviteLink(location.origin, code);
  const copied = store.inviteCopied === "ok";
  return el("div", { class: "lobby-invite" },
    el("span", { class: "invite-label" }, t("lobby.invite")),
    el("strong", { class: "invite-code", title: link }, code),
    el("button", {
      type: "button",
      class: copied ? "invite-copy is-copied" : "invite-copy",
      onClick: () => copyInvite(link),
    }, copied ? t("lobby.copied") : t("lobby.copyLink")),
    // No clipboard access (e.g. plain http on a LAN address): show the link
    // selected so the player can copy it by hand.
    store.inviteCopied === "manual"
      ? el("input", {
        id: "invite-link", // main.js keeps focus across re-renders by id
        class: "invite-manual",
        type: "text",
        readonly: true,
        value: link,
        "aria-label": t("lobby.inviteManualAria"),
        onFocus: (ev) => ev.currentTarget.select(),
        onClick: (ev) => ev.currentTarget.select(),
      })
      : null);
}

const COPIED_MS = 2000;
let copiedTimer = null;

async function copyInvite(link) {
  let copied = false;
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(link);
      copied = true;
    }
  } catch { /* denied or unavailable: try the old way */ }
  if (!copied) copied = copyWithSelection(link);
  store.inviteCopied = copied ? "ok" : "manual";
  update();
  clearTimeout(copiedTimer);
  if (copied) {
    copiedTimer = setTimeout(() => {
      if (store.inviteCopied === "ok") store.inviteCopied = null;
      update();
    }, COPIED_MS);
  }
}

// The pre-Clipboard-API fallback (works outside secure contexts in most
// browsers): select a hidden textarea's text and run the copy command.
function copyWithSelection(text) {
  const ta = document.createElement("textarea");
  ta.value = text;
  ta.setAttribute("readonly", "");
  ta.style.position = "fixed";
  ta.style.opacity = "0";
  document.body.appendChild(ta);
  let ok = false;
  try {
    ta.select();
    ok = document.execCommand("copy");
  } catch { ok = false; }
  ta.remove();
  return ok;
}

// ---------- seats: one section per team (LOBBY_LOOK_SPEC §1, §2) ----------

// A team's two spots in seat order (South before North, East before West;
// seat 0 is South — format.js seatDirection).
function renderTeam(team, s, lobby) {
  const mine = s.your_seat !== null && s.your_seat !== undefined && teamOf(s.your_seat) === team;
  const spots = (team === "A" ? [0, 2] : [1, 3]).map((seat) => renderSpot(seat, s, lobby));
  return el("section", {
    class: `lobby-team team-${team.toLowerCase()}${mine ? " is-mine" : ""}`,
    "aria-label": t("team.named", { team: teamName(team) }),
  },
  el("h2", { class: "lobby-team-head" },
    t("team.named", { team: teamName(team) }),
    mine ? el("span", { class: "lobby-pill" }, t("lobby.yourTeam")) : null),
  el("div", { class: "lobby-spots" }, spots));
}

function renderSpot(seat, s, lobby) {
  const info = s.seats?.[seat] || { display_name: null, connected: false };
  const dir = el("span", { class: "spot-dir" }, seatDirection(seat));

  if (!info.display_name) {
    // Seats are locked between the deals of a match (the next deal's tribute
    // depends on who finished where): no "Sit here" then.
    return el("div", { class: "lobby-spot is-empty" },
      dir,
      lobby.seats_locked
        ? el("span", { class: "muted" }, t("lobby.empty"))
        : el("button", { type: "button", onClick: () => send({ type: "ChooseSeat", seat }) }, t("lobby.sitHere")));
  }

  const isYou = seat === s.your_seat;
  const isReady = !!lobby.ready?.[seat];
  return el("div", { class: isYou ? "lobby-spot is-you" : "lobby-spot" },
    el("div", { class: "spot-who" },
      dir,
      el("div", { class: "spot-name" },
        el("span", { class: "spot-name-text" }, info.display_name),
        isYou ? el("span", { class: "spot-you" }, t("lobby.you")) : null),
      el("div", { class: info.connected ? "spot-conn is-on" : "spot-conn" },
        el("span", { class: "spot-dot", "aria-hidden": "true" }),
        info.connected ? t("lobby.connected") : t("lobby.offline"))),
    isReady ? el("span", { class: "spot-tick", title: t("lobby.ready"), "aria-label": t("lobby.ready") }, "✓") : null);
}

// ---------- Ready card (LOBBY_LOOK_SPEC §1) ----------

// The big I'm ready / Ready ✓ toggle, who we're waiting for, and the
// locked-seats note.
function renderReadyCard(s, lobby) {
  const you = s.your_seat;
  const seats = s.seats || [];
  const ready = lobby.ready || [];
  const isReady = !!ready[you];

  const occupied = [0, 1, 2, 3].filter((i) => seats[i]?.display_name);
  const empty = 4 - occupied.length;
  // Not-ready players in seat order, with you ("you") last.
  const notReady = occupied.filter((i) => !ready[i] && i !== you).map((i) => seatName(seats, i));
  if (occupied.includes(you) && !ready[you]) notReady.push(t("lobby.youInList"));

  let waiting;
  if (empty > 0) {
    waiting = [t("lobby.waitingMore", { n: empty }),
      notReady.length ? t("lobby.notReady", { names: notReady }) : null];
  } else if (notReady.length) {
    waiting = [t("lobby.waitingForPre"), el("strong", {}, list(notReady)), t("lobby.waitingForPost")];
  } else {
    waiting = [t("lobby.starting")];
  }

  return el("section", { class: "lobby-ready" },
    el("button", {
      type: "button",
      class: isReady ? "ready-toggle is-on" : "ready-toggle primary",
      "aria-pressed": isReady ? "true" : "false",
      title: isReady ? t("lobby.cancelReadyTitle") : t("lobby.readyTitle"),
      onClick: () => send({ type: "SetReady", ready: !isReady }),
    }, isReady ? t("lobby.readyOn") : t("lobby.imReady")),
    el("p", { class: "lobby-waiting" }, waiting),
    lobby.seats_locked
      ? el("p", { class: "lobby-locked" }, t("lobby.seatsLocked"))
      : null);
}

// ---------- table settings (LOBBY_FLOW_SPEC §6.3, GAME_RULES house rule #9) ----------

// Anyone may change the levels and the declaring team. Every change sends
// the whole settings at once; the server validates (e.g. "nobody declaring"
// once seats are locked) and un-readies everyone.
function renderSettings(lobby) {
  const progress = lobby.progress || {};
  const levels = progress.team_levels || ["Two", "Two"];
  const declaring = progress.declaring ?? null;
  const attempts = progress.a_attempts || [0, 0];
  const locked = !!lobby.seats_locked;

  const sendSettings = (teamLevels, decl) =>
    send({ type: "UpdateSettings", team_levels: teamLevels, declaring: decl });

  const teamRow = (team) => {
    const i = team === "A" ? 0 : 1;
    const id = `level-${team.toLowerCase()}`; // main.js keeps focus across re-renders by id
    const onChange = (ev) => {
      const next = [...levels];
      next[i] = ev.currentTarget.value;
      sendSettings(next, declaring);
    };
    return el("div", { class: `settings-team team-${team.toLowerCase()}` },
      el("label", { for: id },
        el("strong", { class: "settings-team-name" }, teamName(team)), t("lobby.levelSuffix")),
      el("select", { id, onChange },
        LEVELS.map((lv) => el("option", { value: lv, selected: lv === levels[i] }, rankLabel(lv)))),
      // A-attempts are shown, not editable, and only mean something at A.
      levels[i] === "Ace"
        ? el("span", { class: "settings-tries" }, t("common.aTries", { n: attempts[i] ?? 0 }))
        : null);
  };

  const choices = [["A", teamName("A")], ["B", teamName("B")]];
  // "Nobody declaring" is only allowed before a match's first deal.
  if (!locked) choices.push([null, t("lobby.declaringNone")]);
  const radios = choices.map(([value, label]) => {
    const id = `declaring-${value ? value.toLowerCase() : "none"}`;
    return el("label", { class: "settings-choice", for: id },
      el("input", {
        id,
        type: "radio",
        name: "declaring",
        checked: declaring === value,
        onChange: () => sendSettings([...levels], value),
      }),
      label);
  });

  return el("section", { class: "lobby-settings", "aria-label": t("lobby.settingsAria") },
    el("div", { class: "settings-head" },
      el("h2", {}, t("lobby.settings")),
      el("span", { class: "muted" }, t("lobby.settingsNote"))),
    teamRow("A"),
    teamRow("B"),
    el("div", { class: "settings-declaring", role: "radiogroup", "aria-label": t("lobby.declaringAria") },
      el("strong", { class: "settings-team-name" }, t("lobby.declaring")), radios));
}

// ---------- New match (LOBBY_FLOW_SPEC §6.3) ----------

// An inline confirm, not window.confirm.
function renderNewMatch() {
  const setConfirm = (on) => {
    store.confirmNewMatch = on;
    update();
  };
  if (!store.confirmNewMatch) {
    return el("div", { class: "lobby-actions" },
      el("button", { type: "button", onClick: () => setConfirm(true) }, t("lobby.newMatch")));
  }
  return el("div", { class: "lobby-actions is-confirm", role: "group", "aria-label": t("lobby.newMatchAria") },
    el("span", { class: "lobby-confirm-text" }, t("lobby.startOverQuestion")),
    el("button", {
      type: "button",
      class: "danger",
      onClick: () => {
        send({ type: "NewMatch" });
        setConfirm(false);
      },
    }, t("lobby.startOver")),
    el("button", { type: "button", onClick: () => setConfirm(false) }, t("common.cancel")));
}

// ---------- §3.8 kicked overlay ----------

export function renderKicked() {
  return el("div", { class: "kicked" },
    el("h2", {}, t("kicked.title")),
    el("p", {}, t("kicked.body")),
    el("p", { class: "muted" }, t("kicked.hint")),
    el("div", { class: "kicked-actions" },
      el("button", { class: "primary", onClick: () => reclaim() }, t("kicked.takeBack"))));
}
