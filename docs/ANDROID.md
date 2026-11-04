# openOMSI on Android

openOMSI runs on Android phones and tablets. It is the same game as on the computer: same
renderer, simulation, scripts, maps, buses and mods. Only the controls differ.

## Requirements

- arm64, Android 8.0 or newer.
- A GPU with Vulkan 1.1. The app declares Vulkan 1.1 as required, so app stores do not offer it
  on phones without it. On such a phone only a sideloaded APK installs; openOMSI then tries
  OpenGL ES 3.
- An OMSI 2 installation from a PC.

## Installing the game on the phone

1. Install the APK. Allow installing from this source when Android asks.
2. Start openOMSI. Allow **access to all files** when Android asks (Settings → Apps →
   openOMSI → Permissions → Files and media → Allow management of all files). The game reads
   OMSI 2's thousands of files by path. Without this permission it can read only the app's
   own folder.
3. Copy the **whole OMSI 2 folder** (the one with `Omsi.exe`, `maps`, `Vehicles`) onto the
   phone, for example to `openOMSI/OMSI 2` in the internal storage. Use a USB cable from a
   PC, or a USB stick.
4. openOMSI finds the folder by itself in `openOMSI/`, `Download/`, `Games/` or the top of the
   internal storage (`/storage/emulated/0` or `/sdcard`), either as that folder itself or in a
   folder inside it named `OMSI 2`, `OMSI 2 Original`, `Omsi 2`, `OMSI2`, `omsi2`,
   `OMSI 2 Steam Edition` or `OMSI`. Anywhere else, choose it in the launcher:
   **More → Setup → Browse → Use this folder → Save**.
5. Optional, **mods**: copy mod folders or .zip, .7z and .rar files into `openOMSI/Mods`.
   They are installed when the launcher opens. You can also use the launcher's **Mods** page
   (Choose a folder / Choose an archive). .zip archives in `openOMSI/Archives` are used in
   place. Maps and buses work exactly as on the computer.

## How the app works

- **One app, one window.** The launcher and the game share a window. Press Start in the
  launcher to play. Quit (menu, or back key then Quit) returns to the launcher, kept as you
  left it.
- **Launcher for fingers.** Text at least the system's size, settings in one column. A
  finger on a list at its end scrolls the page on.
- **Files.** Settings, profiles and sessions are in the app's private folder. Screenshots go
  to `openOMSI/Screenshots`.

## Controls

All on-screen buttons use Material Symbols.

| Control | Action |
|---|---|
| Steering wheel (bottom left) | Take the rim and turn it; it follows the finger. Full lock is half of *Full lock at* (Settings → Driving) when that is set, else half of *Wheel rotation*: 450° each way by default. Let go and it returns as the bus's wheel does with the keys: slowly when standing, faster when rolling, not at all with Old Steering. |
| Tilt steering | Tilt the phone to steer (cab panel → Tilt steering). |
| Pedals (bottom right) | Brake and accelerator. The higher up the pedal you press, the harder. |
| Above the pedals | Gearbox (R N D for an automatic, − N + for a manual), one button per door, front to back. |
| Beside the pedals | Parking brake, stop brake / door release. |
| Above the wheel | Indicator left, hazard lights, indicator right. The horn is beside the wheel. |
| Top left | Game menu, pause, camera (driver → outside → passenger), look straight ahead. |
| Top right | Hide / show controls, cab panel, city map, timetable, screenshot. |
| Cab panel (tune button) | Battery, engine start (hold), start the bus by itself, headlights, high beam, wipers, saloon lights, sell a ticket, next seat view, free camera, navigator, information bar, tilt steering. |
| Tap in the cab | Works the switch under the finger (IBIS, ticket printer, light switches). |
| Drag from a switch | Turns it (knobs, ignition key, sun blind). |
| Drag elsewhere / two fingers | Look around / zoom. |
| Navigator (top middle, under the information bar when that is on) | Tap: open the city map. Drag: move it; it stays where you put it. |
| City map | Fingers act as the mouse. |
| Game menu and lists | Scroll with the finger, tap to pick a line. A finger put down to scroll does not pick the line it lands on. |
| Back key | Escape. |
| On foot: stick (bottom left) | Walk. Push to the edge to run. |
| On foot: button (bottom right) | Kneel for a picture from low down, and stand up again. |
| Free camera: stick | Fly, as far as the stick is pushed (cab panel → Free camera). |
| Free camera: buttons (bottom right) | Rise, sink, set the speed. |
| Free camera: drag / two fingers | Look / zoom. "Exit free camera" goes back. |

A game controller connected by Bluetooth works as on the computer.

The buttons fire the actions of `Inputs/keyboard.cfg` (or their keys, for example Shift + n
for the n-th door). The wheel and pedals act as game controller analog axes. So they drive a
mod bus as the keyboard does.

## Settings

The first start on a phone uses lighter graphics defaults. Change them all on the launcher's
Settings page.

