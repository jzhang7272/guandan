# Guandan (掼蛋) — Rules Specification

This is the authoritative rules reference for the variant we are implementing.
It is written to remove ambiguity for a rule engine and its test suite, not to
teach the game conversationally — see external sources (linked at the bottom)
for a gentler introduction.

The baseline is the official **World Guandan Competition Rules (2025)**.
Where that document is silent, we follow pagat.com. Every point where we
interpret an unclear rule or deliberately deviate is listed in
**"Interpretations & house rules"** at the end; everything else in this doc
is the official rule. If any choice there doesn't match how you actually
want to play, flag it and we'll change it.

## Terms

These terms are used with exactly these meanings throughout the doc.

| Term | Meaning |
|---|---|
| **seat** | A player position, `0`–`3`. Seats `0 & 2` are one team, `1 & 3` the other. Players see them as compass directions: seat 0 = **South** (南家), 1 = **East** (东家), 2 = **North** (北家), 3 = **West** (西家). |
| **team** | Team `A` (seats 0 & 2) is **North-South** (南北方); team `B` (seats 1 & 3) is **East-West** (东西方). The code and protocol say `A` / `B`; players see the compass names. |
| **match** | The full game: a sequence of deals, ending when one team wins at level A. |
| **deal** | One cycle of shuffle → deal → tribute → play, ending when one team has both players out. |
| **hand** | The cards a player currently holds (27 at the start of a deal). |
| **trick** | A sequence of plays, starting with a lead, ending when all other active players pass in a row. |
| **lead** | The first play of a trick. The player making it is the **leader**. |
| **play** | The card(s) a player puts down on one turn. Every play must be a legal **combo**. |
| **combo** | A legal card combination: single, pair, triple, full house, straight, tube, plate, or a bomb. |
| **bomb** | A combo that can beat any non-bomb play: a numbered bomb (4–10 same-rank cards), straight flush, or joker bomb. |
| **pass** | Declining to play on your turn. |
| **go out** | Play the last card in your hand. A player who has gone out is no longer **active**. |
| **active player** | A player who still has cards in their hand. |
| **finish place** | The order players go out in a deal: **1st**, **2nd**, **3rd**, **4th**. |
| **finish type** | Where the 1st-place player's partner finished: **1-2**, **1-3**, or **1-4**. |
| **level** | A team's current target rank, `2`→`A`. Each team has its own. |
| **declaring team** | The previous deal's winning team. Its level is the **deal level**. |
| **deal level** | The level in effect for the current deal. |
| **level rank / level card** | The rank equal to the deal level; any card of that rank (any suit). |
| **wildcard** | A heart-suit level card (e.g. `6♥` at level 6). |
| **Big Joker / Small Joker** | The higher / lower joker. The Big Joker is usually printed in color (red), the Small Joker in black and white. |
| **natural rank order** | `A(as 1), 2, 3, …, 10, J, Q, K, A` — used for straights, tubes, plates, and straight flushes. |
| **winning team / losing team** | The team of the deal's 1st-place player / the other team. |
| **tribute** | The card a player must give away before the first trick of a deal. |
| **tribute payer / tribute receiver** | The player who gives a tribute / the player who gets it. |
| **return card** | The card given back in exchange for a tribute. |

## Overview

Guandan ("throwing eggs") is a 4-player, 2-team climbing card game from
Jiangsu, China. On each turn a player must beat the previous play or pass;
the goal is to empty your hand before the other team.

Each **deal** ends when both players on one team have gone out; the
**match** is won by the first team to advance through all levels (2 through
A) and then win a deal at level A with a qualifying finish type.

## Setup & Teams

- 4 players; **partners sit opposite each other**: seats `0 & 2` (South &
  North, team North-South) against seats `1 & 3` (East & West, team
  East-West). The official rules seat players by compass direction the same
  way (东、西 one pair, 南、北 the other).
- Partnerships are fixed for the whole match — no partner rotation.
- Each team tracks its own **level**, starting at 2. The two teams' levels
  can differ during a match.
