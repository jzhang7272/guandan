// Card faces + a row of cards (UI_SPEC §4.1, §2.2; TABLE_LOOK_SPEC §4, §1b).
// Drawn with CSS and text only, no images: a white rounded card with the
// rank + suit in the top-left corner and a big suit pip in the middle.
//
//   cardChip(card, level, { selected, disabled, onClick, small }) → Node
//     A <button> when onClick is given, else a static <span>.
//   cardRow(cards, level, { selected, enabled, onToggle, order, small }) → Node
//     A row of cards. Static (no onToggle) rows show played cards on the
//     table, tribute cards and leftovers: pass small: true there.
//
// Indices: `selected`, `enabled(card, i)` and `onToggle(i)` always use
// indices into `cards` as passed (i.e. into your_hand), never display
// positions. To show the hand sorted, pass `order` (e.g. from
// format.js sortOrder(hand, level)); selection then stays correct.
//
// Card DOM (style.css "cards" section):
//   .chip [.chip-red|.chip-black|.chip-joker .chip-joker-big|-small]
//         [.chip-level] [.chip-wild] [.chip-small] [.is-selected] [.is-disabled]
//     .chip-corner  > .chip-rank ("10" / "BJ" / 大) [+ .chip-suit ("♥")]
//     .chip-pip     big suit symbol, or ★ for a joker (hidden in a stack
//                   except on the fully visible bottom card)
//     .chip-wild-dot  gold dot on a wildcard
import { el } from "./dom.js";
import { cardLabel, cardCorner, rankLabel, suitSymbol, isLevelCard, isWildcard } from "./format.js";
import { t } from "./i18n.js";

// cardChip(card, level, opts)
//   card   — wire Card JSON
//   level  — deal level Rank ("Five") for the level-card border / wildcard
//            dot; null → no level highlighting
//   opts.selected — bool, lifted with a thin green border (TABLE_LOOK_SPEC §1c)
//   opts.disabled — bool, greyed out (and not clickable)
//   opts.onClick  — () => void; makes the chip a button
//   opts.small    — bool, table-size card for played cards / tribute /
//                   results (not a tap target — don't combine with onClick)
export function cardChip(card, level, opts = {}) {
  const { selected = false, disabled = false, onClick = null, small = false } = opts;
  const classes = ["chip"];
  const wild = isWildcard(card, level);
  let corner;
  let pip;

  if (card && card.Joker) {
    // Jokers: just "BJ" / "SJ" in the corner and a star in the middle (no
    // "JOKER" word — it didn't fit; in Chinese one character, 大 / 小).
    // BJ red, SJ black.
    classes.push("chip-joker", card.Joker === "Big" ? "chip-joker-big" : "chip-joker-small");
    corner = [el("span", { class: "chip-rank" }, cardCorner(card))];
    pip = "★";
  } else {
    const std = (card && card.Standard) || {};
    const red = std.suit === "Heart" || std.suit === "Diamond";
    classes.push(red ? "chip-red" : "chip-black");
    corner = [
      el("span", { class: "chip-rank" }, rankLabel(std.rank)),
      el("span", { class: "chip-suit" }, suitSymbol(std.suit)),
    ];
    pip = suitSymbol(std.suit);
  }
  if (isLevelCard(card, level)) classes.push("chip-level");
  if (wild) classes.push("chip-wild");
  if (selected) classes.push("is-selected");
  if (disabled) classes.push("is-disabled");
  if (small) classes.push("chip-small");

  const children = [
    el("span", { class: "chip-corner" }, corner),
    el("span", { class: "chip-pip", "aria-hidden": "true" }, pip),
    wild ? el("span", { class: "chip-wild-dot", "aria-hidden": "true" }) : null,
  ];

  const attrs = {
    class: classes.join(" "),
    title: wild ? t("cards.wildcard") : null,
    "aria-label": wild ? t("cards.wildcardAria", { card: cardLabel(card) }) : cardLabel(card),
  };
  if (onClick) {
    return el("button", {
      ...attrs,
      type: "button",
      disabled,
      "aria-pressed": selected ? "true" : "false",
      onClick: () => onClick(),
    }, children);
  }
  return el("span", attrs, children);
}

// cardRow(cards, level, opts)
//   opts.selected — Set<index into cards>
//   opts.enabled  — (card, i) => bool; false → dimmed, untappable (default: all enabled)
//   opts.onToggle — (i) => void; given → chips are buttons, else static
//   opts.order    — optional display order: array of indices into cards
//                   (default: as given)
//   opts.small    — table-size static cards, overlapping slightly like a
//                   fanned play
export function cardRow(cards, level, opts = {}) {
  const { selected = new Set(), enabled = null, onToggle = null, small = false } = opts;
  const order = opts.order || cards.map((_, i) => i);
  const chips = order.map((i) => {
    const card = cards[i];
    const isEnabled = enabled ? Boolean(enabled(card, i)) : true;
    return cardChip(card, level, {
      selected: selected.has(i),
      disabled: !isEnabled,
      onClick: onToggle ? () => onToggle(i) : null,
      small,
    });
  });
  const row = el("div", { class: small ? "card-row card-row-small" : "card-row" }, chips);
  // The card count lets CSS squeeze a long fan into a narrow table zone.
  row.style.setProperty("--n", String(chips.length));
  return row;
}
