# Wire-format fixtures

One WebSocket message per file, exactly as the server serializes it. Generated
and checked by `src/server/protocol/fixture_tests.rs` (`cargo test`; rewrite
them with `UPDATE_FIXTURES=1 cargo test fixture`). The client loads them with
`?fixture=<name>` (UI_SPEC.md §7). If a file and TECH_SPEC.md §4 disagree, the
file is right.

Seats: 0 Josey, 1 Alex (partners with 3), 2 Sam (partners with 0), 3 Robin.
Most in-game fixtures follow the TECH_SPEC.md §4 story, built from four full
dealt hands: team B (seats 1 & 3) won the first deal 1-2, so deal 2 is at level
Five with team A still at Two; double tribute (seat 0 pays Small Joker to seat
1, seat 2 pays 5♣ to seat 3; returns 3♣ and 4♦); later seat 1 goes out 1st,
seat 3 leads by 接风, bombs with 9-9-9 + both 5♥, and goes out 2nd with a Big
Joker, so team B wins 1-2 again (Five → Eight).

## Client → server

- `client_join.json` — Join with a name and no reconnect token (§4 example).
- `client_join_reconnect.json` — Join with a reconnect token.
- `client_play.json` — Play without a declaration (`declared: null`): seat 1's pair of 9s.
- `client_play_declared.json` — Play resent after ChooseReading, declaring the 5-card bomb of 9s (§4 example).
- `client_pass.json` — Pass.
- `client_take_back.json` — TakeBack: take back your own play while nobody has acted since (GAME_PAGE_V3_SPEC.md §1). Reply: the usual State broadcast, or Rejected `NothingToTakeBack`.
- `client_pay_tribute.json` — PayTribute: seat 2 pays 5♣.
- `client_return_tribute.json` — ReturnTribute: seat 1 returns 3♣.
- `client_set_ready.json` — SetReady `true`.
- `client_choose_seat.json` — ChooseSeat 3 (refused with `SeatsLocked` between deals).
- `client_update_settings.json` — UpdateSettings: team A at Ace, team B at Eight, team B declaring (lobby only; `declaring: null` is allowed only while seats are unlocked).
- `client_new_match.json` — NewMatch: throw the match away (lobby only).
- `client_reset_deal.json` — ResetDeal: abandon this deal, back to the lobby as it was before it (in a deal only).
- `client_classify.json` — Classify (request 7) seat 3's 9-9-9 + 5♥-5♥: "what could these cards be played as?" (HAND_LAYOUT_SPEC.md §3.5).

## Server → client: session and errors

- `joined.json` — Joined as seat 0 with a session token (§4 example).
- `rejected.json` — Rejected with an ActionError code, `NotYourTurn` (§4 example).
- `rejected_name_taken.json` — Rejected with a SessionError code, `NameTaken` (for the name form).
- `rejected_seats_locked.json` — Rejected with `SeatsLocked`: ChooseSeat between the deals of a match.
- `kicked.json` — Kicked: your seat was reclaimed by another tab.
- `classified.json` — Classified, the reply to `client_classify` at level Five: request 7 echoed, two readings (full house, or 5-card bomb with `wildcard_as: ["Nine","Nine"]`). Empty `readings` = not a legal combo or no deal in progress.
- `choose_reading.json` — ChooseReading for seat 3's 9-9-9 + 5♥-5♥ at level Five: full house or 5-card bomb (`wildcard_as: ["Nine","Nine"]`) (§4 example).

## Server → client: lobby

Every deal ends in the lobby (LOBBY_FLOW_SPEC.md). `room.Lobby` carries the
table: `progress` (what the next deal starts from), `seats_locked` (true
between the deals of a match) and `last_deal` (`{summary, final_trick}` of
the most recent deal, or null).