- Turn order proceeds **counterclockwise** (each player's turn passes to the
  player on their right): seat 0 → 1 → 2 → 3 → 0 → …, i.e. South → East →
  North → West. Dealing goes the same way.

## Deal Start (Dealing & the First Lead)

- All 108 cards are dealt out, one at a time, counterclockwise, so each
  player's hand is exactly **27 cards**.
- **First deal of the match**: before dealing, one card is turned face up
  from the middle of the shuffled deck (if it's a joker or the `2♥`, cut
  again and turn up a different card), then put back into the deck face up.
  Whoever ends up with that face-up card in their hand **leads the first
  trick**. There is no tribute in the first deal. (Officially, the turned
  card's rank is also used to count around the table to pick who draws
  first; this only affects dealing, not play.)
  - **Engine equivalent**: after a random deal, pick one physical card
    uniformly at random from all cards except the four jokers and the two
    `2♥`s. Whoever holds that card leads the first trick. The lead doesn't
    have to include that card.
- **Every later deal**: after dealing, tribute happens (see "Tribute &
  Anti-Tribute"), and the tribute outcome decides who leads the first trick.

## Deck & Card Ranking

- **2 standard 52-card decks + 4 jokers = 108 cards.** Two of the four
  jokers are Big Jokers (higher), two are Small Jokers (lower).
- **Base rank order** (low → high, ignoring suit for most purposes):
  `2, 3, 4, 5, 6, 7, 8, 9, 10, J, Q, K, A`.
- **The level rank is elevated above A — but only for non-sequential
  combos.** A level card behaves in one of two ways depending on *what it's
  being played as*, and this dual behavior is central to the whole ranking
  system:
  - **As a single, pair, triple, full house, or numbered bomb** — i.e. any
    combo that isn't a run of consecutive ranks — the level rank is pulled
    out of its natural position and ranks near the top, just below the
    jokers (see the ordering below).
  - **As part of a straight, tube, plate, or straight flush** (defined under
    "Legal Combos" and "Bomb Hierarchy") — the level rank instead stays in
    its natural position and behaves like any other rank, contributing no
    extra strength. See "Level cards inside sequential combos," right after
    this section.
- **Full ranking order for the non-sequential case**, low → high: take the
  13 base ranks (`2` through `A`), remove the level rank from its natural
  position, and append **all four suits' copies of the level rank, tied
  together as one tier** (no suit ranks above another), then the two joker
  ranks on top:

  ```
  <base ranks 2..A, excluding the level rank, in their normal order>,
  <level rank — all 4 suits, tied>,
  Small Joker,
  Big Joker
  ```

  Worked example at level 6 (`6` removed from its natural slot between `5`
  and `7`): `2 < 3 < 4 < 5 < 7 < 8 < 9 < 10 < J < Q < K < A < 6♠=6♣=6♥=6♦ <
  Small Joker < Big Joker`. All four physical `6`s from both decks
  (2 copies × 4 suits = 8 cards) are equal rank here, including the heart
  copies — the heart suit's only distinguishing property is the wildcard
  behavior described in "The Wildcard," not a rank boost. A pair, triple, or
  larger same-rank group made of level cards is equally strong regardless of
  which suits compose it (e.g. `6♥6♠` ranks the same as `6♠6♠` or `6♥6♥`).

- **Deal level**: the **declaring team's** current level — i.e. its level
  *after* the previous deal's advancement and any level-A drop (see
  "Failing at A three times") have been applied. The declaring
  team is the previous deal's winning team (the team of the previous deal's
  1st-place player). The other team's level is tracked independently but
  doesn't affect this deal's card ranking. The first deal of a match has no
  previous winner; its deal level is 2. Whichever team wins the current
  deal becomes the declaring team for the next one, regardless of finish
  type.

### Level cards inside sequential combos

Within a straight, tube, plate, or straight flush, the level rank is **not**
pulled out of sequence — it sits at its natural position, same as any other
rank, and links into a run with its natural neighbors exactly like a
non-level card would.

