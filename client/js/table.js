// Game table: header, felt table with seat plates and per-seat play zones,
// center text (UI_SPEC §3.3, §4.5 as amended by TABLE_LOOK_SPEC).
// Per §2.2, renderGame() lays out the table and then appends the phase's
// controls (tribute panel / hand + action bar) below it.
//
// Works for both in-game phases (LOBBY_FLOW_SPEC: a finished deal goes
// straight back to the lobby, so there's no DealOver / MatchOver screen).
// What each phase provides:
//   Tribute   — level; no trick, no turn
//   Playing   — level, turn, trick, last_trick, finish_order, deal_start
//
// Layout (TABLE_LOOK_SPEC §2, §1b): the partner's plate above the felt, the
// left / right opponents' plates hugging its sides, yours below it. On the
// felt, one zone per seat (top / left / right / bottom = you) shows that
// seat's entries in the shown trick (plays, and passes as "PASS" cards) as a
// pile (§1c, GAME_PAGE_V3_SPEC §5b.3); the center shows text only when it's
// relevant (who leads, 接风, phase name). Faint corner arrows show the
// direction of play, and a "next" chevron marks who acts after the current
// turn (GAME_PAGE_V3_SPEC §4). The header is a ribbon with the levels (§5).
import { el } from "./dom.js";
import { store, update } from "./store.js";
import { send } from "./net.js";
import {
  seatAt, seatName, comboLabel, rankLabel, placeLabel, teamName, teamRelative, nextActiveSeat,
} from "./format.js";
import { t, getLang } from "./i18n.js";
import { cardRow } from "./cards.js";
import { renderPlayControls } from "./play.js";
import { renderTributeControls } from "./tribute.js";

// Seats 0 and 2 are team A, 1 and 3 team B; `team_levels` / `a_attempts`
// are indexed by team (A = 0, B = 1). Players see them as North-South /
// East-West (format.js teamName); the phone header's short labels are
// relative to you, Us / Them (format.js teamRelative).
const TEAMS = ["A", "B"];
const teamOf = (seat) => TEAMS[seat % 2];

// The other players' plates, in DOM order (the phone layout shows them as a
// row: left, partner, right).
const OTHER_POSITIONS = ["left", "top", "right"];
const ZONE_POSITIONS = ["top", "left", "right", "bottom"];

function inGame() {
  return store.state?.room?.InGame ?? null;
}

// The phase's name and body: { name: "Playing", body: {...} } (or null).
function currentPhase() {
  const phase = inGame()?.phase;
  if (!phase) return null;
  for (const name of ["Tribute", "Playing"]) {
    if (phase[name]) return { name, body: phase[name] };
  }
  return null;
}

// dealLevel() → the level Rank of the current deal, e.g. "Five", or null
// outside a game. Both Tribute and Playing carry it as `level`.
export function dealLevel() {
  return currentPhase()?.body.level ?? null;
}

// ---------- trick helpers ----------

// The entries of the trick being played (Playing). Tribute has none.
function shownTrick(p) {
  return p?.name === "Playing" ? p.body.trick || [] : [];
}

// Finish order (for place badges): who has gone out so far this deal.
function finishOrder(p) {
  return p?.name === "Playing" ? p.body.finish_order || [] : [];
}

// TrickEntry → { seat, play } (play null for a pass).
function entryInfo(entry) {
  if (entry?.Played) return { seat: entry.Played.seat, play: entry.Played.play };
  if (entry?.Passed) return { seat: entry.Passed.seat, play: null };
  return { seat: null, play: null };
}

// The last Played entry — the current best play — as { seat, play, index },
// or null.
function bestEntry(entries) {
  for (let i = entries.length - 1; i >= 0; i--) {
    const info = entryInfo(entries[i]);
    if (info.play) return { ...info, index: i };
  }
  return null;
}

// ---------- header ribbon (GAME_PAGE_V3_SPEC §5) ----------

