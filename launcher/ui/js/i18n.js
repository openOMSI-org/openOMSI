// The interface's language: the game's translation tables (English is the key), as
// `omsi_ui::tr` uses them. `t('Start at {time}', { time })` fills `{name}` holes after the
// lookup, so a sentence is translated whole.

import { call } from './api.js';

let table = {};
let lang = '';

export async function setLanguage(iso) {
  lang = iso && iso !== 'en' ? iso : '';
  table = lang ? await call('translations', { lang }).catch(() => ({})) : {};
  document.documentElement.lang = lang || 'en';
}

export const language = () => lang || 'en';

/** Texts asked for that the tables lack in this language (`missingTexts()` in the console
 * lists them for the translation tables). */
const missing = new Set();
window.missingTexts = () => [...missing].sort();

export function t(text, vars) {
  if (lang && text && !(text in table)) missing.add(text);
  let s = table[text] ?? text;
  if (Array.isArray(vars)) {
    // the game's own keys hold Rust's positional `{}`
    let i = 0;
    s = s.replace(/\{\}/g, (m) => (i < vars.length ? vars[i++] : m));
  } else if (vars) s = s.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? vars[k] : m));
  return s;
}

/** A count with its noun: `plural(3, 'bus', 'buses')` (the tables hold both forms). */
export const plural = (n, one, many) => `${n} ${t(n === 1 ? one : many)}`;

/** HH:MM of minutes (or of hours as a float, as the timetables give them). */
export function hhmm(minutes) {
  const m = Math.round(minutes) % 1440;
  return `${String(Math.floor(m / 60)).padStart(2, '0')}:${String(m % 60).padStart(2, '0')}`;
}

/** A displayed clock; hhmm remains the machine-readable/input format. */
export function clockTime(minutes, format = '24h') {
  if (format !== '12h') return hhmm(minutes);
  const m = ((Math.round(minutes) % 1440) + 1440) % 1440;
  const h = Math.floor(m / 60);
  return `${String(h % 12 || 12).padStart(2, '0')}:${String(m % 60).padStart(2, '0')} ${h < 12 ? 'AM' : 'PM'}`;
}

/** A date as the player's language writes it. */
export function longDate(iso) {
  const d = new Date(iso + 'T12:00:00');
  if (isNaN(d)) return iso;
  return d.toLocaleDateString(language(), { weekday: 'short', day: 'numeric', month: 'long', year: 'numeric' });
}

export function bytes(n) {
  if (!n) return '0 B';
  const u = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.min(u.length - 1, Math.floor(Math.log(n) / Math.log(1024)));
  return `${(n / 1024 ** i).toFixed(i > 1 ? 1 : 0)} ${u[i]}`;
}

export function ago(secs) {
  const d = Date.now() / 1000 - secs;
  if (d < 90) return t('just now');
  if (d < 3600) return t('{n} min ago', { n: Math.round(d / 60) });
  if (d < 86400 * 2) return t('{n} h ago', { n: Math.round(d / 3600) });
  return new Date(secs * 1000).toLocaleDateString(language(), { day: 'numeric', month: 'short', year: 'numeric' });
}

// Every kind of dash and the raised dot as a plain hyphen, in all text shown (names from the
// content too): the page's text as it appears, the route map's labels as they are drawn.
const DASHES = /[\u2010-\u2015\u2212\u2E3A\u2E3B\uFE58\uFF0D\u00B7]/g;
export const plain = (text) => (typeof text === 'string' ? text.replace(DASHES, '-') : text);

export function plainText(root) {
  const fix = (n) => {
    if (n.nodeType === 3) {
      if (DASHES.test(n.data)) n.data = plain(n.data);
      DASHES.lastIndex = 0;
    } else if (n.nodeType === 1) {
      const w = document.createTreeWalker(n, NodeFilter.SHOW_TEXT);
      for (let x = w.nextNode(); x; x = w.nextNode()) fix(x);
    }
  };
  fix(root);
  new MutationObserver((list) => {
    for (const m of list) {
      if (m.type === 'characterData') fix(m.target);
      else m.addedNodes.forEach(fix);
    }
  }).observe(root, { childList: true, subtree: true, characterData: true });
}
