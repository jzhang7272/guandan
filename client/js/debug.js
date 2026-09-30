// Debug panel (UI_SPEC §7): latest raw State JSON + last 20 messages in/out,
// and Prev / Next stepping in fixture mode. Opened by the ≡ button or ?debug=1.
import { el } from "./dom.js";
import { store, update } from "./store.js";
import { stepFixture } from "./net.js";

function time(ms) {
  const d = new Date(ms);
  const pad = (n, w = 2) => String(n).padStart(w, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(d.getMilliseconds(), 3)}`;
}

function renderFixtureBar() {
  const f = store.fixture;
  if (!f) return null;
  const n = f.messages.length;
  const current = n > 0 && f.index >= 0 ? f.messages[f.index].label : "—";
  return el("div", { class: "debug-fixture" },
    el("strong", {}, "Fixture mode"),
    " (sending disabled) ",
    el("button", { onClick: () => stepFixture(-1), disabled: f.index <= 0 }, "◀ Prev"),
    el("span", { class: "debug-fixture-pos" }, n > 0 ? ` ${f.index + 1}/${n}: ${current} ` : " none loaded "),
    el("button", { onClick: () => stepFixture(1), disabled: f.index >= n - 1 }, "Next ▶"),
    f.error ? el("div", { class: "debug-error" }, `Load error: ${f.error}`) : null,
  );
}

export function renderDebug() {
  const conn = `conn: ${store.conn}   name: ${store.name ?? "—"}   joined: ${
    store.joined ? `seat ${store.joined.seat}` : "—"}   selected: [${[...store.selected].join(", ")}]`;

  const logText = store.log.length === 0
    ? "(no messages yet)"
    : store.log.map((e) => `${time(e.time)} ${e.dir.padEnd(4)} ${JSON.stringify(e.msg)}`).join("\n");

  return el("div", { class: "debug-panel" },
    el("div", { class: "debug-head" },
      el("strong", {}, "Debug"),
      el("button", { onClick: () => { store.debugOpen = false; update(); } }, "Close"),
    ),
    renderFixtureBar(),
    el("div", { class: "debug-meta" }, conn),
    el("h4", {}, `Messages (last ${store.log.length})`),
    el("pre", { class: "debug-log" }, logText),
    el("h4", {}, "Latest State"),
    el("pre", { class: "debug-state" }, store.state ? JSON.stringify(store.state, null, 2) : "(none)"),
  );
}
