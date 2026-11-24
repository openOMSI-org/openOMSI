// The frame every page shares: head, header, footer. No icon font, no images in the chrome but
// the wordmark; the little glyphs are drawn in CSS.

import { DISCORD, DONATE, REPO, SITE_URL } from "./config.mjs";
import { esc } from "./md.mjs";

export const BASE = (process.env.SITE_BASE || "/").replace(/\/?$/, "/");
export const u = (p) => BASE + String(p).replace(/^\//, "");

export function pageShell({ title, description, path: pagePath, body, bodyClass = "", head = "", scripts = [], assetVersion = "0", noindex = false }) {
  const full = title ? `${title} · openOMSI` : "openOMSI: OMSI 2 recreated from scratch in Rust";
  const desc = description || "openOMSI is OMSI 2 recreated from scratch in Rust: your maps, buses and mods on a new 64-bit engine for Windows, macOS, Linux and Android.";
  const canonical = SITE_URL + (pagePath || "/");
  const v = `?v=${assetVersion}`;
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'self'; script-src 'self'; style-src 'self'; font-src 'self'; img-src 'self' https: data:; connect-src 'self' https://api.github.com https://raw.githubusercontent.com https://openomsi.savvabestbrother.workers.dev; object-src 'none'; base-uri 'self'; form-action 'none'">
<title>${esc(full)}</title>
<meta name="description" content="${esc(desc)}">
<link rel="canonical" href="${esc(canonical)}">
${noindex ? '<meta name="robots" content="noindex">\n' : ""}<meta property="og:title" content="${esc(full)}">
<meta property="og:description" content="${esc(desc)}">
<meta property="og:type" content="website">
<meta property="og:url" content="${esc(canonical)}">
<meta property="og:image" content="${SITE_URL}/assets/icon.png">
<meta name="twitter:card" content="summary">
<link rel="icon" type="image/svg+xml" href="${u("assets/icon.svg")}">
<link rel="apple-touch-icon" href="${u("assets/icon.png")}">
<meta name="color-scheme" content="dark light">
<meta name="theme-color" content="#121212" media="(prefers-color-scheme: dark)">
<meta name="theme-color" content="#f4f5f7" media="(prefers-color-scheme: light)">
<link rel="preload" href="${u("assets/fonts/Inter-latin.woff2")}" as="font" type="font/woff2" crossorigin>
<script src="${u("assets/js/theme.js")}${v}"></script>
<link rel="stylesheet" href="${u("assets/css/site.css")}${v}">
${head}</head>
<body class="${esc(bodyClass)}" data-base="${esc(BASE)}">
<a class="skip" href="#main">Skip to content</a>
${header(pagePath)}
${body}
${footer()}
<script src="${u("assets/js/site.js")}${v}" defer></script>
${scripts.map((s) => `<script src="${u(`assets/js/${s}`)}${v}" defer></script>`).join("\n")}
</body>
</html>
`;
}

const NAV = [
  { href: "docs/", label: "Docs", match: "/docs/" },
  { href: "download/", label: "Download", match: "/download/" },
  { href: "releases/", label: "Releases", match: "/releases/" },
  { href: "issues/", label: "Issues", match: "/issues/" },
];

function header(pagePath = "/") {
  const links = NAV.map((n) => `<a href="${u(n.href)}"${pagePath.startsWith(n.match) ? ' aria-current="page"' : ""}>${n.label}</a>`).join("");
  return `<header class="topbar">
  <div class="topbar-in">
    <a class="brand" href="${u("")}" aria-label="openOMSI, home"><img class="on-dark" src="${u("assets/logos/openomsi-wordmark-light.svg")}" alt="openOMSI" height="22"><img class="on-light" src="${u("assets/logos/openomsi-wordmark-dark.svg")}" alt="" height="22"></a>
    <nav class="topnav" aria-label="Main">${links}</nav>
    <span class="grow"></span>
    <button class="search-btn" type="button" data-search-open aria-label="Search the documentation" hidden><svg class="search-ico" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><circle cx="7" cy="7" r="4.6" fill="none" stroke="currentColor" stroke-width="1.7"/><path d="M10.5 10.5 14 14" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"/></svg><span>Search docs</span><kbd>/</kbd></button>
    <a class="toplink" href="https://github.com/${REPO}">GitHub</a>
    <a class="toplink" href="${DISCORD}">Discord</a>
    <button class="theme-btn" type="button" data-theme-cycle aria-label="Colour theme"><span class="theme-dot" aria-hidden="true"></span></button>
  </div>
</header>`;
}

function footer() {
  return `<footer class="footer">
  <div class="footer-in">
    <div class="footer-cols">
      <div>
        <h2>openOMSI</h2>
        <ul>
          <li><a href="${u("download/")}">Download</a></li>
          <li><a href="${u("releases/")}">Releases</a></li>
          <li><a href="${u("issues/")}">Issues</a></li>
          <li><a href="https://github.com/${REPO}">Source code</a></li>
        </ul>
      </div>
      <div>
        <h2>Documentation</h2>
        <ul>
          <li><a href="${u("docs/user-guide/")}">User guide</a></li>
          <li><a href="${u("docs/modding/")}">Modding</a></li>
          <li><a href="${u("docs/plugins/")}">Plugins</a></li>
          <li><a href="${u("docs/formats/")}">File formats</a></li>
        </ul>
      </div>
      <div>
        <h2>Family</h2>
        <ul>
          <li><a href="${u("docs/openmapper/")}">openMapper</a></li>
          <li><a href="https://github.com/openOMSI-org/openOMSI-Plugins">Plugin catalog</a></li>
          <li><a href="https://github.com/openOMSI-org/openOMSI-Development-Tools">Development Tools</a></li>
        </ul>
      </div>
      <div>
        <h2>Community</h2>
        <ul>
          <li><a href="${DISCORD}">Discord</a></li>
          ${DONATE.map((d) => `<li><a href="${d.url}" rel="noopener">${esc(d.name)}</a></li>`).join("\n          ")}
        </ul>
      </div>
    </div>
    <div class="footer-base">
      <p>An independent project, not affiliated with the makers of OMSI&nbsp;2. OMSI&nbsp;2 is required to play.
      <a href="https://github.com/${REPO}/blob/main/LICENSE">GNU AGPL v3</a> · <a href="${u("privacy/")}">Privacy</a></p>
      <div class="theme-seg" role="group" aria-label="Colour theme">
        <button type="button" data-theme-set="">System</button><button type="button" data-theme-set="light">Light</button><button type="button" data-theme-set="dark">Dark</button>
      </div>
    </div>
  </div>
</footer>
<dialog class="search" id="search" aria-label="Search the documentation">
  <div class="search-form">
    <input type="search" id="search-input" placeholder="Search the documentation" autocomplete="off" spellcheck="false" aria-label="Search the documentation">
    <button class="search-close" type="button" aria-label="Close search"><kbd>Esc</kbd></button>
  </div>
  <div class="search-results" id="search-results" role="listbox"></div>
  <div class="search-foot"><span><kbd>↑</kbd><kbd>↓</kbd> to move</span><span><kbd>↵</kbd> to open</span></div>
</dialog>`;
}
