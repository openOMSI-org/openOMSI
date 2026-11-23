// The public server list of openBus (see README.md beside this file). A server with
// `public_server = 1` registers here and says it is alive every minute; the launcher and the
// game read the list. One Durable Object keeps the listings in memory (a listing lives three
// minutes past its last heartbeat, so a restarted object is right again within a minute) and
// nothing is written to storage. A listing is shown only after the service has read the
// server's own status page and found the challenge it gave at registration there.
//
//   POST /v1/register   {name, motd, map, max_players, version, protocol, web_port, address?, icon?}
//                       -> {id, token, challenge, ttl, interval}
//   POST /v1/heartbeat  {id, token, players, map}  -> {verified: bool, error?}
//   POST /v1/unlist     {id, token}                -> 204
//   GET  /v1/servers    -> {servers: [{id, address, name, motd, map, players, max_players, version, protocol, has_icon, age}]}
//   GET  /v1/servers/<id>/icon.png

import { DurableObject } from "cloudflare:workers";
import { TTL_MS, MIN_BEAT_MS, REVERIFY_MS, MAX_LISTINGS, MAX_PER_SOURCE, REGISTERS_PER_HOUR, MAX_BODY, Limiter, checkRegistration, checkIcon, statusProves, sourceKey, hex } from "./logic.js";

const CORS = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
  "Access-Control-Allow-Headers": "Content-Type",
};

function json(body, status = 200, extra = {}) {
  return new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json", ...CORS, ...extra } });
}

const randomHex = (n) => hex(crypto.getRandomValues(new Uint8Array(n)));

async function sha256(text) {
  return hex(new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text))));
}

export class List extends DurableObject {
  constructor(ctx, env) {
    super(ctx, env);
    this.env = env;
    this.listings = new Map();
    // (a source is kept as a hash for the limits only, and forgotten with its listings)
    this.registers = new Limiter(REGISTERS_PER_HOUR, 60 * 60 * 1000);
    this.swept = 0;
  }

  forget(now) {
    for (const [id, l] of this.listings) if (l.beat < now - TTL_MS) this.listings.delete(id);
    if (now - this.swept > 60000) { this.registers.sweep(now); this.swept = now; }
  }

  async register(body, ip) {
    const now = Date.now();
    this.forget(now);
    const source = await sha256("src" + sourceKey(ip));
    if (!this.registers.allow(source, now)) return { status: 429, body: { error: "too many registrations from this address: try again later" } };
    const checked = checkRegistration(body, ip, this.env);
    if (checked.error) return { status: 400, body: { error: checked.error } };
    const l = checked.listing;
    let icon = null;
    if (body.icon !== undefined) {
      icon = checkIcon(body.icon);
      if (!icon) return { status: 400, body: { error: "the icon is not a PNG of 64x64 or less and 16 KB" } };
    }
    // the same address again (a restart): the old listing goes, the new one takes its place -
    // when it comes from the same sender, or the old one never proved itself; an address
    // somebody else's listing holds is not taken over by a registration (it would be unlisted
    // by anyone who knew the address)
    for (const [id, o] of this.listings) {
      if (o.address !== l.address) continue;
      if (o.source !== source && o.verified) return { status: 409, body: { error: "this address is listed already" } };
      this.listings.delete(id);
    }
    const mine = [...this.listings.values()].filter((o) => o.source === source).length;
    if (mine >= MAX_PER_SOURCE) return { status: 429, body: { error: `at most ${MAX_PER_SOURCE} servers per address` } };
    if (this.listings.size >= MAX_LISTINGS) return { status: 503, body: { error: "the list is full" } };
    const id = randomHex(8), token = randomHex(24), challenge = randomHex(16);
    this.listings.set(id, { id, tokenHash: await sha256(token), challenge, ...l, icon, source, players: 0, registered: now, beat: now, verified: false, verifiedAt: 0, lastTry: 0, error: "not checked yet" });
    return { status: 200, body: { id, token, challenge, ttl: TTL_MS / 1000, interval: 60 } };
  }

  async verify(l, now) {
    l.lastTry = now;
    try {
      const ctl = new AbortController();
      const timer = setTimeout(() => ctl.abort(), 5000);
      const r = await fetch(l.address + "/status", { redirect: "manual", signal: ctl.signal, headers: { "User-Agent": "openBus-serverlist/1" } });
      clearTimeout(timer);
      if (r.status !== 200) throw new Error("the status page answered " + r.status);
      const text = (await r.text()).slice(0, 65536);
      if (statusProves(JSON.parse(text), l.challenge, l.name)) { l.verified = true; l.verifiedAt = now; l.error = ""; return; }
      l.verified = false;
      l.error = "the status page did not show the list's challenge (is public_server = 1 and the server running with it?)";
    } catch (e) {
      l.verified = false;
      l.error = "the service could not read " + l.address + "/status (" + (e && e.message ? String(e.message).slice(0, 60) : "failed") + "): the web port must be reachable from the internet, or use the tunnel";
    }
  }

