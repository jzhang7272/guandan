// Your hand as stacks + groups (HAND_LAYOUT_SPEC). This file holds the
// non-DOM part: pure layout helpers (unit-tested in tests.html) and the
// group state ops that mutate the store, plus the renderer (renderHand) used
// by play.js / tribute.js; net.js calls onNewState / onClassified.
//
// Pure (no DOM, no store):
//   reconcileIds(prevIds, prevHand, newHand, nextId) → { ids, nextId }
//   buildLayout(hand, ids, groups, level)            → columns, left to right
//   dealKeyOf(state)                                 → string | null
//   groupToggle(selected, indices)                   → new selection (tap on a group's edge)
// Store ops (mutate store, persist, call update()):
//   groupSelected(), ungroupSelected(), resetGroups(),
//   onClassified(msg), onNewState(prev, next)
// DOM:
//   renderHand({ enabled, single, onToggle })        → Node (toolbar + columns)
//
// Why local card ids (§3.4): the server sends only card values and hands
// contain duplicates, so a group can't hold hand indices (they shift when any
// card leaves) or values (two 9♠ are indistinguishable). Each physical card
// gets an id "c<n>" that is carried across States by matching values.
import { store, update } from "./store.js";
import { send } from "./net.js";
import { el } from "./dom.js";
import { cardChip } from "./cards.js";
import { comboLabel } from "./format.js";
import { t } from "./i18n.js";

const STORAGE_KEY = "guandan.hand"; // sessionStorage (per tab)

// Natural order, weakest first (TECH_SPEC §3.1).
const RANKS = [
  "Two", "Three", "Four", "Five", "Six", "Seven", "Eight",
  "Nine", "Ten", "Jack", "Queen", "King", "Ace",
];
// Top-to-bottom order inside a stack (§2): ♠ ♥ ♣ ♦.
const SUIT_ORDER = { Spade: 0, Heart: 1, Club: 2, Diamond: 3 };

// ---------- pure helpers ----------

// reconcileIds(prevIds, prevHand, newHand, nextId)
//   prevIds  — one id per prevHand index (may be empty / null)
//   prevHand — previous your_hand (may be null)
//   newHand  — new your_hand
//   nextId   — counter for fresh ids
// → { ids: one id per newHand index, nextId }
// Matching is by card value. For duplicates, the earliest (lowest previous
// index) unmatched id wins; cards with no previous match get "c<nextId++>".
export function reconcileIds(prevIds, prevHand, newHand, nextId) {
  const pool = new Map(); // card JSON → queue of previous ids, earliest first
  (prevHand || []).forEach((card, i) => {
    const id = prevIds ? prevIds[i] : undefined;
    if (id === undefined || id === null) return;
    const key = JSON.stringify(card);
    if (!pool.has(key)) pool.set(key, []);
    pool.get(key).push(id);
  });
  let counter = nextId;
  const ids = (newHand || []).map((card) => {
    const queue = pool.get(JSON.stringify(card));
    if (queue && queue.length > 0) return queue.shift();
    return `c${counter++}`;
  });
  return { ids, nextId: counter };
}

// Stack key for a card: "BJ", "SJ" or its rank. One stack per key.
function stackKey(card) {
  if (card && card.Joker) return card.Joker === "Big" ? "BJ" : "SJ";
  return card?.Standard?.rank ?? "?";
}

// Column position of a stack key, 0 = leftmost (strongest):
// BJ · SJ · level rank · A · K · … · 2 (level rank skipped in its natural
// place). Mirrors face_value, for display only.
function stackStrength(key, level) {
  if (key === "BJ") return 0;
  if (key === "SJ") return 1;
  if (key === level) return 2;
  const natural = RANKS.indexOf(key);
  if (natural < 0) return 99; // unknown → far right rather than crashing
  return 3 + (RANKS.length - 1 - natural);
}

// Sort key inside a stack: suit ♠ ♥ ♣ ♦, wildcards (♥ level card) last so
// the badge stays on the visible bottom card; ties keep hand order.
function withinStackKey(card, level) {
  const std = card?.Standard;
  if (!std) return 0;
  if (std.rank === level && std.suit === "Heart") return 10;
  return SUIT_ORDER[std.suit] ?? 9;
}