Worked example at level 6: `4,4,5,5,6,6` is a valid tube — the `6,6` pair
is just an ordinary link in the 4-5-6 run, using plain natural-position 6s,
no special status. Likewise `5,6,7,8,9` is a valid straight through a
natural-position 6. When comparing two sequential combos, a level card
contributes its **natural** rank value, not the elevated one — a straight
or tube gets no extra strength just because it happens to include the level
rank.

## The Wildcard (逢人配)

The wildcard ("逢人配" — roughly "matches anyone") is the heart-suit level
card (e.g. `6♥` at level 6). There are two in the deck. Its power is broad:

- **Played alone as a single**, it has **no** wildcard power — it's simply
  an ordinary level card, ranked exactly the same as the other three suits'
  copies (see "Deck & Card Ranking" above).
- **In every other combo** — pair, triple, full house, straight, tube,
  plate, and any bomb except the joker bomb (including a straight flush) —
  a wildcard can stand in for **one missing card** of whatever rank (and,
  for a straight flush, suit) is needed to complete that combo. Worked
  examples: holding `4,4,4` plus one wildcard lets you play `4,4,4,4` as a
  quadruple bomb; holding `5,6,7,8` plus one wildcard lets you play
  `4,5,6,7,8` as a straight, the wildcard standing in for the missing `4`. A
  bomb completed this way is still classified and ranked purely by its
  resulting size/rank (a triple plus one wildcard is a quadruple bomb of
  that rank, full stop).
- **A wildcard can never stand in for a joker** — it cannot be used toward
  a joker bomb or a joker pair, and jokers themselves have no wildcard
  behavior of their own.
- Each physical wildcard fills exactly **one** missing slot. If a hand
  holds both wildcards, both can be used as two independent substitutions
  within the same combo (e.g. two different missing ranks in one tube), or
  one can be played normally while the other substitutes — ordinary
  hand-composition choices, not an extra rule.
- **Two wildcards played together as a pair** form a pair of level cards
  (equal to any other pair of level cards). Like a lone wildcard, they can't
  be declared as a pair of any other rank.
- **Declaring a wildcard**: when a player uses a wildcard as a stand-in, they
  place it in the spot of the card it replaces (e.g. between the `3` and `5`
  if it stands for a `4`) and, if it isn't obvious, say what combo and card
  it represents.
- **Declared suit**: a wildcard stands in for a specific rank *and* suit, and
  the player chooses both. So the player decides whether a wildcard-completed
  run is a straight flush or a plain straight. For example, `4♠5♠W7♠8♠` can
  be a straight flush (W = `6♠`) or a straight (W = `6` of any other suit).
  The "5 same-suit cards are always a straight flush" rule applies only when
  all five *physical* cards share a suit.

### Resolving wildcard plays (engine rule)

A set of cards containing wildcards may form more than one legal combo.
Pagat's example: at level 4, `4♥,4♥,8,8,9,9` could be the tube `7-7-8-8-9-9`,
the tube `8-8-9-9-10-10`, or the plate `8-8-8-9-9-9`. The engine handles
this as follows:

1. The engine lists every legal reading of the selected cards: the combo
   type, its rank, and what each wildcard stands for.
2. **When leading**, the player may choose any of these readings.
3. **When following**, only readings that can beat the current play are
   allowed: the same type and strictly higher, or a bomb that beats it. If
   there is exactly one such reading, it's used automatically. Otherwise the
   player chooses.
4. Once chosen, the reading is fixed. The play is compared, and later plays
   are compared against it, purely by that declared type and rank.
5. A wildcard always *may* stand for itself (the heart level card in its
   natural position), but it is never required to. The "wildcard must stand
   for itself if possible" variant is not used.

## Legal Combos

A non-bomb play is compared only against the same combo type already on the
table in the current trick (or beaten by a bomb — see "Bomb Hierarchy").
Suit is irrelevant except where stated.

