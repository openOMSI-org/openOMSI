// Screenshots of the server fleet in the launcher (#370): the launcher's own page, served as
// drive-check.mjs serves it, in headless Chrome at 1920x1080. Only the backend is stood in
// for: the launcher's test fixtures (launcher/tests/fixtures) answer its commands, and
// `server_query` answers as a fleet v1 server would (fleet.example, no real address).
// Run from the repository root: CHROME=<chrome> node docs/screenshots/server-fleet/capture.mjs
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { extname, join, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = fileURLToPath(new URL('.', import.meta.url));
const launcher = fileURLToPath(new URL('../../../launcher/', import.meta.url));
const fixtures = join(launcher, 'tests', 'fixtures');
const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json', '.svg': 'image/svg+xml', '.woff2': 'font/woff2', '.jpg': 'image/jpeg', '.bin': 'application/octet-stream' };

// What a fleet v1 dedicated server answers: three local buses it allows, one it offers that
// is not installed here, each with the paints it publishes.
const SERVER_ONLY = 'Vehicles/Example_Fleet/S21.bus';
const serverQuery = {
  name: 'Example fleet server', time: '07:30', weather: 'Overcast', map: 'maps/Berlin-Spandau/global.cfg',
  vehicles: ['Vehicles/MAN_NL_NG/MAN_EN92_main.bus', 'Vehicles/ABCoach_O560/O560_E5.bus', 'Vehicles/HC_Volvo7900H/Volvo_12.bus', SERVER_ONLY],
  fleet_version: 1,
  fleet: [
    { bus: 'Vehicles/MAN_NL_NG/MAN_EN92_main.bus', paints: ['Beige', 'Server Night Line', 'Server Airport Shuttle'] },
    { bus: 'Vehicles/ABCoach_O560/O560_E5.bus', paints: [] },
    { bus: 'Vehicles/HC_Volvo7900H/Volvo_12.bus', paints: ['Server City Blue'] },
    { bus: SERVER_ONLY, paints: ['Server Night Line', 'Server City Blue'] },
  ],
};

const server = createServer((req, res) => {
  const url = decodeURIComponent(new URL(req.url, 'http://x').pathname);
  if (url === '/mock/server_query.json') return res.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify(serverQuery));
  if (url === '/mock/settings_patch.json') return res.writeHead(200, { 'content-type': 'application/json' }).end('{}');
  if (url === '/mock/launch.json') return res.writeHead(200, { 'content-type': 'application/json' }).end('{"others":0}');
  let file = url.startsWith('/mock/') ? join(fixtures, url.slice(6)) : join(launcher, normalize(url));
  if (statSync(file, { throwIfNoEntry: false })?.isDirectory()) file = join(file, 'index.html');
  if (!file.startsWith(launcher) || !statSync(file, { throwIfNoEntry: false })?.isFile()) return res.writeHead(404).end();
  res.writeHead(200, { 'content-type': types[extname(file)] || 'application/octet-stream', 'cache-control': 'no-store' }).end(readFileSync(file));
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const origin = `http://127.0.0.1:${server.address().port}`;

const profile = mkdtempSync(join(tmpdir(), 'fleet-shots-'));
const browser = spawn(process.env.CHROME || 'google-chrome', [...(process.env.CI ? ['--no-sandbox'] : []), '--headless=new', '--remote-debugging-port=0', `--user-data-dir=${profile}`, '--no-first-run', '--hide-scrollbars', '--mute-audio', '--window-size=1920,1080', 'about:blank'], { stdio: ['ignore', 'ignore', 'pipe'] });
const wsUrl = await new Promise((resolve, reject) => {
  let err = '';
  browser.stderr.on('data', (d) => { err += d; const m = /DevTools listening on (ws:\/\/\S+)/.exec(err); if (m) resolve(m[1]); });
  browser.on('exit', (code) => reject(new Error(`Chrome stopped (${code}): ${err}`)));
});
const ws = new WebSocket(wsUrl);
await new Promise((r, j) => ((ws.onopen = r), (ws.onerror = j)));
let next = 0;
const waiting = new Map();
const errors = [];
ws.onmessage = (e) => {
  const m = JSON.parse(e.data);
  if (m.id && waiting.has(m.id)) { const w = waiting.get(m.id); waiting.delete(m.id); m.error ? w.reject(new Error(m.error.message)) : w.resolve(m.result); }
  else if (m.method === 'Runtime.exceptionThrown') errors.push(m.params.exceptionDetails.exception?.description || m.params.exceptionDetails.text);
};
const send = (method, params = {}, sessionId) => new Promise((resolve, reject) => { const id = ++next; waiting.set(id, { resolve, reject }); ws.send(JSON.stringify({ id, method, params, sessionId })); });
const { targetId } = await send('Target.createTarget', { url: 'about:blank' });
const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true });
const page = (method, params) => send(method, params, sessionId);
const evaluate = async (expression) => {
  const r = await page('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description || r.exceptionDetails.text);
  return r.result.value;
};
const wait = (ms) => new Promise((r) => setTimeout(r, ms));
const shot = async (name) => {
  await wait(600);
  const { data } = await page('Page.captureScreenshot', { format: 'png' });
  writeFileSync(join(here, name), Buffer.from(data, 'base64'));
  console.log(`captured ${name}`);
};

try {
  await page('Runtime.enable');
  await page('Page.enable');
  await page('Emulation.setDeviceMetricsOverride', { width: 1920, height: 1080, deviceScaleFactor: 1, mobile: false });
  await page('Page.navigate', { url: `${origin}/ui/` });
  for (let i = 0; i < 300 && !(await evaluate("!!document.querySelector('.page-drive') && document.fonts.status === 'loaded'").catch(() => false)); i++) await wait(100);
  // Join the server as the launcher does after its query (data.js serverJoined/askJoined).
  await evaluate(`(async () => {
    const d = await import('${origin}/ui/js/data.js');
    d.choose({ lan_mode: 'join', lan_addr: 'fleet.example:27015', free: true, bus: 'Vehicles/MAN_NL_NG/MAN_EN92_main.bus', paint: '' });
    await d.askJoined({ refresh: true });
  })()`);
  await wait(800);
  // The picker, as the drive page opens it.
  const openPicker = (bus, paint = '') => evaluate(`(async () => {
    const { html, render } = await import('${origin}/ui/vendor/preact-htm.js');
    const { BusPicker } = await import('${origin}/ui/js/pages/bus-picker.js');
    const d = await import('${origin}/ui/js/data.js');
    d.choose({ bus: ${JSON.stringify(bus)}, paint: ${JSON.stringify(paint)} });
    let host = document.getElementById('shot-host');
    if (host) render(null, host); else { host = document.createElement('div'); host.id = 'shot-host'; document.body.append(host); }
    render(html\`<\${BusPicker} onClose=\${() => render(null, host)} />\`, host);
  })()`);
  const closePicker = () => evaluate(`(async () => { const { render } = await import('${origin}/ui/vendor/preact-htm.js'); const h = document.getElementById('shot-host'); if (h) render(null, h); })()`);

  await openPicker('Vehicles/MAN_NL_NG/MAN_EN92_main.bus');
  await wait(1500);
  await shot('01-server-bus-catalogue.png');

  await evaluate(`document.querySelector('.picker .detail .field .select')?.click()`);
  await shot('02-server-paint-selection.png');
  await evaluate(`document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); document.body.click()`);
  await closePicker();

  await openPicker(SERVER_ONLY);
  await wait(1000);
  await shot('03-server-only-bus-no-preview.png');
  await closePicker();

  // Join and drive with a server bus: the launcher asks before anything is downloaded.
  await evaluate(`(async () => { const d = await import('${origin}/ui/js/data.js'); d.choose({ bus: ${JSON.stringify(SERVER_ONLY)}, paint: 'Server Night Line' }); })()`);
  await wait(500);
  const clicked = await evaluate(`(() => { const b = [...document.querySelectorAll('.btn.primary')].find((x) => /Join and drive/.test(x.textContent)); if (!b || b.disabled) return b ? 'disabled' : 'missing'; b.click(); return 'ok'; })()`);
  console.log(`Join and drive: ${clicked}`);
  await wait(1200);
  await shot('04-download-consent.png');
} finally {
  if (errors.length) console.log('script errors:\n' + [...new Set(errors)].join('\n'));
  ws.close();
  browser.kill();
  server.close();
  await wait(500);
  rmSync(profile, { recursive: true, force: true });
}
