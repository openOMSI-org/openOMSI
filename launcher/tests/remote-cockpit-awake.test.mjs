import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const src = readFileSync(new URL("../../crates/omsi-remote/page/app.js", import.meta.url), "utf8");
const method = src.slice(src.indexOf("  keepAwake() {"), src.indexOf("\n  init() {"));

function page({ wakeLock, refuse = false, deny = false }) {
  const docListeners = {};
  const toasts = [];
  let created = 0;
  const video = {
    paused: true,
    plays: 0,
    currentTime: 0,
    setAttribute() {},
    addEventListener() {},
    play() {
      if (refuse) return Promise.reject(new Error("NotAllowedError"));
      this.paused = false;
      this.plays++;
      return Promise.resolve();
    },
    pause() { this.paused = true; },
  };
  const attrs = { "aria-pressed": "false" };
  const btn = { hidden: true, onclick: null, getAttribute: (k) => attrs[k], setAttribute: (k, v) => (attrs[k] = v), contains: (e) => e === btn };
  let locks = 0;
  const ctx = {
    AWAKE_VIDEO: "",
    R: { T: (t) => t },
    $: () => btn,
    document: { hidden: false, body: { append() {} }, createElement: () => (created++, video), addEventListener: (k, f) => (docListeners[k] = f) },
    navigator: wakeLock ? { wakeLock: { request: () => (locks++, deny ? Promise.reject(new Error("NotAllowedError")) : new Promise(() => {})) } } : {},
  };
  vm.createContext(ctx);
  ctx.toasts = toasts;
  vm.runInContext(`({toast(t) { toasts.push(t); }, ${method}}).keepAwake()`, ctx);
  return { ctx, video, btn, docListeners, toasts, created: () => created, locks: () => locks, tick: () => new Promise((r) => setTimeout(r)) };
}

test("with a wake lock the page holds it and hides the button", () => {
  const p = page({ wakeLock: true });
  assert.equal(p.locks(), 1);
  assert.equal(p.created(), 0);
  assert.ok(p.btn.hidden);
});

test("a refused wake lock shows the button instead", async () => {
  const p = page({ wakeLock: true, deny: true });
  await p.tick();
  assert.equal(p.btn.hidden, false);
  assert.equal(p.created(), 1);
});

test("without a wake lock the button shows and nothing plays until it is pressed", () => {
  const p = page({ wakeLock: false });
  assert.equal(p.btn.hidden, false);
  p.docListeners.click({ target: {} });
  assert.equal(p.video.plays, 0);
});

test("pressing the button plays the video and says so", async () => {
  const p = page({ wakeLock: false });
  p.docListeners.click({ target: p.btn });
  p.btn.onclick();
  await p.tick();
  assert.equal(p.btn.getAttribute("aria-pressed"), "true");
  assert.equal(p.video.plays, 1);
  assert.equal(p.toasts.length, 1);
});

test("a refused play turns the button off again", async () => {
  const p = page({ wakeLock: false, refuse: true });
  p.btn.onclick();
  await p.tick();
  assert.equal(p.btn.getAttribute("aria-pressed"), "false");
  assert.equal(p.toasts.length, 0);
});

test("pressing the button again stops the video without playing it first", async () => {
  const p = page({ wakeLock: false });
  p.btn.onclick();
  await p.tick();
  p.video.pause();
  p.docListeners.click({ target: p.btn });
  p.docListeners.touchend({ target: p.btn });
  assert.equal(p.video.plays, 1);
  p.btn.onclick();
  assert.equal(p.btn.getAttribute("aria-pressed"), "false");
  assert.ok(p.video.paused);
});

test("the video pauses when the page is hidden and plays again on the next touch", async () => {
  const p = page({ wakeLock: false });
  p.btn.onclick();
  await p.tick();
  p.ctx.document.hidden = true;
  p.docListeners.visibilitychange();
  assert.ok(p.video.paused);
  p.ctx.document.hidden = false;
  p.docListeners.visibilitychange();
  assert.equal(p.video.plays, 2);
  p.video.pause();
  p.docListeners.touchend({ target: {} });
  assert.equal(p.video.plays, 3);
});