function isWild(card, level) {
  const std = card?.Standard;
  return Boolean(std && std.rank === level && std.suit === "Heart");
}

// Split hand indices into rank stacks, strongest first.
function stacksOf(indices, hand, level) {
  const byKey = new Map();
  for (const i of indices) {
    const key = stackKey(hand[i]);
    if (!byKey.has(key)) byKey.set(key, []);
    byKey.get(key).push(i);
  }
  return [...byKey.entries()]
    .sort((a, b) => stackStrength(a[0], level) - stackStrength(b[0], level))
    .map(([, list]) => list.sort((a, b) =>
      withinStackKey(hand[a], level) - withinStackKey(hand[b], level) || a - b));
}

// The ranks a run combo covers, bottom to top ("Ace" first for an A-low
// run), or null if `combo` isn't a straight / tube / plate / straight flush.
//   Straight / StraightFlush: 5 ranks ending at `top` (Five = A-2-3-4-5)
//   Tube: 3 ranks (Three = A-2-3);  Plate: 2 ranks (Two = A-2)
function runRanks(combo) {
  if (!combo || typeof combo !== "object") return null;
  const sf = combo.Bomb?.StraightFlush;
  let top;
  let len;
  if (combo.Straight) [top, len] = [combo.Straight.top, 5];
  else if (combo.Tube) [top, len] = [combo.Tube.top, 3];
  else if (combo.Plate) [top, len] = [combo.Plate.top, 2];
  else if (sf) [top, len] = [sf.top, 5];
  else return null;
  const t = RANKS.indexOf(top);
  if (t < 0 || t - len + 1 < -1) return null;
  const ranks = [];
  for (let k = t - len + 1; k <= t; k++) ranks.push(k === -1 ? "Ace" : RANKS[k]);
  return ranks;
}

// Order of a group's cards, top ("base") to bottom (§3.2), from the group's
// first reading (null / undefined while unclassified or not a legal combo):
//   FullHouse — the triple's cards, then the pair's;
//   runs      — smallest to biggest along the run (A first if A-low);
//   otherwise — smallest to biggest by strength (2 … A, level, SJ, BJ).
// Ties (same rank): ♠ ♥ ♣ ♦, wildcards last, then hand order.
// In a full house or run each wildcard sits at the rank it stands in for:
// one `wildcard_as` entry per wildcard (they're identical cards, so which
// wildcard gets which entry doesn't matter). A wildcard with no entry plays
// as itself or as a level card of another suit, i.e. at the level rank.
function groupOrder(indices, hand, reading, level) {
  const combo = reading?.combo;
  const run = runRanks(combo);
  const triple = combo?.FullHouse?.triple ?? null;
  const subs = run || triple ? [...(reading.wildcard_as || [])] : [];
  const sorted = [...indices].sort((a, b) => a - b);
  const slot = new Map(sorted.map((i) => [i,
    isWild(hand[i], level) && subs.length > 0 ? subs.shift() : stackKey(hand[i])]));
  // Weakest first; the level rank counts at its face value (above A).
  const strength = (i) => -stackStrength(slot.get(i), level);
  let primary;
  if (run) {
    primary = (i) => {
      const pos = run.indexOf(slot.get(i));
      return pos < 0 ? 99 : pos; // not part of the run (shouldn't happen) → bottom
    };
  } else if (triple) {
    primary = (i) => (slot.get(i) === triple ? 0 : 1000) + strength(i);
  } else {
    primary = strength;
  }
  return sorted.sort((a, b) => primary(a) - primary(b)
    || withinStackKey(hand[a], level) - withinStackKey(hand[b], level) || a - b);
}

