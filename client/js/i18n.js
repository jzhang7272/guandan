// The current language, choosing it, saving it, and looking up strings
// (LANGUAGE_SPEC §3, §4). No DOM and no store: the language lives in this
// module, so format.js can call t() and stay pure.
//
//   initLang(env?)        → "en" | "zh": choose the language for this page
//                           load (§3) and make it current. Call once, early
//                           (main.js). Until it's called the language is "en".
//   getLang()             → "en" | "zh"
//   setLang(lang, opts?)  → make `lang` current and save it in
//                           localStorage["guandan.lang"] (the toggle).
//                           setLang(lang, { save: false }) changes the
//                           language without touching storage (tests,
//                           fixture mode). Unknown `lang` → ignored, returns false.
//   langTag()             → the <html lang> value: "en" | "zh-Hans"
//   t(key, params?)       → the current language's string for `key`. A
//                           function value is called with `params` (named
//                           parameters, e.g. t("play.selected", { n: 3 })).
//                           Missing in the current language → the English;
//                           missing there too (or the function throws) →
//                           "?key?", so a gap shows on screen instead of
//                           crashing render().
//   list(items)           → names joined the language's way:
//                           EN "a, b and c", ZH "a、b和c".
//
// initLang's inputs can be injected (tests.html does):
//   initLang({ search: "?lang=zh", storage: { getItem() { … } }, languages: ["en-US"] })
// Each one left out is read from the page (location.search, localStorage,
// navigator.languages). Storage access never throws (private windows).
import { EN } from "./strings_en.js";
import { ZH } from "./strings_zh.js";

export const LANGS = ["en", "zh"];
const TABLES = { en: EN, zh: ZH };
const STORAGE_KEY = "guandan.lang";

let current = "en";

const isLang = (lang) => LANGS.includes(lang);

function readStored(storage) {
  try {
    const s = storage === undefined ? window.localStorage : storage;
    return s ? s.getItem(STORAGE_KEY) : null;
  } catch {
    return null;
  }
}

function browserLanguages() {
  try {
    if (typeof navigator === "undefined") return [];
    if (Array.isArray(navigator.languages) && navigator.languages.length) return navigator.languages;
    return navigator.language ? [navigator.language] : [];
  } catch {
    return [];
  }
}

// §3: ?lang= (this load only) → the saved choice → the browser's first
// language (zh… → Chinese) → English.
export function initLang(env = {}) {
  const search = env.search ?? (typeof location !== "undefined" ? location.search : "");
  const fromUrl = new URLSearchParams(search).get("lang");
  const stored = readStored(env.storage);
  const languages = env.languages ?? browserLanguages();
  const first = String(languages[0] ?? "").toLowerCase();

  if (isLang(fromUrl)) current = fromUrl;
  else if (isLang(stored)) current = stored;
  else current = first.startsWith("zh") ? "zh" : "en";
  return current;
}

export function getLang() {
  return current;
}

export function setLang(lang, { save = true } = {}) {
  if (!isLang(lang)) return false;
  current = lang;
  if (save) {
    try { window.localStorage.setItem(STORAGE_KEY, lang); } catch { /* ignore */ }
  }
  return true;
}

export function langTag() {
  return current === "zh" ? "zh-Hans" : "en";
}

export function t(key, params = {}) {
  const value = TABLES[current]?.[key] ?? EN[key];
  if (value === undefined) return `?${key}?`;
  if (typeof value !== "function") return value;
  try {
    return String(value(params ?? {}));
  } catch {
    return `?${key}?`;
  }
}

export function list(items) {
  const names = [...(items ?? [])].map(String);
  if (names.length <= 1) return names.join("");
  return names.slice(0, -1).join(t("common.listSep")) + t("common.listLast") + names[names.length - 1];
}
