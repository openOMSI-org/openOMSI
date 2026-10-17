# openOMSI on Android

openOMSI runs on Android phones and tablets (arm64, Android 8.0 or newer, a GPU with
Vulkan 1.1; on a phone without Vulkan openOMSI tries OpenGL ES 3). It is the same game as on the computer: the same renderer, simulation, scripts,
maps, buses and mods. What differs is how it is controlled:

- **one app, one window**: the launcher and the game share a window. Start is pressed in the
  launcher, the game plays in its window, and ending the session (the menu's Quit, or the
  back key and Quit) goes back to the launcher, which is kept as it was
  (`crates/omsi-app/src/android.rs`);
- **the launcher for fingers**: the desktop launcher's page (`launcher/ui`, its phone layout
  `js/mobile`) in a web view over the game's window (`LauncherBridge.java`,
  `crates/omsi-app/src/android_launcher.rs`), its commands answered by the same code as on a
  computer (`omsi-launcher-api`); the bus preview and the city map are drawn by the game in
  the same process;
- **on-screen controls** in the game (`crates/omsi-app/src/touch.rs`), all Material Symbols:

| Where | What |
|---|---|
| bottom left | the steering wheel: take the rim and turn it round - it follows the finger, 120° of rim is the full lock (let go, it comes back as the bus's wheel does with the keys: slowly standing, brisker rolling, not at all with Old Steering) - or tilt the phone (panel → Tilt steering) |
| bottom right | the brake and the accelerator: the higher up the pedal, the harder |
| above the pedals | the gearbox (R N D of an automatic, − N + of a manual), a button for each door, front to back |
| beside the pedals | the parking brake, the stop brake / door release |
| above the wheel | indicator left, hazard lights, indicator right; the horn beside the wheel |
| top left | the game menu, pause, the camera (driver → outside → passenger), look straight ahead |
| top right | the controls hidden / shown, the cab panel, the city map, the timetable, a screenshot |
| cab panel | battery, engine start (hold), start the bus by itself, headlights, high beam, wipers, saloon lights, sell a ticket, the next seat view, the navigator, the information bar, tilt steering |

Everything else is in the cab itself, as in OMSI: a tap works the switch under the finger (the
IBIS, the ticket printer, the light switches), a finger dragged from a switch turns it (knobs,
the ignition key, the sun blind), a drag elsewhere looks round, two fingers zoom. The
navigator stands in the top middle (under the information bar when that is on): a tap opens the
city map, a finger dragged on it puts it somewhere else, where it stays. On foot and
with the free camera a stick at the bottom left walks (pushed to the edge: runs); on foot the
button at the bottom right kneels, for a picture from low down, and stands up again. The game
menu and its lists scroll with the finger and a tap picks a line (a finger put down to scroll
does not pick the line it lands on); on the city map the fingers are the mouse. The back key
is Escape. The launcher is laid out for the phone: the text at least at the system's own
size, the settings in one column, and a finger on a list that has reached its end scrolls
the page on.
A game controller connected by Bluetooth works as on the computer.

The buttons fire the actions of `Inputs/keyboard.cfg` (or the keys that stand for them, e.g.
Shift + n for the n-th door), and the wheel and pedals are the analog axes a game controller
gives, so a mod bus is driven by them as by the keyboard.

## Installing the game on the phone

1. Install the APK (allow installing from this source when Android asks).
2. Start openOMSI and allow **access to all files** when Android asks (Settings → Apps →
   openOMSI → Permissions → Files and media → Allow management of all files). The game reads
   OMSI 2's thousands of files by path; without it only the app's own folder can be read.
3. Copy the **whole OMSI 2 folder** of a PC installation (the one with `Omsi.exe`, `maps`,
   `Vehicles`) onto the phone, e.g. to `openOMSI/OMSI 2` in the internal storage (by USB
   cable, from a PC or a USB stick). openOMSI finds it by itself in `openOMSI/`,
   `Download/` or the top of the storage; anywhere else choose it in the launcher under
   **Setup → Browse → Use this folder → Save**.
4. **Mods**: copy mod folders or .zip, .7z and .rar files into `openOMSI/Mods` (installed when the launcher
   opens), or install them from the launcher's **Mods** page (Choose a folder / Choose an
   archive); .zip archives can also lie in `openOMSI/Archives` and are used in place. Maps and buses
   work exactly as on the computer.