// buildLayout(hand, ids, groups, level) → columns, left to right:
//   { kind: "stack", handIndices }                         one per rank, top to bottom
//   { kind: "group", groupId, bomb, readings, handIndices } one column, top (base) to bottom (§3.2)
// Order: bomb groups (bomb === true), newest first → stacks strongest →
// weakest → other groups (bomb false/null), oldest first.
//   hand   — your_hand
//   ids    — store.handIds (one per hand index)
//   groups — store.groups, creation order
//   level  — deal level Rank ("Five")
// Cards in a group are not in the stacks. Group ids missing from `ids` are
// ignored; a group with none of its cards in the hand gets no column. A card
// listed in two groups (shouldn't happen) goes to the first one.
export function buildLayout(hand, ids, groups, level) {
  const indexOfId = new Map();
  (ids || []).forEach((id, i) => {
    if (i < hand.length) indexOfId.set(id, i);
  });

  const grouped = new Set();
  const groupCols = [];
  for (const g of groups || []) {
    const members = [];
    for (const id of g.cardIds || []) {
      const i = indexOfId.get(id);
      if (i === undefined || grouped.has(i)) continue;
      grouped.add(i);
      members.push(i);
    }
    if (members.length === 0) continue;
    groupCols.push({
      kind: "group",
      groupId: g.groupId,
      bomb: g.bomb ?? null,
      readings: g.readings ?? null,
      handIndices: groupOrder(members, hand, (g.readings || [])[0], level),
    });
  }

  const loose = hand.map((_, i) => i).filter((i) => !grouped.has(i));
  const stacks = stacksOf(loose, hand, level).map((handIndices) => ({ kind: "stack", handIndices }));

  const bombs = groupCols.filter((c) => c.bomb === true).reverse(); // newest leftmost
  const others = groupCols.filter((c) => c.bomb !== true);           // newest rightmost
  return [...bombs, ...stacks, ...others];
}

// groupToggle(selected, indices) → the selection after tapping a group's
// edge (GAME_PAGE_V3_SPEC §2): if every one of the group's `indices` is
// already selected they're all deselected; otherwise they're all added (the
// rest of the selection stays either way).
//   selected — the current selection (a Set of hand indices); not mutated
//   indices  — the group's hand indices (its enabled cards)
export function groupToggle(selected, indices) {
  const next = new Set(selected);
  const all = indices.length > 0 && indices.every((i) => next.has(i));
  for (const i of indices) {
    if (all) next.delete(i);
    else next.add(i);
  }
  return next;
}

// The in-deal phase ({ name, inner }) of a State, or null outside
// Tribute/Playing (lobby, no state).
function dealPhase(state) {
  const phase = state?.room?.InGame?.phase;
  if (!phase) return null;
  if (phase.Tribute) return { name: "Tribute", inner: phase.Tribute };
  if (phase.Playing) return { name: "Playing", inner: phase.Playing };
  return null;
}

// dealKeyOf(state) → a string identifying the current deal, or null outside
// Tribute/Playing.
//
// The Tribute phase and the Playing phase of the same deal give the SAME key,
// so groups made while paying/returning tribute survive into play (§3.3,
// §4). Both are built from what the two phases share: the deal level, the
// tribute payer→receiver pairs (Tribute.duties / deal_start.Tribute.exchanges),
// and the match-wide team levels / A-attempts / declaring team. The last three
// change from deal to deal, which keeps keys of consecutive deals apart
// (e.g. two AntiTribute deals with the same leader). A FirstDeal/AntiTribute
// deal has no Tribute phase, so its deal_start goes into the key as is.
export function dealKeyOf(state) {
  const dp = dealPhase(state);
  if (!dp) return null;
  const game = state.room.InGame;
  const pairs = (list) => (list || [])
    .map((x) => [x.payer, x.receiver])
    .sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  let start;
  if (dp.name === "Tribute") {
    start = { tribute: pairs(dp.inner.duties) };
  } else {
    const ds = dp.inner.deal_start;
    start = ds && ds.Tribute ? { tribute: pairs(ds.Tribute.exchanges) } : { other: ds ?? null };
  }
  return JSON.stringify({
    level: dp.inner.level ?? null,
    team_levels: game.team_levels ?? null,
    a_attempts: game.a_attempts ?? null,
    declaring: game.declaring ?? null,
    ...start,
  });
}

// ---------- store helpers ----------

