import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const src = readFileSync(new URL("../../crates/omsi-remote/page/app.js", import.meta.url), "utf8");
const method = src.slice(src.indexOf("  keepAwake() {"), src.indexOf("\n  init() {"));

function page({ wakeLock }) {
  const docListeners = {}, winListeners = {};
  const video = { paused: true, plays: 0, setAttribute() {}, play() { this.paused = false; this.plays++; return Promise.resolve(); }, pause() { this.paused = true; } };
  let reject;
  const ctx = {
    AWAKE_VIDEO: "",
    document: { hidden: false, createElement: () => video, addEventListener: (k, f) => (docListeners[k] = f) },
    navigator: wakeLock ? { wakeLock: { request: () => new Promise((_, r) => (reject = r)) } } : {},
    addEventListener: (k, f) => (winListeners[k] = f),
  };
  vm.createContext(ctx);
  vm.runInContext(`({${method}}).keepAwake()`, ctx);
  return { ctx, video, docListeners, winListeners, reject: () => { reject(new Error("denied")); return new Promise((r) => setTimeout(r)); } };
}

test("a wake lock refused while visible starts the fallback video", async () => {
  const p = page({ wakeLock: true });
  await p.reject();
  assert.equal(p.video.plays, 1);
});

test("a wake lock refused after the tab is hidden keeps the video paused", async () => {
  const p = page({ wakeLock: true });
  p.ctx.document.hidden = true;
  p.docListeners.visibilitychange();
  await p.reject();
  assert.equal(p.video.plays, 0);
  assert.ok(p.video.paused);
});

test("a wake lock refused after pagehide keeps the video paused", async () => {
  const p = page({ wakeLock: true });
  p.winListeners.pagehide();
  await p.reject();
  assert.equal(p.video.plays, 0);
  assert.ok(p.video.paused);
});

test("without a wake lock the video plays again when the page is shown", () => {
  const p = page({ wakeLock: false });
  assert.equal(p.video.plays, 1);
  p.ctx.document.hidden = true;
  p.docListeners.visibilitychange();
  assert.ok(p.video.paused);
  p.ctx.document.hidden = false;
  p.docListeners.visibilitychange();
  assert.equal(p.video.plays, 2);
});
