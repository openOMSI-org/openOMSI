# The desktop launcher

openOMSI's launcher for Windows, macOS and Linux: a [Tauri 2](https://tauri.app) window whose
page (`ui/`) puts the duty together - map, bus, line and tour or a free drive, start time,
date, season, weather and traffic - and keeps the driver's profile, the settings, the key
bindings, the mods and the running games. Android shows the same page in a web view over the
game's window; iPhone, iPad and the Mac app have a SwiftUI launcher (`apple/`) over the
window-free model `crates/omsi-launcher-model`. The game has no launcher window of its own.

* `src-tauri/` - the program. Its commands (`src/commands.rs`) call `omsi-launcher-core`
  directly, the same functions `openomsi-launcher --cli` offers a terminal; `src/protocol.rs`
  serves the content's pictures to the page as `omsi://localhost/…`; `src/service.rs` keeps
  the game program running beside it as a helper with no window (`openomsi
  --launcher-service`, `crates/omsi-app/src/launcher/service.rs`), which draws the bus preview
  with the game's own renderer - the game's showroom, sent as JPEG pictures while it turns -
  and reads the city map with a trip's route the way the
  navigator does (`mapview::export`), which the page draws;
  `src/i18n.rs` hands the page the game's translation tables
  (`crates/omsi-app/locales/app.yml`, the English text is the key).
* `ui/` - the page: Preact with htm from one vendored file, no build step and no npm
  (`ui/vendor/README.md` lists the third-party files). `js/pages/*.js` are the pages,
  `js/ui.js` the controls, `js/data.js` the duty and the content, `styles.css` the look.
* Tauri comes from the organisation's fork,
  [openOMSI-org/tauri-openomsi](https://github.com/openOMSI-org/tauri-openomsi) (branch
  `openomsi/v2`, see its `OPENOMSI.md`): the webview behaves as an application rather than a
  browser tab (no reload, print, find or history keys, no browser menu), the window is shown
  with its first page instead of white, and WebKitGTK's blank window on NVIDIA is avoided.

## How the game and the launcher find each other

A plain start of `openomsi` (a double click) opens `openomsi-launcher-app` beside it; where it
is missing, or its window cannot open (no WebView2 on an old Windows - the game offers
Microsoft's installer -, no WebKitGTK 4.1 on Linux), the game says so and ends. A development
build finds it when the launcher is built into the game's target folder
(`CARGO_TARGET_DIR=target cargo build --manifest-path launcher/src-tauri/Cargo.toml`). The
launcher starts each duty as `openomsi` with a full command line, and rests out of sight
while a game it started runs when the setting "The launcher rests while a game runs" is on. The duty chosen is kept in
`~/.openomsi/launcher-duty.json`, which the Swift launcher reads as well.

## Building

```bash
cargo build --release --manifest-path launcher/src-tauri/Cargo.toml
```

On Linux the webview needs `libwebkit2gtk-4.1-dev` (and `libgtk-3-dev`, `librsvg2-dev`).
`src-tauri/Cargo.toml` patches Tauri with a checkout of the fork beside this repository
(`../tauri-openomsi`); point the `[patch.crates-io]` entries at the fork's git branch to build
without one.

## Checking it

* **Without a window on the screen**: `OPENOMSI_LAUNCHER_SELFTEST=1 openomsi-launcher-app`
  keeps the window hidden (and out of the Dock), lets the page check itself - the commands,
  the content lists, a map picture through `omsi://`, the Inter font, every page drawn without
  a script error - prints the report as JSON and exits with 0 when everything passed.
  `OPENOMSI_LAUNCHER_SELFTEST=full` also has the game program draw a bus preview and read a
  route map. Run it with `HOME` set to a folder of its own (with `.openomsi/settings.cfg`,
  `.openomsi/launcher.json` and `.openomsi-root` copied in): the page saves the duty it shows.
* **The layout in a browser**: serve `launcher/` (`python3 -m http.server -d launcher`) and open
  `/ui/`; outside Tauri the commands answer from `launcher/mock/<command>.json` (not in the
  repository). Most can be made with the terminal commands, e.g.
  `openomsi-launcher-app --cli maps > launcher/mock/maps.json` (also `vehicles`, `weather`,
  `profiles`, `settings` → `settings_get.json`, `keybindings` → `keybindings_get.json`,
  `lines '{"map":"maps/Grundorf/global.cfg","date":"1989-05-30"}'`); `bootstrap.json` is
  written by hand. In the browser console, `missingTexts()` lists the texts the translation
  tables lack in the language shown.
* **Every Drive page layout, checked**: `node launcher/tests/drive-check.mjs` (Node 22, or 20
  with `--experimental-websocket`; Chrome from `CHROME`, else `google-chrome`) serves the page
  with the small test data of `launcher/tests/fixtures` and, in a headless Chrome, for every
  layout in both themes and at a phone's width up to an ultrawide screen
  (`DRIVE_CHECK_SIZES=400x820,1280x720`), looks for every option of the list in
  `launcher/tests/drive-check.js`, opens every dialog and menu and clicks the lists; it says
  what is missing, covered, cut off or not opening, and fails then. A pull request that changes
  the launcher's page runs it (`.github/workflows/pr_checks.yml`). A new option of the Drive
  page goes into that list, so that no layout can lack it.

### Multiplayer interface regression checks

Serve `launcher/` locally, then open `/tests/multiplayer-check.html` and press **Run multiplayer
checks**. These checks render the real page with local command fixtures; they cover code
validation, full and unavailable sessions, continuing while status is pending, stale queries,
the server list, code-only and address dialogs, cancellation restoring the duty, and host setup.
They do not start a game, query a remote server, or change a real saved duty.
