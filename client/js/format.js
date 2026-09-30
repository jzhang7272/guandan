// Pure display formatting, no DOM, no store (UI_SPEC §4). Tested by
// client/tests.html. Text goes through i18n.js t(), so it comes out in the
// current language (LANGUAGE_SPEC §4.3); the examples below are English. All inputs are the wire JSON shapes (TECH_SPEC §4):
//   Rank / Level   "Ten"                     (a Level is just its Rank string)
//   Card           {"Standard":{"rank":"Ten","suit":"Heart"}} | {"Joker":"Small"|"Big"}
//   Face           {"Rank":"Nine"} | "SmallJoker" | "BigJoker"
//   Play           {cards: Card[], combo: Combo, wildcard_as: Rank[]}
//   SeatId         0..3
//
// Usage (U2–U5):
//   rankLabel("Ten")                        → "10"
//   cardLabel(card)                         → "10♥" / "SJ" / "BJ" (the name in text; ZH 小王 / 大王)
//   cardCorner(card)                        → "10" / "SJ" / "BJ" (the card's corner; ZH 小 / 大)
//   comboLabel(play, level)                 → "Pair of 9s", "Bomb: five 9s (♥5 as 9, 9)"
//       level = the deal level ("Five"); only used for the wildcard suffix.
//   dealStartLine(dealStart, state.seats)   → "Josey paid Alex SJ, got back 3♣; … Josey leads."
//   returnsLine(dealStart, state.seats)     → "Returns: Alex → Josey 3♣, Robin → Sam 4♦" ("" if no tribute)
//   nextActiveSeat(turn, cardCounts, finishOrder) → the seat that acts after `turn` (skipping those out), or null
//   seatName(state.seats, seat)             → "Josey" (or "Seat 3" when empty)
//   sortOrder(hand, level)                  → display order (strongest left) as indices into `hand`
//   sortHand(hand, level)                   → sorted copy of `hand` (= sortOrder mapped)
//   isLevelCard(card, level), isWildcard(card, level) → bool
//   placeLabel(i)                           → "1st" … "4th" (i = 0-based index in finish order)
//   seatAt(yourSeat, "bottom"|"right"|"top"|"left") → SeatId
//   teamName("A") → "North-South" (LOBBY_LOOK_SPEC §1)
//   teamRelative(team, yourTeam)            → "Us" / "Them" (LANGUAGE_SPEC §1)
//   seatDirection(seat)                     → "South" / "East" / "North" / "West"
//
// Unknown / malformed input yields a string containing "?" rather than
// throwing, so a protocol mismatch is visible on screen instead of crashing
// render(). tests.html treats "?" in a label as a failure.

import { t } from "./i18n.js";

const POSITION_OFFSET = { bottom: 0, right: 1, top: 2, left: 3 };

// §3.3 rotation: you are always at the bottom; the seat that acts after you
// (seat + 1) is on your right, your partner (seat + 2) on top.
// seatAt(yourSeat, "bottom" | "right" | "top" | "left") → SeatId (0..3)
export function seatAt(yourSeat, position) {
  const offset = POSITION_OFFSET[position];
  if (offset === undefined) throw new Error(`seatAt: unknown position ${position}`);
  return (yourSeat + offset) % 4;
}

// ---------- teams and compass directions (LOBBY_LOOK_SPEC §1, §2) ----------

// Play goes counterclockwise, seat 0 → 1 → 2 → 3, so the seats sit South,
// East, North, West (LANGUAGE_SPEC §1). Team A (seats 0 & 2) is
// North-South, team B (1 & 3) East-West. The server and protocol keep
// "A" / "B"; these are display only.
const TEAMS = ["A", "B"];

// teamName("A") → "North-South" / 南北方, teamName("B") → "East-West" /
// 东西方. Unknown → "?".
export function teamName(team) {
  return TEAMS.includes(team) ? t(`team.${team}`) : "?";
}

// teamRelative(team, yourTeam) → "Us" / 我方 for your own team, "Them" /
// 对方 for the other (the phone header's short labels). Unknown team → "?".
export function teamRelative(team, yourTeam) {
  if (!TEAMS.includes(team)) return "?";
  return team === yourTeam ? t("team.us") : t("team.them");
}

