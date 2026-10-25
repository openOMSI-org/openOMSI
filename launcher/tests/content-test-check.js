import { html, render } from '../ui/vendor/preact-htm.js';
import { ContentTest } from '../ui/js/pages/content-test.js';
import { set } from '../ui/js/store.js';

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const assert = (ok, message) => { if (!ok) throw new Error(message); };
const until = async (test) => {
  for (let i = 0; i < 80; i++) { if (test()) return; await sleep(25); }
  throw new Error('Timed out waiting for interface state');
};

// A mod folder holding one bus and one scenery object, each with a warning of its own.
const check = {
  ok: true,
  errors: 0,
  warnings: 2,
  seconds: 0.4,
  log_lines: 12,
  file: 'Test mod',
  summary: {
    kind: 'folder',
    name: 'Test mod',
    counts: { vehicle: 1, object: 1 },
    items: [
      { file: 'Vehicles/Test/test.bus', kind: 'vehicle', ok: true, summary: { name: 'Test bus' } },
      { file: 'Sceneryobjects/Test/test.sco', kind: 'object', ok: true, summary: { name: 'Test object' } },
    ],
  },
  findings: [
    { level: 'warn', part: 'test.bus', what: 'mesh', text: 'BUS WARNING' },
    { level: 'warn', part: 'test.sco', what: 'mesh', text: 'OBJECT WARNING' },
  ],
};

export async function run(root) {
  const originalFetch = window.fetch;
  const report = [];
  window.fetch = async (url, ...args) => {
    const cmd = /\/mock\/([^/.]+)\.json/.exec(String(url))?.[1];
    if (!cmd) return originalFetch(url, ...args);
    if (cmd === 'content_check') return Response.json(check);
    if (cmd === 'mods_status') return Response.json({ installed: [] });
    if (cmd === 'game_log') return Response.json([]);
    return Response.json(true);
  };
  const buttons = () => [...root.querySelectorAll('button')];
  const click = async (el, what) => {
    assert(el, `Missing: ${what}`);
    el.click();
    await sleep(40);
  };
  const filter = (kind) => buttons().find((b) => b.classList.contains('tag-button') && b.textContent.trim().startsWith(`${kind}:`));
  // (the pieces of "What it holds" only: a finding's line names its piece too)
  const card = (title) => [...root.querySelectorAll('section.card')].find((el) => el.querySelector('h2')?.textContent.trim() === title);
  const item = (file) => [...(card('What it holds')?.querySelectorAll('.list-item') || [])].find((el) => el.textContent.includes(file));
  const shows = (text) => root.textContent.includes(text);
  const mount = async () => {
    render(null, root);
    await sleep(30);
    set({ vehicles: [], maps: [], poll: { instances: [] }, choice: {} });
    render(html`<${ContentTest} initial=${{ kind: 'path' }} />`, root);
    await sleep(100);
    const input = root.querySelector('input.input');
    assert(input, 'Missing the path input');
    input.value = 'C:/Test mod';
    input.dispatchEvent(new Event('input', { bubbles: true }));
    await sleep(40);
    await click(buttons().find((b) => b.textContent.trim() === 'Check'), 'Check button');
    await until(() => !!filter('object'));
  };
  try {
    await mount();
    assert(shows('BUS WARNING') && shows('OBJECT WARNING'), 'Show every finding of the folder at first');
    report.push('PASS a checked folder lists the findings of all its pieces');

    // a bus item selected, then the object kind chosen: the bus is hidden, so are its findings
    await click(item('test.bus'), 'bus item');
    assert(shows('BUS WARNING') && !shows('OBJECT WARNING'), 'Selecting an item shows its findings only');
    await click(filter('object'), 'object kind filter');
    assert(!item('test.bus'), 'The object kind hides the bus item');
    assert(!shows('BUS WARNING'), 'Findings of an item hidden by the kind filter must not stay shown');
    assert(shows('OBJECT WARNING'), 'Choosing a kind shows the findings of that kind');
    assert(!root.textContent.includes('Findings: test.bus'), 'The hidden item must not stay selected');
    report.push('PASS changing the kind clears the selected item');

    // an object selected, then the kind filter cleared with "checked": all findings again
    await click(item('test.sco'), 'object item');
    assert(!shows('BUS WARNING') && shows('OBJECT WARNING'), 'Selecting the object shows its findings only');
    await click(filter('checked'), 'checked filter');
    assert(shows('BUS WARNING') && shows('OBJECT WARNING'), 'Clearing the kind shows every finding again');
    report.push('PASS clearing the kind clears the selected item');

    return `${report.join('\n')}\n${report.length} checks passed.`;
  } finally {
    render(null, root);
    window.fetch = originalFetch;
  }
}