  async heartbeat(body) {
    const now = Date.now();
    this.forget(now);
    const l = this.listings.get(String(body.id));
    if (!l) return { status: 404, body: { error: "unknown listing" } };
    if (l.tokenHash !== await sha256(String(body.token))) return { status: 403, body: { error: "wrong token" } };
    if (now - l.beat < MIN_BEAT_MS && l.verified) return { status: 429, body: { error: "too fast: one heartbeat a minute" } };
    l.beat = now;
    const p = Number(body.players);
    if (Number.isFinite(p)) l.players = Math.max(0, Math.min(l.max_players || 1000, Math.trunc(p)));
    if (!l.verified && now - l.lastTry > 15000) await this.verify(l, now);
    else if (l.verified && now - l.verifiedAt > REVERIFY_MS) await this.verify(l, now);
    return { status: 200, body: l.verified ? { verified: true } : { verified: false, error: l.error } };
  }

  async unlist(body) {
    const l = this.listings.get(String(body.id));
    if (l && l.tokenHash === await sha256(String(body.token))) this.listings.delete(l.id);
    return { status: 204 };
  }

  list() {
    const now = Date.now();
    this.forget(now);
    const out = [...this.listings.values()]
      .filter((l) => l.verified)
      .sort((a, b) => b.players - a.players || a.name.localeCompare(b.name))
      .map((l) => ({ id: l.id, address: l.address, name: l.name, motd: l.motd, map: l.map, players: l.players, max_players: l.max_players, version: l.version, protocol: l.protocol, has_icon: !!l.icon, age: Math.round((now - l.beat) / 1000) }));
    return { servers: out };
  }

  icon(id) {
    const l = this.listings.get(id);
    return l && l.verified && l.icon ? l.icon : null;
  }
}

export default {
  async fetch(request, env, ctx) {
    try {
      return await handle(request, env, ctx);
    } catch (e) {
      console.error("openBus serverlist:", e);
      return json({ error: "unavailable" }, 503, { "Retry-After": "120" });
    }
  },
};

async function bodyOf(request) {
  const len = Number(request.headers.get("content-length") || 0);
  if (len > MAX_BODY) return null;
  const text = await request.text();
  if (text.length > MAX_BODY) return null;
  try {
    const b = JSON.parse(text);
    return b && typeof b === "object" ? b : null;
  } catch {
    return null;
  }
}

async function handle(request, env, ctx) {
  const url = new URL(request.url);
  const list = env.LIST.get(env.LIST.idFromName("openbus"));
  if (request.method === "OPTIONS") return new Response(null, { status: 204, headers: CORS });
  const ip = request.headers.get("CF-Connecting-IP") || "";
  if (request.method === "POST" && ["/v1/register", "/v1/heartbeat", "/v1/unlist"].includes(url.pathname)) {
    const b = await bodyOf(request);
    if (!b) return json({ error: "not a JSON object (or too big)" }, 400);
    const r = url.pathname === "/v1/register" ? await list.register(b, ip) : url.pathname === "/v1/heartbeat" ? await list.heartbeat(b) : await list.unlist(b);
    return r.status === 204 ? new Response(null, { status: 204, headers: CORS }) : json(r.body, r.status);
  }
  if (request.method === "GET" && url.pathname === "/v1/servers") {
    // (cached at the edge for a few seconds: the launchers of everybody ask the same)
    const cache = caches.default;
    const key = new Request(url.origin + "/v1/servers");
    const hit = await cache.match(key);
    if (hit) return hit;
    const out = json(await list.list(), 200, { "Cache-Control": "public, max-age=15" });
    ctx.waitUntil(cache.put(key, out.clone()));
    return out;
  }
  const m = url.pathname.match(/^\/v1\/servers\/([0-9a-f]{16})\/icon\.png$/);
  if (request.method === "GET" && m) {
    const b64 = await list.icon(m[1]);
    if (!b64) return new Response("no icon\n", { status: 404, headers: { "Content-Type": "text/plain", ...CORS } });
    const bin = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
    return new Response(bin, { headers: { "Content-Type": "image/png", "Cache-Control": "public, max-age=300", "X-Content-Type-Options": "nosniff", ...CORS } });
  }
  return new Response("openBus server list: GET /v1/servers\n", { status: url.pathname === "/" ? 200 : 404, headers: { "Content-Type": "text/plain", ...CORS } });
}
