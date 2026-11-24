// The server list's rules, apart from Cloudflare's runtime so that `node --test` can try them
// (logic.test.mjs): what a listing may say, which address it is reached at, how often a
// source may ask. The clients check everything they show again themselves (omsi-serverlist);
// this is the service's own first line against junk and abuse.

export const TTL_MS = 3 * 60 * 1000; // a listing is forgotten this long after its last heartbeat
export const MIN_BEAT_MS = 10 * 1000; // a heartbeat faster than this is refused
export const REVERIFY_MS = 30 * 60 * 1000; // the status page is read again this often
export const MAX_LISTINGS = 2000;
export const MAX_PER_SOURCE = 3; // listings one address may hold at once
export const REGISTERS_PER_HOUR = 8; // registrations one address may make
export const MAX_ICON = 16 * 1024;
export const MAX_BODY = 32 * 1024;

const CONTROL = /[\u0000-\u001f\u007f-\u009f​-‏‪-‮⁠-⁤﻿]/;

// A text of a listing: cut to size, spaces folded; null when it holds hidden characters,
// an address or an invitation (a server list is no advertising board) or a term the operator
// blocks (`BLOCKED_TERMS`, comma separated, in the Worker's variables).
export function cleanText(v, max, blocked = []) {
  if (typeof v !== "string") return "";
  if (CONTROL.test(v)) return null;
  const t = v.split(/\s+/).filter(Boolean).join(" ").slice(0, max);
  const low = t.toLowerCase();
  if (/(https?:|www\.|discord\.gg|t\.me\/|\.com\b|\.gg\b)/.test(low)) return null;
  if (blocked.some((b) => b && low.includes(b))) return null;
  return t;
}

export function blockedTerms(env) {
  return String((env && env.BLOCKED_TERMS) || "").toLowerCase().split(",").map((s) => s.trim()).filter(Boolean);
}

// A host name a tunnel gives (https, port 443, a real name: no address, no local name).
export function tunnelAddress(a) {
  if (typeof a !== "string" || a.length > 120) return null;
  let u;
  try { u = new URL(a); } catch { return null; }
  if (u.protocol !== "https:" || u.port !== "" || u.username || u.password || u.search || u.hash) return null;
  if (u.pathname !== "/" ) return null;
  const h = u.hostname.toLowerCase();
  if (!/^[a-z0-9]([a-z0-9-]*[a-z0-9])?(\.[a-z0-9]([a-z0-9-]*[a-z0-9])?)+$/.test(h)) return null;
  if (/^\d+(\.\d+)*$/.test(h) || h.endsWith(".local") || h.endsWith(".internal") || h.endsWith(".localhost") || h === "localhost") return null;
  return "https://" + h;
}

// The address a listing is joined at: its tunnel, else the sender's own address with the web
// port it names (the service never reaches an address anybody else chose).
export function listingAddress(body, senderIp) {
  const t = tunnelAddress(body.address);
  if (t) return t;
  const port = Number(body.web_port);
  if (!Number.isInteger(port) || port < 1024 || port > 65535) return null;
  if (typeof senderIp !== "string") return null;
  if (/^\d{1,3}(\.\d{1,3}){3}$/.test(senderIp)) {
    const o = senderIp.split(".").map(Number);
    if (o.some((n) => n > 255)) return null;
    // (not this network's own: a server there cannot be reached from the list)
    if (o[0] === 10 || o[0] === 127 || o[0] === 0 || (o[0] === 172 && o[1] >= 16 && o[1] <= 31) || (o[0] === 192 && o[1] === 168) || (o[0] === 169 && o[1] === 254) || (o[0] === 100 && o[1] >= 64 && o[1] <= 127)) return null;
    return `http://${senderIp}:${port}`;
  }
  if (/^[0-9a-f:]+$/i.test(senderIp) && senderIp.includes(":") && !/^(::1?|fe80|fc|fd)/i.test(senderIp)) return `http://[${senderIp.toLowerCase()}]:${port}`;
  return null;
}

// The listing a registration asks for, checked; { error } when it is refused.
export function checkRegistration(b, senderIp, env) {
  if (!b || typeof b !== "object") return { error: "not a registration" };
  const blocked = blockedTerms(env);
  const name = cleanText(b.name, 48, blocked);
  if (name === null || !name) return { error: "the server's name is not allowed in the list (no addresses, invitations, hidden characters or blocked words): change `name` in server.cfg" };
  const motd = cleanText(b.motd ?? "", 120, blocked);
  if (motd === null) return { error: "the server's message is not allowed in the list: change `motd` in server.cfg" };
  const map = cleanText(b.map ?? "", 120);
  const address = listingAddress(b, senderIp);
  if (!address) return { error: "no address to list: the server needs its tunnel, or a public address and a web_port from 1024 up" };
  const max_players = Math.max(0, Math.min(1000, Number.isFinite(Number(b.max_players)) ? Math.trunc(Number(b.max_players)) : 0));
  const protocol = Math.max(0, Math.min(1000, Number.isFinite(Number(b.protocol)) ? Math.trunc(Number(b.protocol)) : 0));
  const version = cleanText(b.version ?? "", 40) ?? "";
  return { listing: { name, motd, map: map ?? "", address, max_players, protocol, version } };
}