// Same rule as net.js: fixture mode never touches storage, so fixtures render
// the same every time and never clobber a real tab's saved groups.
const fixtureMode = typeof location !== "undefined"
  && new URLSearchParams(location.search).has("fixture");

function loadSaved() {
  if (fixtureMode) return null;
  try {
    const raw = window.sessionStorage.getItem(STORAGE_KEY);
    return raw ? JSON.parse(raw) : null;
  } catch {
    return null;
  }
}

// Saves { dealKey, handIds, hand, groups, nextCardId }. `hand` (the your_hand
// the ids belong to) is saved too so a restore can re-match ids by value.
function persist() {
  if (fixtureMode) return;
  try {
    if (store.dealKey === null) {
      window.sessionStorage.removeItem(STORAGE_KEY);
      return;
    }
    window.sessionStorage.setItem(STORAGE_KEY, JSON.stringify({
      dealKey: store.dealKey,
      handIds: store.handIds,
      hand: currentHand(store.state),
      groups: store.groups,
      nextCardId: store.nextCardId,
    }));
  } catch { /* ignore: private windows can block storage */ }
}

// your_hand during Tribute/Playing, else [].
function currentHand(state) {
  return dealPhase(state)?.inner?.your_hand ?? [];
}

function isBombReading(play) {
  const combo = play?.combo;
  return Boolean(combo && typeof combo === "object" && "Bomb" in combo);
}

// Ask the server what the group's cards can be played as (§3.5). Asked once
// per group: when it's created, or when it's restored after a refresh that
// lost the reply. Groups never change afterwards (§3.3). The reply
// (onClassified) is matched by requestId; anything else is ignored.
function classify(group, hand) {
  const indexOfId = new Map(store.handIds.map((id, i) => [id, i]));
  const cards = group.cardIds
    .map((id) => indexOfId.get(id))
    .filter((i) => i !== undefined && i < hand.length)
    .sort((a, b) => a - b)
    .map((i) => hand[i]);
  group.requestId = store.nextRequestId++;
  send({ type: "Classify", request_id: group.requestId, cards });
}

function nextGroupId() {
  let max = 0;
  for (const g of store.groups) {
    const n = Number(String(g.groupId).replace(/^g/, ""));
    if (Number.isFinite(n) && n > max) max = n;
  }
  return `g${max + 1}`;
}

// Selected hand indices → their card ids (valid indices only).
function selectedIds() {
  return [...store.selected]
    .filter((i) => i >= 0 && i < store.handIds.length)
    .map((i) => store.handIds[i]);
}

// Break up (remove) every group holding any of `ids`; all their cards go
// back to the stacks (§3.3).
function breakUpGroupsWith(ids) {
  const hit = new Set(ids);
  store.groups = store.groups.filter((g) => !g.cardIds.some((id) => hit.has(id)));
}

// ---------- group state ops ----------

// Group button (§3.1): every group a selected card came from breaks up, and
// only the selected cards form the new group (on the right until the server
// says it's a bomb).
export function groupSelected() {
  const ids = selectedIds();
  if (ids.length === 0) return;
  const hand = currentHand(store.state);
  const groupId = nextGroupId(); // before the break-up, so the new id isn't a broken group's
  breakUpGroupsWith(ids);
  const group = { groupId, cardIds: ids, bomb: null, readings: null, requestId: null };
  store.groups.push(group);
  classify(group, hand);
  store.selected.clear();
  persist();
  update();
}

// Ungroup button: dissolve every group containing a selected card.
export function ungroupSelected() {
  breakUpGroupsWith(selectedIds());
  store.selected.clear();
  persist();
  update();
}

// Clear groups button: dissolve all groups; the selection is kept.
export function resetGroups() {
  store.groups = [];
  persist();
  update();
}

// A Classified reply: { request_id, readings }. Only the group whose latest
// request it answers takes it; anything else is stale and ignored.
export function onClassified(msg) {
  const group = store.groups.find((g) => g.requestId === msg?.request_id);
  if (!group) return;
  const readings = Array.isArray(msg.readings) ? msg.readings : [];
  group.readings = readings;
  group.bomb = readings.some(isBombReading);
  persist();
  update();
}