| Setting | Phone default |
|---|---|
| MSAA | 2x |
| Ambient occlusion | Off |
| Shadow map | 1024 |
| Frame rate | 60 fps |
| Object distance | 900 m |

When the frame rate stays below 40 because the graphics chip is the limit, the 3D picture is
drawn smaller in steps (85 %, 70 %, 55 % of the screen), as on the computer.

`openOMSI/env.txt` on the shared storage takes the `OMSI_*` switches a computer takes from its
environment, one `NAME=value` per line.

## Updates

1. The launcher looks for a newer GitHub release when it starts and offers it. Choose the
   update channel (Stable or Beta) under Settings → General → Updates.
2. Press **Update now**. The APK is downloaded and handed to Android's package installer.
3. The first time, Android asks you to allow openOMSI to install apps.
4. Android asks "Do you want to update this app?". **Update** replaces openOMSI and starts it
   again. **Cancel** returns to the launcher as "not updated".

The APK must be signed with the same key as the installed app (release builds are).

To try an update from a local release description, put
`OMSI_UPDATE_URL=file:///sdcard/…/release.json` in `openOMSI/env.txt`.

## Troubleshooting

- **Game finds no files:** allow access to all files, or pick the folder under **Setup → Browse** (install steps 2 and 4).
- **Textures show up as photos in the gallery:** every folder in `openOMSI/` except
  `Screenshots` (and the OMSI 2 installation, wherever it is) gets an empty `.nomedia` file to
  hide them. A gallery that listed them before may need a moment, or a phone restart, to forget
  them. Do not delete them there: they are game content, and deleting them leaves buses white.
- **Logs:** `adb logcat -s openOMSI`.

## For developers

### Building

```sh
rustup target add aarch64-linux-android
# the Android SDK (command line tools), e.g. with Homebrew on a Mac:
brew install --cask android-commandlinetools
sdkmanager "platforms;android-35" "build-tools;35.0.0" "ndk;28.2.13676358" "platform-tools"
scripts/build-android.sh            # → dist/android/openOMSI-<version>.apk
scripts/build-android.sh install    # ... and adb install it on the attached phone
```

- `android/env.sh` finds the SDK (`ANDROID_HOME`, else Homebrew's), the newest NDK and a JDK 17.
- The script builds the game library as a shared object
  (`cargo rustc --lib --crate-type cdylib --profile android`).
- It compiles the two Java classes in `android/java` with javac and d8:
  `OmsiActivity` (full screen, storage permission, vibration, installing updates) and
  `LauncherBridge` (the launcher's web view).
- It packs, aligns and signs the APK with a debug key made on the first build
  (`android/debug.keystore`, not in git).

### Where the code is

- One window, launcher and game: `crates/omsi-app/src/android.rs`.
- Launcher: the desktop launcher's page (`launcher/ui`, phone layout `js/mobile`) in a web
  view over the game's window (`LauncherBridge.java`, `crates/omsi-app/src/android_launcher.rs`).
  Its commands are answered by the same code as on a computer (`omsi-launcher-api`). The game
  draws the bus preview and the city map in the same process.
- On-screen controls: `crates/omsi-app/src/touch.rs`.

### Testing the touch controls on a computer

- `OMSI_TOUCH=1` shows the touch controls in the game window.
- `OMSI_INPUT` drives them with `touch down|move|up x,y[,finger]` (logical pixels). This
  presses the accelerator for two seconds and writes the picture with the controls on it:
  `OMSI_TOUCH=1 OMSI_INPUT="t=8 touch down 1500,700; t=10 touch up 1500,700; t=11 shot touch.png"`
- `OMSI_MOBILE=1` gives the game the phone's screen cut-outs.
- Phone-sized picture with no window on the screen:
  `OMSI_HIDDEN_WINDOW=1 OMSI_BACKGROUND=1 OMSI_TOUCH=1 openomsi --size 360x800 --exit-after 12`
  with `OMSI_INPUT="t=7 shot phone.png"`. The picture is the window's, with the controls on
  it. Sizes to check: 360x800, 412x915, 820x1180 and 800x360.
- Free camera: cab panel (tune button) → Free camera, then the stick and the buttons at the
  bottom. Photo mode: `OMSI_INPUT="t=7 photo"`, then touch the buttons.
- The bus preview's cost per picture (drawing, read-back, JPEG) is the `ms` in the answer to
  the launcher service's `frame` command: `openomsi --launcher-service`, one JSON request per
  line.

### Releases

Every release (started by hand from main) builds the APK in GitHub Actions (`android` job of
`.github/workflows/release.yml`). It is attached to the beta as
`openOMSI-<version>-android-arm64.apk` and as `openOMSI-android.apk`. So
`releases/latest/download/openOMSI-android.apk` is always the newest stable APK.

Set the repository secret `ANDROID_KEYSTORE_B64` (`base64 < android/debug.keystore`). Then
every release is signed with the same key and installs over the previous one.
