# openOMSI on iPhone, iPad and Mac

openOMSI runs on iPhone, iPad and Mac with a launcher made for Apple's systems. This page covers getting OMSI 2 onto the device, where files go, how the launcher is laid out, and what is still missing. The developer part at the end explains how the apps are built and released.

## Requirements

| Device | Needed |
|---|---|
| iPhone, iPad | iOS / iPadOS 18 or newer |
| Mac | macOS 15 or newer |
| All | A complete copy of a PC installation of OMSI 2 |

One App ID covers iPhone, iPad and Mac, so one purchase covers all three.

## Put OMSI 2 on an iPhone or iPad

The app's Documents folder is the content folder. The Files app shows it as **openOMSI**. On first start the launcher walks you through these ways:

**From a Mac**

1. Connect the iPhone or iPad to the Mac with a cable.
2. In the Finder, select the device in the sidebar, then **Files**.
3. Drag the OMSI 2 folder onto **openOMSI**.

**From a Windows PC**

1. Install **Apple Devices** from the Microsoft Store (or iTunes).
2. Connect the device with a cable and open **Files** (File Sharing in iTunes).
3. Drag the OMSI 2 folder onto **openOMSI**.

**From a USB drive or iCloud Drive**

1. Copy the OMSI 2 folder onto the drive and plug it in.
2. In the launcher, choose the folder on the drive (**Setup**). The access is remembered for later starts.
3. Or copy the folder into **openOMSI** in the Files app; then the drive is no longer needed.

The folder to copy is the one that holds `Omsi.exe`, `maps` and `Vehicles`. On Steam: right-click OMSI 2, then **Manage → Browse local files**. Name the copy `OMSI 2`. The launcher finds it by itself once the copy is complete, and warns if the folder is not a complete installation.

## Files

| What | iPhone / iPad | Mac |
|---|---|---|
| OMSI 2, mods, screenshots | The app's Documents ("openOMSI" in Files) | The content folder beside the app, or `~/.openomsi/content` |
| Settings, profiles, sessions | `Library/Application Support/openOMSI` inside the app | `~/.openomsi` |
| `game.log` | `Library/Application Support/openOMSI` | `~/.openomsi` |
| `crash.log` | The app's Documents | `~/.openomsi` |
| Testing switches | `env.txt` in the content folder, one `OMSI_*=value` per line | Environment variables |

See [Environment variables](DEBUG_FLAGS.md) for the `OMSI_*` switches.

## The launcher

The launcher has the look and the pages of the desktop launcher ([Launcher and settings](LAUNCHER.md)). Its layout depends on the window's size, so Split View, Stage Manager and a folding phone change it as they change.

**Mac, iPad and wide windows.** A sidebar with Drive, Depot, Multiplayer, Profile, Mods, Timetable, Settings and Controls; then Running games, Tutorials and Setup; your driver at the foot. Below 1040 points wide, the sidebar shows icons only.

The Drive page has the **Map**, **Bus**, **Duty**, **Time and weather** and **On the road** cards beside the summary (destination sign, **Start the duty**) and **Continue**. The map and bus pickers open as sheets.

**Phone (narrower than 700 points or lower than 500).** A rail of icons: Drive, Multiplayer, Profile, Mods, Settings and More (where Depot and Timetable are). Drive shows the bus with its destination sign and **Start** over it. The duty is a set of grouped rows; each opens a full-screen sheet:

- maps, buses and paints
- lines, tours, trips and start places, beside the route map
- time and date, set by steps
- weather

## Playing

