// The lobby's "Last deal" card / match-won banner (LOBBY_FLOW_SPEC §6.3 as
// amended by LOBBY_LOOK_SPEC §1; the old deal-over panel, UI_SPEC §3.7,
// moved from the game into the lobby).
//
//   lastDealSummary(lastDeal, names, you) → plain data for the card (pure,
//       no DOM, no store; tested by tests.html). lastDeal is
//       `room.Lobby.last_deal` ({summary, …}); names as for format.js
//       seatName (usually `state.seats`); you = your SeatId.
//   renderLastDeal() → the card for store.state's lobby, or null when there
//       is no last deal. Used by lobby.js.
import { el } from "./dom.js";
import { store } from "./store.js";
import { rankLabel, seatName, placeLabel, teamName } from "./format.js";
import { t } from "./i18n.js";

// Partners sit opposite: seats 0 & 2 are team A, 1 & 3 team B.
const teamOf = (seat) => (seat % 2 === 0 ? "A" : "B");
const teamIndex = (team) => (team === "A" ? 0 : 1);

// lastDealSummary(lastDeal, names, you) → null (no last deal) or
//   {
//     matchWinner: "A" | "B" | null,     // set → the match-won banner
//     headline:    "Team East-West wins 1-3",
//     places:      "Alex 1st, Sam 2nd, Robin 3rd, Josey 4th",   // no parentheses
//     levels:      [{ text: "Team East-West: 6 → 8", changed: true }, …]  // deal's winners first
//     notes:       [{ text, bad }]       // dropped-to-two / failed A attempt
//   }
// Teams are shown by their compass names (format.js teamName). The deal's
// final play is not shown (LOBBY_LOOK_SPEC §1). Display only: every value
// comes from the server's DealSummary; nothing here decides a rule. The
// text is in the current language (i18n.js t()).
export function lastDealSummary(lastDeal, names, you) {
  const summary = lastDeal?.summary;
  if (!summary) return null;

  const order = summary.result?.order || [];
  const before = summary.before || {};
  const after = summary.after || {};

  const winner = order[0];
  const winTeam = teamOf(winner);
  const loseTeam = winTeam === "A" ? "B" : "A";

  // Finish type from where the winner's partner finished (TECH_SPEC DealResult).
  const partnerPlace = order.indexOf((winner + 2) % 4);
  const finish = partnerPlace === 1 ? "1-2" : partnerPlace === 2 ? "1-3" : "1-4";
  const places = t("results.places", {
    places: order.map((seat, i) => t("results.place", { name: seatName(names, seat), place: placeLabel(i) })),
  });

  // Levels before → after, the deal's winners first.
  const levels = [winTeam, loseTeam].map((team) => {
    const i = teamIndex(team);
    const from = before.team_levels?.[i];
    const to = after.team_levels?.[i];
    const yours = you !== null && you !== undefined && teamOf(you) === team;
    const text = t(yours ? "results.levelYours" : "results.level",
      { team: teamName(team), from: rankLabel(from), to: rankLabel(to) });
    return { text, changed: from !== to };
  });

  const notes = [];
  if (summary.dropped_to_two) {
    notes.push({ text: t("results.droppedToTwo", { team: teamName(summary.dropped_to_two) }), bad: true });
  }
  // A-attempt line: the declaring team was at A and didn't win the match
  // with this deal. A team that just dropped to Two is covered by the line
  // above (its counter is already reset to 0 in `after`) — Decision 24.
  const declaring = before.declaring;
  if (declaring && before.team_levels?.[teamIndex(declaring)] === "Ace"
      && summary.match_winner !== declaring && summary.dropped_to_two !== declaring) {
    const n = after.a_attempts?.[teamIndex(declaring)] ?? "?";
    notes.push({ text: t("results.aAttemptFailed", { team: teamName(declaring), n }), bad: false });
  }

  return {
    matchWinner: summary.match_winner || null,
    headline: t("results.wins", { team: teamName(winTeam), finish }),
    places,
    levels,
    notes,
  };
}

export function renderLastDeal() {
  const s = store.state;
  const d = lastDealSummary(s?.room?.Lobby?.last_deal, s?.seats, s?.your_seat);
  if (!d) return null;

  const won = Boolean(d.matchWinner);
  const lines = [];

  // The match-won banner replaces the "Last deal" title.
  if (won) {
    lines.push(el("h2", { class: "results-headline" },
      t("results.matchWon", { team: teamName(d.matchWinner) })));
  } else {
    lines.push(el("h2", { class: "results-title" }, t("results.lastDeal")));
  }

  lines.push(el("p", { class: "results-deal" },
    el("strong", {}, won ? t("results.finalDeal", { headline: d.headline }) : d.headline),
    d.places ? el("span", { class: "results-places" }, t("results.placesParen", { places: d.places })) : null));

  // After a match win the levels are back to 2 (the banner says so), so the
  // before → after line of the winning deal would only confuse.
  if (!won) {
    lines.push(el("p", { class: "results-levels" },
      d.levels.map((l) => el("span", { class: l.changed ? "results-level is-changed" : "results-level" }, l.text))));
  }

  for (const n of d.notes) {
    lines.push(el("p", { class: n.bad ? "results-note is-bad" : "results-note" }, n.text));
  }

  return el("section", {
    class: won ? "results-panel is-match-over" : "results-panel",
    "aria-label": won ? t("results.matchWonAria") : t("results.lastDeal"),
  }, lines);
}