| Combo | Cards | Notes |
|---|---|---|
| Single | 1 card | Any card. |
| Pair | 2 cards, same rank | A joker pair must be two Big Jokers or two Small Jokers. |
| Triple | 3 cards, same rank | |
| Full house | Triple + pair | Ranked by the triple's rank only (the pair doesn't matter). The pair can be any pair, including a joker pair, but it must be a **different rank** from the triple. Five cards *read as* one rank (e.g. `8,8,8,8,8`, or `8,8,8,W,W` with both wildcards standing for `8`s) are only ever a quintuple bomb, never a full house. But `8,8,8,W,W` has a second reading: the two wildcards stand for themselves as a pair of level cards, giving a full house of `8`s (see "Resolving wildcard plays"). |
| Straight | Exactly 5 consecutive ranks, one card each | Cards must **not** all be the same suit — 5 same-suit consecutive physical cards are always a straight flush (a bomb), never a straight. (With a wildcard, the player picks its suit, so they choose between straight and straight flush — see "Declared suit.") A can sit at either end: `A,2,3,4,5` (A low) and `10,J,Q,K,A` (A high) are both legal, but a straight can't wrap through both ends at once (`Q,K,A,2,3` is invalid). Ranked by highest card in natural rank order — so `A,2,3,4,5` is the *lowest* straight, since its A is acting as a 1. Level cards participate at their natural position (see "Level cards inside sequential combos"). |
| Tube (连对) | Exactly **3** consecutive-rank pairs (6 cards) | Fixed length — 2 consecutive pairs is **not** a legal combo. Level cards participate at natural position. A can be high or low but can't wrap: `A,A,2,2,3,3` is the lowest, `Q,Q,K,K,A,A` the highest, `K,K,A,A,2,2` is illegal. Ranked by the highest pair in natural rank order. |
| Plate (钢板) | Exactly **2** consecutive-rank triples (6 cards) | Fixed length. Level cards participate at natural position. A can be high or low: `A,A,A,2,2,2` is the lowest and `K,K,K,A,A,A` the highest. Ranked by the higher triple in natural rank order. |

**Jokers can never be part of a straight, tube, plate, or straight flush.**

**Anything not in the table above (or in "Bomb Hierarchy" below) is not a
legal play.** Common illegal examples: a triple plus a single card, two
pairs (4 cards), a run of 4 or 6 single cards, two consecutive pairs, a
Big Joker + Small Joker "pair."

A play must always be the same combo type as the play it's beating (a pair
must be beaten by a higher pair, a full house by another full house, etc.)
— **except that any bomb beats any non-bomb play of any type**, and a
higher bomb beats a lower one.

## Bomb Hierarchy

Bombs can be played on *any* turn regardless of what combo type the trick
currently requires, and once one is played, only a higher bomb can beat it
(a non-bomb cannot). Lowest to highest:

1. **Quadruple bomb** — 4 of the same rank (any suits).
2. **Quintuple bomb** — 5 of the same rank.
3. **Straight flush** — 5 consecutive ranks, all one suit. Sits *between*
   quintuple and sextuple bombs in strength.
4. **Sextuple bomb** — 6 of the same rank.
5. **Septuple bomb** — 7 of the same rank.
6. **Octuple bomb** — 8 of the same rank.
7. **Nonuple bomb** — 9 of the same rank.
8. **Decuple bomb** — 10 of the same rank. The 2 decks hold only 8 copies of
   any rank, so 9- and 10-card bombs are only possible by adding one or
   both wildcards (e.g. eight `7`s + two wildcards). A bomb of the level
   rank itself maxes out at 8, since the wildcards are already among those
   8 cards.
9. **Joker bomb** — all 4 jokers together. Unbeatable; the single strongest
   play in the game.

In short: among numbered bombs, **more cards always beats fewer cards**,
with the straight flush slotted between 5-card and 6-card bombs.

Within a numbered-bomb size (e.g. two different quadruple bombs), compare by
rank using the non-sequential ranking order above — level-rank bombs rank
higher than non-level ranks of the same size, since jokers can't form a
numbered bomb (there are only 2 of each joker).
Between straight flushes, compare by highest card in natural rank order,
same as a straight (`A,2,3,4,5` is the lowest straight flush). Suit never
matters.

**Ties never beat.** A bomb only beats another bomb if it is strictly
higher. Two straight flushes with the same top card (in any suits), or two
same-size bombs of equal rank (e.g. two quadruple `9` bombs), cannot beat
each other. The same applies to every combo type.