// One team's side of the ribbon: its name (outside) and a small box with its
// level (next to the pennant). The declaring team's box is filled with the
// team color and gets a ★; "Attempts: n/3" sits under the box of a team at A.
// `side` is "mine" (left) or "theirs" (right; box first, then the name).
// On phones the name is the short label relative to you (Us / Them), so the
// "(your team)" note is shown on wide screens only.
function ribbonSide(g, team, side, yourTeam) {
  const i = TEAMS.indexOf(team);
  const lv = rankLabel(g.team_levels?.[i]);
  const declaring = g.declaring === team;
  const mine = side === "mine";
  const triesN = g.team_levels?.[i] === "Ace" ? (g.a_attempts?.[i] ?? 0) : null;
  const tries = triesN !== null ? t("common.aTries", { n: triesN }) : null;
  const what = t("table.sideAria", { team: teamName(team), mine, level: lv, declaring, tries: triesN });

  const name = el("span", { class: "rb-name" },
    el("span", { class: "long" }, t("team.named", { team: teamName(team) })),
    el("span", { class: "short" }, teamRelative(team, yourTeam)),
    mine ? el("span", { class: "rb-you long" }, t("table.yourTeam")) : null);
  const box = el("span", { class: "rb-box-wrap" },
    el("span", { class: `rb-box${declaring ? " is-declaring" : ""}` },
      lv, declaring ? el("span", { class: "rb-star", "aria-hidden": "true" }, "★") : null),
    // Phones show just "1/3": the full "Attempts: 1/3" would squeeze the
    // Us / Them name (the whole label is still the side's hover / aria).
    tries ? el("span", { class: "rb-tries" },
      el("span", { class: "long" }, tries),
      el("span", { class: "short" }, `${triesN}/3`)) : null);

  return el("div", {
    class: `rb-side rb-${side} team-${team.toLowerCase()}`,
    title: what,
    "aria-label": what,
  }, mine ? [name, box] : [box, name]);
}

// The pennant in the middle of the ribbon, hanging above and below it: a
// small "level" label and the deal level big. Three layers: the shadow (a
// filter on the clipped shape itself would be clipped away), the gold edge,
// and the green face with the text.
function renderPennant(level) {
  const lv = level ? rankLabel(level) : "?";
  return el("div", {
    class: "pennant",
    title: t("table.pennantTitle", { level: lv }),
    "aria-label": t("table.pennantAria", { level: lv }),
  },
    el("div", { class: "pennant-edge" },
      el("div", { class: "pennant-face" },
        el("span", { class: "pennant-label" }, t("table.pennantLabel")),
        el("span", { class: "pennant-level" }, lv))));
}

function renderHeader(g, p, level, you) {
  const reset = el("button", {
    type: "button",
    class: "reset-deal-btn",
    "aria-expanded": resetConfirmOpen ? "true" : "false",
    onClick: () => { resetConfirmOpen = !resetConfirmOpen; update(); },
  }, t("table.redeal"));

  // The ribbon (GAME_PAGE_V3_SPEC §5, §5b.1): your team on the left, the
  // pennant in the middle, the other team on the right, across the full
  // width. The header is a 3-column grid with equal outer columns, so the
  // pennant sits over the middle of the table. Redeal is inline at the
  // ribbon's right end (a row under it on phones).
  const yourTeam = teamOf(you);
  const otherTeam = TEAMS.find((t) => t !== yourTeam);
  return el("header", { class: "table-header" },
    ribbonSide(g, yourTeam, "mine", yourTeam),
    renderPennant(level),
    ribbonSide(g, otherTeam, "theirs", yourTeam),
    // Redeal sits inline at the ribbon's right end (it's about the whole
    // deal). Last trick / Take back are about the trick, so they live in the
    // hand box (play.js) — user decision.
    el("div", { class: "hdr-actions" }, reset),
    resetConfirmOpen ? renderResetConfirm() : null);
}

// ---------- Redeal (LOBBY_FLOW_SPEC §6.4; the button was "Reset deal…",
// renamed by Decision 42) ----------

// Whether the "are you sure?" box is open. On a computer it floats beside
// the Redeal button (adds no height, so the page doesn't scroll); on a phone
// it's a row under the header. Local UI state; net.js closes it whenever the room leaves the deal, so it never
// reappears in the next one.
let resetConfirmOpen = false;

export function closeResetConfirm() {
  resetConfirmOpen = false;
}

function renderResetConfirm() {
  const cancel = () => { resetConfirmOpen = false; update(); };
  return el("div", { class: "reset-confirm", role: "group", "aria-label": t("table.redeal") },
    el("span", { class: "reset-confirm-text" }, t("table.redealConfirm")),
    el("span", { class: "reset-confirm-buttons" },
      el("button", {
        type: "button",
        class: "primary",
        onClick: () => { resetConfirmOpen = false; send({ type: "ResetDeal" }); },
      }, t("table.redeal")),
      el("button", { type: "button", onClick: cancel }, t("common.cancel"))));
}

// ---------- seat plates (§3.3) ----------

