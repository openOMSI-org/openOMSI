// Run: node --test launcher/tests/changelog.test.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { blocks, inline, compareVersions, freshVersions } from '../ui/js/markdown.js';

test('release notes as the release script writes them', () => {
  const notes = '**openOMSI 0.3.42**\n\n> **Beta, for testing.** Report what breaks\n> in an issue.\n\n### New\n\n- **Remote cockpit**: the game serves a page\n  on your home network.\n  - a nested point\n- `OMSI_NO_SPLINE_HOLES` and [the page](https://github.com/openOMSI-org/openOMSI)\n\n---\n<!-- hidden -->\nLast line\r\ngoes on.';
  assert.deepEqual(blocks(notes), [
    { kind: 'paragraph', text: '**openOMSI 0.3.42**' },
    { kind: 'quote', text: '**Beta, for testing.** Report what breaks in an issue.' },
    { kind: 'heading', level: 3, text: 'New' },
    { kind: 'list', items: [
      { depth: 0, text: '**Remote cockpit**: the game serves a page on your home network.' },
      { depth: 1, text: 'a nested point' },
      { depth: 0, text: '`OMSI_NO_SPLINE_HOLES` and [the page](https://github.com/openOMSI-org/openOMSI)' },
    ] },
    { kind: 'paragraph', text: 'Last line goes on.' },
  ]);
  assert.deepEqual(blocks(''), []);
  assert.deepEqual(blocks(null), []);
});

test('discarding a comment keeps the surrounding text separated', () => {
  assert.deepEqual(blocks('New<!-- internal note -->features'), [
    { kind: 'paragraph', text: 'New features' },
  ]);
  assert.deepEqual(blocks('<!<!-- internal note -->-- left as text -->'), [
    { kind: 'paragraph', text: '<! -- left as text -->' },
  ]);
});

test('inline marks, and links only to https', () => {
  assert.deepEqual(inline('a **b** *c* `d` [e](https://x.org/y) f'), [
    { kind: 'text', text: 'a ' }, { kind: 'bold', text: 'b' }, { kind: 'text', text: ' ' },
    { kind: 'italic', text: 'c' }, { kind: 'text', text: ' ' }, { kind: 'code', text: 'd' },
    { kind: 'text', text: ' ' }, { kind: 'link', text: 'e', url: 'https://x.org/y' }, { kind: 'text', text: ' f' },
  ]);
  for (const bad of ['javascript:alert(1)', 'http://x.org', 'file:///etc/passwd', 'data:text/html,x']) {
    const parts = inline(`[go](${bad})`);
    assert(!parts.some((p) => p.kind === 'link'), bad);
    assert.equal(parts[0].text, 'go', bad);
  }
  assert.deepEqual(inline('<img src=x onerror=alert(1)>'), [{ kind: 'text', text: '<img src=x onerror=alert(1)>' }]);
  assert.deepEqual(inline('2 * 3 * 4 and snake_case_name'), [{ kind: 'text', text: '2 * 3 * 4 and snake_case_name' }]);
});

test('versions compare as numbers', () => {
  assert(compareVersions('0.3.42', '0.3.9') > 0);
  assert(compareVersions('0.3.9', '0.3.42') < 0);
  assert.equal(compareVersions('0.3.0', '0.3'), 0);
  assert(compareVersions('1.0.0', '0.99.99') > 0);
});

test('the notes opened are those since the last seen, up to this version', () => {
  const list = ['0.3.44', '0.3.43', '0.3.42', '0.3.41', '0.3.40'].map((version) => ({ version }));
  assert.deepEqual(freshVersions(list, '0.3.43', '0.3.40'), ['0.3.43', '0.3.42', '0.3.41']);
  assert.deepEqual(freshVersions(list, '0.3.43', ''), ['0.3.43']);
  assert.deepEqual(freshVersions(list, '0.3.43', '0.3.43'), ['0.3.43']);
  assert.deepEqual(freshVersions(list, '0.3.41', '0.3.43'), ['0.3.41']);
  assert.deepEqual(freshVersions(list, '0.3.50-pr1', ''), ['0.3.44']);
  assert.deepEqual(freshVersions([], '0.3.43', ''), []);
});

test('the changelog adds no interface text the translations lack', () => {
  const yml = readFileSync(new URL('../../crates/omsi-app/locales/app.yml', import.meta.url), 'utf8');
  for (const key of ["What's new", 'Open on GitHub', 'New', 'Beta', 'Installed', 'Latest', 'Close', 'Try again',
    'Asking GitHub for the releases…', 'The list of versions could not be read.', 'No release to choose from']) {
    assert(yml.includes('\n' + JSON.stringify(key) + ':\n'), key);
  }
});
