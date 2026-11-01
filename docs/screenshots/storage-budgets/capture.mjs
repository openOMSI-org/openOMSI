// Screenshots of the disk budgets in the launcher's Troubleshooting page (#372): the
// launcher's own page, served as drive-check.mjs serves it, in headless Chrome at 1920x1080.
// Only the backend is stood in for: the launcher's test fixtures answer its commands, and
// `storage_status` / `storage_cleanup` answer with the fields and limits of
// omsi-launcher-core's storage::Status (1 GB local, 4 GB multiplayer) at sizes chosen here.
// Run from the repository root: CHROME=<chrome> node docs/screenshots/storage-budgets/capture.mjs
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

const GB = 1e9;
// storage::Status, as the backend would answer at each step
let status = { local_bytes: 0.62 * GB, local_limit: 1 * GB, lan_bytes: 2.85 * GB, lan_limit: 4 * GB, reclaimed_local: 0, reclaimed_lan: 0, busy: false, server_store: false };
let afterCleanup = status;
const json = (res, v) => res.writeHead(200, { 'content-type': 'application/json', 'cache-control': 'no-store' }).end(JSON.stringify(v));

const server = createServer((req, res) => {
  const url = decodeURIComponent(new URL(req.url, 'http://x').pathname);
  if (url === '/mock/storage_status.json') return json(res, status);
  if (url === '/mock/storage_cleanup.json') { status = afterCleanup; return json(res, status); }
  if (url === '/mock/settings_patch.json') return json(res, {});
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
  const open = async () => {
    await evaluate(`(async () => { const st = await import('${origin}/ui/js/store.js'); st.set({ page: 'drive' }); await new Promise((r) => setTimeout(r, 200)); st.set({ page: 'troubleshoot', toasts: [] }); })()`);
    await wait(1200);
  };
  const button = (label) => evaluate(`(() => { const b = [...document.querySelectorAll('button')].find((x) => x.textContent.trim().startsWith(${JSON.stringify(label)})); if (!b) return 'missing'; if (b.disabled) return 'disabled'; b.click(); return 'ok'; })()`);

  // 1. Usage under both limits
  await open();
  await shot('01-storage-usage.png');

  // 2. A game running, the multiplayer downloads over their soft limit: nothing is cleaned
  status = { ...status, lan_bytes: 4.37 * GB, busy: true };
  await open();
  await shot('02-game-running-over-soft-limit.png');

  // 3. Clear local cache asks first
  status = { ...status, lan_bytes: 4.37 * GB, busy: false };
  await open();
  console.log(`Clear local cache: ${await button('Clear local cache')}`);
  await wait(600);
  await shot('03-clear-local-cache-confirmation.png');
  await evaluate(`(() => { const b = [...document.querySelectorAll('.dialog button, .overlay button')].find((x) => x.textContent.trim() === 'Cancel'); b?.click(); })()`);

  // 4. Clean to limits: older multiplayer files go back under 4 GB, the freed size shown
  afterCleanup = { ...status, lan_bytes: 3.91 * GB, reclaimed_lan: 0.46 * GB };
  console.log(`Clean to limits: ${await button('Clean to limits')}`);
  await wait(400);
  await shot('04-clean-to-limits-result.png');

  // 5. A computer that holds a server's vehicle files: those are left to the server
  status = { ...status, reclaimed_lan: 0, server_store: true };
  await open();
  await shot('05-server-store-protected.png');
} finally {
  if (errors.length) console.log('script errors:\n' + [...new Set(errors)].join('\n'));
  ws.close();
  browser.kill();
  server.close();
  await wait(500);
  rmSync(profile, { recursive: true, force: true });
}