function countLabel(count) {
  if (count === "MoreThanTen") return "10+";
  if (count && typeof count.Exact === "number") return String(count.Exact);
  return "?";
}

// Everything a plate needs to know about one seat.
function seatFacts(g, p, seat) {
  const s = store.state;
  const info = s.seats?.[seat];
  const place = finishOrder(p).indexOf(seat);
  const playing = p?.name === "Playing";
  // Who acts after the current turn, skipping players who are out (§4).
  const next = playing ? nextActiveSeat(p.body.turn, g.card_counts, finishOrder(p)) : null;
  return {
    name: seatName(s.seats, seat),
    // An empty seat mid-game shows as disconnected too.
    offline: !info || info.connected === false,
    count: countLabel(g.card_counts?.[seat]),
    place: place >= 0 ? placeLabel(place) : null,
    turn: playing && p.body.turn === seat,
    next: playing && next === seat && next !== p.body.turn,
  };
}

// The "next" chevron: faint, in the plate's corner, taking no layout space
// (§4). No text on screen; the hover title and label say what it means.
function nextMarker() {
  const what = t("table.playsNext");
  return el("span", { class: "plate-next", title: what, "aria-label": what }, "›");
}

// `role` ("(partner)") follows the name after a space in English; the
// Chinese one has full-width brackets, so no space.
function renderPlate(f, pos, label, role) {
  const roleText = role ? (getLang() === "zh" ? role : ` ${role}`) : null;
  const classes = ["plate", `plate-${pos}`, f.turn ? "is-turn" : "", f.offline ? "is-offline" : ""];
  return el("section", { class: classes.filter(Boolean).join(" ") },
    f.next ? nextMarker() : null,
    el("span", { class: "plate-name" }, label,
      roleText ? el("span", { class: "plate-role" }, roleText) : null),
    el("span", { class: "plate-badges" },
      el("span", { class: "badge badge-count", title: t("table.cardsLeft") }, f.count),
      f.place ? el("span", { class: "badge badge-place" }, f.place) : null,
      f.offline ? el("span", { class: "badge badge-off", title: t("table.disconnected") }, t("table.offline")) : null,
      f.turn ? el("span", { class: "badge badge-turn" }, t("table.turn")) : null));
}

// ---------- play zones: a pile per seat (TABLE_LOOK_SPEC §1c.3, §5) ----------

const SLIDE_MS = 200; // keep in step with the zone-in animation in style.css
const FADE_MS = 300;  // keep in step with the zone-out animation in style.css
const PILE_DEPTH = 3; // the newest entry + up to 2 earlier ones

// When each entry of the live trick first appeared: entry key → time. The
// whole screen is rebuilt on every render, so this is what tells a new entry
// (slide it in) from one already on the table (don't replay). `primed` stays
// false until the first game render, so what's on the table when the page
// loads just appears.
const entrySeen = new Map();
let primed = false;

// The finished trick fading out: { key, at }, keyed on last_trick so the
// fade runs once per finished trick. `fadeTimer` re-renders when it's over.
let fadeSeen = null;
let fadeTimer = null;

function reducedMotion() {
  return typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
}

// Identifies entry `i` within its trick: the entry itself, its position,
// and the trick's opening play (so the same Pass in the next trick counts
// as new).
function entryKey(entries, i) {
  return `${JSON.stringify(entries[i])}#${i}|${JSON.stringify(entries[0] ?? null)}`;
}

// → ms since the live entry first appeared (0 = new now), or null when it's
// been there longer than the slide.
function slideAge(key, now) {
  if (!entrySeen.has(key)) entrySeen.set(key, primed ? now : -Infinity);
  const age = now - entrySeen.get(key);
  return age < SLIDE_MS ? age : null;
}

// One seat's pile in `entries`: { layers }, the seat's entries in the trick
// (plays AND passes, GAME_PAGE_V3_SPEC §5b.3), oldest first, at most
// PILE_DEPTH, as { play, best, age } (play null for a pass). `bestIndex` is
// the trick's best play; `ages` (entry index → slide age) only for the live
// trick.
function seatPile(entries, seat, bestIndex, ages) {
  const layers = [];
  entries.forEach((entry, i) => {
    const info = entryInfo(entry);
    if (info.seat !== seat) return;
    layers.push({ play: info.play, best: i === bestIndex, age: ages?.get(i) ?? null });
  });
  return { layers: layers.slice(-PILE_DEPTH) };
}

