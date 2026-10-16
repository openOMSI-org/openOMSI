import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { mkdtempSync, readFileSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { extname, join, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';

const launcher = fileURLToPath(new URL('..', import.meta.url));
const fixtures = join(launcher, 'tests', 'fixtures');
const sizes = (process.env.DRIVE_CHECK_SIZES || '400x820,1280x720,1920x1080,3440x1440').split(',').map((s) => s.split('x').map(Number));
const chrome = process.env.CHROME || 'google-chrome';
const types = { '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.json': 'application/json', '.svg': 'image/svg+xml', '.woff2': 'font/woff2', '.jpg': 'image/jpeg', '.bin': 'application/octet-stream' };

const server = createServer((req, res) => {
  const url = decodeURIComponent(new URL(req.url, 'http://x').pathname);
  let file = url.startsWith('/mock/') ? join(fixtures, url.slice(6)) : join(launcher, normalize(url));
  if (statSync(file, { throwIfNoEntry: false })?.isDirectory()) file = join(file, 'index.html');
  if (!file.startsWith(launcher) || !statSync(file, { throwIfNoEntry: false })?.isFile()) {
    res.writeHead(404).end();
    return;
  }
  res.writeHead(200, { 'content-type': types[extname(file)] || 'application/octet-stream', 'cache-control': 'no-store' }).end(readFileSync(file));
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const origin = `http://127.0.0.1:${server.address().port}`;

const profile = mkdtempSync(join(tmpdir(), 'drive-check-'));
const sandbox = process.env.CI ? ['--no-sandbox'] : [];
const browser = spawn(chrome, [...sandbox, '--headless=new', '--remote-debugging-port=0', `--user-data-dir=${profile}`, '--no-first-run', '--no-default-browser-check', '--disable-gpu', '--hide-scrollbars', '--mute-audio', '--disable-background-timer-throttling', '--disable-renderer-backgrounding', '--disable-backgrounding-occluded-windows', '--window-size=1280,720', 'about:blank'], { stdio: ['ignore', 'ignore', 'pipe'] });
const wsUrl = await new Promise((resolve, reject) => {
  let err = '';
  browser.stderr.on('data', (d) => {
    err += d;
    const m = /DevTools listening on (ws:\/\/\S+)/.exec(err);
    if (m) resolve(m[1]);
  });
  browser.on('exit', (code) => reject(new Error(`Chrome stopped (${code}): ${err}`)));
});

const ws = new WebSocket(wsUrl);
await new Promise((r, j) => ((ws.onopen = r), (ws.onerror = j)));
let next = 0;
const waiting = new Map();
const events = [];
ws.onmessage = (e) => {
  const m = JSON.parse(e.data);
  if (m.id && waiting.has(m.id)) {
    const { resolve, reject } = waiting.get(m.id);
    waiting.delete(m.id);
    m.error ? reject(new Error(m.error.message)) : resolve(m.result);
  } else if (m.method) events.push(m);
};
const send = (method, params = {}, sessionId) =>
  new Promise((resolve, reject) => {
    const id = ++next;
    waiting.set(id, { resolve, reject });
    ws.send(JSON.stringify({ id, method, params, sessionId }));
  });

const { targetId } = await send('Target.createTarget', { url: 'about:blank' });
const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true });
const page = (method, params) => send(method, params, sessionId);
const evaluate = async (expression) => {
  const r = await page('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description || r.exceptionDetails.text);
  return r.result.value;
};
await page('Runtime.enable');
await page('Page.enable');

const report = {};
let failed = false;
try {
  for (const [width, height] of sizes) {
    await page('Emulation.setDeviceMetricsOverride', { width, height, deviceScaleFactor: 1, mobile: false });
    await page('Page.navigate', { url: `${origin}/ui/?check=${width}` });
    for (let i = 0; i < 300 && !(await evaluate("!!document.querySelector('.page-drive') && !!window.document.fonts && document.fonts.status === 'loaded'").catch(() => false)); i++) await new Promise((r) => setTimeout(r, 100));
    Object.assign(report, await evaluate(`import('${origin}/tests/drive-check.js').then((m) => m.run())`));
  }
  const errors = events.filter((e) => e.method === 'Runtime.exceptionThrown').map((e) => e.params.exceptionDetails.exception?.description || e.params.exceptionDetails.text);
  for (const [where, issues] of Object.entries(report)) {
    console.log(`${issues.length ? 'FAIL' : 'ok  '} ${where}`);
    for (const i of issues) console.log(`     ${i}`);
    failed ||= issues.length > 0;
  }
  for (const e of new Set(errors)) console.log(`FAIL script error: ${e}`);
  failed ||= errors.length > 0;
} catch (e) {
  console.log(`FAIL ${e.message}`);
  failed = true;
} finally {
  ws.close();
  const exited = browser.exitCode !== null || browser.signalCode !== null ? Promise.resolve() : new Promise((r) => browser.once('exit', r));
  browser.kill();
  await exited;
  server.close();
  try {
    rmSync(profile, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
  } catch {}
}
process.exit(failed ? 1 : 0);
