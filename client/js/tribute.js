// Tribute panel + its hand (UI_SPEC §3.6).
//
// Shows one line per tribute duty, what you have to do (if anything), and
// your hand. Only the cards the server lists in `your_task.options` can be
// tapped — the client never re-implements the tribute rules.
import { el } from "./dom.js";
import { store } from "./store.js";
import { send } from "./net.js";
import { cardChip } from "./cards.js";
import { renderHand } from "./hand.js";
import { seatName } from "./format.js";
import { t, list } from "./i18n.js";

// Cards are compared by value (their wire JSON): hands hold duplicates, and
// every physical copy of an allowed card is allowed.
const cardKey = (card) => JSON.stringify(card);

export function renderTributeControls() {
  const s = store.state;
  const tribute = s?.room?.InGame?.phase?.Tribute;
  if (!s || !tribute) return el("div", { class: "waiting" }, "No tribute state.");

  const names = s.seats;
  const you = s.your_seat;
  const level = tribute.level;
  const duties = tribute.duties || [];
  const hand = tribute.your_hand || [];
  const task = tribute.your_task;

  // Task → which message the button sends and its label.
  let kind = null;
  let options = [];
  if (task && task.PayTribute) {
    kind = "PayTribute";
    options = task.PayTribute.options || [];
  } else if (task && task.ReturnTribute) {
    kind = "ReturnTribute";
    options = task.ReturnTribute.options || [];
  }
  const allowed = new Set(options.map(cardKey));
  const enabled = (card) => kind !== null && allowed.has(cardKey(card));

  // The one selected card, if it's still a legal choice. (Without a task the
  // hand is multi-select for grouping, so a task that appears later could
  // find several selected: then nothing is sent until one is picked.)
  const [selIndex] = store.selected.size === 1 ? [...store.selected] : [];
  const selCard = selIndex !== undefined ? hand[selIndex] : undefined;
  const canSend = kind !== null && selCard !== undefined && enabled(selCard);

  const onSend = () => {
    if (!canSend) return;
    send({ type: kind, card: selCard });
  };

  return el("section", { class: "tribute-panel", "aria-label": t("tribute.aria") },
    el("div", { class: "tribute-info" },
      el("h2", { class: "tribute-title" },
        duties.length > 1 ? t("tribute.titleDouble") : t("tribute.titleSingle")),
      el("ul", { class: "tribute-duties" }, duties.map((d) => renderDuty(d, names, level, you))),
      el("p", { class: kind ? "tribute-task is-yours" : "tribute-task muted" },
        kind ? taskText(kind, duties, names, you) : waitingText(duties, names)),
      // The button sits with the task (left column on a computer, so the
      // hand gets the full height on the right).
      kind
        ? el("div", { class: "tribute-actions" },
          el("button", { type: "button", class: "primary", disabled: !canSend, onClick: onSend },
            kind === "PayTribute" ? t("tribute.payButton") : t("tribute.returnButton")))
        : null),
    el("div", { class: "tribute-hand" },
      // With a task: single-select (tapping a card replaces the selection,
      // tapping it again clears it), only the server's options tappable.
      // No task: nothing dimmed, and cards stay tappable so they can be
      // grouped (HAND_LAYOUT_SPEC §4).
      kind ? renderHand({ enabled, single: true }) : renderHand()));
}

// "Josey → Alex   paid: SJ   return: waiting…"
function renderDuty(duty, names, level, you) {
  const who = (seat) => el("span", { class: seat === you ? "tribute-name is-you" : "tribute-name" },
    seatName(names, seat));

  let paid;
  if (duty.tribute) {
    paid = el("span", { class: "tribute-paid" }, t("tribute.paidPrefix"),
      cardChip(duty.tribute, level, { small: true }));
  } else if (duty.paid) {
    paid = el("span", { class: "tribute-paid" }, t("tribute.paid"));
  } else {
    paid = el("span", { class: "tribute-paid muted" }, t("tribute.waitingTribute"));
  }

  const ret = duty.returned
    ? el("span", { class: "tribute-return" }, t("tribute.returnHidden"))
    : el("span", { class: "tribute-return muted" }, t("tribute.returnWaiting"));

  return el("li", { class: "tribute-duty" },
    el("span", { class: "tribute-pair" }, who(duty.payer), " → ", who(duty.receiver)),
    paid,
    ret);
}

function taskText(kind, duties, names, you) {
  if (kind === "ReturnTribute") {
    const mine = duties.find((d) => d.receiver === you);
    return t("tribute.taskReturn", { name: mine ? seatName(names, mine.payer) : null });
  }
  // Receivers are fixed before anyone pays (the server compares the payers'
  // best cards when it plans the tribute), so the duty names ours.
  const mine = duties.find((d) => d.payer === you);
  return t("tribute.taskPay", { name: mine ? seatName(names, mine.receiver) : null });
}

// No task for you: say who everyone is waiting on, derived from the duties.
function waitingText(duties, names) {
  // At most two duties, so list() reads "a and b" as before.
  const who = (seats) => list(seats.map((seat) => seatName(names, seat)));
  const payers = duties.filter((d) => !d.paid).map((d) => d.payer);
  if (payers.length) return t("tribute.waitingPay", { names: who(payers) });
  const receivers = duties.filter((d) => !d.returned).map((d) => d.receiver);
  if (receivers.length) return t("tribute.waitingReturn", { names: who(receivers) });
  return t("tribute.waiting");
}
