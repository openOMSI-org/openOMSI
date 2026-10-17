// The web panel in a real browser against a real gateway and session (no OMSI 2 needed):
//   cargo run -p omsi-lan --example admin_demo -- 27315 27325 &
//   NODE_PATH=<where playwright is installed> node crates/omsi-admin/panel/test/panel.e2e.js http://127.0.0.1:27325 <screenshot dir>
// It sends a player of the demo away, so start the demo afresh for each run. Not run by the
// pull request checks (they have no browser): run it when the panel changes.
const { chromium } = require("playwright");
const assert = require("assert");

const base = process.argv[2] || "http://127.0.0.1:27325";
const shots = process.argv[3] || ".";
const log = (...a) => console.log("•", ...a);

async function login(page, pw) {
  await page.goto(base + "/admin/");
  await page.waitForSelector("#login:not([hidden])");
  await page.fill("#password", pw);
  await page.click("#login-form button[type=submit]");
}

(async () => {
  const browser = await chromium.launch();
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 860 }, locale: "en-US" });
  const page = await ctx.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  // (the browser logs every 4xx answer as an error: those are the statuses the test expects)
  page.on("console", (m) => { if (m.type() === "error" && !/Failed to load resource/.test(m.text())) errors.push(m.text()); });

  // wrong password: a message, no session
  await login(page, "wrong password");
  await page.waitForFunction(() => document.querySelector("#login-msg").textContent.length > 0);
  assert.match(await page.textContent("#login-msg"), /wrong password/);
  assert.strictEqual(await page.evaluate(() => sessionStorage.getItem("token")), null);
  log("wrong password refused with a message");

  // the admin
  await login(page, "admin secret");
  await page.waitForSelector("#app:not([hidden])");
  await page.waitForFunction(() => document.querySelectorAll("#players li").length >= 4);
  assert.match(await page.textContent("#who"), /admin/);
  assert.match(await page.textContent("#facts"), /Grundorf/);
  // the password is nowhere in the page or its storage
  const stored = await page.evaluate(() => JSON.stringify(sessionStorage) + JSON.stringify(localStorage) + document.cookie);
  assert(!stored.includes("admin secret"));
  // a name with markup is text, not markup
  const boldAsText = await page.evaluate(() => [...document.querySelectorAll("#players li")].some((li) => li.textContent.includes("<b>Bold</b>") && !li.querySelector("b")));
  assert(boldAsText, "markup in a name shown as text");
  log("logged in; 4 players listed; markup shown as text");

  // search pichau: both alike names, the exact one first
  await page.fill("#search", "pichau");
  await page.waitForFunction(() => document.querySelectorAll("#players li").length === 2);
  const names = await page.$$eval("#players li > div:first-child", (d) => d.map((x) => x.textContent));
  assert.deepStrictEqual(names.map((n) => n.replace(/ \(#\d+\)$/, "")), ["Pichau", "pichau_fan"]);
  log("search 'pichau':", names.join(", "));

  // pick Pichau, send away with a reason: confirmation names the player, number and connection
  await page.click("#players li:first-child");
  await page.waitForSelector("#selected:not([hidden])");
  assert.match(await page.textContent("#sel-name"), /^Pichau \(#\d+\)$/);
  await page.fill("#reason", "spam <script>x</script>");
  await page.click("#kick");
  await page.waitForSelector("dialog#confirm[open]");
  const confirmText = await page.textContent("#confirm-text");
  assert.match(confirmText, /Send away Pichau \(number \d+, connection \d+\)\?/);
  log("confirmation:", confirmText);
  await page.screenshot({ path: shots + "/panel-confirm-desktop.png" });
  await page.click("#confirm-ok");
  await page.waitForFunction(() => [...document.querySelectorAll("#events li")].some((li) => /applied/.test(li.textContent) && /Pichau/.test(li.textContent)), null, { timeout: 15000 });
  const ev = await page.$eval("#events li", (li) => li.textContent);
  log("event:", ev);
  // gone from the list; pichau_fan still there; the old selection is not acted on
  await page.waitForFunction(() => [...document.querySelectorAll("#players li")].every((li) => !/^Pichau \(/.test(li.textContent)));
  assert.strictEqual(await page.$$eval("#players li", (l) => l.length), 1);
  assert.strictEqual(await page.isVisible("#sel-gone"), true);
  assert.strictEqual(await page.isDisabled("#kick"), true);
  log("Pichau gone, pichau_fan still listed, stale selection disabled");

  // passenger density: applied and shown
  await page.fill("#pax", "2");
  await page.click("#pax-go");
  await page.waitForFunction(() => /density 2\.00/.test(document.querySelector("#facts").textContent), null, { timeout: 15000 });
  log("passenger density 2 applied and shown");
  // a value out of range: refused by the server, with the reason
  await page.fill("#pax", "9");
  await page.click("#pax-go");
  await page.waitForFunction(() => [...document.querySelectorAll("#events li")].some((li) => /rejected/.test(li.textContent) && /at most 3/.test(li.textContent)), null, { timeout: 15000 });
  log("passenger density 9 refused with the reason");

  // Portuguese
  await page.click("#lang");
  assert.strictEqual(await page.textContent("h1"), "Administração do servidor");
  await page.click("#lang");

  // the moderator: world controls disabled, and the server refuses them anyway
  const mctx = await browser.newContext({ viewport: { width: 375, height: 740 }, locale: "pt-BR", isMobile: true, hasTouch: true });
  const m = await mctx.newPage();
  m.on("pageerror", (e) => errors.push(String(e)));
  await login(m, "mod secret");
  await m.waitForSelector("#app:not([hidden])");
  await m.waitForFunction(() => document.querySelector("#pax-go").disabled === true);
  assert.strictEqual(await m.isVisible("#world-role"), true);
  assert.strictEqual(await m.isDisabled("#say"), false);
  const forced = await m.evaluate(async () => {
    const r = await fetch("/api/admin/ops", { method: "POST", headers: { "Content-Type": "application/json", Authorization: "Bearer " + sessionStorage.getItem("token") }, body: JSON.stringify({ op: "passenger_density", value: 0, role: "admin" }) });
    return r.status;
  });
  assert.strictEqual(forced, 403);
  log("moderator: world disabled; a forced request refused 403");
  // phone layout: no sideways scrolling
  const wide = await m.evaluate(() => document.scrollingElement.scrollWidth - window.innerWidth);
  assert(wide <= 0, "no horizontal scroll on a phone: " + wide);
  await m.screenshot({ path: shots + "/panel-mobile-moderator.png", fullPage: true });
  log("phone width 375: no horizontal scroll");
  // logout ends the session on the server too
  const token = await m.evaluate(() => sessionStorage.getItem("token"));
  await m.click("#logout");
  await m.waitForSelector("#login:not([hidden])");
  const after = await m.evaluate(async (t) => (await fetch("/api/admin/session", { headers: { Authorization: "Bearer " + t } })).status, token);
  assert.strictEqual(after, 401);
  log("logout: the token no longer works");

  await page.screenshot({ path: shots + "/panel-desktop-admin.png", fullPage: true });
  assert.deepStrictEqual(errors, [], "no script errors");
  console.log("ALL OK");
  await browser.close();
})().catch((e) => { console.error("FAILED:", e); process.exit(1); });
