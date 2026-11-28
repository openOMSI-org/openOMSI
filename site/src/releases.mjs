// Releases, downloads and issues as HTML, written when the site is built from the data src/github.mjs
// read. The pages open complete; static/js/dynamic.js only asks GitHub in the background whether
// something newer is out, and redraws when it is.

import { PLATFORMS, REPO } from "./config.mjs";
import { esc } from "./md.mjs";
import { u } from "./layout.mjs";

export const mb = (n) => `${(n / 1048576).toFixed(0)} MB`;
export const date = (s) => new Date(s).toLocaleDateString("en-US", { year: "numeric", month: "short", day: "numeric", timeZone: "UTC" });
const ver = (tag) => tag.replace(/^v/, "");

export function newer(a, b) {
  const x = ver(a).split(".").map(Number), y = ver(b).split(".").map(Number);
  for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] > y[i];
  return false;
}
export const byVersion = (a, b) => (newer(a.tag_name, b.tag_name) ? -1 : newer(b.tag_name, a.tag_name) ? 1 : 0);

export const latestStable = (gh) => gh?.releases.find((r) => r.tag_name === gh.latest) || null;
export const latestBeta = (gh) => gh?.releases.filter((r) => r.prerelease && newer(r.tag_name, gh.latest)).sort(byVersion)[0] || null;

export function asset(rel, p) {
  const v = ver(rel.tag_name);
  const find = (n) => (rel.assets || []).find((a) => a.name === n);
  const zip = find(`openOMSI-${v}-${p.key}.${p.ext || "zip"}`);
  const setup = p.setup && find(`openOMSI-${v}-${p.key}${p.setup}`);
  return { main: setup || zip, zip: setup && zip ? zip : null };
}

// ---- home: one big button per system (CSS shows the visitor's, see theme.js), the others listed
export function homeDownload(gh) {
  const rel = latestStable(gh);
  if (!rel) return { version: "", button: `<a class="btn primary wide hero-dl" data-for="any" href="${u("download/")}">Download</a>`, info: "", others: "", notes: `<p class="muted">The release notes are on <a href="https://github.com/${REPO}/releases">GitHub</a>.</p>` };
  const v = ver(rel.tag_name);
  const buttons = [`<a class="btn primary wide hero-dl" data-for="any" href="${u("download/")}">Download ${esc(v)}</a>`];
  const others = [];
  for (const p of PLATFORMS) {
    const a = asset(rel, p).main;
    if (!a) continue;
    buttons.push(`<a class="btn primary wide hero-dl" data-for="${esc(p.os)}" href="${esc(a.browser_download_url)}" title="${esc(a.name)}">Download for ${esc(p.name)} (${esc(p.arch)})</a>`);
    others.push(`<li data-os="${esc(p.os)}"><a href="${esc(a.browser_download_url)}">${esc(p.name)} ${esc(p.arch)}</a><span class="num">${mb(a.size)}</span></li>`);
  }
  return {
    version: `Latest release ${esc(v)}`,
    button: buttons.join(""),
    info: `Version ${esc(v)}, ${date(rel.published_at)}`,
    others: others.join(""),
    notes: rel.body_html || `<p class="muted">No notes.</p>`,
  };
}

// ---- download page rows
export function downloadRows(rel, list) {
  return list.map((p) => {
    const a = asset(rel, p);
    const ext = a.main ? a.main.name.match(/\.[a-z0-9]+$/)[0] : "";
    return `<div class="dl-row" data-os="${esc(p.os || "")}"><div class="dl-name">${esc(p.name)}<small>${esc(p.arch)}<span class="yours"> · your system</span></small></div>
      <div class="dl-note">${esc(p.note)}${a.main ? `<span class="num muted">${mb(a.main.size)}</span>` : ""}</div>
      <div class="dl-actions">${a.main ? `<a class="btn dl-main" href="${esc(a.main.browser_download_url)}" download>Download ${esc(ext)}</a>` : `<span class="muted">Not in this release</span>`}${a.zip ? `<a class="btn" href="${esc(a.zip.browser_download_url)}" download>Portable .zip</a>` : ""}</div></div>`;
  }).join("");
}
export function downloadLine(rel, beta) {
  return `${beta ? "Latest beta" : "Stable"} <b>${esc(ver(rel.tag_name))}</b> · ${date(rel.published_at)} · <a href="${u(`releases/${beta ? "beta/" : ""}`)}">release notes</a>`;
}