// seat → pile for every seat, with `extra` fields (dim / fading / age).
function trickPiles(entries, seats, extra, ages) {
  const bestIndex = bestEntry(entries)?.index ?? -1;
  return new Map(seats.map((seat) => [seat, { ...seatPile(entries, seat, bestIndex, ages), ...extra }]));
}

// What the zones show: { piles: seat → { layers, dim, fading, age },
// fading, lastShown } (fading: the finished trick is fading out;
// lastShown: the "Last trick" toggle's view).
//   "Last trick" on        — the last trick as piles, dimmed; one "Last
//                            trick" label in the center. The only time the
//                            last trick is shown (user decision).
//   Trick under way        — the trick as piles; the best play glows.
//   Trick just finished    — the finished trick's piles fading out (FADE_MS).
//   Trick empty after that — nothing: the felt is clear for the next lead.
function zoneContents(p, seats) {
  if (!p || p.name !== "Playing") {
    entrySeen.clear();
    fadeSeen = null;
    return { piles: new Map(), fading: false };
  }
  const now = performance.now();
  const entries = shownTrick(p);
  const last = p.body.last_trick;

  // Record the live trick even while "Last trick" hides it: an entry that
  // arrives meanwhile then doesn't slide in late when the toggle goes off.
  const ages = new Map();
  const liveKeys = new Set();
  entries.forEach((_, i) => {
    const key = entryKey(entries, i);
    liveKeys.add(key);
    ages.set(i, slideAge(key, now));
  });
  for (const key of [...entrySeen.keys()]) if (!liveKeys.has(key)) entrySeen.delete(key);

  // A finished trick fades out once, from the first render that shows it
  // with an empty trick (not on page load, not with reduced motion). The
  // client only has the latest State, so the fade is drawn from last_trick.
  let fadeAge = null;
  if (entries.length === 0 && last) {
    const key = JSON.stringify(last);
    if (fadeSeen?.key !== key) fadeSeen = { key, at: primed && !reducedMotion() ? now : -Infinity };
    const age = now - fadeSeen.at;
    if (age < FADE_MS) {
      fadeAge = age;
      clearTimeout(fadeTimer);
      fadeTimer = setTimeout(update, FADE_MS - age + 10);
    }
  }

  if (last && store.showLastTrick) {
    return { piles: trickPiles(last.entries || [], seats, { dim: true }, null), fading: false, lastShown: true };
  }
  if (entries.length === 0 && last) {
    if (fadeAge !== null) {
      return { piles: trickPiles(last.entries || [], seats, { fading: true, age: fadeAge }, null), fading: true };
    }
    return { piles: new Map(), fading: false };
  }
  return { piles: trickPiles(entries, seats, {}, ages), fading: false };
}

function hasContent(z) {
  return Boolean(z && z.layers.length > 0);
}

// Attributes for a node that may be sliding in. A render during the slide
// (e.g. a tap) rebuilds it, so the animation starts part-way through.
function slideAttrs(age, base) {
  const sliding = age !== null && age !== undefined;
  return {
    class: sliding ? `${base} zone-in` : base,
    style: sliding && age > 0 ? `animation-delay: -${Math.round(age)}ms` : null,
  };
}

// A pass, as a table-size card: muted, "PASS" in the middle (§5b.3).
function passCard() {
  return el("div", { class: "card-row card-row-small" },
    el("div", { class: "chip chip-small chip-pass" }, t("table.passCard")));
}

// A zone: the seat's pile (the newest entry, play or pass, on top; earlier
// ones underneath, offset toward the seat's edge and faded). Dimmed = the
// "Last trick" toggle's view (its one label is in the center). A play's
// combo label is its hover title; a pass card's is "Pass".
function renderZone(pos, z, level) {
  const classes = ["zone", `zone-${pos}`];
  if (!hasContent(z)) return el("div", { class: classes.join(" ") });
  if (z.dim) classes.push("is-dim");

  const n = z.layers.length;
  const layers = z.layers.map((layer, i) => {
    const label = layer.play ? comboLabel(layer.play, level) : t("table.pass");
    const depth = n - 1 - i; // 0 = newest, on top
    const kind = layer.play ? "" : " is-pass";
    return el("div", {
      class: `pile-play${kind}${depth > 0 ? " is-under" : ""}${layer.best ? " is-best" : ""}`,
      style: `--depth: ${depth}`,
      title: label,
      "aria-label": label,
    }, el("div", slideAttrs(layer.age, "pile-cards"),
      layer.play ? cardRow(layer.play.cards || [], level, { small: true }) : passCard()));
  });

  // The finished trick fades out; a render mid-fade picks up where it was.
  const entry = z.fading
    ? { class: "zone-entry zone-out", style: `animation-delay: -${Math.round(z.age)}ms` }
    : { class: "zone-entry" };
  return el("div", { class: classes.join(" ") },
    el("div", entry,
      el("div", { class: "pile" }, layers)));
}

