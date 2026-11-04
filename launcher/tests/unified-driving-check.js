import { html, render } from '../ui/vendor/preact-htm.js';
import { DriveMode } from '../ui/js/pages/drive-mode.js';
import { MultiplayerPage } from '../ui/js/pages/multiplayer.js';
import { S, set } from '../ui/js/store.js';
import { CHOICE_DEFAULTS } from '../ui/js/data.js';

export async function run(root) {
  const saved = { ...S };
  const originalFetch = window.fetch;
  const report = [];
  const assert = (ok, text) => { if (!ok) throw Error(text); };
  const sleep = () => new Promise((resolve) => setTimeout(resolve, 80));
  const button = (text) => [...root.querySelectorAll('button')].find((b) => b.getClientRects().length && b.textContent.replace(/→/g, '').trim() === text);
  const host = { name: 'Fixture host', map: 'maps/Host/global.cfg', players: 2, max_players: 8, vehicles: [], reached_at: 'http://test.invalid:20100' };
  let savedServers = [{ name: 'Saved fixture', address: 'saved.invalid:20100' }];
  let launched = 0;
  let slow = false;
  let finishQuery;
  window.fetch = async (url, ...args) => {
    const cmd = /\/mock\/([^/.]+)\.json/.exec(String(url))?.[1];
    if (cmd === 'check_join') return Response.json({ ok: true, text: 'Valid session code' });
    if (cmd === 'server_query') return slow ? new Promise((resolve) => { finishQuery = () => resolve(Response.json(host)); }) : Response.json(host);
    if (cmd === 'servers_get') return Response.json(savedServers);
    if (cmd === 'public_servers') return Response.json([{ id: 'public', name: 'Public fixture', address: 'public.invalid:20100' }]);
    if (cmd === 'servers_save') return Response.json(null);
    if (cmd === 'launch') { launched++; return Response.json({ others: 0 }); }
    if (['lines', 'saved_situations'].includes(cmd)) return Response.json([]);
    if (cmd === 'default_hof') return Response.json('');
    return originalFetch(url, ...args);
  };
  const type = async (selector, value) => {
    const input = root.querySelector(selector);
    assert(input, 'Missing input: ' + selector);
    input.value = value;
    input.dispatchEvent(new Event('input', { bubbles: true }));
    await sleep();
  };
  const mount = async (Component) => {
    render(null, root);
    render(html`<${Component} />`, root);
    await sleep();
  };
  try {
    const original = { ...CHOICE_DEFAULTS, map: 'maps/Local/global.cfg', bus: 'vehicles/Local.bus', paint: 'Custom', date: '1995-05-30', number: '', time: 615, lan_mode: 'off' };
    set({ page: 'drive', choice: original, maps: [{ file: original.map, year_span: [1989, 1989] }, { file: host.map, year_span: [1980, 1980] }], vehicles: [], poll: { instances: [] }, cache: {}, driveBeforeJoin: null });
    await mount(DriveMode);
    const before = JSON.stringify(S.choice);
    button('Join friends').click(); await sleep();
    assert(root.querySelector('[role="dialog"]'), 'Join friends must open a dialog without changing pages');
    assert(S.page === 'drive' && JSON.stringify(S.choice) === before, 'Opening a dialog must preserve the full duty');
    assert(!root.querySelector('[role="dialog"]').textContent.includes('Use a server address'), 'Friend dialog must contain codes only');
    assert(root.querySelector('[aria-modal="true"]'), 'Join dialog must declare modal semantics');
    report.push('PASS code-only Join dialog preserves the driving page and settings');
    await type('#multiplayer-code', ' OMSI-TEST-CODE ');
    button('Confirm session').click(); await sleep(); await sleep();
    assert(!root.querySelector('[role="dialog"]') && S.choice.lan_mode === 'join', 'Confirmed code must select the session and close the dialog');
    assert(S.choice.lan_addr === 'OMSI-TEST-CODE' && S.choice.lan_map === host.map, 'Selected room must use the trimmed code and host map');
    assert(S.choice.bus === original.bus && S.choice.paint === original.paint, 'Selecting a room must retain an unrestricted bus and paint');
    assert(launched === 0, 'Selecting a room must not launch or connect a game');
    button('Cancel').click(); await sleep();
    assert(S.choice.map === original.map && S.choice.time === original.time && S.choice.date === original.date && S.choice.number === original.number && S.choice.lan_mode === 'off', 'Cancelling must restore the local driving settings');
    report.push('PASS confirmation selects without launch; Cancel restores the local duty');
    set({ page: 'multiplayer', cache: {} });
    await mount(MultiplayerPage);
    assert(!button('Join friends') && !button('Invite friends'), 'Servers must not duplicate driving actions');
    assert(!root.querySelector('input[placeholder="play.example.org"]'), 'Server address form must not occupy the page');
    button('Use a server address').click(); await sleep();
    assert(root.querySelector('[role="dialog"]'), 'Use a server address must open a dialog');
    await type('input[placeholder="play.example.org"]', 'direct.invalid:20100');
    button('Select server').click(); await sleep(); await sleep();
    assert(S.page === 'drive' && S.choice.lan_mode === 'join', 'Direct address must return to Drive');
    assert(!root.querySelector('[role="dialog"]') && launched === 0, 'Direct selection must close without launching');
    assert(S.cache.servers.length === 1, 'Direct selection must not save implicitly');
    report.push('PASS address dialog selects without saving or launching');
    set({ page: 'multiplayer' }); await mount(MultiplayerPage);
    button('Use a server address').click(); await sleep();
    await type('input[placeholder="play.example.org"]', 'new.invalid:20100');
    button('Save to list').click(); await sleep();
    assert(S.cache.servers.length === 2 && !root.querySelector('[role="dialog"]'), 'Save to list must close the dialog and retain the new server');
    assert(S.page === 'multiplayer', 'Saving must not navigate to Drive');
    report.push('PASS saving remains separate from selecting a server');
    button('Public servers').click(); await sleep();
    button('Show the public list').click(); await sleep();
    const count = S.cache.servers.length;
    button('Select server').click(); await sleep(); await sleep();
    assert(S.page === 'drive' && S.cache.servers.length === count, 'Public server selection must not save implicitly');
    report.push('PASS public selection and saving remain independent');
    set({ page: 'multiplayer' }); await mount(MultiplayerPage);
    button('Use a server address').click(); await sleep();
    await type('input[placeholder="play.example.org"]', 'pending.invalid:20100');
    const selected = S.choice.lan_addr;
    slow = true;
    button('Select server').click(); await sleep();
    assert(finishQuery, 'Expected a pending address query');
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' })); await sleep();
    finishQuery(); await sleep();
    assert(S.page === 'multiplayer' && S.choice.lan_addr === selected, 'A closed address dialog must ignore its late query');
    report.push('PASS closing the address dialog invalidates pending results');
    return report.join('\n');
  } finally {
    render(null, root);
    window.fetch = originalFetch;
    set(saved);
  }
}