- `state_lobby.json` — A fresh lobby (levels 2/2, nobody declaring, unlocked, `last_deal: null`) seen by seat 0: three seated, seat 3 empty, seats 0 and 2 ready — the viewer's own seat is ready (§4 example).
- `state_lobby_not_ready.json` — The same lobby seen by seat 1, whose seat is not ready.
- `state_lobby_between_deals.json` — LOBBY_FLOW_SPEC.md §7: team A declared at Ace and lost; team B won 1-3 (`order: [1, 2, 3, 0]`, Six → Eight) and declares; team A stays at Ace with `a_attempts: [1, 0]`. Seats locked; seats 0 and 3 ready; final trick = Josey's pair of 7s, Robin's pair of Ks. Seen by seat 0.
- `state_lobby_dropped_to_two.json` — Team A (declaring at Ace, 2 failed attempts) loses 1-4: third failure, `dropped_to_two: "A"`, team B Nine → Ten and declares. Seats locked; Robin is offline but keeps the seat. Seen by seat 0.
- `state_lobby_match_won.json` — Team A won the match at level A with a 1-3 (`last_deal.summary.match_winner: "A"`); the table is fresh again (`progress` 2/2, nobody declaring, `seats_locked: false`). Final trick ends with Sam's Big Joker. Seen by seat 3.

## Server → client: tribute (deal 2, level Five)

- `state_tribute.json` — Seen by seat 1 (28 cards): both tributes paid and public, seat 3 has returned blind, seat 1 must return (`ReturnTribute` task with 10 options); Robin offline (§4 example).
- `state_tribute_pay.json` — Earlier: seat 0 has paid (tribute still hidden), seat 2 must pay — `PayTribute` task with options 5♠/5♣; seen by seat 2.
- `state_tribute_waiting.json` — The same earlier moment seen by seat 0, who already paid: no task (`your_task: null`).

## Server → client: playing

- `state_playing.json` — Deal 2 seen by seat 2 (19 cards): seat 1 led a pair of 9s, seat 2 passed, seat 3's turn; `last_trick` = two straights won by seat 1; `deal_start` = Tribute with both exchanges (§4 example).
- `state_playing_your_turn.json` — The same moment seen by seat 3, whose turn it is, facing a non-empty trick.
- `state_playing_lead_jiefeng.json` — Later in deal 2, seen by seat 3: seat 1 went out on a winning tube (`finish_order: [1]`), so seat 3 leads an empty trick by 接风 (`last_trick.next_leader` 3 ≠ `winner` 1).
- `state_playing_first_deal.json` — First deal of a match (level Two, `declaring: null`, `deal_start` = FirstDeal with 8♦ revealed, seat 2 led); seen by seat 0, whose turn it is, facing pair 6s then pair Js; `last_trick: null`.
- `state_playing_anti_tribute.json` — Deal 3 (level Eight): tribute cancelled because payers 0 and 2 each hold a Big Joker, `deal_start` = AntiTribute, seat 1 led a triple; seen by seat 0.
- `state_playing_all_bombs.json` — A first-deal trick escalating through every bomb: 4-, 5-of-a-kind, straight flush, 6, 7, 8, 10-of-a-kind (8 Kings + both 2♥ wildcards, `wildcard_as: ["King","King"]`), joker bomb; `last_trick` is plate over plate; seen by seat 2, whose turn it is. Together with the fixtures above, every Combo and Bomb variant appears in some Play.
- `state_playing_can_take_back.json` — The `state_playing_first_deal` moment seen by seat 3, which just played the pair of Jacks and nobody has acted since: `can_take_back: true` (only ever true for that player).
- `state_playing_took_back.json` — Seat 3 then took the Jacks back: the trick is just seat 2's pair of 6s, it's seat 3's turn again with 27 cards, and `took_back: 3` tells everyone (cleared by the next play or pass); seen by seat 0.

Every Playing view carries `can_take_back` (true only for the viewer who may take back their play right now) and `took_back` (the seat whose play was just taken back, or null; the same for everyone).