// Called by net.js for every State (after store.state = next).
//   prev — the previous State (store.state before), or null
//   next — the new State
// A new deal (different dealKey, or coming from outside Tribute/Playing —
// lobby, page load) resets ids and groups, then restores
// the sessionStorage copy if it belongs to this deal (page refresh). Within a
// deal, ids are re-matched to the new hand; a group that lost any card (played,
// paid or returned as tribute) breaks up (§3.3).
export function onNewState(prev, next) {
  const key = dealKeyOf(next);
  const hand = currentHand(next);

  if (key === null) {
    store.dealKey = null;
    store.handIds = [];
    store.groups = [];
    persist();
    update();
    return;
  }

  let prevIds = store.handIds;
  let prevHand = currentHand(prev);
  let nextId = store.nextCardId;
  if (key !== store.dealKey || dealPhase(prev) === null) {
    store.dealKey = key;
    store.groups = [];
    prevIds = [];
    prevHand = [];
    nextId = 0;
    const saved = loadSaved();
    if (saved && saved.dealKey === key && Array.isArray(saved.handIds) && Array.isArray(saved.hand)) {
      prevIds = saved.handIds;
      prevHand = saved.hand;
      nextId = Number(saved.nextCardId) || 0;
      store.groups = Array.isArray(saved.groups) ? saved.groups : [];
      // request ids restart at 1 after a refresh; keep them ahead of any
      // restored group's so an old id can't match a new request's reply.
      for (const g of store.groups) {
        if (Number.isFinite(g.requestId) && g.requestId >= store.nextRequestId) {
          store.nextRequestId = g.requestId + 1;
        }
        // Its reply was lost with the old page: ask again (below).
        if (g.bomb === null) g.requestId = null;
      }
    }
  }

  const r = reconcileIds(prevIds, prevHand, hand, nextId);
  store.handIds = r.ids;
  store.nextCardId = r.nextId;

  const present = new Set(r.ids);
  store.groups = store.groups.filter((g) => g.cardIds.every((id) => present.has(id)));
  // Restored groups still waiting for a reply (refresh mid-request).
  for (const g of store.groups) {
    if (g.requestId === null) classify(g, hand);
  }
  persist();
  update();
}

// ---------- rendering ----------

// The hand cards shown selected in the last render, by deal key + card id
// (or "#i" when the ids aren't in step with the hand).
let shownSelected = new Set();

// Switch a chip's selected look on the next frame, once the browser has
// laid it out in its old state, so the lift transition plays. If another
// render replaces the chip first, this only touches a detached node.
function flipSelected(node, selected) {
  const apply = () => {
    getComputedStyle(node).transform; // eslint-disable-line no-unused-expressions -- commit the old style
    node.classList.toggle("is-selected", selected);
    node.setAttribute("aria-pressed", selected ? "true" : "false");
  };
  if (typeof requestAnimationFrame === "function") requestAnimationFrame(apply);
  else apply();
}

// Hover text for a group: all of its readings, or why there are none.
function groupLabel(readings, level) {
  if (readings === null || readings === undefined) return t("hand.checking");
  if (readings.length === 0) return t("hand.notLegal");
  return readings.map((r) => comboLabel(r, level)).join(t("hand.readingSep"));
}

