// Run: node --test launcher/tests/invite-ui-translations.test.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const required = [
  'Play mode', 'Solo driving', 'Drive on your own without creating a room.',
  'Host your selected map, bus and duty. Share the code after starting.',
  'Running hosted session', 'Host and drive',
  'As the host, your game receives your players\' driver names, network addresses, chat and voice. You are responsible for what you keep of it, such as logs and bans.',
  'Sessions and servers are run by players, not by openOMSI. The host receives your driver name, network address, chat and voice, and may keep logs.',
  'The server plays {}, which is not installed here: it is fetched from the server on joining.',
  'the session is full ({} players)', 'Use a server address',
  'Manage server addresses and lists. Select a server, then choose your bus on Drive.',
  'Saved servers', 'Select server', 'Save to list', 'Connection protocol',
  'Select a server to prepare your bus, or save its address for later. No game starts yet.',
  'Paste the complete session code from your friend.',
];
const languages = ['es', 'zh-tw', 'pt', 'ca', 'fr', 'sr', 'ru', 'de', 'pt-pt', 'ka', 'sv', 'uk', 'be', 'kk', 'pl', 'cs', 'hu', 'it', 'nl', 'tr', 'ja', 'zh', 'hi', 'ko', 'th', 'vi', 'id', 'ms', 'tl'];
const catalogue = new Map();
for (const language of languages) {
  const source = readFileSync(new URL(`../../crates/omsi-app/locales/app.${language}.yml`, import.meta.url), 'utf8');
  const table = new Map();
  for (const line of source.split(/\r?\n/)) {
    if (!line.trim() || line.startsWith('#')) continue;
    const match = /^("(?:[^"\\]|\\.)*"): (".*")$/.exec(line);
    assert(match, 'Invalid translation line in app.' + language + '.yml: ' + line);
    const key = JSON.parse(match[1]);
    assert(!table.has(key), 'Duplicate key in app.' + language + '.yml: ' + key);
    table.set(key, JSON.parse(match[2]));
  }
  catalogue.set(language, table);
}
const placeholders = value => [...value.matchAll(/%\{\w+\}|(?<!%)\{[^{}]*\}/g)].map(m => m[0]).sort();
for (const language of languages) {
  test('invitation UI translations are complete: ' + language, () => {
    for (const key of required) {
      const value = catalogue.get(language).get(key);
      assert.equal(typeof value, 'string', language + ' missing: ' + key);
      assert(value.trim(), language + ' empty: ' + key);
      assert.deepEqual(placeholders(value), placeholders(key), language + ' placeholders: ' + key);
      assert(!value.includes('\uFFFD'), language + ' invalid encoding: ' + key);
    }
  });
}
