import { test } from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import vm from "node:vm";

const app = fs.readFileSync(new URL("../site/app.js", import.meta.url), "utf8");
const code = app.slice(app.indexOf("function scanResult("), app.indexOf("async function home("));
const scan = vm.runInNewContext(`${code}; ({ scanResult, scanVerdict })`);
const verdict = d => scan.scanVerdict(scan.scanResult({ f: d }, "f"));

test("no scan yet", () => {
  assert.equal(scan.scanVerdict(scan.scanResult({}, "f")).kind, "wait");
  assert.equal(verdict({ m: 0, s: 0, n: 0 }).kind, "wait");
});

test("no engine flags the file", () => {
  const v = verdict({ m: 0, s: 0, n: 66 });
  assert.equal(v.kind, "clean");
  assert.equal(v.label, "Clean");
});

test("one malicious answer is shown with its count, not as checked", () => {
  const v = verdict({ m: 1, s: 0, n: 70 });
  assert.equal(v.kind, "review");
  assert.equal(v.label, "1 of 70 engines");
  assert.doesNotMatch(v.note, /false positive/);
});

test("one suspicious answer is shown with its count, not as checked", () => {
  const v = verdict({ m: 0, s: 1, n: 70 });
  assert.equal(v.kind, "review");
  assert.equal(v.label, "1 of 70 engines");
});

test("two answers are shown with their count", () => {
  const v = verdict({ m: 1, s: 1, n: 68 });
  assert.equal(v.kind, "review");
  assert.equal(v.label, "2 of 68 engines");
});

test("three answers or more are flagged", () => {
  assert.equal(verdict({ m: 3, s: 0, n: 68 }).kind, "flagged");
  assert.equal(verdict({ m: 2, s: 5, n: 68 }).label, "Flagged");
});

test("only a file reviewed as a false positive shows as checked, with the reason", () => {
  const v = verdict({ m: 1, s: 0, n: 66, fp: 1, why: "a machine-learning guess" });
  assert.equal(v.kind, "reviewed");
  assert.equal(v.label, "Checked");
  assert.match(v.note, /reviewed it as a false positive \(a machine-learning guess\)/);
});

test("odd data is read as numbers and never trusted as a review", () => {
  const v = verdict({ m: "1", s: null, n: "66", fp: "yes" });
  assert.equal(v.kind, "review");
  assert.equal(v.label, "1 of 66 engines");
});
