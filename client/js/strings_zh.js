// Simplified Chinese UI strings, from docs/TRANSLATION.md (LANGUAGE_SPEC
// §4.2). Exactly the same keys as strings_en.js, which documents each key's
// parameters. Groups are by the file that shows them.
export const ZH = {
  // ---------- shared (several files) ----------
  "app.name": "掼蛋",
  "common.cancel": "取消",
  "common.listSep": "、", // i18n.js list(): "a、b和c"
  "common.listLast": "和",
  "common.aTries": ({ n }) => `打A：${n}/3`,

  // ---------- language toggle (main.js); the same in both languages ----------
  "lang.groupAria": "Language / 语言",
  "lang.en": "EN",
  "lang.zh": "中文",

  // ---------- main.js: tab title, loading, connection banner ----------
  "main.titleYourTurn": "▶ 轮到你了 — 掼蛋",
  "main.loading": "加载中……",
  "main.lookingFor": ({ code }) => `正在查找牌局 ${code}……`,
  "main.joiningAs": ({ name }) => `正在以 ${name} 的身份加入……`,
  "main.connecting": "连接中……",
  "main.reconnecting": "连接已断开，正在重新连接……",
  "main.debugTitle": "调试面板",
  "main.debugAria": "打开或关闭调试面板",

  // ---------- home.js: Home ----------
  "home.yourName": "你的名字",
  "home.createGame": "创建牌局",
  "home.creating": "创建中……",
  "home.or": "或",
  "home.inviteCode": "邀请码",
  "home.codePlaceholder": "482193",
  "home.joinGame": "加入牌局",
  "home.errNameFirst": "请先输入你的名字。",
  "home.errCodeMissing": "请输入你收到的邀请码。",
  "home.errCodeInvalid": ({ typed, length }) =>
    `“${typed}”不是有效的邀请码：邀请码由 ${length} 位数字组成，例如 482193。`,
  "home.errServerFull": "服务器已满（牌局太多），请几分钟后再试。",
  "home.errCreateFailed": "无法创建牌局，服务器是否在运行？请重试。",

  // ---------- home.js: no such game / game ended ----------
  "gone.noGame": ({ code }) => `找不到邀请码为 ${code} 的牌局`,
  "gone.noGameHint": "请检查你收到的邀请码或链接，或创建一个新牌局。",
  "gone.ended": "该牌局已结束",
  "gone.endedHint": "所有人离开后牌局已关闭。请回到首页创建新牌局。",
  "gone.backHome": "返回首页",

  // ---------- lobby.js: name form ----------
  "nameForm.yourName": "你的名字",
  "nameForm.join": "加入",

  // ---------- lobby.js: lobby ----------
  "lobby.title": "大厅",
  "lobby.invite": "邀请码：",
  "lobby.copyLink": "复制链接",
  "lobby.copied": "已复制 ✓",
  "lobby.inviteManualAria": "邀请链接，请手动复制",
  "lobby.yourTeam": "我方",
  "lobby.you": "你",
  "lobby.connected": "在线",
  "lobby.offline": "离线",
  "lobby.ready": "已准备",
  "lobby.sitHere": "坐这里",
  "lobby.empty": "空位",
  "lobby.imReady": "准备",
  "lobby.readyOn": "已准备 ✓",
  "lobby.readyTitle": "准备好开始时点击",
  "lobby.cancelReadyTitle": "点击取消准备",
  "lobby.waitingForPre": "等待 ",
  "lobby.waitingForPost": " 准备",
  "lobby.youInList": "你",
  "lobby.waitingMore": ({ n }) => `还差 ${n} 位玩家`,
  "lobby.notReady": ({ names }) => ` · 未准备：${names.join("、")}`,
  "lobby.starting": "即将开始……",
  "lobby.seatsLocked": "比赛进行中座位已锁定。如需换座，请开始新比赛。",
  "lobby.settings": "设置",
  "lobby.settingsAria": "牌桌设置",
  "lobby.settingsNote": "更改设置会取消所有人的准备。",
  "lobby.levelSuffix": "级数",
  "lobby.declaring": "主打方",
  "lobby.declaringAria": "主打方",
  "lobby.declaringNone": "无 — 首局打2",
  "lobby.newMatch": "新比赛……",
  "lobby.newMatchAria": "确认开始新比赛",
  "lobby.startOverQuestion": "重新开始？级数将回到2，座位解锁。",
  "lobby.startOver": "重新开始",

  // ---------- lobby.js: kicked overlay ----------
  "kicked.title": "座位已被接管",
  "kicked.body": "该座位已在另一个标签页或设备上登录。",
  "kicked.hint": "夺回后将在此处重新连接，并断开另一处的连接。",
  "kicked.takeBack": "夺回座位",

  // ---------- results.js: Last deal card / match-won banner ----------
  "results.lastDeal": "上一局",
  "results.matchWonAria": "比赛获胜",
  "results.matchWon": ({ team }) => `🏆 ${team}赢得比赛！`,
  "results.levelsBack": "级数已回到2。",
  "results.played": ({ name }) => `${name} 出牌：`,
  "results.wins": ({ team }) => `${team}获胜！`,
  "results.finish": ({ finish }) => (finish === "1-2" ? "1-2 双下" : finish),
  "results.place": ({ name, place }) => `${name} ${place}`,
  "results.places": ({ places }) => places.join("，"),
  "results.yourTeam": "（我方）",

  // ---------- table.js: header ribbon, pennant, Redeal ----------
  "table.yourTeam": "（我方）",
  "table.sideAria": ({ team, mine, level, declaring, tries }) =>
    `${team}${mine ? "（我方）" : ""}：${level}级`
    + `${declaring ? "，主打" : ""}${tries !== null && tries !== undefined ? `，打A：${tries}/3` : ""}`,
  "table.pennantLabel": "打",
  "table.pennantTitle": ({ level }) => `本局打${level}`,
  "table.pennantAria": ({ level }) => `本局级牌 ${level}`,
  "table.lastTrick": "上一轮",
  "table.redeal": "重新发牌",
  "table.redealConfirm": "放弃本局并返回大厅？级数保持本局开始前的状态。",

  // ---------- table.js: seat plates ----------
  "table.partner": "（对家）",
  "table.you": ({ name }) => `你（${name}）`,
  "table.cardsLeft": "剩余张数",
  "table.offline": "离线",
  "table.disconnected": "已断线",
  "table.turn": "出牌中",
  "table.playsNext": "下一个出牌",

  // ---------- table.js: felt ----------
  "table.passCard": "过",
  "table.pass": "过",
  "table.leads": ({ name }) => `${name} 出牌`,
  "table.jiefeng": ({ a, b }) => `接风 — ${a} 已出完，对家 ${b} 出牌`,
  "table.tributeBeforeDeal": "开局前进贡",

  // ---------- play.js: action bar, reading picker ----------
  "play.out": "你已出完，等待本局结束。",
  "play.yourLead": "你先出",
  "play.yourTurn": "轮到你出牌",
  "play.waitingFor": ({ name }) => `等待 ${name} 出牌……`,
  "play.selected": ({ n }) => `已选 ${n} 张`,
  "play.clear": "重选",
  "play.takeBack": "悔牌",
  "play.takeBackTitle": "撤回你的上一步（出牌或过）",
  "play.pass": "过",
  "play.play": "出牌",
  "reading.title": "你想出哪种牌型？",

  // ---------- hand.js: toolbar, group hover ----------
  "hand.group": "理牌",
  "hand.ungroup": "拆开",
  "hand.clearGroups": "全部拆开",
  "hand.checking": "理牌（检查中……）",
  "hand.notLegal": "理牌（不是合法牌型）",
  "hand.readingSep": " / ",

  // ---------- tribute.js ----------
  "tribute.aria": "进贡",
  "tribute.titleSingle": "进贡（单贡）",
  "tribute.titleDouble": "进贡（双贡）",
  "tribute.paidPrefix": "已进贡：",
  "tribute.paid": "已进贡",
  "tribute.waitingTribute": "进贡：等待中……",
  "tribute.returnHidden": "还贡：✓（未公开）",
  "tribute.returnWaiting": "还贡：等待中……",
  "tribute.taskPay": ({ name }) => (name ? `请选择一张牌进贡给 ${name}。` : "请选择一张牌进贡。"),
  "tribute.taskReturn": ({ name }) => (name ? `请选择一张牌还贡给 ${name}。` : "请选择一张牌还贡。"),
  "tribute.waitingPay": ({ names }) => `等待 ${names} 进贡……`,
  "tribute.waitingReturn": ({ names }) => `等待 ${names} 还贡……`,
  "tribute.waiting": "等待中……",
  "tribute.payButton": "进贡",
  "tribute.returnButton": "还贡",

  // ---------- cards.js ----------
  "cards.wildcard": "逢人配",
  "cards.wildcardAria": ({ card }) => `${card}（逢人配）`,

  // ---------- net.js: toast, notices ----------
  "net.notConnected": "未连接，请稍后再试。",
  "net.tookBack": ({ name }) => `${name} 悔牌了`,

  // ---------- net.js: Rejected, by code ----------
  "error.WrongPhase": "现在不能进行此操作",
  "error.NotYourTurn": "还没轮到你",
  "error.CardsNotInHand": "这些牌不在你的手牌中",
  "error.NotAValidCombo": "这些牌不是合法牌型",
  "error.DoesNotBeatCurrent": "你的牌管不上",
  "error.InvalidDeclaration": "这些牌不能按所选牌型打出",
  "error.CannotPassWhenLeading": "首家出牌时不能过",
  "error.withCards": ({ message, cards }) => `${message}：${cards}`,
  "error.NothingToTakeBack": "没有可以悔的牌",
  "error.NotATributePayer": "你不需要进贡",
  "error.AlreadyPaid": "你已经进贡过了",
  "error.InvalidTributeCard": "这张牌不能用来进贡",
  "error.NotATributeReceiver": "你不是受贡方",
  "error.TributeNotComplete": "请等待所有人进贡完毕",
  "error.AlreadyReturned": "你已经还贡过了",
  "error.InvalidReturnCard": "这张牌不能用来还贡",
  "error.NotJoined": "请先加入房间",
  "error.AlreadyJoined": "你已经加入了",
  "error.InvalidName": "名字须为 1 到 20 个字符",
  "error.NameTaken": "该名字已被在线玩家使用",
  "error.NoSeatsAvailable": "座位已满",
  "error.NotInDeal": "当前没有进行中的牌局",
  "error.NotInLobby": "只能在大厅中进行此操作",
  "error.SeatTaken": "该座位已有人",
  "error.SeatsLocked": "比赛进行中座位已锁定；如需换座，请开始新比赛",
  "error.InvalidSettings": "打过一局后，必须有一方主打",

  // ---------- format.js: teams, seats, places ----------
  "team.A": "南北方",
  "team.B": "东西方",
  "team.named": ({ team }) => `${team}`, // 方 already means "side": no 队
  "team.us": "我方",
  "team.them": "对方",
  "seat.dir0": "南家",
  "seat.dir1": "东家",
  "seat.dir2": "北家",
  "seat.dir3": "西家",
  "seat.fallback": ({ n }) => `${n}号座位`, // not in TRANSLATION.md
  "place.0": "头游",
  "place.1": "二游",
  "place.2": "三游",
  "place.3": "末游",

  // ---------- format.js: cards ----------
  "card.smallJoker": "小王",
  "card.bigJoker": "大王",
  "card.smallJokerCorner": "小",
  "card.bigJokerCorner": "大",

  // ---------- format.js: combo labels ----------
  "combo.plural": ({ rank }) => `${rank}`, // no plurals: 对9
  "combo.smallJoker": "小王",
  "combo.bigJoker": "大王",
  "combo.smallJokers": "小王",
  "combo.bigJokers": "大王",
  "combo.pair": ({ face }) => `对${face}`,
  "combo.triple": ({ rank }) => `三个${rank}`,
  "combo.fullHouse": ({ rank }) => `三带二（${rank}）`,
  "combo.straight": ({ span }) => `顺子 ${span}`,
  "combo.tube": ({ run }) => `连对 ${run}`,
  "combo.plate": ({ run }) => `钢板 ${run}`,
  "combo.bomb": ({ size, rank }) => `${size}炸（${rank}）`,
  "combo.size4": "四",
  "combo.size5": "五",
  "combo.size6": "六",
  "combo.size7": "七",
  "combo.size8": "八",
  "combo.size9": "九",
  "combo.size10": "十",
  "combo.straightFlush": ({ span }) => `同花顺 ${span}`,
  "combo.jokerBomb": "天王炸",
  "combo.wildcardAs": ({ wild, ranks }) => `（${wild} 当 ${ranks.join("、")}）`,
  "combo.wildcardAsNoLevel": ({ ranks }) => `（逢人配当 ${ranks.join("、")}）`,

  // ---------- format.js: dealStartLine, returnsLine ----------
  // deal.firstDeal / antiTribute / exchange / tribute aren't in
  // TRANSLATION.md. On screen: only deal.antiTribute (net.js notice).
  "deal.firstDeal": ({ card, name }) => `首局：翻出 ${card}，${name} 持有此牌并出牌。`,
  "deal.antiTribute": ({ name }) => `抗贡：输方持有两张大王，不进贡。${name} 出牌。`,
  "deal.exchange": ({ payer, receiver, tribute, returned }) =>
    `${payer} 向 ${receiver} 进贡 ${tribute}，还贡 ${returned}`,
  "deal.tribute": ({ exchanges, name }) => `${exchanges.join("；")}。${name} 出牌。`,
  "deal.returnPart": ({ from, to, card }) => `${from} → ${to} ${card}`,
  "deal.returns": ({ parts }) => `还贡：${parts.join("，")}`,
};