// seatDirection(0) → "South", 1 → "East", 2 → "North", 3 → "West"
// (南家 / 东家 / 北家 / 西家). Unknown → "?".
export function seatDirection(seat) {
  return [0, 1, 2, 3].includes(seat) ? t(`seat.dir${seat}`) : "?";
}

// ---------- ranks, suits, cards ----------

// Natural order (TECH_SPEC §3.1).
const RANKS = [
  "Two", "Three", "Four", "Five", "Six", "Seven", "Eight",
  "Nine", "Ten", "Jack", "Queen", "King", "Ace",
];
const RANK_LABEL = {
  Two: "2", Three: "3", Four: "4", Five: "5", Six: "6", Seven: "7", Eight: "8",
  Nine: "9", Ten: "10", Jack: "J", Queen: "Q", King: "K", Ace: "A",
};
const SUIT_SYMBOL = { Spade: "♠", Heart: "♥", Club: "♣", Diamond: "♦" };
// Display order within a rank (§4.2): ♠ ♥ ♣ ♦.
const SUIT_ORDER = { Spade: 0, Heart: 1, Club: 2, Diamond: 3 };

// rankLabel("Ten") → "10", "Jack" → "J", "Ace" → "A". Unknown → "?".
export function rankLabel(rank) {
  return RANK_LABEL[rank] ?? "?";
}

// suitSymbol("Heart") → "♥". Unknown → "?".
export function suitSymbol(suit) {
  return SUIT_SYMBOL[suit] ?? "?";
}

// cardLabel(card) → "10♥" / "SJ" / "BJ" (§4.1): a card's name in text
// (and its aria label). Chinese names the jokers in full: 小王 / 大王.
export function cardLabel(card) {
  if (card && card.Standard) {
    return rankLabel(card.Standard.rank) + suitSymbol(card.Standard.suit);
  }
  if (card && card.Joker === "Small") return t("card.smallJoker");
  if (card && card.Joker === "Big") return t("card.bigJoker");
  return "?";
}

// cardCorner(card) → what the card's corner shows: the rank for a standard
// card ("10"; cards.js adds the suit under it), "SJ" / "BJ" for a joker
// (Chinese: one character, 小 / 大, to fit).
export function cardCorner(card) {
  if (card && card.Standard) return rankLabel(card.Standard.rank);
  if (card && card.Joker === "Small") return t("card.smallJokerCorner");
  if (card && card.Joker === "Big") return t("card.bigJokerCorner");
  return "?";
}

// Any suit's copy of the level rank. level = the deal level, e.g. "Five".
export function isLevelCard(card, level) {
  return Boolean(card && card.Standard && level && card.Standard.rank === level);
}

// The Heart copy of the level rank (e.g. 5♥ at level Five).
export function isWildcard(card, level) {
  return isLevelCard(card, level) && card.Standard.suit === "Heart";
}

// ---------- combos (§4.3) ----------

// Bomb sizes with a word (combo.size4 … combo.size10).
const BOMB_SIZES = [4, 5, 6, 7, 8, 9, 10];

// Face → "9" / "Small Joker" / "Big Joker"; plural → "9s" / "Small Jokers".
function faceLabel(face, plural) {
  if (face === "SmallJoker") return t(plural ? "combo.smallJokers" : "combo.smallJoker");
  if (face === "BigJoker") return t(plural ? "combo.bigJokers" : "combo.bigJoker");
  if (face && typeof face === "object" && "Rank" in face) {
    return plural ? rankPlural(face.Rank) : rankLabel(face.Rank);
  }
  return "?";
}

// "9s" (Chinese has no plural: "9").
function rankPlural(rank) {
  return t("combo.plural", { rank: rankLabel(rank) });
}

// The `length` ranks of a run ending at `top`, stepping back in natural
// order with A before 2 (A-low runs): runRanks("Five", 5) → [A,2,3,4,5].
function runRanks(top, length) {
  const ext = ["Ace", ...RANKS]; // Ace-low slot at index 0
  const end = ext.lastIndexOf(top);
  const start = end - length + 1;
  if (end < 0 || start < 0) return null;
  return ext.slice(start, end + 1);
}

