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
import { cardRow } from "./cards.js";
import { rankLabel, seatName, placeLabel, teamName } from "./format.js";
import { t } from "./i18n.js";

// Partners sit opposite: seats 0 & 2 are team A, 1 & 3 team B.
const teamOf = (seat) => (seat % 2 === 0 ? "A" : "B");
const teamIndex = (team) => (team === "A" ? 0 : 1);

// lastDealSummary(lastDeal, names, you) → null (no last deal) or
//   {
//     matchWinner: "A" | "B" | null,     // set → the match-won band, no score tiles
//     winTeam:     "A" | "B",            // the deal's winners (the strip's colour)
//     headline:    "East-West wins!",
//     finish:      "1-3"                 // ZH "1-2 双下" for a 1-2
//     places:      "Alex 1st, Sam 2nd, Robin 3rd, Josey 4th",
//     levels:      [{ team, name, from, to, changed, yours, attempts }, …]  // deal's winners first
//                  // attempts: { text: "Attempts: 1/3", bad } for a team whose A attempt
//                  // just failed (bad: it dropped back to 2), else null
//     finalPlay:   { cards, level, who: "Robin played:" } | null
//   }
// Teams are shown by their compass names (format.js teamName). finalPlay is
// the play that ended the deal: the last Played entry of final_trick (null
// when there is none). Its level is the deal's level (from summary.before),
// for the cards' level / wildcard marks. Display only: every value comes from
// the server's DealSummary; nothing here decides a rule. The text is in the
// current language (i18n.js t()).
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

  // The declaring team's A attempt failed (it was at A and didn't win the
  // match): its counter after the deal, or 3 when it dropped back to Two
  // (the counter is already reset to 0 in `after` then) — Decision 24.
  const declaring = before.declaring;
  const failedAt = declaring && before.team_levels?.[teamIndex(declaring)] === "Ace"
    && summary.match_winner !== declaring ? declaring : null;
  const attemptsOf = (team) => {
    if (team !== failedAt) return null;
    const dropped = summary.dropped_to_two === team;
    const n = dropped ? 3 : after.a_attempts?.[teamIndex(team)] ?? "?";
    return { text: t("common.aTries", { n }), bad: dropped };
  };

  // Levels before → after, the deal's winners first.
  const levels = [winTeam, loseTeam].map((team) => {
    const i = teamIndex(team);
    const from = before.team_levels?.[i];
    const to = after.team_levels?.[i];
    return {
      team,
      name: teamName(team),
      from: rankLabel(from),
      to: rankLabel(to),
      changed: from !== to,
      yours: you !== null && you !== undefined && teamOf(you) === team,
      attempts: attemptsOf(team),
    };
  });

  return {
    matchWinner: summary.match_winner || null,
    winTeam,
    headline: t("results.wins", { team: teamName(winTeam) }),
    finish: t("results.finish", { finish }),
    places,
    levels,
    finalPlay: finalPlayOf(lastDeal.final_trick, before, names),
  };
}

// The deal's level: the declaring team's level before the deal (a match's
// first deal has no declaring team and is played at 2).
function dealLevel(before) {
  const d = before.declaring;
  return d ? before.team_levels?.[teamIndex(d)] ?? null : "Two";
}

// The last Played entry of the final trick (passes skipped) → finalPlay.
function finalPlayOf(finalTrick, before, names) {
  if (!Array.isArray(finalTrick)) return null;
  for (let i = finalTrick.length - 1; i >= 0; i--) {
    const played = finalTrick[i]?.Played;
    if (!played?.play) continue;
    return {
      cards: played.play.cards || [],
      level: dealLevel(before),
      who: t("results.played", { name: seatName(names, played.seat) }),
    };
  }
  return null;
}

// The card (docs/mockups/last_deal_mockup.html, option D):
// a header band with the title; below it the final play on a patch of felt
// on the left, and on the right the winners' strip (headline, finish badge,
// places) over one score tile per team (level before → after, A attempts).
// A match win turns the band gold with the trophy title and drops the tiles
// (the levels are back to 2, which the band says).
export function renderLastDeal() {
  const s = store.state;
  const d = lastDealSummary(s?.room?.Lobby?.last_deal, s?.seats, s?.your_seat);
  if (!d) return null;

  const won = Boolean(d.matchWinner);
  const band = el("header", { class: "sec-band" },
    el("h2", {}, won ? t("results.matchWon", { team: teamName(d.matchWinner) }) : t("results.lastDeal")),
    won ? el("span", { class: "sec-band-note" }, t("results.levelsBack")) : null);

  const felt = d.finalPlay
    ? el("div", { class: "results-felt" },
      el("span", { class: "results-who" }, d.finalPlay.who),
      cardRow(d.finalPlay.cards, d.finalPlay.level, { small: true }))
    : null;

  const strip = el("div", { class: `results-strip team-${d.winTeam.toLowerCase()}` },
    el("div", { class: "results-headline" },
      el("span", { class: "results-win" }, d.headline), " ",
      el("span", { class: "results-finish" }, d.finish)),
    el("div", { class: "results-places" }, d.places));

  const tile = (l) => el("div", { class: `results-tile team-${l.team.toLowerCase()}` },
    el("span", { class: "results-tile-name" },
      el("span", {}, l.name),
      l.yours ? el("span", { class: "results-tile-you" }, t("results.yourTeam")) : null),
    el("span", { class: "results-score" },
      el("span", { class: "results-levels" },
        l.changed ? [el("span", {}, l.from), el("span", { class: "results-arrow" }, "→")] : null,
        el("span", {}, l.to)),
      l.attempts
        ? el("span", { class: l.attempts.bad ? "results-attempts is-bad" : "results-attempts" }, l.attempts.text)
        : null));

  return el("section", {
    class: won ? "results-panel is-match-over" : "results-panel",
    "aria-label": won ? t("results.matchWonAria") : t("results.lastDeal"),
  },
  band,
  el("div", { class: felt ? "results-body" : "results-body no-felt" },
    felt,
    el("div", { class: "results-right" },
      strip,
      won ? null : el("div", { class: "results-board" }, d.levels.map(tile)))));
}
