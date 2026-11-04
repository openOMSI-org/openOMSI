// Run: node --test launcher/tests/invite-ui-translations.test.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const source = readFileSync(new URL('../../crates/omsi-app/locales/app.yml', import.meta.url), 'utf8');
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
let current;
for (const line of source.split(/\r?\n/)) {
  if (/^\S/.test(line)) {
    current = undefined;
    if (line.startsWith('"') && line.endsWith(':')) {
      const key = JSON.parse(line.slice(0, -1));
      if (required.includes(key)) {
        assert(!catalogue.has(key), 'Duplicate key: ' + key);
        current = new Map();
        catalogue.set(key, current);
      }
    }
  } else if (current && line.trim()) {
    const match = /^  ([a-z-]+): (".*")$/.exec(line);
    assert(match, 'Invalid translation line: ' + line);
    const [, language, value] = match;
    assert(!current.has(language), 'Duplicate language: ' + language);
    current.set(language, JSON.parse(value));
  }
}
const placeholders = value => [...value.matchAll(/%\{\w+\}|(?<!%)\{[^{}]*\}/g)].map(m => m[0]).sort();
for (const language of languages) {
  test('invitation UI translations are complete: ' + language, () => {
    for (const key of required) {
      const value = catalogue.get(key)?.get(language);
      assert.equal(typeof value, 'string', language + ' missing: ' + key);
      assert(value.trim(), language + ' empty: ' + key);
      assert.deepEqual(placeholders(value), placeholders(key), language + ' placeholders: ' + key);
      assert(!value.includes('\uFFFD'), language + ' invalid encoding: ' + key);
    }
  });
}