// "5–9" style (en dash) span label for straights / straight flushes.
function spanLabel(top, length) {
  const ranks = runRanks(top, length);
  if (!ranks) return "?";
  return `${rankLabel(ranks[0])}–${rankLabel(ranks[ranks.length - 1])}`;
}

// "7-7-8-8-9-9" style label: each rank of the run repeated `copies` times.
function repeatedRunLabel(top, length, copies) {
  const ranks = runRanks(top, length);
  if (!ranks) return "?";
  return ranks.flatMap((r) => Array(copies).fill(rankLabel(r))).join("-");
}

function bombLabel(bomb) {
  if (bomb === "Jokers") return t("combo.jokerBomb");
  if (bomb && bomb.OfAKind) {
    const n = bomb.OfAKind.size;
    const size = BOMB_SIZES.includes(n) ? t(`combo.size${n}`) : "?";
    return t("combo.bomb", { size, rank: rankPlural(bomb.OfAKind.rank) });
  }
  if (bomb && bomb.StraightFlush) {
    return t("combo.straightFlush", { span: spanLabel(bomb.StraightFlush.top, 5) });
  }
  return "?";
}

// Label for a bare Combo (no wildcard suffix). Exported for callers that
// only have a Combo (e.g. a `declared` value); prefer comboLabel(play, level).
export function comboOnlyLabel(combo) {
  if (!combo || typeof combo !== "object") return "?";
  if ("Single" in combo) return faceLabel(combo.Single, false);
  if ("Pair" in combo) return t("combo.pair", { face: faceLabel(combo.Pair, true) });
  if ("Triple" in combo) return t("combo.triple", { rank: rankPlural(combo.Triple) });
  if ("FullHouse" in combo) return t("combo.fullHouse", { rank: rankPlural(combo.FullHouse?.triple) });
  if ("Straight" in combo) return t("combo.straight", { span: spanLabel(combo.Straight?.top, 5) });
  if ("Tube" in combo) return t("combo.tube", { run: repeatedRunLabel(combo.Tube?.top, 3, 2) });
  if ("Plate" in combo) return t("combo.plate", { run: repeatedRunLabel(combo.Plate?.top, 2, 3) });
  if ("Bomb" in combo) return bombLabel(combo.Bomb);
  return "?";
}

// comboLabel(play, level) → "Pair of 9s", "Straight A–5",
// "Bomb: five 9s (♥5 as 9, 9)".
//   play  — a Play: {cards, combo, wildcard_as}
//   level — the deal level Rank ("Five"); used only to name the wildcard in
//           the `wildcard_as` suffix. If null/undefined the suffix says
//           "wildcard as …" instead of "♥5 as …".
export function comboLabel(play, level) {
  if (!play) return "?";
  let label = comboOnlyLabel(play.combo);
  const as = Array.isArray(play.wildcard_as) ? play.wildcard_as : [];
  if (as.length > 0) {
    const ranks = as.map(rankLabel);
    label += level
      ? t("combo.wildcardAs", { wild: `♥${rankLabel(level)}`, ranks })
      : t("combo.wildcardAsNoLevel", { ranks });
  }
  return label;
}

// ---------- seats, places, deal start (§4.4) ----------

// seatName(names, seat) → display name for a seat.
//   names — array indexed by SeatId whose entries are either strings or
//           SeatInfo objects ({display_name, connected}), so `state.seats`
//           can be passed directly. Missing / null names → "Seat N" (seat.fallback).
export function seatName(names, seat) {
  const entry = names ? names[seat] : null;
  const name = entry && typeof entry === "object" ? entry.display_name : entry;
  return typeof name === "string" && name !== "" ? name : t("seat.fallback", { n: seat });
}

// dealStartLine(dealStart, names) → one line describing how the deal began.
//   dealStart — the DealStart JSON from Playing.deal_start
//   names     — as for seatName (usually `state.seats`)
export function dealStartLine(dealStart, names) {
  if (!dealStart || typeof dealStart !== "object") return "?";
  const n = (seat) => seatName(names, seat);
  if (dealStart.FirstDeal) {
    const { revealed, leader } = dealStart.FirstDeal;
    return t("deal.firstDeal", { card: cardLabel(revealed), name: n(leader) });
  }
  if (dealStart.AntiTribute) {
    return t("deal.antiTribute", { name: n(dealStart.AntiTribute.leader) });
  }
  if (dealStart.Tribute) {
    const { exchanges, leader } = dealStart.Tribute;
    const clauses = (exchanges || []).map((x) => t("deal.exchange", {
      payer: n(x.payer), receiver: n(x.receiver), tribute: cardLabel(x.tribute), returned: cardLabel(x.returned),
    }));
    return t("deal.tribute", { exchanges: clauses, name: n(leader) });
  }
  return "?";
}