- **iPhone and iPad:** the launcher and the game share one app. Press **Start** and the game opens in front of the launcher. **Quit** in the game menu returns to the launcher. Hold the device in landscape. The touch controls are the same as on Android: see [Touch controls](ANDROID.md#touch-controls).
- **Mac:** the app is the launcher. Every drive runs as a separate game program, as on Windows and Linux. Keyboard, mouse and game controllers work as described in [Driving](DRIVING.md).

On Apple silicon Macs, the Graphics settings offer Apple MetalFX upscaling and MetalFX frame generation. See [Graphics](GRAPHICS.md).

## Updates

- **iPhone and iPad:** updates come through the store. The app never looks for updates itself.
- **Mac App Store build:** the same; the store updates it.
- **Mac, downloaded from the releases:** **Settings → General → Updates** sets the update check and the channel (Stable or Beta).

## Known gaps

- **Mac App Store build:** it runs in the sandbox, and the game program inherits it. The OMSI 2 folder chosen in the launcher must be passed on to the game program (as a bookmark it resolves itself). This is not done yet.
- **Mac, downloaded build:** the launcher has update settings but no update dialog yet.

## For developers

### How it is put together

- **The game** is the same Rust library as on every system (`crates/omsi-app`, `openomsi_game`). It is built as a static library for `aarch64-apple-ios`, `aarch64-apple-ios-sim` and the Mac (`scripts/build-apple.sh rust …`), and linked into a Swift app (`apple/`).
- **iPhone and iPad** (`crates/omsi-app/src/ios.rs`): as on Android, the launcher and the game share one process.
  - `main.swift` calls `openomsi_ios_main`, which runs winit's event loop (UIKit's `UIApplicationMain`).
  - The app keeps UIKit's scene life cycle. Its scene delegate (`apple/iOS/SceneDelegate.swift`) shows the launcher in the scene's window.
  - For a drive, the game's window is put on the same scene in front of it (`apple.rs` ↔ `apple/iOS/GameHost.swift`). When the session ends, that window goes and the launcher is back.
- **Mac:** the app is the launcher (`apple/macOS/OpenOMSIApp.swift`). The game program `openomsi` lies beside it in `Contents/MacOS` and runs every drive as its own process. The app's own program is `openOMSI-Mac`, because a Mac's disk does not tell `openOMSI` from `openomsi`.
- **The launcher** (`apple/Shared`) is SwiftUI in the look and structure of the desktop launcher (`launcher/ui`):
  - neutral graphite surfaces without outlines, one amber accent;
  - Inter for all text (`apple/Resources/Fonts`, with its licence);
  - radius 8 for cards and 6 for controls;
  - Liquid Glass only where the system draws it (the tab bar).
- The Swift launcher is only a view. What it knows and does is the game launcher's own `launcher::state::State`, read and changed through `apple_bridge.rs` (`openomsi_bridge(cmd, json)`). Its `api` command answers every command of the desktop launcher's page (`omsi-launcher-api`), as Android's web view gets them.
- The bus preview is drawn by the game's renderer (the launcher's showroom, off screen). The Drive page's map uses the same data as the game's city map (`mapview::overview`).
- The phone layout follows the phone launcher of the other systems (`launcher/ui/js/mobile`).
- Its texts are translated through the string catalog `apple/Resources/Localizable.xcstrings`.
- The key bindings page (`apple/Shared/Pages/KeysPage.swift`) edits the same `keyboard.cfg` as the other launchers.

### Building

Needs Xcode 26 (with the iOS platform), Rust and XcodeGen (`brew install xcodegen`).

```bash
scripts/build-apple.sh sim                 # iOS Simulator: build, install, start
scripts/build-apple.sh mac                 # dist/apple/openOMSI.app
scripts/build-apple.sh project             # apple/openOMSI.xcodeproj to work in Xcode
OPENOMSI_TEAM_ID=XXXXXXXXXX scripts/build-apple.sh ipa        # App Store .ipa
OPENOMSI_TEAM_ID=XXXXXXXXXX scripts/build-apple.sh mac-store  # Mac App Store
scripts/build-apple.sh shots <dir> 1440x880 drive settings    # the launcher's pages as PNG
```

The Xcode project builds the Rust library itself before the Swift sources, through a build phase that calls `scripts/build-apple.sh rust-for-xcode`.

### Releasing (one-time setup)

1. **Apple Developer:** create the App ID `io.github.turbo-devv.openomsi` (one for iPhone, iPad and Mac). Give it the capabilities *Increased Memory Limit* and *Extended Virtual Addressing*. For LAN sessions found by broadcast, also *Multicast Networking* (Apple grants it on request).
2. **App Store Connect:** create the app record (iOS and macOS), and an API key for the CI (**Users and Access → Integrations**, role App Manager).
3. **GitHub:** add the secrets `APPLE_TEAM_ID`, `ASC_KEY_ID`, `ASC_ISSUER_ID` and `ASC_KEY_P8_BASE64`.
4. Run the *Apple* workflow (`.github/workflows/apple.yml`, started by hand). It builds the .ipa, the notarised Mac app and the Mac App Store package, and uploads them with `upload`.

App Store builds never look for or install updates (`updater::STORE_BUILD`); iOS builds always count as store builds. The Mac App Store game program uses `openomsi-game-AppStore.entitlements`.