// renderHand({ enabled, single, onToggle }) → one Node: the Group / Ungroup /
// Clear groups toolbar (§3.1) followed by the stack and group columns (§2, §3.2, §5).
//   enabled(card, i) — false → that card is dimmed and untappable (tribute);
//                      default: all enabled
//   single           — true → tapping keeps at most one card selected
//   onToggle(i)      — given → called on tap instead of the default toggle
// The hand and level come from store.state's Tribute/Playing phase (empty
// outside a deal); the selection is store.selected (hand indices).
export function renderHand({ enabled = null, single = false, onToggle = null } = {}) {
  const inner = dealPhase(store.state)?.inner;
  const hand = inner?.your_hand ?? [];
  const level = inner?.level ?? null;

  // onNewState keeps handIds in step with the hand. If they don't match
  // (not wired yet, or a render before the State was processed), show plain
  // stacks keyed by index and disable Group/Ungroup: they map indices to ids
  // through handIds and would pick the wrong cards.
  const idsOk = store.handIds.length === hand.length;
  const columns = idsOk
    ? buildLayout(hand, store.handIds, store.groups, level)
    : buildLayout(hand, hand.map((_, i) => i), [], level);

  const tap = (i) => {
    if (onToggle) {
      onToggle(i);
      return;
    }
    if (store.selected.has(i)) {
      store.selected.delete(i);
    } else {
      if (single) store.selected.clear();
      store.selected.add(i);
    }
    update();
  };

  // Selection lifts the card with a CSS transition (TABLE_LOOK_SPEC §1c).
  // The whole screen is rebuilt on every update, so a chip whose selection
  // just changed is first built as it looked last time and switched on the
  // next frame; otherwise the new node would start out lifted and nothing
  // would animate.
  const nowSelected = new Set();
  const chip = (i) => {
    // The deal key too, so ids reused by the next deal don't carry over.
    const key = `${store.dealKey}:${idsOk ? store.handIds[i] : `#${i}`}`;
    const sel = store.selected.has(i);
    if (sel) nowSelected.add(key);
    const was = shownSelected.has(key);
    const node = cardChip(hand[i], level, {
      selected: was,
      disabled: enabled ? !enabled(hand[i], i) : false,
      onClick: () => tap(i),
    });
    if (was !== sel) flipSelected(node, sel);
    return node;
  };
  // --n = the column's card count: CSS tightens the strip of a column too
  // tall to fit on screen, rather than making the page scroll.
  const stackNode = (indices) => {
    const node = el("div", { class: "hand-stack" }, indices.map(chip));
    node.style.setProperty("--n", String(indices.length));
    return node;
  };

  // Group edge select goes with the plain multi-select only (not tribute's
  // pick-one-card, nor a custom tap handler).
  const edgeSelect = !single && !onToggle;
  const cols = columns.map((col) => {
    if (col.kind === "stack") return stackNode(col.handIndices);
    // What the group can be played as is shown only on hover (user
    // decision): every reading, e.g. "Full house, 9s / Bomb: five 9s".
    const label = groupLabel(col.readings, level);
    const classes = ["hand-group"];
    if (col.bomb === true) classes.push("is-bomb");
    if (edgeSelect) classes.push("is-selectable");
    return el("div", {
      class: classes.join(" "),
      title: label,
      "aria-label": label,
      // Tapping the outline / background (not a card) selects the whole
      // group, or deselects it when it's all selected (§2). A card's own tap
      // is handled by its chip (a disabled chip swallows the click).
      onClick: edgeSelect
        ? (e) => {
          if (e.target instanceof Element && e.target.closest(".chip")) return;
          const usable = col.handIndices.filter((i) => !enabled || enabled(hand[i], i));
          const next = groupToggle(store.selected, usable);
          store.selected.clear();
          for (const i of next) store.selected.add(i);
          update();
        }
        : null,
    }, stackNode(col.handIndices));
  });

  // Toolbar enable rules (§3.1).
  const selected = [...store.selected].filter((i) => i >= 0 && i < hand.length);
  const groupedIds = new Set(store.groups.flatMap((g) => g.cardIds || []));
  const selectionInGroup = idsOk && selected.some((i) => groupedIds.has(store.handIds[i]));
  const toolbar = el("div", { class: "hand-toolbar" }, [
    el("button", { type: "button", disabled: !idsOk || selected.length === 0, onClick: groupSelected }, t("hand.group")),
    el("button", { type: "button", disabled: !selectionInGroup, onClick: ungroupSelected }, t("hand.ungroup")),
    el("button", { type: "button", disabled: store.groups.length === 0, onClick: resetGroups }, t("hand.clearGroups")),
  ]);

  shownSelected = nowSelected;
  return el("div", { class: "hand" }, [toolbar, el("div", { class: "hand-cols" }, cols)]);
}