## Turn Play & Trick Resolution

- The leader (see "Deal Start," and Tribute) leads a trick with any legal
  combo. **The leader may not pass.**
- Play proceeds around the table, **skipping any player who has already
  gone out** — they take no further turns this deal. Each active player, on
  their turn, must either:
  - **Follow** with a strictly higher combo of the *same type* as the
    current best play in the trick, or
  - **Play a bomb** (any bomb beats any non-bomb; a higher bomb beats a lower
    bomb), or
  - **Pass.** A player may always pass, even if they could beat the current
    play.
- **Passing only skips your current turn.** If someone else plays after you
  passed, play keeps going around the table and you get another turn — you
  may play then even though you passed earlier in the same trick. Example:
  seat 0 leads, seat 1 passes, seat 2 beats seat 0, seat 3 passes, seat 0
  passes — seat 1 now gets a turn again and may beat seat 2.
- **The trick ends the moment every other active player has passed in a
  row since the last play.** The player who made that last (unbeaten) play
  wins the trick; the trick's cards are set aside (they never return to
  anyone's hand), and the trick winner **leads the next trick** — *unless*
  they just went out on that winning play (see 接风, immediately below).
- A player who has already gone out is skipped for the rest of the deal —
  they no longer take turns, but the deal continues among the active
  players until one team has both players out (see "Deal End").