Settings, profiles and sessions are in the app's private folder; screenshots go to
`openOMSI/Screenshots`. Every other folder in `openOMSI/` (and the OMSI 2 installation, wherever
it lies) gets an empty `.nomedia` file, so that the gallery apps do not list the thousands of
textures as photos - they are the game's content, deleting them leaves buses white. A gallery
that listed them before may need a moment (or a restart of the phone) to forget them. The first start on a phone uses lighter graphics defaults (2x MSAA, no
ambient occlusion, a 1024 shadow map, 60 fps, a 900 m object distance); everything can be
changed on the launcher's Settings page. When the frame rate stays below 40 because the
graphics chip is the limit, the 3D picture is drawn smaller in steps (85 %, 70 %, 55 % of the
screen), as on the computer.

Logs: `adb logcat -s openOMSI`.

## Building

```sh
rustup target add aarch64-linux-android
# the Android SDK (command line tools), e.g. with Homebrew on a Mac:
brew install --cask android-commandlinetools
sdkmanager "platforms;android-35" "build-tools;35.0.0" "ndk;28.2.13676358" "platform-tools"
scripts/build-android.sh            # → dist/android/openOMSI-<version>.apk
scripts/build-android.sh install    # ... and adb install it on the attached phone
```

`android/env.sh` finds the SDK (`ANDROID_HOME`, else Homebrew's), the newest NDK and a JDK
17. The script builds the game library as a shared object (`cargo rustc --lib --crate-type
cdylib --profile android`), compiles the two Java classes in `android/java` (`OmsiActivity`:
full screen, storage permission, vibration, installing updates; `LauncherBridge`: the
launcher's web view) with javac and d8, and packs, aligns and signs the APK
with a debug key made on the first build (`android/debug.keystore`, not in git).

The touch controls can be looked at on the computer: `OMSI_TOUCH=1` shows them in the game
window, and `OMSI_INPUT` drives them with `touch down|move|up x,y[,finger]` (logical pixels),
e.g. `OMSI_TOUCH=1 OMSI_INPUT="t=8 touch down 1500,700; t=10 touch up 1500,700; t=11 shot
touch.png"` presses the accelerator for two seconds and writes the picture with the controls
on it. `OMSI_MOBILE=1` gives the game the phone's screen cut-outs.

## Releases

Every release (started by hand from main) builds the APK in GitHub Actions (`android` job of
`.github/workflows/release.yml`) and attaches it to the beta as
`openOMSI-<version>-android-arm64.apk` and as `openOMSI-android.apk`, so that
`releases/latest/download/openOMSI-android.apk` is always the newest stable APK. Set the repository secret `ANDROID_KEYSTORE_B64`
(`base64 < android/debug.keystore`) so that every release is signed with the same key and
installs over the previous one.

## Updates

The launcher looks for a newer GitHub release when it starts (Settings → General → Updates,
where the update channel, Stable or Beta, is chosen too) and offers it; **Update now** downloads the APK and hands it to Android's package installer. The first
time Android asks to allow openOMSI to install apps. Then Android asks "Do you want to update
this app?": **Update** replaces openOMSI and starts it again, **Cancel** comes back to the
launcher as "not updated". The APK must be signed with the same key as the installed app
(release builds are).

`openOMSI/env.txt` on the shared storage takes the `OMSI_*` switches a computer takes from its
environment (one `NAME=value` a line), for example `OMSI_UPDATE_URL=file:///sdcard/…/release.json`
to try an update from a local release description.

