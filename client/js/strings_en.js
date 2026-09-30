// English UI strings (LANGUAGE_SPEC §4.2). strings_zh.js has exactly the
// same keys. A value is a string, or a function of named parameters for
// text with names, numbers or grammar in it; look strings up with i18n.js
// t(key, params). Groups are by the file that shows them.
//
// Parameters are already-formatted text (a player name, a rank label like
// "9", a team name from format.js teamName, …) unless noted: `n` is a
// number, and a parameter noted "array" is an array of such text.
export const EN = {
  // ---------- shared (several files) ----------
  "app.name": "Guandan",
  "common.cancel": "Cancel",
  "common.listSep": ", ", // i18n.js list(): "a, b and c"
  "common.listLast": " and ",
  "common.aTries": ({ n }) => `Attempts: ${n}/3`, // failed A attempts: lobby settings, table.js ribbon, results.js tiles

  // ---------- language toggle (main.js); the same in both languages ----------
  "lang.groupAria": "Language / 语言",
  "lang.en": "EN",
  "lang.zh": "中文",

  // ---------- main.js: tab title, loading, connection banner ----------
  "main.titleYourTurn": "▶ Your turn — Guandan",
  "main.loading": "Loading…",
  "main.lookingFor": ({ code }) => `Looking for game ${code}…`,
  "main.joiningAs": ({ name }) => `Joining as ${name}…`,
  "main.connecting": "Connecting…", // waiting screen and banner
  "main.reconnecting": "Connection lost — reconnecting…",
  "main.debugTitle": "Debug panel",
  "main.debugAria": "Toggle debug panel",

  // ---------- home.js: Home ----------
  "home.yourName": "Your name",
  "home.createGame": "Create game",
  "home.creating": "Creating…",
  "home.or": "or",
  "home.inviteCode": "Invite code",
  "home.codePlaceholder": "482193",
  "home.joinGame": "Join game",
  "home.errNameFirst": "Enter your name first.",
  "home.errCodeMissing": "Enter the invite code you were given.",
  // typed = what was typed (first 20 characters), length = CODE_LENGTH
  "home.errCodeInvalid": ({ typed, length }) =>
    `“${typed}” isn't an invite code: codes are ${length} digits, like 482193.`,
  "home.errServerFull": "The server is full right now (too many games). Try again in a few minutes.",
  "home.errCreateFailed": "Couldn't create a game — is the server running? Try again.",

  // ---------- home.js: no such game / game ended ----------
  "gone.noGame": ({ code }) => `No game with code ${code}`,
  "gone.noGameHint": "Check the code or link you were given, or start a new game.",
  "gone.ended": "This game has ended",
  "gone.endedHint": "The game closed after everyone left. Start a new one from the home page.",
  "gone.backHome": "Back to home",

  // ---------- lobby.js: name form (heading: app.name) ----------
  "nameForm.yourName": "Your name",
  "nameForm.join": "Join",

  // ---------- lobby.js: lobby ----------
  "lobby.title": "Lobby",
  "lobby.invite": "Invite:",
  "lobby.copyLink": "Copy link",
  "lobby.copied": "Copied ✓",
  "lobby.inviteManualAria": "Invite link — copy it by hand",
  // Team section heading and aria: team.named. The pill:
  "lobby.yourTeam": "Your team",
  "lobby.you": "You", // next to your name in your spot
  "lobby.connected": "Connected",
  "lobby.offline": "Offline",
  "lobby.ready": "Ready", // ✓ title / aria
  "lobby.sitHere": "Sit here",
  "lobby.empty": "Empty",
  "lobby.imReady": "I'm ready",
  "lobby.readyOn": "Ready ✓",
  "lobby.readyTitle": "Click when you're ready to start",
  "lobby.cancelReadyTitle": "Click to cancel",
  // "Waiting for <strong>names</strong>": the text before and after the
  // bold names (names = list([...]), with lobby.youInList for you).
  "lobby.waitingForPre": "Waiting for ",
  "lobby.waitingForPost": "",
  "lobby.youInList": "you",
  "lobby.waitingMore": ({ n }) => `Waiting for ${n} more player${n === 1 ? "" : "s"}`,
  "lobby.notReady": ({ names }) => ` · not ready: ${names.join(", ")}`, // names: array
  "lobby.starting": "Starting…",
  "lobby.seatsLocked": "Seats are locked during a match — start a new match to change seats.",
  "lobby.settings": "Settings",
  "lobby.settingsAria": "Table settings",
  "lobby.settingsNote": "Changing a setting un-readies everyone.",
  "lobby.levelSuffix": " level", // after the bold team name: "<strong>North-South</strong> level"
  "lobby.declaring": "Declaring",
  "lobby.declaringAria": "Declaring team",
  "lobby.declaringNone": "None — first deal at 2",
  "lobby.newMatch": "New match…",
  "lobby.newMatchAria": "Confirm new match",
  "lobby.startOverQuestion": "Start over? Levels go back to 2 and seats unlock.",
  "lobby.startOver": "Start over",

  // ---------- lobby.js: kicked overlay ----------
  "kicked.title": "Seat taken over",
  "kicked.body": "This seat was taken over by another tab or device.",
  "kicked.hint": "Taking it back reconnects here and disconnects the other one.",
  "kicked.takeBack": "Take it back",

  // ---------- results.js: Last deal card / match-won banner ----------
  // team = format.js teamName(...); finish = "1-2" | "1-3" | "1-4".
  "results.lastDeal": "Last deal", // band title and aria
  "results.matchWonAria": "Match won",
  "results.matchWon": ({ team }) => `🏆 ${team} won the match!`, // band title
  "results.levelsBack": "Levels are back to 2.", // band note after a match win
  "results.played": ({ name }) => `${name} played:`, // over the final play's cards
  "results.wins": ({ team }) => `${team} wins!`,
  "results.finish": ({ finish }) => finish, // "1-2" | "1-3" | "1-4" badge
  "results.place": ({ name, place }) => `${name} ${place}`, // place = format.js placeLabel
  "results.places": ({ places }) => places.join(", "), // places: array of results.place
  "results.yourTeam": "(your team)", // in a score tile

  // ---------- table.js: header ribbon, pennant, Redeal ----------
  // Side names: team.named (long), format.js teamRelative (short).
  "table.yourTeam": "(your team)",
  // Side hover / aria. mine, declaring: booleans; tries: the A-attempt count
  // (number) for a team at A, else null.
  "table.sideAria": ({ team, mine, level, declaring, tries }) =>
    `Team ${team}${mine ? " (your team)" : ""}: level ${level}`
    + `${declaring ? ", declaring" : ""}${tries !== null && tries !== undefined ? `, attempts: ${tries}/3` : ""}`,
  "table.pennantLabel": "level",
  "table.pennantTitle": ({ level }) => `This deal is played at level ${level}`,
  "table.pennantAria": ({ level }) => `Deal level ${level}`,
  "table.lastTrick": "Last trick", // header toggle and the center tag
  "table.redeal": "Redeal", // header button, confirm aria, confirm button
  "table.redealConfirm": "Throw away this deal and go back to the lobby? Levels stay as they were before it.",

  // ---------- table.js: seat plates ----------
  "table.partner": "(partner)",
  "table.you": ({ name }) => `You (${name})`,
  "table.cardsLeft": "Cards left",
  "table.offline": "Offline",
  "table.disconnected": "Disconnected",
  "table.turn": "TURN",
  "table.playsNext": "Plays next",

  // ---------- table.js: felt ----------
  "table.passCard": "PASS", // the pass card in a seat's pile
  "table.pass": "Pass", // its hover / aria
  "table.leads": ({ name }) => `${name} leads`,
  "table.jiefeng": ({ a, b }) => `${a} went out — partner ${b} leads`, // 接风: a went out, b leads
  "table.tributeBeforeDeal": "Tribute before the deal",

  // ---------- play.js: action bar, reading picker ----------
  "play.out": "You're out — waiting for the deal to end.",
  "play.yourLead": "Your lead",
  "play.yourTurn": "Your turn",
  "play.waitingFor": ({ name }) => `Waiting for ${name}…`,
  "play.selected": ({ n }) => `${n} selected`,
  "play.clear": "Deselect",
  "play.takeBack": "Take back",
  "play.takeBackTitle": "Take back your last move (a play or a pass)",
  "play.pass": "Pass",
  "play.play": "Play",
  "reading.title": "Which play do you mean?", // heading and aria

  // ---------- hand.js: toolbar, group hover ----------
  "hand.group": "Group",
  "hand.ungroup": "Ungroup",
  "hand.clearGroups": "Clear groups",
  "hand.checking": "Group (checking…)",
  "hand.notLegal": "Group (not a legal play)",
  "hand.readingSep": " / ",

  // ---------- tribute.js ----------
  "tribute.aria": "Tribute",
  "tribute.titleSingle": "Tribute (single)",
  "tribute.titleDouble": "Tribute (double)",
  "tribute.paidPrefix": "Paid: ", // followed by the card chip
  "tribute.paid": "Paid",
  "tribute.waitingTribute": "Tribute: waiting…",
  "tribute.returnHidden": "Return: ✓ (hidden)",
  "tribute.returnWaiting": "Return: waiting…",
  // name = who you pay / return to; null when unknown (no " to …").
  "tribute.taskPay": ({ name }) =>
    `Your task: choose a card to pay as tribute${name ? ` to ${name}` : ""}.`,
  "tribute.taskReturn": ({ name }) =>
    `Your task: choose a card to return${name ? ` to ${name}` : ""}.`,
  "tribute.waitingPay": ({ names }) => `Waiting for ${names} to pay tribute…`, // names = list([...])
  "tribute.waitingReturn": ({ names }) => `Waiting for ${names} to return a card…`,
  "tribute.waiting": "Waiting…",
  "tribute.payButton": "Pay tribute",
  "tribute.returnButton": "Return card",

  // ---------- cards.js ----------
  "cards.wildcard": "Wildcard", // the gold dot's title
  "cards.wildcardAria": ({ card }) => `${card} (wildcard)`, // card = format.js cardLabel

  // ---------- net.js: toast, notices ----------
  "net.notConnected": "Not connected — try again in a moment.",
  "net.tookBack": ({ name }) => `${name} took back their move`,

  // ---------- net.js: Rejected, by code (the server's English text) ----------
  "error.WrongPhase": "That action isn't allowed right now",
  "error.NotYourTurn": "It is not your turn",
  "error.CardsNotInHand": "Those cards are not in your hand",
  // A rejected play also lists the cards you tried (net.js rejectedText).
  "error.withCards": ({ message, cards }) => `${message}: ${cards}`,
  "error.NotAValidCombo": "Those cards don't form a valid combination",
  "error.DoesNotBeatCurrent": "That play doesn't beat the current play",
  "error.InvalidDeclaration": "Those cards can't be played as the declared combination",
  "error.CannotPassWhenLeading": "You can't pass when you are leading",
  "error.NothingToTakeBack": "There's nothing of yours to take back",
  "error.NotATributePayer": "You don't owe a tribute",
  "error.AlreadyPaid": "You have already paid your tribute",
  "error.InvalidTributeCard": "That card can't be paid as tribute",
  "error.NotATributeReceiver": "You aren't receiving a tribute",
  "error.TributeNotComplete": "Wait until every tribute has been paid",
  "error.AlreadyReturned": "You have already returned a card",
  "error.InvalidReturnCard": "That card can't be returned",
  "error.NotJoined": "Join the room first",
  "error.AlreadyJoined": "You have already joined",
  "error.InvalidName": "Names must be 1 to 20 characters",
  "error.NameTaken": "That name is already taken by a connected player",
  "error.NoSeatsAvailable": "All seats are taken",
  "error.NotInDeal": "There is no deal in progress",
  "error.NotInLobby": "That can only be done in the lobby",
  "error.SeatTaken": "That seat is taken",
  "error.SeatsLocked": "Seats are locked during a match; start a new match to change seats",
  "error.InvalidSettings": "Once a deal has been played, a team must be declaring",

  // ---------- format.js: teams, seats, places ----------
  "team.A": "North-South",
  "team.B": "East-West",
  "team.named": ({ team }) => `Team ${team}`, // team = teamName(...); lobby/table/results headings
  "team.us": "Us", // teamRelative: the viewer's team
  "team.them": "Them",
  "seat.dir0": "South",
  "seat.dir1": "East",
  "seat.dir2": "North",
  "seat.dir3": "West",
  "seat.fallback": ({ n }) => `Seat ${n}`, // seatName for an empty seat
  "place.0": "1st",
  "place.1": "2nd",
  "place.2": "3rd",
  "place.3": "4th",

  // ---------- format.js: cards ----------
  "card.smallJoker": "SJ", // cardLabel: the name in text
  "card.bigJoker": "BJ",
  "card.smallJokerCorner": "SJ", // cardCorner: the card's corner
  "card.bigJokerCorner": "BJ",

  // ---------- format.js: combo labels ----------
  // rank / face are rank labels already made plural with combo.plural
  // ("9s"); span "5–9"; run "7-7-8-8-9-9"; size a combo.size* word.
  "combo.plural": ({ rank }) => `${rank}s`,
  "combo.smallJoker": "Small Joker",
  "combo.bigJoker": "Big Joker",
  "combo.smallJokers": "Small Jokers",
  "combo.bigJokers": "Big Jokers",
  "combo.pair": ({ face }) => `Pair of ${face}`,
  "combo.triple": ({ rank }) => `Three ${rank}`,
  "combo.fullHouse": ({ rank }) => `Full house, ${rank}`,
  "combo.straight": ({ span }) => `Straight ${span}`,
  "combo.tube": ({ run }) => `Tube ${run}`,
  "combo.plate": ({ run }) => `Plate ${run}`,
  "combo.bomb": ({ size, rank }) => `Bomb: ${size} ${rank}`,
  "combo.size4": "four",
  "combo.size5": "five",
  "combo.size6": "six",
  "combo.size7": "seven",
  "combo.size8": "eight",
  "combo.size9": "nine",
  "combo.size10": "ten",
  "combo.straightFlush": ({ span }) => `Straight flush ${span}`,
  "combo.jokerBomb": "Joker bomb",
  // Wildcard suffix. wild = "♥5"; ranks: array of rank labels.
  "combo.wildcardAs": ({ wild, ranks }) => ` (${wild} as ${ranks.join(", ")})`,
  "combo.wildcardAsNoLevel": ({ ranks }) => ` (wildcard as ${ranks.join(", ")})`,

  // ---------- format.js: dealStartLine, returnsLine ----------
  "deal.firstDeal": ({ card, name }) => `First deal: ${card} was turned up — ${name} holds it and leads.`,
  // Also the anti-tribute notice when play begins (net.js). DealStart only
  // carries the leader, so the payers are "the losing side".
  "deal.antiTribute": ({ name }) => `Anti-tribute: the losing side holds both Big Jokers — no tribute. ${name} leads.`,
  "deal.exchange": ({ payer, receiver, tribute, returned }) =>
    `${payer} paid ${receiver} ${tribute}, got back ${returned}`,
  "deal.tribute": ({ exchanges, name }) => `${exchanges.join("; ")}. ${name} leads.`, // exchanges: array of deal.exchange
  "deal.returnPart": ({ from, to, card }) => `${from} → ${to} ${card}`,
  "deal.returns": ({ parts }) => `Returns: ${parts.join(", ")}`, // parts: array of deal.returnPart
};