// ---------- center: text only when relevant (§1b) ----------

function renderCenter(p, fading, lastShown) {
  const names = store.state.seats;
  const n = (seat) => seatName(names, seat);
  const lines = [];

  if (p?.name === "Playing") {
    const b = p.body;
    const last = b.last_trick;
    // "Last trick" toggle on: one label for the whole felt.
    if (lastShown) lines.push(el("span", { class: "zone-tag" }, t("table.lastTrick")));
    // Mid-trick the center stays empty, and so it does while the finished
    // trick fades out.
    if ((b.trick || []).length === 0 && !fading) {
      lines.push(el("div", { class: "center-lead" }, t("table.leads", { name: n(b.turn) })));
      // 接风: the winner went out, so their partner leads instead.
      if (last && last.next_leader !== last.winner) {
        lines.push(el("div", { class: "center-jiefeng" },
          t("table.jiefeng", { a: n(last.winner), b: n(last.next_leader) })));
      }
      // How the deal started isn't shown here any more (GAME_PAGE_V3_SPEC
      // §3); a tribute deal's returns are a notice when play begins.
    }
  } else if (p?.name === "Tribute") {
    lines.push(el("div", { class: "center-phase" }, t("table.tributeBeforeDeal")));
  }

  return el("div", { class: "center" }, lines);
}

// ---------- turn order arrows (GAME_PAGE_V3_SPEC §4) ----------

const SVG_NS = "http://www.w3.org/2000/svg";

// One faint curved arrow for a felt corner. They're drawn once, for the
// bottom-right corner (from your side, curving up toward the right player),
// and CSS turns the copy in each other corner a quarter turn further
// counterclockwise, so together they read you → right → partner → left.
// Absolutely placed, behind the piles, so they take no felt space.
function turnArrow(corner) {
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("class", `turn-arrow turn-arrow-${corner}`);
  svg.setAttribute("viewBox", "0 0 28 28");
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("focusable", "false");
  for (const d of ["M3 22 Q22 22 22 6", "M17.5 10.5 L22 5 L26.5 10.5"]) {
    const path = document.createElementNS(SVG_NS, "path");
    path.setAttribute("d", d);
    svg.appendChild(path);
  }
  return svg;
}

function turnArrows() {
  return ["br", "tr", "tl", "bl"].map(turnArrow);
}

// ---------- notice (take back, returns: net.js sets store.notice) ----------

function renderNotice() {
  const n = store.notice;
  if (!n || n.until <= Date.now()) return null;
  return el("div", { class: "felt-notice", role: "status" }, n.text);
}

// ---------- screen ----------

function renderPhaseControls() {
  const phase = store.state?.room?.InGame?.phase ?? {};
  if ("Tribute" in phase) return renderTributeControls();
  if ("Playing" in phase) return renderPlayControls();
  return null;
}

export function renderGame() {
  const g = inGame();
  const p = currentPhase();
  const you = store.state.your_seat;
  const level = dealLevel();

  const seatOf = (pos) => seatAt(you, pos);
  const { piles: zones, fading, lastShown } = zoneContents(p, ZONE_POSITIONS.map(seatOf));
  primed = true;

  const plates = OTHER_POSITIONS.map((pos) => {
    const f = seatFacts(g, p, seatOf(pos));
    return renderPlate(f, pos, f.name, pos === "top" ? t("table.partner") : null);
  });
  const mine = seatFacts(g, p, you);
  // Something in the left or right zone → the center text keeps to the
  // middle column; otherwise it may use the whole middle row.
  const hasSide = ["left", "right"].some((pos) => hasContent(zones.get(seatOf(pos))));

  return el("div", { class: `game phase-${(p?.name ?? "none").toLowerCase()}` },
    renderHeader(g, p, level, you),
    el("div", { class: "table" },
      plates,
      el("div", { class: hasSide ? "felt has-side" : "felt" },
        turnArrows(),
        ZONE_POSITIONS.map((pos) => renderZone(pos, zones.get(seatOf(pos)), level)),
        renderCenter(p, fading, lastShown),
        renderNotice()),
      renderPlate(mine, "you", t("table.you", { name: mine.name }), null)),
    renderPhaseControls());
}
