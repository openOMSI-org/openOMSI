// What the site shows of GitHub (releases, their notes, the issue list) is read once, here, when the
// site is built, and served from openomsi.org itself: a visitor's browser then neither waits for
// GitHub nor runs into its request limit (60 an hour for a visitor without a token), and a bad
// answer from GitHub (it does give out error pages) costs nothing. The page still asks GitHub once in
// the background whether a newer release is out than the build knew (see static/js/dynamic.js).
//
// The last good answer is kept in .cache/: a build that cannot reach GitHub uses it.

import fs from "node:fs";
import path from "node:path";
import { REPO } from "./config.mjs";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function api(url, tries = 4) {
  const headers = { Accept: "application/vnd.github+json", "User-Agent": "openomsi-site-build" };
  if (process.env.GITHUB_TOKEN) headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  let last;
  for (let i = 0; i < tries; i++) {
    try {
      const r = await fetch(url, { headers, signal: AbortSignal.timeout(30000) });
      const text = await r.text();
      if (!r.ok) throw new Error(`${r.status}`);
      return JSON.parse(text); // (an error page with status 200 is not JSON: that retries too)
    } catch (e) {
      last = e;
      await sleep(1500 * (i + 1));
    }
  }
  throw new Error(`${url}: ${last?.message}`);
}

async function fetchRaw() {
  const base = `https://api.github.com/repos/${REPO}`;
  const releases = [];
  for (let page = 1; page < 10; page++) {
    const batch = await api(`${base}/releases?per_page=100&page=${page}`);
    releases.push(...batch);
    if (batch.length < 100) break;
  }
  const latest = await api(`${base}/releases/latest`);
  const issues = {};
  for (const state of ["open", "closed"]) {
    const list = await api(`${base}/issues?state=${state}&per_page=100&sort=updated`);
    issues[state] = list.filter((i) => !i.pull_request);
  }
  return {
    fetched: new Date().toISOString(),
    latest: latest.tag_name,
    releases: releases
      .filter((r) => !r.draft && /^v\d+\.\d+\.\d+$/.test(r.tag_name))
      .map((r) => ({
        tag_name: r.tag_name, name: r.name, prerelease: r.prerelease, published_at: r.published_at, html_url: r.html_url,
        body: r.body || "",
        assets: (r.assets || []).map((a) => ({ name: a.name, size: a.size, browser_download_url: a.browser_download_url })),
      })),
    issues: Object.fromEntries(Object.entries(issues).map(([k, list]) => [k, list.map((i) => ({
      number: i.number, title: i.title, state: i.state, comments: i.comments, updated_at: i.updated_at, html_url: i.html_url,
      user: { login: i.user?.login }, labels: (i.labels || []).map((l) => ({ name: l.name, color: l.color })),
    }))])),
  };
}

export async function loadGithub(cacheDir) {
  fs.mkdirSync(cacheDir, { recursive: true });
  const cache = path.join(cacheDir, "github.json");
  try {
    const raw = await fetchRaw();
    fs.writeFileSync(cache, JSON.stringify(raw));
    return raw;
  } catch (e) {
    if (fs.existsSync(cache)) {
      console.warn(`  ! GitHub: ${e.message}; using the cached copy`);
      return JSON.parse(fs.readFileSync(cache, "utf8"));
    }
    console.warn(`  ! GitHub: ${e.message}; the pages will ask GitHub themselves`);
    return null;
  }
}

// Release notes are Markdown written by the maintainers; they get the same care as anything else
// that ends up on the page (the Content-Security-Policy stops scripts anyway).
export function cleanHtml(html) {
  return html
    .replace(/<\s*(script|iframe|object|embed|style|link|meta|base|form)\b[\s\S]*?(<\/\s*\1\s*>|$)/gi, "")
    .replace(/<\s*(script|iframe|object|embed|style|link|meta|base|form)\b[^>]*>/gi, "")
    .replace(/\son\w+\s*=\s*("[^"]*"|'[^']*'|[^\s>]+)/gi, "")
    .replace(/\s(href|src)\s*=\s*("\s*javascript:[^"]*"|'\s*javascript:[^']*')/gi, "")
    .replace(/<a\s/gi, '<a rel="noopener" ');
}
