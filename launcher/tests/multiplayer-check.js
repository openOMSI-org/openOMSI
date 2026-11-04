import { html, render } from '../ui/vendor/preact-htm.js';
import { DriveMode } from '../ui/js/pages/drive-mode.js';
import { MultiplayerPage } from '../ui/js/pages/multiplayer.js';
import { CHOICE_DEFAULTS } from '../ui/js/data.js';
import { S, set } from '../ui/js/store.js';

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const assert = (ok, message) => { if (!ok) throw new Error(message); };
const until = async (test) => {
  for (let i = 0; i < 80; i++) { if (test()) return; await sleep(25); }
  throw new Error('Timed out waiting for interface state');
};

export async function run(root) {
  const originalFetch = window.fetch;
  const report = [];
  let scenario = 'ok';
  let pending;
  let queries = 0;
  const server = { name: 'Test host', map: 'maps/Test/global.cfg', players: 2, max_players: 8, vehicles: [] };
  window.fetch = async (url, ...args) => {
    const cmd = /\/mock\/([^/.]+)\.json/.exec(String(url))?.[1];
    if (!cmd) return originalFetch(url, ...args);
    if (cmd === 'check_join') {
      if (scenario === 'invalid') return Response.json({ ok: false, text: 'Invalid test code' });
      return Response.json({ ok: true, text: 'Valid session code' });
    }
    if (cmd === 'server_query') {
      queries++;
      if (scenario === 'offline') throw new Error('Host did not answer');
      if (scenario === 'slow') return new Promise((resolve) => { pending = () => resolve(Response.json(server)); });
      return Response.json({ ...server, players: scenario === 'full' ? 8 : 2 });
    }
    if (cmd === 'lines' || cmd === 'saved_situations') return Response.json([]);
    if (cmd === 'default_hof') return Response.json('');
    if (cmd === 'servers_get') return Response.json([{ name: 'Saved test server', address: 'test.invalid' }]);
    if (cmd === 'prepare_tunnel' && scenario === 'tunnel-error') throw new Error('Tunnel unavailable');
    return Response.json(true);
  };
  const button = (label) => [...root.querySelectorAll('button')].find((b) => b.textContent.trim() === label);
  const click = async (label) => {
    const el = button(label);
    assert(el, `Missing button: ${label}`);
    assert(!el.disabled, `Disabled button: ${label}`);
    el.click();
    await sleep(40);
  };
  const type = async (text) => {
    const input = root.querySelector('input[placeholder="OMSI-XXXX-XXXX-..."]');
    assert(input?.tagName === 'INPUT', 'The session code must be a single-line input');
    input.value = text;
    input.dispatchEvent(new Event('input', { bubbles: true }));
    await sleep(40);
  };
  const mount = async () => {
    render(null, root);
    await sleep(30);
    set({ page: 'drive', driveBeforeJoin: null, choice: { ...CHOICE_DEFAULTS }, maps: [{ file: server.map }], vehicles: [], poll: { instances: [] }, cache: {} });
    render(html`<${DriveMode} />`, root);
    await sleep(100);
    await click('Join friends');
  };
  try {
    await mount();
    assert(root.querySelector('[role="dialog"]'), 'Join friends must use a modal dialog');
    assert(button('Confirm session')?.disabled, 'An empty code must not be submitted');
    assert(root.querySelector('details.multiplayer-help-panel:not([open])'), 'Keep first-time help collapsed');
    report.push('PASS separate actions, compact input and collapsed help');

    scenario = 'invalid';
    await type('OMSI-BAD-CODE');
    await click('Confirm session');
    await until(() => root.textContent.includes('Invalid test code'));
    assert(!button('Choose a bus'), 'An invalid code must not advance');
    assert(S.choice.lan_mode === 'off', 'Checking must not change the saved duty');
    report.push('PASS invalid code cannot advance or change the duty');

    scenario = 'ok';
    await type(' OMSI-TEST-CODE ');
    await click('Confirm session');
    await until(() => S.choice.lan_mode === 'join');
    assert(!root.querySelector('[role="dialog"]'), 'Confirmation must close the dialog');
    assert(S.page === 'drive' && S.choice.lan_mode === 'join', 'Continue to real Drive settings');
    assert(S.choice.lan_addr === 'OMSI-TEST-CODE', 'Trim the code');
    assert(S.choice.map === server.map, 'Use the host map when installed');
    report.push('PASS confirmation transfers the actual host and map to Drive');

    await mount();
    scenario = 'full';
    await type('OMSI-FULL');
    await click('Confirm session');
    await until(() => root.textContent.includes('The session is full'));
    assert(!button('Choose a bus'), 'A full session must not advance');
    report.push('PASS full sessions cannot advance');

    scenario = 'offline';
    await type('OMSI-OFFLINE');
    await click('Confirm session');
    await until(() => !!button('Choose a bus without session details'));
    const technical = root.querySelector('details.multiplayer-error-details');
    assert(technical && !technical.open && technical.textContent.includes('Host did not answer'), 'Keep technical details collapsed');
    assert(S.choice.lan_mode === 'off', 'A failed query must not join automatically');
    await click('Choose a bus without session details');
    assert(S.choice.lan_mode === 'join' && S.choice.lan_addr === 'OMSI-OFFLINE', 'Preserve direct joining when status is unavailable');
    assert(S.choice.lan_map === '' && !S.choice.lan_server.asked, 'Do not reuse a previous session description');
    report.push('PASS unavailable status offers an explicit fallback');
    await mount();

    scenario = 'slow';
    pending = null;
    await type('OMSI-DIRECT');
    await click('Confirm session');
    await until(() => !!pending);
    assert(button('Choose a bus without session details'), 'Allow direct continuation while status is still pending');
    await click('Choose a bus without session details');
    assert(S.page === 'drive' && S.choice.lan_addr === 'OMSI-DIRECT', 'Keep the validated code when skipping status');
    pending();
    await sleep(100);
    assert(!S.choice.lan_server.asked, 'A skipped query must not change the selected duty later');
    report.push('PASS a slow status query does not block direct joining');
    await mount();

    scenario = 'slow';
    await type('OMSI-OLD');
    await click('Confirm session');
    await until(() => !!pending);
    await type('OMSI-NEW');
    pending();
    await sleep(100);
    assert(!button('Choose a bus'), 'Ignore results belonging to the previous code');
    assert(!root.textContent.includes('Test host'), 'Do not show stale host details');
    report.push('PASS editing a code invalidates pending results');

    pending = null;
    await click('Confirm session');
    await until(() => !!pending);
    root.querySelector('.dialog-head button').click();
    pending();
    await sleep(100);
    assert(!root.textContent.includes('Test host'), 'Ignore the query after leaving the join flow');
    report.push('PASS leaving the join flow invalidates pending results');

    scenario = 'ok';
    render(null, root);
    render(html`<${MultiplayerPage} />`, root);
    await until(() => !!button('Select server') && root.textContent.includes('Saved test server'));
    assert(button('Use a server address') && button('Select server'), 'Keep server management and selection');
    assert(!root.querySelector('#multiplayer-code'), 'Servers must not duplicate friend joining');
    report.push('PASS separate server list and address dialog entry retained');

    await mount();
    root.querySelector('.dialog-head button').click();
    await sleep(80);
    scenario = 'tunnel-error';
    root.querySelector('input[value="host"]').click();
    assert(S.choice.lan_mode === 'host' && S.page === 'drive', 'Host mode must use the existing Drive page');
    await until(() => S.toasts.some((x) => x.text.includes('Internet preparation could not be started')));
    assert(!S.toasts.some((x) => x.text.includes('Tunnel unavailable')), 'Do not expose raw technical errors in the background warning');
    report.push('PASS host setup continues even if background preparation fails');

    set({ poll: { instances: [{ running: false, lan_status: { role: 'host', code: 'STOPPED' } }, { running: true, lan_status: { role: 'host', code: 'OMSI-LIVE', tunnel: true } }] } });
    await sleep(150);
    assert(root.textContent.includes('OMSI-LIVE') && !root.textContent.includes('STOPPED'), 'Share only a running host code');
    report.push('PASS share the current running session code');
    return `${report.join('\n')}\n${report.length} checks passed; ${queries} fixture queries.`;
  } finally {
    pending?.();
    render(null, root);
    window.fetch = originalFetch;
  }
}
