import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const source = readFileSync(new URL('../../crates/omsi-app/locales/app.yml', import.meta.url), 'utf8');
const englishSource = source.replace(/\r\n/g, '\n');
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
for (const language of languages) {
  const file = readFileSync(new URL(`../../crates/omsi-app/locales/app.${language}.yml`, import.meta.url), 'utf8');
  const translations = new Map();
  for (const line of file.split(/\r?\n/)) {
    if (!line.trim() || line.startsWith('#')) continue;
    const match = /^("(?:[^"\\]|\\.)*"): (".*")$/.exec(line);
    assert(match, `Invalid translation line in app.${language}.yml: ${line}`);
    const key = JSON.parse(match[1]);
    assert(!translations.has(key), `Duplicate key in app.${language}.yml: ${key}`);
    translations.set(key, JSON.parse(match[2]));
  }
  catalogue.set(language, translations);
}
const placeholders = value => [...value.matchAll(/%\{\w+\}|(?<!%)\{[^{}]*\}/g)].map(m => m[0]).sort();
for (const language of languages) {
  test('settings navigation translations are complete: ' + language, () => {
    for (const key of required) {
      const value = catalogue.get(language).get(key);
      assert.equal(typeof value, 'string', language + ' missing: ' + key);
      assert(value.trim(), language + ' empty: ' + key);
      assert.deepEqual(placeholders(value), placeholders(key), language + ' placeholders: ' + key);
      assert(!value.includes('\uFFFD'), language + ' invalid encoding: ' + key);
    }
  });
}

test('every settings navigation label has an English reference', () => {
  for (const key of required) assert(englishSource.includes(`${JSON.stringify(key)}:\n  en: ${JSON.stringify(key)}`), 'Missing English key: ' + key);
});

test('every custom setting label has a catalogue key', () => {
  for (const label of customLabels) assert(englishSource.includes(`${JSON.stringify(label)}:\n  en: ${JSON.stringify(label)}`), 'Missing custom label: ' + label);
});
