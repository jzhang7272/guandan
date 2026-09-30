// Your hand + action bar during Playing, reading picker (UI_SPEC §3.4, §3.5).
//
// Server actions: Play (Play button, reading picker), Pass and TakeBack
// (GAME_PAGE_V3_SPEC §1: undo your last play until the next player acts). Local UI
// state: store.selected (indices into your_hand — hands have duplicates, so
// never card values). The hand itself (stacks, groups, their toolbar and
// tapping to select) is hand.js renderHand().
// main.js wraps renderReadingPicker() in the modal overlay, so it returns
// just the box content.
import { el } from "./dom.js";
import { store, update } from "./store.js";
import { send } from "./net.js";
import { renderHand } from "./hand.js";
import { comboLabel, seatName } from "./format.js";
import { t } from "./i18n.js";

function playingPhase() {
  return store.state?.room?.InGame?.phase?.Playing ?? null;
}

// Selected indices that still point into the hand, in hand order. (net.js
// clears the selection whenever your_hand changes; the filter is just a
// guard against a stale index.)
function selectedIndices(hand) {
  return [...store.selected].filter((i) => i >= 0 && i < hand.length).sort((a, b) => a - b);
}

function clearSelection() {
  store.selected.clear();
  update();
}

// ---------- §3.4 your hand & action bar ----------

export function renderPlayControls() {
  const s = store.state;
  const p = playingPhase();
  if (!s || !p) return null;

  const hand = p.your_hand || [];
  const you = s.your_seat;
  const yourTurn = p.turn === you;
  const trickEmpty = (p.trick || []).length === 0;
  const picked = selectedIndices(hand);

  const canPlay = yourTurn && picked.length > 0;
  const canPass = yourTurn && !trickEmpty;
  // The server says when a take back is possible (only for the player who
  // just played, until the next player acts). Absent (an older server) → no.
  const canTakeBack = p.can_take_back === true;

  const onPlay = () => {
    if (!canPlay) return;
    // Hand order, duplicates included (Decision 17: Play.cards is in hand order).
    send({ type: "Play", cards: picked.map((i) => hand[i]), declared: null });
  };
  const onPass = () => {
    if (canPass) send({ type: "Pass" });
  };
  const onTakeBack = () => {
    if (canTakeBack) send({ type: "TakeBack" });
  };

  let status;
  if (hand.length === 0) {
    status = el("span", { class: "play-status" }, t("play.out"));
  } else if (yourTurn) {
    status = el("span", { class: "play-status is-turn" }, trickEmpty ? t("play.yourLead") : t("play.yourTurn"));
  } else {
    status = el("span", { class: "play-status" }, t("play.waitingFor", { name: seatName(s.seats, p.turn) }));
  }

  // Stacks + groups; tapping toggles store.selected (multi-select). Your
  // name, count and TURN are on your plate under the felt (table.js).
  const row = hand.length > 0 ? renderHand() : null;

  const bar = el("div", { class: "play-actions" },
    status,
    picked.length > 0 ? el("span", { class: "play-picked muted" }, t("play.selected", { n: picked.length })) : null,
    el("div", { class: "play-buttons" },
      el("button", { type: "button", disabled: picked.length === 0, onClick: clearSelection }, t("play.clear")),
      el("button", { type: "button", disabled: !canPass, onClick: onPass }, t("play.pass")),
      el("button", { type: "button", class: "primary", disabled: !canPlay, onClick: onPlay }, t("play.play")),
    ),
  );

  // Trick buttons (about the current trick, not the deal): top right of the
  // hand box, on the same line as Group / Ungroup / Clear groups.
  const trickActions = el("div", { class: "trick-actions" },
    el("button", {
      type: "button",
      class: "take-back-btn",
      disabled: !canTakeBack,
      title: t("play.takeBackTitle"),
      onClick: onTakeBack,
    }, t("play.takeBack")),
    p.last_trick
      ? el("button", {
        type: "button",
        class: `last-trick-toggle${store.showLastTrick ? " is-on" : ""}`,
        "aria-pressed": store.showLastTrick ? "true" : "false",
        onClick: () => { store.showLastTrick = !store.showLastTrick; update(); },
      }, t("table.lastTrick"))
      : null);

  return el("section", { class: `play-controls${yourTurn ? " is-your-turn" : ""}` }, trickActions, row, bar);
}

// ---------- §3.5 reading picker ----------

// main.js renders this inside a modal overlay while store.reading is set.
export function renderReadingPicker() {
  const reading = store.reading;
  if (!reading) return null;
  const level = playingPhase()?.level ?? store.lastLevel;

  const close = () => {
    store.reading = null;
    update();
  };
  const pick = (option) => {
    send({ type: "Play", cards: reading.cards, declared: option.combo });
    close();
  };

  return el("div", { class: "reading-picker", role: "dialog", "aria-label": t("reading.title") },
    el("h2", {}, t("reading.title")),
    el("div", { class: "reading-options" },
      (reading.options || []).map((option) =>
        el("button", { type: "button", class: "reading-option", onClick: () => pick(option) },
          comboLabel(option, level))),
    ),
    el("div", { class: "reading-actions" },
      el("button", { type: "button", onClick: close }, t("common.cancel"))),
  );
}
