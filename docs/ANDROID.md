# openOMSI on Android

openOMSI runs on Android phones and tablets. It is the same game as on a computer: the same renderer, simulation, scripts, maps, buses and mods. This page explains how to install it, how the touch controls work and where the files go.

## Requirements

| What | Needed |
|---|---|
| Android | 8.0 or newer |
| Processor | arm64 |
| Graphics | Vulkan 1.1 (without Vulkan, openOMSI tries OpenGL ES 3) |
| OMSI 2 | A complete copy of a PC installation of OMSI 2 |

openOMSI does not include OMSI 2's content. You copy your own OMSI 2 folder onto the device.

## Install

1. Download the APK. The newest stable one is always at `releases/latest/download/openOMSI-android.apk` on the project's GitHub page.
2. Open the APK. When Android asks, allow installing apps from this source.
3. Start openOMSI. When Android asks, allow **access to all files**.
   If you missed it: **Android Settings → Apps → openOMSI → Permissions → Files and media → Allow management of all files**.
   The game reads thousands of OMSI 2 files by path. Without this permission it can only read its own folder.
4. Copy the **whole OMSI 2 folder** from a PC onto the device: the folder that holds `Omsi.exe`, `maps` and `Vehicles`. Use a USB cable, a file manager or a USB stick.
   Put it at `openOMSI/OMSI 2` in the internal storage.
5. Open the launcher. openOMSI finds the folder by itself if it lies in `openOMSI/`, `Download/`, `Games/` or at the top of the internal storage.
   Anywhere else: open **Setup → Browse**, pick the folder, then **Use this folder → Save**.

> **Note:** The microphone permission is asked only when you first switch on voice chat. The microphone is used only while a talk button is held.

## Mods

Maps and buses work exactly as on a computer. There are three ways to add mods:

- Copy mod folders or `.zip`, `.7z` and `.rar` files into `openOMSI/Mods`. They are installed the next time the launcher opens.
- On the launcher's **Mods** page, use **Choose a folder** or **Choose an archive**.
- Put `.zip` archives into `openOMSI/Archives`. They are used in place, without unpacking.

See [Mods](MODS.md) for priorities and the rest of mod handling.

## The launcher and the game

The launcher and the game share one app and one window.

- Press **Start** in the launcher and the game opens in the same window.
- End the session with **Quit** in the game menu (or the back key, then **Quit**). You return to the launcher, as you left it.
- The launcher is laid out for a phone: text at least at the system's size, settings in one column. A finger on a list that has reached its end scrolls the page on.
- On a phone the launcher has a rail of tabs. Pages without a tab (Add-ons, Plugins, Setup, Tutorials and others) are under **More**.

The bus preview and the city map in the launcher are drawn by the game itself.

## Touch controls

The on-screen buttons fire the same actions as the keys in `Inputs/keyboard.cfg`. The wheel and the pedals are the same analog inputs a game controller gives. So every bus, stock or mod, is driven by them as by the keyboard.

### Layout while driving

| Where | Controls |
|---|---|
| Top left | Game menu, pause, camera, look straight ahead |
| Top right | Show/hide the controls, cab panel, city map, timetable (on a duty), screenshot |
| Bottom left | Steering wheel |
| Above the wheel | Indicator left, hazard lights, indicator right |
| Beside the wheel | Horn |
| Bottom right | Brake and accelerator pedals |
| Left of the brake | Parking brake, stop brake / door release |
| Above the pedals | Gearbox and door buttons |

**Camera** cycles driver's view → outside view → passenger view (when the bus has passenger cameras) → driver's view.

### Steering

- Put a finger on the wheel's rim and turn it. The wheel follows the finger.
- The drawn wheel turns as far as your wheel settings say: half of **Full lock at** each way, or half of **Wheel rotation** when Full lock at is set to OMSI. With the default 900° this is one and a quarter turns each way.
- Let go and the wheel comes back by itself, as with the steering keys: slowly when standing, faster when rolling. With **Old Steering** on, it stays where you left it.
- **Tilt steering** (in the cab panel) steers by tilting the device instead. Hold it like a wheel.

### Pedals

- The higher up the pedal you press, the harder it acts.
- The foot of each pedal is a rest zone that presses nothing, so a finger put down does not brake hard at once.
- On a manual bus with **Automatic clutch** off, a clutch pedal appears left of the brake buttons.

### Gearbox

The gear buttons match the bus's gearbox:

