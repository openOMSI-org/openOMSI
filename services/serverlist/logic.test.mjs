// node --test services/serverlist
import test from "node:test";
import assert from "node:assert/strict";
import { cleanText, tunnelAddress, listingAddress, checkRegistration, checkIcon, statusProves, sourceKey, Limiter, ListingStore } from "./logic.js";

test("texts: folded, cut, and refused when they carry hidden characters, addresses or blocked words", () => {
  assert.equal(cleanText("  Berlin   24/7  ", 48), "Berlin 24/7");
  assert.equal(cleanText("x".repeat(100), 10), "xxxxxxxxxx");
  assert.equal(cleanText("a‮b", 48), null);
  assert.equal(cleanText("line\nbreak", 48), null);
  assert.equal(cleanText("join discord.gg/abc", 48), null);
  assert.equal(cleanText("see www.example.org", 48), null);
  assert.equal(cleanText("Free Stuff", 48, ["free"]), null);
  assert.equal(cleanText(5, 48), "");
});

test("a tunnel address is a plain https host name", () => {
  assert.equal(tunnelAddress("https://abc-def.trycloudflare.com"), "https://abc-def.trycloudflare.com");
  assert.equal(tunnelAddress("https://ABC.trycloudflare.com/"), "https://abc.trycloudflare.com");
  for (const bad of ["http://abc.example.org", "https://127.0.0.1", "https://localhost", "https://a.local", "https://x.example.org:8443", "https://user@x.example.org", "https://x.example.org/path", "https://x.example.org/?a=1", "javascript:alert(1)", "", "https://nodots"]) {
    assert.equal(tunnelAddress(bad), null, bad);
  }
});

test("without a tunnel the sender's own public address and the web port are the address", () => {
  assert.equal(listingAddress({ web_port: 27025 }, "203.0.113.9"), "http://203.0.113.9:27025");
  assert.equal(listingAddress({ web_port: 27025 }, "2001:db8::1"), "http://[2001:db8::1]:27025");
  assert.equal(listingAddress({ web_port: 80 }, "203.0.113.9"), null);
  assert.equal(listingAddress({ web_port: 27025 }, "192.168.1.5"), null);
  assert.equal(listingAddress({ web_port: 27025 }, "10.0.0.1"), null);
  assert.equal(listingAddress({ web_port: 27025 }, "127.0.0.1"), null);
  assert.equal(listingAddress({ web_port: 27025 }, "::1"), null);
  assert.equal(listingAddress({ web_port: 27025 }, ""), null);
  // a tunnel wins, and nobody else's address can be named by the port field
  assert.equal(listingAddress({ address: "https://a.trycloudflare.com", web_port: 27025 }, "203.0.113.9"), "https://a.trycloudflare.com");
  assert.equal(listingAddress({ address: "http://198.51.100.1:27025", web_port: 27025 }, "203.0.113.9"), "http://203.0.113.9:27025");
});

test("a registration is refused for a name the list would not show", () => {
  const ok = checkRegistration({ name: "My server", motd: "Hello", map: "Berlin", max_players: 16, web_port: 27025, version: "0.3.40", protocol: 14 }, "203.0.113.9", {});
  assert.equal(ok.listing.name, "My server");
  assert.equal(ok.listing.address, "http://203.0.113.9:27025");
  assert.match(checkRegistration({ name: "Visit www.x.org", web_port: 27025 }, "203.0.113.9", {}).error, /name/);
  assert.match(checkRegistration({ name: "", web_port: 27025 }, "203.0.113.9", {}).error, /name/);
  assert.match(checkRegistration({ name: "ok", motd: "a​b", web_port: 27025 }, "203.0.113.9", {}).error, /message/);
  assert.match(checkRegistration({ name: "ok", web_port: 27025 }, "192.168.0.2", {}).error, /address/);
  assert.match(checkRegistration({ name: "badword here", web_port: 27025 }, "203.0.113.9", { BLOCKED_TERMS: "badword" }).error, /name/);
  assert.ok(checkRegistration(null, "1.2.3.4", {}).error);
  // numbers are bounded
  assert.equal(checkRegistration({ name: "x", web_port: 27025, max_players: 99999 }, "203.0.113.9", {}).listing.max_players, 1000);
});

function png(w, h, extra = 40) {
  const b = new Uint8Array(33 + extra);
  b.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52]);
  new DataView(b.buffer).setUint32(16, w);
  new DataView(b.buffer).setUint32(20, h);
  return Buffer.from(b).toString("base64");
}

test("an icon is a small real PNG", () => {
  assert.ok(checkIcon(png(64, 64)));
  assert.equal(checkIcon(png(65, 64)), null);
  assert.equal(checkIcon(png(0, 64)), null);
  assert.equal(checkIcon(png(64, 64, 20000)), null);
  assert.equal(checkIcon(Buffer.from("GIF89a" + "x".repeat(60)).toString("base64")), null);
  assert.equal(checkIcon("not base64!"), null);
  assert.equal(checkIcon(""), null);
});

test("a status page proves a listing only with the challenge and the name", () => {
  assert.ok(statusProves({ list_challenge: "abc", name: "N" }, "abc", "N"));
  assert.ok(!statusProves({ list_challenge: "abd", name: "N" }, "abc", "N"));
  assert.ok(!statusProves({ list_challenge: "abc", name: "Other" }, "abc", "N"));
  assert.ok(!statusProves(null, "abc", "N"));
  assert.ok(!statusProves("abc", "abc", "N"));
  // (the page says the name as server.cfg has it: folded and cut like the listing's)
  assert.ok(statusProves({ list_challenge: "abc", name: "My   server " }, "abc", "My server"));
  assert.ok(statusProves({ list_challenge: "abc", name: "x".repeat(60) }, "abc", "x".repeat(48)));
  // an IPv6 sender counts by its /64
  assert.equal(sourceKey("2001:db8:1:2:aaaa::1"), sourceKey("2001:DB8:1:2:0:0:0:ffff"));
  assert.notEqual(sourceKey("2001:db8:1:3::1"), sourceKey("2001:db8:1:2::1"));
  assert.equal(sourceKey("203.0.113.9"), "203.0.113.9");
});

test("a source may register only so often", () => {
  const l = new Limiter(3, 1000);
  assert.ok(l.allow("a", 0) && l.allow("a", 1) && l.allow("a", 2));
  assert.ok(!l.allow("a", 3));
  assert.ok(l.allow("b", 3));
  assert.ok(l.allow("a", 1500));
  l.sweep(5000);
  assert.equal(l.hits.size, 0);
});

test("listings outlive the object's memory: a woken object reads them back from storage (#4165)", async () => {
  const kept = new Map();
  const storage = {
    async list({ prefix }) { return new Map([...kept].filter(([k]) => k.startsWith(prefix))); },
    async put(k, v) { kept.set(k, structuredClone(v)); },
    async delete(k) { kept.delete(k); },
  };
  const before = new ListingStore(storage);
  await before.load();
  await before.save({ id: "a1", name: "Berlin", beat: 1 });
  await before.save({ id: "b2", name: "Spandau", beat: 1 });
  const l = before.get("a1");
  l.beat = 2;
  await before.save(l);
  await before.delete("b2");
  // (hibernated: a new object over the same storage)
  const after = new ListingStore(storage);
  await after.load();
  assert.equal(after.size, 1);
  assert.equal(after.get("a1").beat, 2);
  assert.equal(after.get("b2"), undefined);
  assert.deepEqual([...after].map(([id]) => id), ["a1"]);
});
