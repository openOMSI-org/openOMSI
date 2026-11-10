import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { extname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('..', import.meta.url));
const fixture = JSON.parse(readFileSync(join(root, 'tests/fixtures/bootstrap.json')));
const dictionary = {};
for (const name of ['app', 'graphics']) {
  let key;
  for (const line of readFileSync(join(root, '../crates/omsi-app/locales/' + name + '.yml'), 'utf8').split(/\r?\n/)) {
    try {
      if (line.startsWith('"') && line.endsWith(':')) key = JSON.parse(line.slice(0, -1));
      else if (line.startsWith('  zh-tw: ') && key) dictionary[key] = JSON.parse(line.slice(9));
    } catch {}
  }
}
const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json', '.woff2': 'font/woff2' };
const harness = `<!doctype html><html data-theme="dark"><head><link rel="stylesheet" href="/ui/styles.css"></head>
<body style="padding:24px"><main id="app"></main><script>
window.calls = []; window.profiles = {}; window.importResult = null; window.exportResult = null;
window.__TAURI__ = {core: {invoke: async (cmd, args) => {
  calls.push([cmd, args]);
  if (cmd === 'graphics_profile_import') {
    if (importResult instanceof Error) throw importResult;
    if (importResult) profiles[importResult] = {msaa: 8};
    return importResult;
  }
  if (cmd === 'graphics_profile_export') {
    if (exportResult instanceof Error) throw exportResult;
    return exportResult;
  }
  if (cmd === 'nvidia_dlss_status') return {supported: false};
  if (cmd !== 'call') throw new Error('Wrong desktop routing: ' + cmd);
  if (args.cmd === 'translations') return fetch('/dictionary.json').then(r => r.json());
  if (args.cmd === 'graphics_presets') return [];
  if (args.cmd === 'graphics_profiles') return {...profiles};
  if (args.cmd === 'settings_patch') return {...window.settings, ...args.args.patch};
  return {};
}}};
</script><script type="module">
import {html, render} from '/ui/vendor/preact-htm.js';
import {SettingsPage} from '/ui/js/pages/settings.js';
import {S} from '/ui/js/store.js';
const fixture = ${JSON.stringify(fixture)};
window.settings = fixture.settings;
S.settings = fixture.settings; S.boot = {...fixture, platform: 'windows'}; S.config = fixture.config;
localStorage.setItem('settings-tab', 'graphics');
render(html\`<\$\{SettingsPage\} />\`, document.querySelector('#app'));
window.ready = true;
</script></body></html>`.replaceAll('\\`', '`').replaceAll('\\$\\{', '${');
const server = createServer((req, res) => {
  const url = new URL(req.url, 'http://localhost');
  if (url.pathname === '/dictionary.json') return res.writeHead(200, {'content-type':'application/json'}).end(JSON.stringify(dictionary));
  if (url.pathname === '/test') return res.writeHead(200, {'content-type': 'text/html'}).end(harness);
  const file = resolve(root, '.' + decodeURIComponent(url.pathname));
  if (!file.startsWith(root) || !statSync(file, {throwIfNoEntry: false})?.isFile()) return res.writeHead(404).end();
  res.writeHead(200, {'content-type': types[extname(file)] || 'application/octet-stream'}).end(readFileSync(file));
});
await new Promise(r => server.listen(0, '127.0.0.1', r));
const profile = mkdtempSync(join(tmpdir(), 'graphics-profile-check-'));
const browser = spawn(process.env.CHROME || 'google-chrome', [...(process.env.CI ? ['--no-sandbox'] : []), '--headless=new', '--remote-debugging-port=0', '--no-first-run', '--disable-gpu', '--disable-background-timer-throttling', '--hide-scrollbars', '--user-data-dir=' + profile, 'about:blank'], {stdio: ['ignore', 'ignore', 'pipe']});
let ws; let closeBrowser;
try {
  const wsUrl = await new Promise((resolve, reject) => {
    let text = '';
    browser.on('error', reject);
    browser.stderr.on('data', d => { text += d; const match = /DevTools listening on (ws:\/\/\S+)/.exec(text); if (match) resolve(match[1]); });
    browser.on('exit', code => reject(new Error('Chrome exited ' + code + ': ' + text)));
  });
  ws = new WebSocket(wsUrl);
  await new Promise((resolve, reject) => {ws.onopen = resolve; ws.onerror = reject;});
  const pending = new Map(); const errors = []; let next = 0;
  ws.onmessage = ({data}) => {
    const m = JSON.parse(data);
    if (pending.has(m.id)) { const p = pending.get(m.id); pending.delete(m.id); m.error ? p.reject(new Error(m.error.message)) : p.resolve(m.result); }
    if (m.method === 'Runtime.exceptionThrown') errors.push(m.params.exceptionDetails.exception?.description || m.params.exceptionDetails.text);
  };
  const send = (method, params = {}, sessionId) => new Promise((resolve, reject) => { const id = ++next; pending.set(id, {resolve, reject}); ws.send(JSON.stringify({id, method, params, sessionId})); });
  closeBrowser = () => send('Browser.close');
  const {targetId} = await send('Target.createTarget', {url: 'about:blank'});
  const {sessionId} = await send('Target.attachToTarget', {targetId, flatten: true});
  const page = (method, params) => send(method, params, sessionId);
  const evaluate = async expression => {
    const r = await page('Runtime.evaluate', {expression, awaitPromise: true, returnByValue: true});
    if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description || r.exceptionDetails.text);
    return r.result.value;
  };
  const wait = async expression => {
    for (let i = 0; i < 100; i++) {if (await evaluate(expression)) return; await new Promise(r => setTimeout(r, 50));}
    throw new Error('Timed out: ' + expression + '\nScript errors: ' + JSON.stringify(errors) + '\n' + await evaluate("JSON.stringify({card: window.card?.()?.textContent, calls: window.calls, profiles: window.profiles, importResult: window.importResult})"));
  };
  await page('Runtime.enable'); await page('Page.enable');
  await page('Emulation.setDeviceMetricsOverride', {width: 1280, height: 900, deviceScaleFactor: 1, mobile: false});
  await page('Page.navigate', {url: 'http://127.0.0.1:' + server.address().port + '/test'});
  await wait("window.ready && [...document.querySelectorAll('button')].some(b => b.textContent.includes('Import graphics profile'))");
  await evaluate(`window.button = text => [...document.querySelectorAll('button')].find(b => b.textContent.trim() === text); window.card = () => button('Import graphics profile').closest('.card');`);
  await wait("card().textContent.includes('No saved profiles')");
  assert.equal(await evaluate("button('Export graphics profile').disabled"), true);
  assert.equal(await evaluate("button('Import graphics profile').disabled"), false);
  await evaluate("button('Import graphics profile').click()");
  await wait("!button('Import graphics profile').disabled");
  assert.equal(await evaluate("card().textContent.includes('Imported')"), false);
  await evaluate("window.importResult = 'Friend (2)'; button('Import graphics profile').click()");
  await wait("card().textContent.includes('Imported')");
  assert.equal(await evaluate("card().querySelector('input[placeholder=\"Profile name\"]').value"), 'Friend (2)');
  assert.equal(await evaluate("[...card().querySelectorAll('.setting-row')].find(r => r.textContent.startsWith('Saved profile')).querySelector('.select').textContent.trim()"), 'Friend (2)');
  assert.equal(await evaluate("calls.some(([cmd, args]) => cmd === 'call' && args.cmd === 'settings_patch')"), false);
  await evaluate("window.exportResult = null; button('Export graphics profile').click()");
  await wait("!button('Export graphics profile').disabled");
  assert.equal(await evaluate("card().textContent.includes('Exported')"), false);
  await evaluate("window.exportResult = true; button('Export graphics profile').click()");
  await wait("card().textContent.includes('Exported')");
  assert.equal(await evaluate("calls.filter(([cmd]) => cmd === 'graphics_profile_export').at(-1)[1].name"), 'Friend (2)');
  await evaluate("window.importResult = new Error('Invalid graphics profile file.'); button('Import graphics profile').click()");
  await wait("card().textContent.includes('Invalid graphics profile file.')");
  assert.equal(await evaluate("card().querySelector('input[placeholder=\"Profile name\"]').value"), 'Friend (2)');
  await evaluate("button('Load').click()");
  await wait("card().textContent.includes('Loaded')");
  assert.equal(await evaluate("calls.filter(([cmd, args]) => cmd === 'call' && args.cmd === 'settings_patch').at(-1)[1].args.patch.msaa"), 8);
  await evaluate("window.profileCard = card(); window.profileStart = [...profileCard.querySelectorAll('.setting-row')].find(r => r.textContent.startsWith('Saved profile')).previousElementSibling;");
  await evaluate("import('/ui/js/i18n.js').then(m => m.setLanguage('zh-tw'))");
  await evaluate("window.importResult = null; button('Import graphics profile').click()");
  await wait("profileCard.textContent.includes('匯入圖像設定檔')");
  assert.equal(await evaluate("profileCard.textContent.includes('匯出圖像設定檔')"), true);
  for (const theme of ['dark', 'light']) {
    await evaluate("document.documentElement.dataset.theme = '" + theme + "'; profileStart.scrollIntoView({block:'start'});");
    await page('Emulation.setDeviceMetricsOverride', {width: 1280, height: 900, deviceScaleFactor: 1, mobile: false});
    await evaluate("document.fonts.ready");
    const clip = await evaluate("(() => { const r = profileCard.getBoundingClientRect(), start = profileStart.getBoundingClientRect(); return {x: r.x + scrollX, y: start.y + scrollY - 12, width:r.width, height:r.bottom - start.y + 24, scale:1}; })()");
    const shot = await page('Page.captureScreenshot', {format: 'png', captureBeyondViewport: true, clip});
    if (process.env.PROFILE_CHECK_SCREENSHOTS) writeFileSync(join(process.env.PROFILE_CHECK_SCREENSHOTS, 'graphics-profiles-' + theme + '.png'), Buffer.from(shot.data, 'base64'));
  }
  await page('Emulation.setDeviceMetricsOverride', {width: 440, height: 900, deviceScaleFactor: 1, mobile: false});
  await evaluate("profileStart.scrollIntoView({block:'start'})");
  assert.equal(await evaluate("document.documentElement.scrollWidth <= 440"), true);
  assert.deepEqual(errors, []);
  console.log('PASS graphics profile UI: empty list, import/export cancellation, import selection, explicit load, error recovery, desktop command routing, Traditional Chinese labels, both themes and narrow layout');
} finally {
  await closeBrowser?.().catch(() => {});
  ws?.close();
  if (browser.exitCode === null) await Promise.race([new Promise(r => browser.once('exit', r)), new Promise(r => setTimeout(r, 3000))]);
  if (browser.exitCode === null) browser.kill();
  server.close();
  try { rmSync(profile, {recursive: true, force: true, maxRetries: 5, retryDelay: 200}); } catch (e) { console.warn('Temporary browser profile retained: ' + e.message); }
}