A player may **take back** their most recent play until the next player
acts (house rule; see "Interpretations & house rules" #10).

### Announcing cards left

After making a play, a player who has **10 or fewer cards** left in their
hand must say out loud how many cards they have left. Other players may
not ask about card counts, and the player shouldn't repeat it unprompted.

**Engine rule — hand-size visibility**: a player's exact card count is
shown to everyone only when it is 10 or fewer. While a player holds more
than 10 cards, other players see only that the count is "more than 10."

### 接风 ("Welcoming the wind") — lead passes to the partner

If the player who just won a trick has **no cards left** (their winning
play was also their last card), they can't lead the next trick. In that
case, the lead passes to their **partner** instead of continuing to the
next seat in normal rotation — this is called 接风 ("welcoming the wind"),
and it comes up in most deals, since going out on a winning play is common.
(The partner can never have *also* gone out already at this point: if they
had, this player's going out would give their team both players out, which
ends the deal immediately — see "Deal End.")

## Deal End & Level Advancement

Players get finish places in the order they go out: **1st (头游), 2nd
(二游), 3rd (三游), 4th (下游)**. **A deal ends as soon as both players on
one team have gone out.** Cards left in anyone's hand don't matter. In
practice:

- If one team takes 1st and 2nd, the deal ends right there — the other
  team's two players both lose and no 3rd/4th order between them is
  needed (tribute for this case doesn't depend on it; see Tribute).
- Otherwise the deal ends when the 3rd player goes out, and the one player
  still holding cards is 4th.

The winning team is the team of the 1st-place player. Its level advances
based on the finish type:

| Finish type | Partner of 1st place finished | Levels advanced |
|---|---|---|
| 1-2 | 2nd | **+3** |
| 1-3 | 3rd | **+2** |
| 1-4 | 4th | **+1** |

Only the winning team's level advances; the other team's level is
unchanged. **Levels stop at A**: a team can never jump past A. If an
advance would take a team beyond A (e.g. a team at Q wins with a 1-2), it
lands on A, and must then win a deal at level A to win the match — see
"Match End Condition."

## Tribute & Anti-Tribute (进贡 / 还贡 / 抗贡)

Tribute happens **after dealing, before the first trick** of each deal
(except the first deal of a match, which has no tribute — there's no
previous deal's result to derive it from).

**Who pays tribute, to whom** — determined by the *previous* deal's finish
type. The tribute is always the **highest-ranked card among the tribute
payer's non-wildcard cards**. A wildcard may never be given as tribute. This
doesn't mean dropping to the next rank: at level 6, a player holding `6♥`
and `6♠` must give the `6♠` (a level card of the same tier), not an A.

- **1-3 or 1-4 finish ("single tribute")**: the **4th-place player** gives
  their highest card to the **1st-place player**. This is true even in a
  1-4 finish, where the 4th-place player is the 1st-place player's own
  partner — tribute follows finish place, not team.
- **1-2 finish ("double tribute")**: **both** players of the losing team
  each give their highest card. There's no 3rd/4th order between them (the
  deal ended when the winning team went out 1st and 2nd), and none is needed:
  the two tributes are compared, and
  - the **higher** card goes to the **1st-place player**,
  - the **lower** card goes to the **2nd-place player**.
  - **If the two cards are equal rank**, each tribute payer gives their card
    to the winning-team player seated **clockwise** from them — i.e. the
    one on their left, who takes their turn just *before* them. (So the
    tribute payer who plays right
    after the 1st-place player pays the 1st-place player.)
- **"Highest card" is evaluated using the *new* deal's ranking** — i.e. the
  ranking order for the deal level the new declaring team is about to play
  (per "Deal level" above) — since that's the ranking that governs the deal
  tribute is setting up. If a player holds multiple physical cards tied for
  highest rank (possible with two decks — e.g. two Aces), they choose which
  physical copy to hand over; it's inconsequential which, since tied-rank
  cards are interchangeable here.

**Anti-tribute (抗贡) — tribute is cancelled** if the player(s) who would
pay tribute hold **both Big Jokers**:

- **Single tribute**: the 4th-place player must hold **both** Big Jokers
  themselves. (Their partner holding one doesn't count.)
- **Double tribute**: the two tribute payers together hold both Big Jokers — either
  one holds both, or each holds one.

When cancelled, there is no tribute or return card at all, and **the
previous deal's 1st-place player leads the first trick**.

**Return card (还贡)**: each player who received a tribute gives back one
card of their choosing, ranked **10 or below**, to the player who paid
them. A level card cannot be used as a return card even if its natural rank
is ≤10 (e.g. at level 6, a plain `6` cannot be returned) — level cards are
excluded from the returnable pool regardless of natural rank, since they're
elevated/valuable at the moment, not "junk." **If the player has no
eligible card of 10 or below, they return their lowest-ranked card
instead.** If several cards are tied for lowest, the receiver chooses which
one. The receiver **may not return the physical card they just received as
tribute**. This is rare but possible: a tribute can be ≤10, and the
lowest-card fallback could otherwise pick it. The receiver may return
another physical copy of the same rank and suit.

**Order of operations**:

1. After dealing, check anti-tribute using the hands as dealt. If it
   applies, skip to the first trick.
2. Determine every tribute card from the hands as dealt. In a double
   tribute, both tributes are decided before any card changes hands.
3. Tribute cards are moved into the receivers' hands and **shown to all
   players**.
4. Each receiver chooses a return card from their hand, which now includes
   the tribute. In a double tribute, both receivers choose **blind** (neither
   sees the other's choice), and the two returns are revealed together.
5. Return cards are moved into the payers' hands and **shown to all
   players**.
6. The first trick begins (see "Who leads after tribute").

**Who leads after tribute**:

- **Single tribute**: the tribute payer leads the first trick.
- **Double tribute**: the tribute payer who gave to the **1st-place player** leads.
- **Anti-tribute (cancelled)**: the previous deal's 1st-place player leads.

## Match End Condition

Levels stop at A (see "Deal End & Level Advancement"), so every team must
eventually play a deal at level A. A team **wins the match** when, as the
declaring team at level A, it wins a deal with a **1-2 or 1-3** finish
type. A 1-4 win at level A does **not** win the match — the team simply
stays at A and play continues. (If the *other* team wins a deal, the usual
rules apply: they advance from their own level, and the next deal is played
at their level.)

### Failing at A three times (house rule)

Each team has an **A-attempt counter**, starting at 0.

- An **A attempt** is any deal in which the team is the declaring team at
  deal level A. Being at level A while the *other* team is declaring does
  not count.
- A **failed A attempt** is an A attempt that doesn't end in a match win:
  either the team loses the deal, or it wins with a 1-4 finish.
- After each failed A attempt, increment the team's counter. Attempts don't
  need to be consecutive.
- When the counter reaches **3**, the team's level drops back to **2** and
  its counter resets to 0. This happens after the deal's level advancement
  is resolved, and before the next deal starts.
- The drop doesn't change who the declaring team is for the next deal. If
  the third failure was a 1-4 win, the team is still the winning team, so
  the next deal is played at its new level, 2. If the third failure was a
  loss, the other team declares as usual.
- The counter resets only on this drop, at the start of a match, and when
  the team's level is changed in the lobby settings (house rule #9). Once a
  team is at A, this drop (or a settings change) is the only way its level
  can change.

## Glossary

| Chinese | Pinyin | Term used in this doc |
|---|---|---|
| 掼蛋 | guàndàn | Guandan ("throwing eggs") |
| 级牌 | jípái | level card |
| 红桃级牌 / 逢人配 | hóngtáo jípái / féngrénpèi | wildcard ("matches anyone") |
| 连对 / 三连对 | liánduì / sān liánduì | tube (exactly 3 consecutive pairs) |
| 钢板 / 二连三 | gāngbǎn / èr lián sān | plate (exactly 2 consecutive triples) |
| 三带二 / 三带对 | sān dài èr / sān dài duì | full house (the UI says 三带二) |
| 顺子 | shùnzi | straight |
| 同花顺 | tónghuāshùn | straight flush |
| 炸弹 | zhàdàn | bomb |
| 王炸 / 天王炸 | wángzhà / tiānwángzhà | joker bomb |
| 头游 / 上游 | tóuyóu / shàngyóu | 1st place |
| 二游 | èryóu | 2nd place |
| 三游 | sānyóu | 3rd place |
| 下游 / 末游 | xiàyóu / mòyóu | 4th place |
| 双下 | shuāngxià | the losing team in a 1-2 finish |
| 进贡 | jìngòng | tribute |
| 还贡 | huángòng | return card |
| 抗贡 | kànggòng | anti-tribute |
| 接风 | jiēfēng | lead passes to partner when the trick winner has gone out |
| 过 | guò | pass (what the official rules say to announce) |
| 悔牌 | huǐpái | take back a play (house rule #10) |
| 大王 | dàwáng | Big Joker |
| 小王 | xiǎowáng | Small Joker |

## Interpretations & house rules

Points where the official rules are unclear or silent and we picked an
interpretation, or where we deliberately deviate. Flag any of these if you
want a different variant:

1. **Level cards can't be return cards.** The official rule only says the
   return card must be "10 or lower" (with the lowest-card fallback). We
   additionally exclude level cards from the returnable pool regardless of
   natural rank (e.g. a level-6 `6` can't be returned even though
   `6 ≤ 10`), for internal consistency — level cards are valuable, not
   "junk," at the moment. House rule.
2. **Same-suit 5-card run: straight flush only, never a straight.** The
   official rules define a straight flush as "a straight where all cards
   share the same suit" without saying whether a player may choose to play
   one as an ordinary straight. We follow pagat.com, which defines a
   straight as 5 consecutive cards that "are not all of the same suit" — so
   a same-suit run is always a bomb.
3. **Double-tribute tie ("clockwise precedence").** When both tributes are
   equal rank, the official rules say only that "the order of tribute
   follows clockwise precedence." We read this as: each tribute payer gives to
   the winning-team player seated clockwise from them (the one who plays
   just before them). Pagat instead lets the winning team pick.
4. **Failing at A three times sends a team back to level 2.** Not in the
   official rules; taken from pagat.com, which states it as: "If a team
   have been declarers on level A three times (not necessarily
   consecutive) without winning," they go back to level 2. House rule —
   see "Failing at A three times" under "Match End Condition."
5. **A wildcard's declared suit is the player's choice.** So a run completed
   by a wildcard can be declared as either a straight or a straight flush.
   The official rules and pagat don't cover this. It follows from "a
   wildcard can stand in for any card."
6. **A full house's pair must be a different rank from its triple.** The
   official rules say only "a triplet combined with any pair." We treat five
   cards read as one rank as a quintuple bomb only, as most apps do. A
   triple plus two wildcards standing for themselves (a level-card pair) is
   still a legal full house, e.g. `8,8,8,6♥,6♥` at level 6.
7. **Information visibility.** Exact hand sizes are hidden until a player
   has 10 or fewer cards. Tribute and return cards are shown to everyone.
   The official rules only require reporting counts of 10 or fewer, and
   require return cards to be revealed.
8. **No time limits.** The official rules set time limits (20s to pay
   tribute, 40s to return, 40s for the first lead, 20s per turn). We don't
   enforce any timers for now.
9. **Table settings.** Between deals (and before a match), the players may
   set each team's level and choose the declaring team; this overrides
   "levels start at 2" and "the first deal is played at level 2". Changing a
   team's level resets that team's A-attempt counter to 0. Once a deal has
   been played, some team must be declaring. The first deal of a match still
   has no tribute and its leader is picked by the turned-up card (see "Deal
   Start"), whatever the levels, and if its declaring team is at A it is a
   real A attempt. Every later deal's tribute uses the previous deal's
   finish places as usual, so seats can't change mid-match. A deal may also
   be abandoned and dealt again (**Redeal**, 重新发牌); this changes nothing
   but the cards.
   Not in the official rules; a convenience for friendly play and testing
   (see `LOBBY_FLOW_SPEC.md`).
10. **Taking back a play.** After making a **play** (not a pass), a player
   may take it back as long as **no other player has acted since**. Taking
   it back restores exactly the state before the play: the cards return to
   their hand, the trick (its best play and who has passed) is as it was,
   and it's their turn again. If the play made them go out, that is undone
   too. A play that **ended the deal** can't be taken back, and only the
   most recent play can be (one step). Passes, tributes and returns can't be
   taken back. Everyone is told when a play is taken back. Not in the
   official rules; a convenience for friendly play (see
   `GAME_PAGE_V3_SPEC.md` §1).

### Variants and official provisions *not* used

Anything not described in this doc is not part of the game. The following
are listed only so it's clear they were left out on purpose:

- Pagat's "declarers on level A who lose to a final play made entirely of
  Aces drop to level 2."
- "A wildcard must stand for itself when it can."
- Pagat's +4 for a 1-2 finish (we use the official +3).
- The official time-based and deal-count match formats, and the official
  penalty system (warnings, violations, fouls, forfeits).
- The official rules for who shuffles, cuts and draws first in later
  deals. The engine deals randomly.

## Sources

- [World Guandan Competition Rules (2025)](https://gawsf.b-cdn.net/files/World%20Guandan%20Competition%20Rules%20(2025)%20.pdf)
  (also in [Chinese](https://gawsf.b-cdn.net/files/%E4%B8%96%E7%95%8C%E6%8E%BC%E7%89%8C%EF%BC%88%E6%8E%BC%E8%9B%8B%EF%BC%89%E7%AB%9E%E8%B5%9B%E8%A7%84%E5%88%99%EF%BC%882025%EF%BC%89.pdf))
  — the official competition ruleset and the primary authority for this
  doc. Its English version uses "Banker" for the 1st-place player and
  "Dweller" for the 4th-place player.
- pagat.com — [Guan Dan](https://www.pagat.com/climbing/guan_dan.html)
  (John McLeod's card game rules reference) — the most detailed English
  source; used where the official rules are silent.
- Official inter-university/trade-union tournament rule documents
  (gh.nuist.edu.cn, ddgh.seu.edu.cn, gonghui.njau.edu.cn,
  gonghui.hhu.edu.cn), which publish near-identical "掼蛋比赛规则"
  documents.
- 知乎 articles on 掼蛋规则 (e.g. zhuanlan.zhihu.com/p/688507894,
  /p/690222407), a CSDN rules writeup
  (blog.csdn.net/2301_81874585/article/details/136947816), and 知乎/CSDN/
  gameabc.com articles on 进贡, 还贡, 抗贡.
- [Guandan — Wikipedia](https://en.wikipedia.org/wiki/Guandan) and
  [officialgamerules.org — How to Play Guan Dan](https://officialgamerules.org/game-rules/guan-dan/)
  for general overview.