| Gearbox | Buttons |
|---|---|
| Automatic | R, N, D |
| Sequential | −, N, + |
| Manual | The whole gate: R, N and every gear the bus has, in two rows as on the lever (R 1 3 5 over N 2 4 6) |

### Doors

There is one button per door, front to back. **Settings → Driving → Touch controls → Door buttons** chooses how they work:

| Setting | Buttons |
|---|---|
| Standard (physical doors) | One button per door of the bus, up to four |
| Direct OMSI triggers | Up to three buttons for OMSI's door triggers 0, 1, and 2+3 together. A button shows only if the bus has that trigger. |

### Cab panel

The tune button at the top right opens the cab panel:

| Button | What it does |
|---|---|
| Battery | Main switch on/off |
| Engine start (hold) | Starts the engine while held |
| Start the bus by itself | Runs the bus's whole start-up |
| Headlights, High beam | Lights |
| Wipers | Wipers on/off |
| Saloon lights | Passenger compartment lights |
| Sell ticket | Sells a ticket |
| Announcement (Q) | Holds the Q key: the bus's announcement or microphone |
| Voice: nearby (hold), Voice: radio (hold) | Voice chat talk keys (only when voice chat is on) |
| Next seat view | Next passenger seat camera |
| Free camera | Opens the free camera |
| Navigator | Shows or hides the navigator |
| Information bar | Shows or hides the information bar |
| Tilt steering | Steering by tilting the device |

### The cab itself

Everything else works in the cab, as in OMSI:

- A **tap** works the switch under the finger: the IBIS, the ticket printer, light switches.
- A **drag starting on a switch** turns it: knobs, the ignition key, the sun blind.
- A **drag elsewhere** looks round. **Two fingers** zoom.

### Navigator, menus and the city map

- The small navigator sits at the top middle (below the information bar when it is on). A tap opens the city map. Drag it to put it somewhere else; it stays there.
- In the game menu and its lists, a drag scrolls and a tap picks a line. A finger put down to scroll does not pick the line it lands on.
- On the city map, your fingers act as the mouse.
- The back key works as **Esc**.

### On foot and the free camera

- **On foot:** a stick at the bottom left walks. Push it to the edge to run. A button at the bottom right kneels (for a photo from low down) and stands up again.
- **Free camera** (**cab panel → Free camera**): the stick flies, as fast as it is pushed. The buttons at the bottom right rise, sink, and halve or double the speed. A drag looks, two fingers zoom. **Exit free camera** goes back to the driver's seat.

### Photo mode

In photo mode the driving controls are hidden. The top left has close, the settings sheet and hide-the-buttons. The bottom right has the shutter, rise, sink and a speed button; the stick moves the camera. A drag turns the camera, a tap focuses, two fingers zoom.

### Game controllers

A game controller connected by Bluetooth works as on a computer.

## Touch settings

**Settings → Driving → Touch controls** (shown on phones and tablets only):

| Setting | What it does | Default |
|---|---|---|
| Door buttons | Standard (physical doors) or Direct OMSI triggers | Standard |
| Pedal length | Height of the on-screen pedals, 80 % to 160 % | 100 % |
| Pedal rest zone | Share of each pedal at its foot that presses nothing, 0 % to 40 % | 15 % |

The wheel's turn follows **Wheel rotation** and **Full lock at** on the same page.

## Files on the device

| What | Where |
|---|---|
| Content folder | `openOMSI/` in the internal storage |
| OMSI 2 | Usually `openOMSI/OMSI 2` |
| Mods to install | `openOMSI/Mods` |
| Archives used in place | `openOMSI/Archives` |
| Screenshots | `openOMSI/Screenshots` |
| Crash report | `openOMSI/crash.log` |
| Switches for testing | `openOMSI/env.txt` |
| Settings, profiles, sessions, `game.log` | The app's private folder |

If openOMSI cannot write to the internal storage (no permission yet), it uses its own folder under `Android/data/` instead.

**Gallery apps.** Every folder in `openOMSI/` except `Screenshots`, and the OMSI 2 folder wherever it is, gets an empty `.nomedia` file. This keeps the thousands of textures out of gallery apps. They are game content, not photos: deleting them turns buses white. A gallery that listed them before may need a while, or a restart of the device, to forget them.

## Graphics on a phone

The first start on a phone uses lighter defaults:

| Setting | Phone default |
|---|---|
| Anti-aliasing | 2x MSAA |
| Anisotropic filtering | 4x |
| Ambient occlusion | Off |
| Shadow map | 1024 |
| Mirrors | 128 |
| Frame limit | 60 fps |
| Object distance | 900 m |
| Passengers | 70 % |

