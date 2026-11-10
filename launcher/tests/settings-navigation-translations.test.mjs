import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const source = readFileSync(new URL('../../crates/omsi-app/locales/app.yml', import.meta.url), 'utf8');
const layout = readFileSync(new URL('../ui/js/pages/settings-layout.js', import.meta.url), 'utf8');
const customLabels = [...layout.match(/const customLabels = \{([^}]+)\};/)[1].matchAll(/:\s*'([^']+)'/g)].map((match) => match[1]);
const required = [
  'Optimise for my PC',
  'NVIDIA DLSS runtime (optional)',
  'Corner',
  "Lighting",
  "Weather & effects",
  "Mouse",
  "Information on screen",
  "Support & reset",
  "Favourite settings",
  "Game preferences",
  "Search all settings",
  "Choose a topic to find the settings you need.",
  "{n} options",
  "Search results",
  "Remove from favourites: {name}",
  "Add to favourites: {name}",
  "Remove from favourite settings",
  "Add to favourite settings",
  "No favourite settings yet",
  "Try a setting name or its English key.",
  "Use the star beside a setting to keep it here.",
  "About this setting",
  "Save current graphics settings",
  "Profiles only store graphics settings; window size and fullscreen are kept.",
  "Setting help",
  "View help for {name}",
  "Select Help beside a setting to see what it does.",
  "No additional description is available for this setting."
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
  test('settings navigation translations are complete: ' + language, () => {
    for (const key of required) {
      const value = catalogue.get(key)?.get(language);
      assert.equal(typeof value, 'string', language + ' missing: ' + key);
      assert(value.trim(), language + ' empty: ' + key);
      assert.deepEqual(placeholders(value), placeholders(key), language + ' placeholders: ' + key);
      assert(!value.includes('\uFFFD'), language + ' invalid encoding: ' + key);
    }
  });
}

test('settings navigation entries cover exactly the supported locales', () => {
  assert.equal(catalogue.size, required.length);
  for (const key of required) {
    assert.deepEqual([...catalogue.get(key).keys()].sort(), [...languages].sort(), key);
  }
});

test('every custom setting label has a catalogue key', () => {
  for (const label of customLabels) assert(source.includes(JSON.stringify(label) + ':'), 'Missing custom label: ' + label);
});