// returnsLine(dealStart, names) → the returns of a tribute deal, shown to
// everyone when play begins (GAME_PAGE_V3_SPEC §3), e.g.
// "Returns: Alex → Josey 3♣, Robin → Sam 4♦". Each return goes from the
// player who received the tribute back to the one who paid it. "" for a
// deal without a tribute (first deal, anti-tribute) or bad input.
export function returnsLine(dealStart, names) {
  const exchanges = dealStart?.Tribute?.exchanges;
  if (!Array.isArray(exchanges) || exchanges.length === 0) return "";
  const n = (seat) => seatName(names, seat);
  const parts = exchanges.map((x) =>
    t("deal.returnPart", { from: n(x.receiver), to: n(x.payer), card: cardLabel(x.returned) }));
  return t("deal.returns", { parts });
}

// nextActiveSeat(turn, cardCounts, finishOrder) → the seat that acts after
// `turn` (play goes seat + 1), skipping players who are out, or null when
// nobody else is still in. Display only (the "next" marker, §4).
//   cardCounts  — Playing's card_counts: { Exact: n } | "MoreThanTen" per seat
//   finishOrder — seats that have gone out, in order
// A seat is out when it's in finishOrder or has exactly 0 cards.
export function nextActiveSeat(turn, cardCounts, finishOrder) {
  if (!Number.isInteger(turn) || turn < 0 || turn > 3) return null;
  const out = (seat) => (finishOrder || []).includes(seat) || cardCounts?.[seat]?.Exact === 0;
  for (let step = 1; step < 4; step++) {
    const seat = (turn + step) % 4;
    if (!out(seat)) return seat;
  }
  return null;
}

// placeLabel(index) → "1st" / "2nd" / "3rd" / "4th" (头游 / 二游 / 三游 / 末游).
//   index — 0-based position in finish_order / summary.result.order.
export function placeLabel(index) {
  return [0, 1, 2, 3].includes(index) ? t(`place.${index}`) : "?";
}

// ---------- hand sort (HAND_LAYOUT_SPEC §2, display only) ----------

// Sort key [column, suit] for one card, strongest first:
// BJ · SJ · level rank · A · K · … · 2, the level rank skipped in its natural
// place. Within a rank ♠ ♥ ♣ ♦, with the wildcard (♥ level card) last — the
// same order as hand.js's stacks. Mirrors face_value, for display only.
function sortKey(card, level) {
  if (card && card.Joker) return [card.Joker === "Big" ? 0 : 1, 0];
  const std = (card && card.Standard) || {};
  const suitPos = SUIT_ORDER[std.suit] ?? 9;
  if (level && std.rank === level) return [2, std.suit === "Heart" ? 10 : suitPos];
  const rankPos = RANKS.indexOf(std.rank);
  // Unknown rank → far right rather than crashing.
  return [rankPos < 0 ? 99 : 3 + (RANKS.length - 1 - rankPos), suitPos];
}

// sortOrder(cards, level) → indices into `cards` in display order, strongest
// on the left. Stable: equal cards keep hand order. Use this when you need
// to map back to your_hand indices (store.selected).
export function sortOrder(cards, level) {
  const keys = cards.map((c) => sortKey(c, level));
  return cards
    .map((_, i) => i)
    .sort((a, b) => {
      const ka = keys[a];
      const kb = keys[b];
      for (let j = 0; j < ka.length; j++) {
        if (ka[j] !== kb[j]) return ka[j] - kb[j];
      }
      return a - b;
    });
}

// sortHand(cards, level) → a new, sorted array of the same cards.
export function sortHand(cards, level) {
  return sortOrder(cards, level).map((i) => cards[i]);
}