Everything can be changed on the launcher's **Settings** page. When the frame rate stays below 40 because the graphics chip is the limit, the 3D picture is drawn smaller in steps (85 %, 70 %, 55 % of the screen), as on a computer.

## Updates

1. When the launcher starts, it looks for a newer release. Choose the channel (Stable or Beta) under **Settings → General → Updates**.
2. Press **Update now**. The launcher downloads the APK and hands it to Android's installer.
3. The first time, Android asks you to allow openOMSI to install apps.
4. Android asks "Do you want to update this app?". **Update** replaces openOMSI and starts it again. **Cancel** returns to the launcher, not updated.

The APK must be signed with the same key as the installed app. Release builds are.

If a start crashed before the launcher appeared, the next start looks for an update at once in the background and offers it. This lets a broken version update itself.

## Troubleshooting

- **Logs:** `game.log` is in the app's private folder; with a computer attached, `adb logcat -s openOMSI` shows the same lines live.
- **Crashes:** see `openOMSI/crash.log`.
- **Black picture or grey previews on Adreno 6xx to 8xx chips:** these drivers can corrupt the shader cache. After a run on such a chip, openOMSI throws the cache away and builds the shaders again on the next start. That start takes longer.
- **Testing switches:** `openOMSI/env.txt` takes the `OMSI_*` switches a computer takes from its environment, one `NAME=value` per line. For example `OMSI_UPDATE_URL=file:///sdcard/…/release.json` tries an update from a local release description. See [Environment variables](DEBUG_FLAGS.md).

For anything else, see [Troubleshooting](TROUBLESHOOTING.md).

## For developers

### Building the APK

```bash
rustup target add aarch64-linux-android
# the Android SDK command line tools, e.g. with Homebrew on a Mac:
brew install --cask android-commandlinetools
sdkmanager "platforms;android-35" "build-tools;35.0.0" "ndk;28.2.13676358" "platform-tools"
scripts/build-android.sh            # → dist/android/openOMSI-<version>.apk
scripts/build-android.sh install    # ... and adb install it on the attached phone
```

- `android/env.sh` finds the SDK (`ANDROID_HOME`, else Homebrew's), the newest NDK and a JDK 17.
- The script builds the game library as a shared object (`cargo rustc --lib --crate-type cdylib --profile android`).
- It compiles the two Java classes in `android/java` with javac and d8:
  - `OmsiActivity`: full screen, storage and microphone permissions, vibration, installing updates.
  - `LauncherBridge`: the launcher's web view.
- It packs, aligns and signs the APK with a debug key made on the first build (`android/debug.keystore`, not in git).

The launcher is the desktop launcher's page (`launcher/ui`, phone layout in `js/mobile`) in a web view over the game's window. Its commands are answered by the same code as on a computer (`omsi-launcher-api`).

### Looking at the touch controls on a computer

- `OMSI_TOUCH=1` shows the touch controls in the game window.
- `OMSI_INPUT` drives them with `touch down|move|up x,y[,finger]` (logical pixels). Example: press the accelerator for two seconds and save a picture with the controls on it:
  ```bash
  OMSI_TOUCH=1 OMSI_INPUT="t=8 touch down 1500,700; t=10 touch up 1500,700; t=11 shot touch.png" openomsi
  ```
- `OMSI_MOBILE=1` gives the game the phone's screen cut-outs.
- A phone-sized picture without a window on screen:
  ```bash
  OMSI_HIDDEN_WINDOW=1 OMSI_BACKGROUND=1 OMSI_TOUCH=1 OMSI_INPUT="t=7 shot phone.png" openomsi --size 360x800 --exit-after 12
  ```
  Sizes worth checking: 360x800, 412x915, 820x1180 and 800x360.
- Photo mode: `OMSI_INPUT="t=7 photo"`, then touch the buttons.
- The bus preview's cost per picture (drawing, read-back, JPEG) is in the `ms` of the launcher service's `frame` answer: run `openomsi --launcher-service` and send one JSON request per line.

### Releases

Every release (started by hand from main) builds the APK in GitHub Actions (the `android` job of `.github/workflows/release.yml`). It is attached as `openOMSI-<version>-android-arm64.apk` and as `openOMSI-android.apk`.

Set the repository secret `ANDROID_KEYSTORE_B64` (`base64 < android/debug.keystore`). Then every release is signed with the same key and installs over the previous one.
