# openOMSI on iPhone, iPad and Mac

## How it is put together

- **The game** is the same Rust library as everywhere (`crates/omsi-app`, `openomsi_game`),
  built as a static library for `aarch64-apple-ios`, `aarch64-apple-ios-sim` and the Mac
  (`scripts/build-apple.sh rust …`) and linked into a Swift app (`apple/`).
- **iPhone and iPad** (`crates/omsi-app/src/ios.rs`): as on Android, the launcher and the
  game share one process. `main.swift` calls `openomsi_ios_main`, which runs winit's event
  loop (that is UIKit's `UIApplicationMain`). The app keeps UIKit's scene life cycle: its
  scene delegate (`apple/iOS/SceneDelegate.swift`) shows the launcher in the scene's window,
  and for a drive the game's window is put on the same scene in front of it
  (`apple.rs` ↔ `apple/iOS/GameHost.swift`). When the session ends (the menu's Quit) the
  game's window goes and the launcher is back.
- **Mac**: the app is the launcher (`apple/macOS/OpenOMSIApp.swift`); the game program
  `openomsi` lies beside it in `Contents/MacOS` and runs every drive as a process of its own,
  as the launcher starts it on the other computers. The app's own program is `openOMSI-Mac`:
  a Mac's disk does not tell `openOMSI` from `openomsi`, and the game took its place.
- **The launcher** (`apple/Shared`) is SwiftUI in the look and structure of the desktop
  launcher (`launcher/ui`): neutral graphite surfaces without outlines, one amber accent,
  Inter for every text (`apple/Resources/Fonts`, with its licence), radius 8 for cards and 6
  for controls; Liquid Glass only where the system draws it (the tab bar). It is only a view: what it knows and does is the game
  launcher's own `launcher::state::State`, read and changed through `apple_bridge.rs`
  (`openomsi_bridge(cmd, json)`). The bus preview is drawn by the game's renderer (the
  launcher's showroom, off the screen), the Drive page's map from the same data the game's
  city map uses (`mapview::overview`).
- **Layouts**: the desktop launcher's sidebar (Drive, Multiplayer, Profile, Mods, Settings,
  Controls; Running games, Tutorials, Setup; the driver at the foot) on a Mac and an iPad,
  icons only below 1040 points. A phone has a launcher of its own, as the phone launcher
  of the other systems (`launcher/ui/js/mobile`): a rail of icons (Drive, Multiplayer,
  Profile, Mods, Settings, More); Drive is the bus with the destination sign and Start over
  it and the duty as grouped rows, each opening a full-screen sheet (maps, buses, paints,
  lines, tours, trips and start places beside the route map, time and date by steps,
  weather). The desktop Drive page: the
  Map, Bus, Duty, Time and weather and On the road cards beside the Summary (destination
  sign, Start the duty) and Continue; the map and bus pickers are sheets. The window's size decides, so Split View, Stage Manager and a
  phone that folds change it as they change.
- **Cross-Platform Launcher** (Settings → General, and Setup): the launcher drawn by the game
  instead - on a phone laid out as on Android, on a Mac as on Windows and Linux. Its own
  Settings → General switches back. The setting is `cross_platform_launcher` in
  `settings.cfg`.

## Files on the device

- iPhone/iPad: the app's Documents are the content folder - the Files app shows them as
  "openOMSI" (also reachable from a Mac's Finder over a cable). Put a complete OMSI 2 there
  as `OMSI 2`, or choose a copy on iCloud Drive or a USB drive under Setup (the access is kept
  as a bookmark). Settings, profiles and logs are in `Library/Application Support/openOMSI`.
  `game.log` and `crash.log` as on Android; `openOMSI/env.txt` takes `OMSI_*` switches.
- Mac: as before (`~/.openomsi`, the content folder beside the app or `~/.openomsi/content`).

## Building

Needs Xcode 26 (with the iOS platform), Rust and XcodeGen (`brew install xcodegen`).

```sh
scripts/build-apple.sh sim                 # iOS Simulator: build, install, start
scripts/build-apple.sh mac                 # dist/apple/openOMSI.app
scripts/build-apple.sh project             # apple/openOMSI.xcodeproj to work in Xcode
OPENOMSI_TEAM_ID=XXXXXXXXXX scripts/build-apple.sh ipa        # App Store .ipa
OPENOMSI_TEAM_ID=XXXXXXXXXX scripts/build-apple.sh mac-store  # Mac App Store
scripts/build-apple.sh shots <dir> 1440x880 drive settings    # the launcher's pages as PNG
```

The Xcode project builds the Rust library itself before the Swift sources (a build phase
calling `scripts/build-apple.sh rust-for-xcode`).

## Releasing (to do once)

1. Apple Developer: an App ID `io.github.turbo-devv.openomsi` (one for iPhone, iPad and Mac:
   one purchase), with the capabilities *Increased Memory Limit* and *Extended Virtual
   Addressing*; for LAN sessions found by broadcast also *Multicast Networking* (Apple grants
   it on request).
2. App Store Connect: the app record (iOS and macOS), and an API key (Users and Access →
   Integrations, role App Manager) for the CI.
3. GitHub secrets `APPLE_TEAM_ID`, `ASC_KEY_ID`, `ASC_ISSUER_ID`, `ASC_KEY_P8_BASE64`; then
   the *Apple* workflow (`.github/workflows/apple.yml`, started by hand) builds the .ipa, the
   notarised Mac app and the Mac App Store package, and uploads them with `upload`.

App Store builds never look for or install updates (`updater::STORE_BUILD`): the store does.

## Open points

- The Mac App Store build runs in the sandbox: the game program inherits it
  (`openomsi-game-AppStore.entitlements`); the OMSI 2 folder chosen in the launcher must be
  passed on to it (a bookmark it resolves itself) - not done yet.
- The Swift launcher has no key-binding editor (the Cross-Platform Launcher's Controls page
  edits the same file) and no update dialog for the Developer ID Mac build yet.
- The Swift launcher's texts are English; the game launcher's translations
  (`crates/omsi-app/locales/app.yml`) are to be turned into a string catalog.