// A PNG icon sent as base64: small, really a PNG, no bigger than 64x64.
export function checkIcon(b64) {
  if (typeof b64 !== "string" || !b64 || b64.length > Math.ceil(MAX_ICON * 4 / 3) + 8 || !/^[A-Za-z0-9+/]+={0,2}$/.test(b64)) return null;
  let bin;
  try { bin = atob(b64); } catch { return null; }
  if (bin.length > MAX_ICON || bin.length < 33 || !bin.startsWith("\x89PNG\r\n\x1a\n") || bin.slice(12, 16) !== "IHDR") return null;
  const be = (i) => (bin.charCodeAt(i) << 24 | bin.charCodeAt(i + 1) << 16 | bin.charCodeAt(i + 2) << 8 | bin.charCodeAt(i + 3)) >>> 0;
  const w = be(16), h = be(20);
  if (w < 1 || h < 1 || w > 64 || h > 64) return null;
  return b64;
}

// A text as a listing keeps it: spaces folded, cut to size (what `cleanText` does before it
// looks for anything else).
export function foldText(v, max) {
  return typeof v === "string" ? v.split(/\s+/).filter(Boolean).join(" ").slice(0, max) : "";
}

// What the status page of a server must say for it to be listed: the challenge, and the name
// (as the listing has it: the page says it as `server.cfg` does, long or with double spaces).
export function statusProves(status, challenge, name) {
  return !!status && typeof status === "object" && status.list_challenge === challenge && foldText(status.name, 48) === name;
}

// Whom a sender counts as for the limits: the address, and for IPv6 the /64 it belongs to (one
// customer holds a whole /64, and a limit per /128 is no limit).
export function sourceKey(ip) {
  if (typeof ip !== "string" || !ip.includes(":")) return String(ip);
  const [head, tail = ""] = ip.toLowerCase().split("::");
  const a = head ? head.split(":") : [];
  const b = ip.includes("::") && tail ? tail.split(":") : [];
  const groups = ip.includes("::") ? [...a, ...Array(Math.max(0, 8 - a.length - b.length)).fill("0"), ...b] : a;
  return groups.slice(0, 4).map((g) => g.replace(/^0+(?=.)/, "")).join(":") + "::/64";
}

// The listings, in the Durable Object's storage as well as in memory. Cloudflare hibernates
// an idle object about ten seconds after its last request, and its memory goes with it: kept
// in memory alone, a server beating once a minute was unknown at every heartbeat and listed
// only between its re-registration and the next beat (#4165). Every change is written; the
// memory is only the copy the object reads from.
export const LISTING_PREFIX = "listing:";

export class ListingStore {
  constructor(storage) { this.storage = storage; this.map = new Map(); }
  // (once, when the object starts or wakes)
  async load() {
    this.map.clear();
    for (const l of (await this.storage.list({ prefix: LISTING_PREFIX })).values()) this.map.set(l.id, l);
  }
  get size() { return this.map.size; }
  get(id) { return this.map.get(id); }
  values() { return this.map.values(); }
  [Symbol.iterator]() { return this.map[Symbol.iterator](); }
  save(l) { this.map.set(l.id, l); return this.storage.put(LISTING_PREFIX + l.id, l); }
  delete(id) { this.map.delete(id); return this.storage.delete(LISTING_PREFIX + id); }
}

// A sliding window of events per key, in memory.
export class Limiter {
  constructor(max, windowMs) { this.max = max; this.windowMs = windowMs; this.hits = new Map(); }
  allow(key, now) {
    const list = (this.hits.get(key) || []).filter((t) => t > now - this.windowMs);
    if (list.length >= this.max) { this.hits.set(key, list); return false; }
    list.push(now);
    this.hits.set(key, list);
    return true;
  }
  sweep(now) {
    for (const [k, v] of this.hits) if (!v.some((t) => t > now - this.windowMs)) this.hits.delete(k);
  }
}

export function hex(bytes) {
  return [...bytes].map((b) => b.toString(16).padStart(2, "0")).join("");
}