// ---- the release list: the newest 15 with their notes in the page, older ones on request
export const NOTES_IN_PAGE = 15;
export function releaseList(gh, beta) {
  if (!gh) return `<p>The release list could not be read. <a href="https://github.com/${REPO}/releases">Releases on GitHub</a></p>`;
  const list = gh.releases.filter((r) => r.prerelease === beta).sort(byVersion);
  if (!list.length) return `<p class="muted">No ${beta ? "beta" : "stable"} release has been published yet.</p>`;
  const latest = beta ? list.findIndex((r) => newer(r.tag_name, gh.latest)) : list.findIndex((r) => r.tag_name === gh.latest);
  const items = list.map((r, i) => {
    const inPage = i < NOTES_IN_PAGE;
    return `<details class="rel${i >= NOTES_IN_PAGE ? " rel-old" : ""}" data-tag="${esc(r.tag_name)}"${inPage ? ' data-done="1"' : ""}${i === latest ? " open" : ""}>
      <summary><span class="rel-tag">${esc(ver(r.tag_name))}</span>${i === latest ? `<span class="label ok">${beta ? "Latest beta" : "Latest"}</span>` : ""}${r.prerelease ? `<span class="label warn">Pre-release</span>` : ""}
      <span class="rel-name">${esc(r.name && r.name !== r.tag_name ? r.name : "")}</span><time datetime="${esc(r.published_at)}">${date(r.published_at)}</time></summary>
      <div class="rel-body gh" data-body>${inPage ? r.body_html || `<p class="muted">No notes.</p>` : `<p class="muted">Loading…</p>`}</div>
      <div class="assets" data-assets>${inPage ? (r.assets || []).map((a) => `<a class="asset" href="${esc(a.browser_download_url)}">${esc(a.name)} <small>${mb(a.size)}</small></a>`).join("") : ""}</div></details>`;
  }).join("\n");
  const more = list.length > NOTES_IN_PAGE ? `<button class="btn" id="rel-more" type="button">Older releases (${list.length - NOTES_IN_PAGE})</button>` : "";
  return items + more;
}

// ---- issues: open and closed in the page, the tabs and the filter only hide rows
export function issueRow(i) {
  const labels = (i.labels || []).map((l) => `<span class="label"${/^[0-9a-f]{6}$/i.test(l.color) ? ` data-c="${esc(l.color)}"` : ""}>${esc(l.name)}</span>`).join(" ");
  const q = [i.title, `#${i.number}`, ...(i.labels || []).map((l) => l.name)].join(" ").toLowerCase();
  return `<a class="issue-row" data-state="${esc(i.state)}" data-q="${esc(q)}" data-updated="${esc(i.updated_at)}" href="${u(`issues/?n=${i.number}`)}"><span class="st ${esc(i.state)}" aria-label="${esc(i.state)}"></span>
    <span><span class="issue-title">${esc(i.title)}</span>${labels}<span class="issue-meta">#${i.number} · ${esc(i.user?.login)} · updated ${date(i.updated_at)}</span></span>
    <span class="issue-count">${i.comments ? `${i.comments} comment${i.comments > 1 ? "s" : ""}` : ""}</span></a>`;
}
export function issueList(gh) {
  if (!gh?.issues) return `<p>The issue list could not be read. <a href="https://github.com/${REPO}/issues">Issues on GitHub</a></p>`;
  const all = [...gh.issues.open, ...gh.issues.closed].sort((a, b) => b.updated_at.localeCompare(a.updated_at));
  return `<div class="issue-list">${all.map(issueRow).join("")}</div>`;
}
