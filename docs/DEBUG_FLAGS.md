# OMSI_* environment variables

Every `OMSI_*` environment variable openOMSI reads, grouped by what it is for. These are
power-user and developer switches: settings you normally change belong in the launcher
([Launcher and settings](LAUNCHER.md)), and command-line flags are in [Command line](COMMAND_LINE.md).

## Setting a variable

Start openOMSI from a terminal with the variable set. The launcher hands its environment on to
the game it starts, so a variable set for the launcher reaches the game too.

**Windows** (Command Prompt, in the openOMSI folder):

```bat
set OMSI_PROFILE=1
openomsi.exe
```

In PowerShell: `$env:OMSI_PROFILE = "1"; .\openomsi.exe`

**macOS** (Terminal):

```bash
OMSI_PROFILE=1 /Applications/openOMSI.app/Contents/MacOS/openomsi
```

**Linux** (in the openOMSI folder):

```bash
OMSI_PROFILE=1 ./openomsi
```

**Android:** a phone has no environment to set. Put `NAME=value` lines, one per line, into
`openOMSI/env.txt` in the content folder. The game sets every `OMSI_*` line from it at start
and notes each name in the log.

How values are read:

- **flag** in the tables below: on when the variable is set to anything, even `0` or an empty
  value. To turn a flag off, unset the variable.
- **number**: the whole value is parsed as a number. A value that does not parse counts as unset.
- **default: the setting** means that without the variable the matching launcher setting applies.
- The game reads each variable once per process. Changing it while the game runs has no effect.

The list is kept in `crates/omsi-cfg/src/flags.rs`. A test there fails when the code reads an
`OMSI_*` name the list lacks, or the list keeps a name no code reads any more.

## Graphics

### Renderer, API and display

| Variable | Values / default | What it does |
| --- | --- | --- |
| `OMSI_GRAPHICS` | `vanilla`, `vanilla_plus` or `enhanced`; default: the setting | Use another renderer for this run. |
| `OMSI_ENHANCED` | flag | Enhanced graphics for this run. |
| `OMSI_ENHANCED_PLUS` | flag | Enhanced+ (ray traced) graphics for this run. |
| `OMSI_MX` | `on`/`1`/`true` or `off`/`0`/`false`; default: the setting | mX Graphics for this run, over the `mx_graphics` setting. On starts the game on mX, off on wgpu. |
| `OMSI_BACKEND` | `vulkan`, `dx12`, `metal`, `gl`, `angle` (Windows: also `dx11`, `d3d11`); default: the setting | The graphics API tried first. The launcher and Android set it at runtime. |
| `OMSI_PRESENT_MODE` | `fifo`, `fifo_relaxed`, `mailbox`, `immediate` or `auto`; default `auto` | How frames reach the screen. `fifo` is vsync. A mode the screen does not support falls back to `auto`. |
| `OMSI_FSR1` | `1`, `true` or `on`; unset: off | Experimental FSR 1 spatial upscaling (EASU + RCAS) for scaled window views. Needs a restart. |
| `OMSI_OPENXR` | flag | Start in OpenXR (VR) mode. |
| `OMSI_OPENXR_SCALE` | number; default: the setting | VR: render scale of the eyes. |
| `OMSI_OPENXR_MIRROR_RATE` | number; default: the setting | VR: how often the mirrors are redrawn. Negative: every mirror every frame. |
| `OMSI_MIRROR_HUD` | number; default: the setting | Mirror HUD mode. |
| `OMSI_CHARACTER_MODELS` | `original` or `improved`; default: the setting | Character models for this session: OMSI's people, or the Microsoft Rocketbox avatars (drawn on this computer only). |
| `OMSI_PAX_ANIMATION` | `original` or `enhanced`; default: the setting | Passenger animation for this session: OMSI's, or procedural IK. |
| `OMSI_HTML_ENGINE` | `modern`; default `classic` | Run every `[htmltexture]` page on the modern engine (flexbox, modern JavaScript), not only pages that ask for it with `<meta name="omsi-engine" content="modern">`. For comparing the two. |
| `OMSI_HTML_FALLBACK_FONT` | font file | Font for HTML page characters that Inter lacks (Hangul, CJK). Without it: Malgun Gothic (Windows) or AppleGothic (macOS). |
| `OMSI_HTML_FALLBACK_FONT_BOLD` | font file | The bold face for `OMSI_HTML_FALLBACK_FONT`. |

See [Graphics](GRAPHICS.md) for what the renderers and upscalers do, and [Virtual reality](VR.md) for VR.

### Picture tuning

Values for looking at and comparing pictures. The defaults are what the game uses.

| Variable | Values / default | What it does |
| --- | --- | --- |
| `OMSI_METER` | `gain,target,dark,bright,bias,night` | The exposure metering. |
| `OMSI_TONE_CONTRAST` | `day[,night]`; default `1` | Tone mapping contrast. |
| `OMSI_TONEMAP` | `natural`, `agx` or `neutral`; default `agx` | Enhanced's tone curve: the old natural curve, AgX, Khronos PBR Neutral. |
| `OMSI_GRADE` | `0` | Enhanced without its colour grade (white balance, saturation and the shadows' lift by the sun's height and the weather). |
| `OMSI_WIDE_AO` | `reach,strength,gain,max` | The street-scale occlusion of sky light and bounced light (metres, 0..1, 0..1, radiance). |
| `OMSI_PUDDLE_F0` | number; default `0.08` | Puddle reflectance at normal incidence (0.02 to 0.2). |
| `OMSI_PUDDLE_THICKNESS` | number; default `0.12` | Puddle water film thickness. |
| `OMSI_SURFACE_BIAS` | number; default `-24` | Depth bias of road surfaces. |
| `OMSI_SURFACE_FLUSH` | number; default `0.12` | Height in metres under which a surface counts as flush with the road. |
| `OMSI_WINDY_TREES` | `0` or `1`; default: the setting | Trees moving in the wind off or on. |
| `OMSI_ROAD_SNOW` | `off` or a number 0..1; default: the setting | Built-up snow on roads and roofs, over the `road_snow` setting. `off` uses the weather's plain on/off snow on the road; a number is the cover to start with. |
| `OMSI_TEXTURE_RAIN` | flag | OMSI 2's own texture rain on the glass instead of the drops. |

## Performance

| Variable | Values / default | What it does |
| --- | --- | --- |
| `OMSI_MAX_FPS` | number; default: the setting | Frame rate limit for this run. |
| `OMSI_VIDEO_MEMORY` | MB; default: the setting | How much the game may hold on the graphics card in total (textures, meshes, render targets). |
| `OMSI_TEXTURE_MEMORY` | MB; default: the setting | Texture budget. |
| `OMSI_SIM_THREAD` | `0`/`false`/`off`/`no`, or anything else; default: the `sim_thread` setting (off) | Run the AI traffic, people and scenery scripts on their own thread while the picture is drawn (one frame later). The offscreen run ignores it. |
| `OMSI_FIXED_SCALE` | flag | Keep the render scale fixed (no dynamic resolution). |
| `OMSI_FULL_GPU` | flag | Keep the requested settings on a small or shared graphics chip, and 8x MSAA with Enhanced+. |
| `OMSI_INTEL_FULL_GPU` | flag | Keep the requested settings on an Intel Vulkan adapter. |
| `OMSI_BASIC_PIPELINES` | flag | Use the reduced (basic) render pipelines, as after a driver failed to build the full ones. |
| `OMSI_NO_PIPELINE_CACHE` | flag | Compile every pipeline at each start, without the cache in `~/.openomsi/cache/pipelines-*.bin`. |
| `OMSI_NO_TEXTURE_CACHE` | flag | Compress every texture at each start, without the cache in `~/.openomsi/cache/textures`. |
| `OMSI_KEEP_ALLOCATOR` | flag | Skip the restart that swaps in the faster allocator at start. |
| `OMSI_NO_BC` | flag | Upload every texture as RGBA, also DXT files (no BC compression on the GPU). |
| `OMSI_NO_TEXCOMPRESS` | flag | No runtime texture compression: DXT files stay compressed blocks, everything else is RGBA. |

### Measuring performance

`OMSI_PROFILE=1` logs the time each stage of the frame takes; the session summary is written
when the game closes or after `--exit-after`. `OMSI_BENCH` works on offscreen runs only.

| Variable | Values / default | What it does |
| --- | --- | --- |
| `OMSI_PROFILE` | flag | Log the time each stage of the frame takes, with a session summary at the end. |
| `OMSI_PROFILE_GPU` | flag | Profile: wait for the GPU so its time shows as a stage of its own. |
| `OMSI_PROFILE_JSON` | file | With `OMSI_PROFILE`: also write the session summary (after the 15 s warm-up) as JSON into this file. Compare runs with `scripts/compare-performance.py`. |
| `OMSI_GPU_TIMERS` | flag | Measure GPU time per pass with timestamp queries. |
| `OMSI_GPU_TIMERS_RAW` | flag | With `OMSI_GPU_TIMERS`: the passes in the order the GPU finished them. |
| `OMSI_BENCH` | number | Offscreen: draw the final picture this many more times and log the median CPU and GPU-wait time. |
| `OMSI_BENCH_FRAMES` | flag | With `OMSI_BENCH`: log every bench frame's times, not only the medians. |
| `OMSI_BENCH_STEREO` | flag | With `OMSI_BENCH`: benchmark a still stereo pair without a headset. `--size` is per eye; 100 degree vertical field of view, 64 mm eye distance. |

## Gameplay and simulation

| Variable | Values / default | What it does |
| --- | --- | --- |
| `OMSI_REALISTIC_ROAD` | flag | Turn on the **Realistic road physics** setting for this run (slip-angle tyres, surfaces, road roughness, wind), to compare with the OMSI 2 model. |
| `OMSI_FOOT_WALK` | number; default `1.4` | On foot: the walking pace in m/s. |
| `OMSI_FOOT_RUN` | number; default `3.8` | On foot: the pace of a run (Shift) in m/s. |
| `OMSI_IBIS_BUDGET` | number; default `10` | Seconds the IBIS typist may take per entry. |
| `OMSI_RUNNER_CHANCE` | number; default `0.05` | Chance a late passenger runs for a bus standing at a stop (0: never, 1: every bus). |
| `OMSI_PARK_IN` | number; default `0.04` | Chance per step a car parks. |
| `OMSI_PARKED_PULL_OUT` | number; default `0.035` | Chance per step a parked car pulls out. |
| `OMSI_STORM` | `activity[,azimuth]` | A thunderstorm of this activity (0..1) whatever the weather. With an azimuth (degrees, 0 east, 90 north) the flash stays frozen there at its first stroke, for checks. |
| `OMSI_IGNORE_TRANSPORT` | flag | Run a vehicle on a map built for other transport modes (the log still says so). |
| `OMSI_TRAFFIC_ALL_GROUPS` | flag | Let restricted AI groups (aircraft, depot fleets) drive everywhere. |
| `OMSI_TRACKIR_NATIVE` | `0`; default on | `0` turns the native TrackIR interface off. |
| `OMSI_TOBII_DLL` | path | The Tobii Stream Engine DLL to load instead of the one from Tobii Experience. |

## Content, folders and plugins

| Variable | Values / default | What it does |
| --- | --- | --- |
| `OMSI_ROOT` | folder; default: found automatically | The OMSI 2 installation folder. Tests that need real content also use it as content root. |
| `OMSI_CONTENT` | folder; default: beside the game | The content folder (mods, archives, screenshots). Android and iOS set it at runtime. |
| `OMSI_CONTENT_ZIP` | paths, separated like `PATH` | Extra content archives, as `--content-zip`. |
| `OMSI_AMBIENCE_DIR` | folder | Folder of the ambience recordings (with its `pack.json`), instead of `ambience` beside the program or in `~/.openomsi`. |
| `OMSI_AVATARS_DIR` | folder | Folder of the passenger avatars (with its `pack.json`), instead of `avatars` beside the program or in `~/.openomsi`. |
| `OMSI_TROLLEY_FILE` | file | An overhead wire description to use instead of the map's own `trolley.txt`. |
| `OMSI_TOMORROW_API_KEY` | key; default: read from the key file | Private Tomorrow.io API key for live weather. Never put it in map files or logs. |
| `OMSI_TOMORROW_KEY_FILE` | file; default `settings/tomorrow-api-key.txt` | Private text file holding your Tomorrow.io API key. |
| `OMSI_TOMORROW_CACHE_DIR` | folder; default `settings/weather-cache` | Private cache of Tomorrow.io answers and the shared request budget. |
| `OMSI_NO_PLUGINS` | flag | No plugins loaded. |
| `OMSI_PLUGIN_HOST32` | path; default: beside the game | Path of `omsi-plugin-host32.exe`, which runs 32-bit OMSI plugins. |
| `OMSI_PLUGIN_PROCESS_ALLOW` | path of a JSON manifest | Pre-approve plugin helper programs: the manifest binds plugin content and helpers in its folder to SHA-256 hashes. Otherwise the game asks the player. Any change revokes the approval; packages must declare `process`. Approved programs run with the game's account permissions. |
| `OMSI_WINE` | path; default: found on the PATH | The Wine program used to run Windows plugins. |

## Network and online services

LAN play and the services the game talks to. See [Playing together](MULTIPLAYER.md) and
[Weather and seasons](WEATHER.md) for the features themselves.

| Variable | Values / default | What it does |
| --- | --- | --- |
| `OMSI_LAN_IP` | `a.b.c.d[,e.f.g.h]` | LAN addresses to offer first, when the detection gets them wrong. |
| `OMSI_LAN_JOIN_TIMEOUT` | seconds; default: built-in | How long to wait when joining a LAN game. |
| `OMSI_NO_BRIDGE` | flag | LAN: leave the internet alone - no rendezvous bridge, no tunnel (tests, LAN parties). |
| `OMSI_NO_TUNNEL` | flag | LAN: no tunnel. |
| `OMSI_CLOUDFLARED` | path | The `cloudflared` program for the LAN tunnel. Searched after the game's folder and before the `PATH`. |
| `OMSI_NO_LAN_MODS` | flag | LAN: do not exchange mods. |
| `OMSI_NO_LAN_UPLOADS` | flag | LAN: a game hosting a session takes no buses from the players who join. |
| `OMSI_NO_VAR_SYNC` | flag | LAN: no variable sync. |
| `OMSI_NO_HARDWARE_ID` | flag | LAN: the game sends no hardware identifier (a host can then turn the player away, and cannot ban the machine). |
| `OMSI_OFFICIAL_KEY` | file | The official server's signing key. |
| `OMSI_NO_PRESENCE` | flag | No presence ("playing now") service. |
| `OMSI_PRESENCE_URL` | URL; default: built-in | Base URL of the presence ("playing now") service. |
| `OMSI_NO_UPDATE` | flag | No update check. |
| `OMSI_UPDATE_URL` | URL or `file:///...json`; default: built-in | Another release description for the update check. |
| `OMSI_HEALTH_URL` | URL or `file:///...json`; default: built-in | Another list of broken releases for the start guard, instead of `release/health.json` on GitHub. |

## Start-up and the start guard

The start guard watches each start and heals an installation that crashes or hangs (see
[Troubleshooting](TROUBLESHOOTING.md) and [Versions and releases](VERSIONING.md#handling-a-broken-release)).
The last two variables are set by the launcher and the guard for the processes they start; you
do not set them yourself.

| Variable | Values / default | What it does |
| --- | --- | --- |
| `OMSI_SAFE_MODE` | flag | Safe mode: basic render pipelines and no plugins. The start guard sets it after failed starts. |
| `OMSI_SAFE_GPU` | number; default `0` | After a lost graphics device the game restarts lighter on the card each time; this counts the restarts. Set by the restart and on Android. |
| `OMSI_NO_GUARD` | flag | No start guard: a start that crashes or hangs is not healed by an update or by going back to the previous version. |
| `OMSI_GUARD_DEADLINE` | seconds; default `120` | How long the launcher may take to its first frames before the start counts as hung. The guard then ends it and heals the installation. |
| `OMSI_LAUNCHER` | path | The program a plain start opens as launcher, instead of `openomsi-launcher-app` beside the game. |
| `OMSI_SCREEN_AT` | `x,y` (desktop pixels) | The screen the game window opens on. The launcher sets it to the screen it stands on. |
| `OMSI_INSTANCE` | id | The id of a game instance the launcher started. Set for the child process. |
| `OMSI_GUARD_STATE` | `<pid>:<state file>` | Set by the start guard for the process it watches. |

## Debug output

None of these changes what the game does. They only write more to the log (`game.log`), into
files, or show a debug view.

### Logs

| Variable | Values / default | What it does |
| --- | --- | --- |
| `OMSI_AMBIENCE_DENSITY` | default `1` | The ambience's one-shots (birds, the far life, thunder, the objects' calls) this many times as often (a number), or only some of them (slot=factor,…: bird.geese=40) - what fits the place, season and hour stays the same (for demos and checks). |
| `OMSI_DEBUG_AI_WIDE` | flag | Log every tenth of a second an AI car standing over 1.5 m beside its way. |
| `OMSI_DEBUG_ANIM` | text | Log the animations of the meshes whose file name contains this text. |
| `OMSI_DEBUG_BOARDS` | flag | Log boarding at bus stops. |
| `OMSI_DEBUG_CABLES` | flag | Log every control cable value a train car reads when it changes. |
| `OMSI_DEBUG_CAMERA` | number; default `1` | Camera arm log: 1 the types and what stops the arm, 2 also every frame. |
| `OMSI_DEBUG_CAR` | number | Log the state of the AI car with this id every step. |
| `OMSI_DEBUG_CAREER` | flag | Log the jolts the career scoring counts. |
| `OMSI_DEBUG_CAR_VARS` | text | Log each change, per car of a train, of the script variables and strings whose name contains one of these comma-separated texts. |
| `OMSI_DEBUG_COLLISION` | flag | Log the player's collision state. |
| `OMSI_DEBUG_CONES` | flag | Log the light coronas and beams prepared per frame. |
| `OMSI_DEBUG_CULL` | number; unset: off | Log what near and in the picture is left out of the draw list, and why. |
| `OMSI_DEBUG_DOORS` | flag | Log the door variables of AI buses twice a second. |
| `OMSI_DEBUG_DRAWS` | flag | Log how many instances changed per prepare. |
| `OMSI_DEBUG_DRIVER` | flag | Log the driver figure's settling (grips, wrists, elbows). |
| `OMSI_DEBUG_EXPOSURE` | flag | Read back and log the adapted exposure metering now and then. |
| `OMSI_DEBUG_FACE` | text | Log the avatars' faces as they are posed (the strongest expressions of the near faces, once a second); `smile`, `open` or `blink` also puts that expression on every face. |
| `OMSI_DEBUG_FLICKER` | flag | Log near instances drawn in one frame and not the next (objects blinking). |
| `OMSI_DEBUG_FLOAT` | flag | Log objects floating over the ground while placing them. |
| `OMSI_DEBUG_FOG_LAMPS` | flag | Log the fog lamp passes per frame. |
| `OMSI_DEBUG_FOOT` | flag | Log where the walking avatar is drawn. |
| `OMSI_DEBUG_HUMANS` | flag | Log the people (passengers, pedestrians) and their placement. |
| `OMSI_DEBUG_IBIS` | flag | Log the IBIS typist's reasoning. |
| `OMSI_DEBUG_INTERIOR` | flag | Log the interior lamps of each vehicle type. |
| `OMSI_DEBUG_JUNCTION` | flag | Log why AI cars wait at a junction. |
| `OMSI_DEBUG_LAMPS` | flag | Log traffic lights without a crossing, and the moving lamps (barriers). |
| `OMSI_DEBUG_LAN` | flag | LAN: every few seconds what we know of every player, counts and bytes a second. |
| `OMSI_DEBUG_LANES` | text | Log the lanes with these ids (comma-separated). |
| `OMSI_DEBUG_LIGHTS` | `all`, `near` or controller names | Log every light change of those traffic light programs. |
| `OMSI_DEBUG_LIGHT_GRID` | flag | Log lights left out of a full light-grid cell. |
| `OMSI_DEBUG_MESHES` | text; unset: off | Log per material slot which part of its texture a mesh shows (display texts). |
| `OMSI_DEBUG_MIRRORS` | flag | Log each mirror's eye, angles and field of view. |
| `OMSI_DEBUG_MISSING` | flag | Log where missing textures were looked for. |
| `OMSI_DEBUG_NAN` | flag | Warn at every write of a NaN or infinity into a script variable. |
| `OMSI_DEBUG_NAV` | flag | Log navigation details (e.g. stops whose tiles are not loaded). |
| `OMSI_DEBUG_OBJECTS` | flag | Log objects placed outside their tile. |
| `OMSI_DEBUG_OBJMAT` | text | Log how the material slots of the objects whose file name contains this text are made. |
| `OMSI_DEBUG_PAD` | flag | Log every game controller button event: the raw buttons the assignment menu waits for, and the system gamepad ones. |
| `OMSI_DEBUG_PARTICLES` | flag | Log the smoke particles. |
| `OMSI_DEBUG_PASS` | flag | Log twice a second why a car standing behind something does not go round it. |
| `OMSI_DEBUG_PAX` | flag | Log every change of state of the passengers. |
| `OMSI_DEBUG_PHYSICS` | number; unset: off | Log the player's pose, speed, wheel contacts and crashes (offscreen: =secs, the interval). |
| `OMSI_DEBUG_POPULATION` | flag | Log where cars appear and vanish relative to the view. |
| `OMSI_DEBUG_PROPS` | flag | Log the destination-display (matrix) variables. |
| `OMSI_DEBUG_RAIN` | flag | Log the tyres' spray and puddles. |
| `OMSI_DEBUG_RASTER` | text | x,y: log the ground raster at that point. |
| `OMSI_DEBUG_REPEATERS` | flag | Log repeater objects whose spline chain disagrees with the map. |
| `OMSI_DEBUG_REST` | flag | Log where the bus came to rest on the ground after spawning. |
| `OMSI_DEBUG_ROUTES` | flag | Log where consecutive lanes of an AI route do not join. |
| `OMSI_DEBUG_SEAT` | flag | Log the driver seat's suspension meshes. |
| `OMSI_DEBUG_SERVICES` | flag | Log petrol station boxes and the bus's distance to them. |
| `OMSI_DEBUG_SHADOW_FAR` | flag | Log each redraw of the far shadow map. |
| `OMSI_DEBUG_SIGNALS` | flag | Log what each railway signal shows. |
| `OMSI_DEBUG_SKY` | flag | Log the sky and sun values per frame. |
| `OMSI_DEBUG_SOUND` | number; default `5` | Log which sounds were heard (number: the interval in seconds). |
| `OMSI_DEBUG_SPLINES` | flag | Log the splines and lanes built per tile. |
| `OMSI_DEBUG_STARTUP` | flag | Log the start-up steps of a vehicle's systems. |
| `OMSI_DEBUG_STOPS` | flag | Log the bus stops a bus missed. |
| `OMSI_DEBUG_STUCK` | flag | Log the report of stuck AI cars (also explains junction waits). |
| `OMSI_DEBUG_SURFACES` | flag | Log the painted ground layers per tile. |
| `OMSI_DEBUG_SWITCHES` | flag | Log railway switches thrown for a train. |
| `OMSI_DEBUG_TEXT` | flag | Log the scripts' string variable operations. |
| `OMSI_DEBUG_TEXTURES` | flag | Log all textures by size and format. |
| `OMSI_DEBUG_TRAFFIC` | flag | Log traffic light programs, cars standing for long and hard bends. |
| `OMSI_DEBUG_TRAILER` | flag | Log a trailer's rest, sag and lift on the ground. |
| `OMSI_DEBUG_TRAILERS` | flag | Log coupled parts off the level of what pulls them. |
| `OMSI_DEBUG_TRIGGERS` | flag | Log the variables a mouse trigger changed. |
| `OMSI_DEBUG_TROLLEY` | number; unset: off | Log the trolleybus gear (offscreen: =secs, the interval): voltage, current, controller position, the poles, and every event of the poles (frogs, insulators, dewirings). |
| `OMSI_DEBUG_UNLINKED` | flag | Log lane ends with an untaken lane start near them. |
| `OMSI_DEBUG_UPLOAD` | flag | Log slow GPU uploads item by item. |
| `OMSI_DEBUG_VARIANTS` | text | Log material variants whose variable name contains this text. |
| `OMSI_DEBUG_VARS` | number | Offscreen: log these script variables of the player's bus (comma-separated). |
| `OMSI_DEBUG_VARS_EVERY` | number | With `OMSI_DEBUG_VARS`: log them every this many seconds through the drive. |
| `OMSI_DEBUG_VAR_SYNC` | text | LAN: log this variable, ours every two seconds and theirs as it comes. |
| `OMSI_DEBUG_VIEW_LAMPS` | flag | Log the view lamps value per frame. |
| `OMSI_DEBUG_WALLS` | flag | Log the walls a walking player slides along. |
| `OMSI_DEBUG_WARP` | flag | Log crossings whose ground was moved by more than a metre. |
| `OMSI_DEBUG_WHEELS` | flag | Log where the wheel meshes of every vehicle type turn. |
| `OMSI_FRAME_ORDER` | flag | Log the window's frame and the offscreen run's step as their ordered stages (phase, what each reads and writes, its profile key), once at start. |
| `OMSI_GROUND_LANES` | flag | Along every street lane, every metre, how far the ground lies over or under the lane. |
| `OMSI_LIST_ALIGNED` | flag | Log the splines aligned to the terrain. |
| `OMSI_TRACE_VARS` | text | a,b,$c: the listed variables every half second of the run. |
| `OMSI_WATCH_VARS` | text | a,b: log every change of these variables of the player's bus. |
| `OMSI_WHEEL_TRACE` | flag | Log the deepest a drawn tyre goes into the road, once a second. |

### Traces and dumps

Variables whose value is a file or folder name write their data there. CSV traces are written
frame by frame.

| Variable | Values / default | What it does |
| --- | --- | --- |
| `OMSI_AUDIO_TRACE` | text | Write what the game asks of the mixer (every sound started, changed, stopped, the listener each frame) into this file, rounded and deterministic for offscreen recordings (see scripts/audio-trace.sh). |
| `OMSI_DUMP_CLASSES` | flag | mapgen_dump example: paint the paving by class (road, footway, yard) in telling colours. |
| `OMSI_DUMP_CUT` | text | Directory: write each tile's road-cut alpha masks as images. |
| `OMSI_DUMP_GROUND` | text | File: every loaded tile's final terrain and road cut as text. |
| `OMSI_DUMP_SCENERY_TEXT` | text | Directory: write the scenery objects' text textures as drawn. |
| `OMSI_DUMP_SCRIPTTEX` | text | Directory: write the player's bus's script (display) textures. |
| `OMSI_GROUND_GAP` | text | Offscreen CSV (or -): how far every drawn tyre stands over or sinks into the ground. |
| `OMSI_GROUND_GAP_RADIUS` | number; default `150` | With `OMSI_GROUND_GAP`: the radius in metres. |
| `OMSI_LAN_TRACE` | text | LAN CSV file: where the host's people are drawn. |
| `OMSI_SUSP_TRACE` | text | Offscreen CSV: body height and each wheel's travel and load every frame. |
| `OMSI_SUSP_TRACE_WINDOW` | text | Window CSV: each wheel's travel every frame. |
| `OMSI_TRACE_AI` | text | CSV: every AI car's pose, steering and speed every frame. |
| `OMSI_TRACE_AI_BUSES` | flag | With `OMSI_TRACE_AI`: the timetable buses only. |
| `OMSI_TRACE_FFB` | text | CSV: the force feedback frame by frame (Windows): the wheel's position and the force sent. |
| `OMSI_TRACE_LOOK` | text | CSV: the mouse look frame by frame and event by event - the cursor, the head's turn and the camera drawn. |
| `OMSI_TRACE_PAX` | text | File: trace the passengers. |
| `OMSI_TRACE_REMOTE` | text | LAN CSV: where each other player's bus is drawn every frame. |
| `OMSI_TRACE_STEER` | text | CSV: the mouse steering frame by frame. |
| `OMSI_TRAFFIC_STATS` | text | CSV: the traffic's flow every minute (standing cars, waits-for cycles, junction entries, red runs, overlaps), a summary at the end. |

### Debug views

| Variable | Values / default | What it does |
| --- | --- | --- |
| `OMSI_DEBUG_ENHANCED` | number; default `0` | Enhanced main pass shows one of its terms alone (n selects it: 1 sun shadow, 2 occlusion, ..., 18 traced bounced light, 19 traced share of the sky light). |
| `OMSI_DEBUG_SHADOW` | number; unset: off | Shadow debug overlay (with a number: its radius, default 3). |
| `OMSI_DEBUG_PUDDLES` | number; default `0` | Puddle debug view (a number selects it). |
| `OMSI_DEBUG_RT` | number; unset: off | Log the ray tracing structures every 30 frames (with a number: a debug view). |
| `OMSI_DEBUG_LIGHT` | flag | Add a test point light above the camera. |
| `OMSI_ENV_PHOTO` | number; default `1` | 0 leaves the environment photo out of the debug views. |
| `OMSI_FG_SHOW` | `generated` or `drawn` | Frame generation: the window shows only that kind of frame, to look at it alone. |
| `OMSI_FORCE_LOD` | flag | Draw the lower levels of detail of the AI vehicles at any size (to look at them). |
| `OMSI_ZFIGHT_PROBE` | number | Z-fighting check: the draw order's depth steps within an object from a hash (1) or its reverse (2); offscreen pictures of both differ where an object's surfaces lie in one plane. |

## Test hooks

Hooks for automated tests, map checks and offscreen pictures. Most act only in an offscreen run
(`--offscreen`, see [Command line](COMMAND_LINE.md)); the description says so where it matters.

### Offscreen camera and pictures

| Variable | Values / default | What it does |
| --- | --- | --- |
| `OMSI_CAM_VEHICLE` | text | Offscreen: x,y,z,yaw,pitch[,fov] - a camera in the bus's own frame. |
| `OMSI_CAM_SWEEP` | text | yaw[,jump]: the offscreen warm frames come from a camera turning yaw degrees a frame and ending in the picture's pose; with a jump (m), from a place that far to the side (a cut). For checking what reads the last frame. |
| `OMSI_FOLLOW_CAM` | text | right,ahead,up,yaw,pitch: a camera in the followed car's frame. |
| `OMSI_ORBIT_DIST` | number; default `18` | Offscreen: the outside camera's distance from the vehicle in metres. |
| `OMSI_PAX_CAM` | number | Offscreen `--view` pax: the n-th passenger camera. |
| `OMSI_PHOTO` | text | Photo mode settings for a test, key=value by commas: orbit (dist, around, up), kmh (the bus moving), focal (mm), dof, fstop (f-number), focus (m; none: on the bus), motion, shutter (1/s), filter (0..10), vignette, grain, exposure (EV), aspect (0..7), grid (0..3), quality (0..3). |
| `OMSI_WARM_FRAMES` | number; default `0` | Offscreen: frames drawn before the picture. |
| `OMSI_RENDER_CLOCK` | number; default `0` | Seconds the animation clock starts on (offscreen pictures). |
| `OMSI_RECORD` | number | Offscreen `--drive`: a picture every 1/this second into <out>_frames/ and the run's mixed sound into <out>.wav (films with sound). |
| `OMSI_RECORD_FROM` | number; default `0` | With `OMSI_RECORD`: start the film this many seconds into the drive. |
| `OMSI_BATCH` | number | Offscreen: prepare the map tiles this many at a time, as the window's streaming does. |
| `OMSI_CHURN` | text | Offscreen: x,y - load the tiles round that point and unload/reload the start area (streaming test). |
| `OMSI_BUDGET_FROM` | text | Offscreen with `OMSI_TEXTURE_MEMORY`: x,y[,MB] - the texture budget is met as seen from that point. |
| `OMSI_BLEND_AB` | flag | Offscreen: a second picture with the blended draws in the old order, for a before/after of the draw order. |
| `OMSI_SCENE_DIGEST` | flag | Log a hash of the renderer's scene (its instances in order, their places, materials and parameters) before each offscreen picture: two runs whose renderers saw the same calls log the same digests. |
| `OMSI_POPULATION_SHOTS` | flag | Offscreen: pictures of the framed population spawns. |

### Driving and world state

| Variable | Values / default | What it does |
| --- | --- | --- |
| `OMSI_AUTOPILOT` | number | Offscreen: the player's bus follows the lanes at this speed in km/h (finds where it falls through or leaves the road). |
| `OMSI_DRIVE_PROFILE` | text | Offscreen: "t throttle brake [steer]/..." piecewise constant pedals (crash/kerb tests). |
| `OMSI_DRIVE_V0` | number | Offscreen: the bus's speed in km/h on the first frame. |
| `OMSI_SEED` | number; default: random | Seed of the scripts' random numbers (repeat a session). |
| `OMSI_JOINT_ANGLE` | number | Degrees: the rear section of an articulated bus held at that angle. |
| `OMSI_SPOT_SELECT` | number | Turn on spotlight n (headlight pictures). |
| `OMSI_FLASHLIGHT` | flag | Offscreen: the player's flashlight is on (in the window it is Alt+F). |
| `OMSI_TROLLEY_UP` | flag | Start with the trolleybus's poles put on the wire by themselves (tests and demos). |
| `OMSI_TROLLEY_HANDS` | text | Offscreen: scripted hands on the poles' ropes, "from-to:raise\|pull:poles" separated by ";" (seconds; poles 0, 1 or 01): the player aims at the wire behind the bus. |
| `OMSI_TROLLEY_DEAD` | text | Section ids of the overhead wire (comma separated) that carry no current, whatever trolley.txt says. |
| `OMSI_TROLLEY_DEAD_AT` | text | x,y,r;...: the overhead wire the map derives within r metres of each point carries no current (a dead section between two insulators). |
| `OMSI_DRIVER_HANDS` | text | left,right: both driver hands held at these angles (grip close-ups). |
| `OMSI_DRIVER_SHIFTER` | text | Part of a variable or file name: force the shifter the driver's hand uses; off: both hands on the wheel. |
| `OMSI_PAX_WAITING` | number | Number of passengers waiting at each stop. |
| `OMSI_PAX_CROSS` | text | x,y: send pedestrians across the signalised crossing nearest that point. |
| `OMSI_FLEET_AHEAD` | number; default: built-in | Minutes ahead the timetable fleet is planned (for tests). |
| `OMSI_FLEET_IDLE` | number; default: built-in | Seconds a fleet vehicle idles (shortened for tests). |
| `OMSI_CABIN_LOG` | flag | Offscreen `--drive`: print the cabin air and the film on the windscreen, sides and rear every 20 s. |
| `OMSI_CONDENSATION` | text | Offscreen: minutes,people[,engine] - the cabin air and condensation on the glass after that long. |
| `OMSI_GLASS_WIND` | number | Offscreen: the rain on the glass as met at this speed in m/s. |
| `OMSI_WETNESS` | number; default: weather | Ground wetness the picture is drawn with. |
| `OMSI_DAY_AIR` | text | haze,angstrom[,height,strat]: fix the day's air (twilight colours) instead of the varying one. |
| `OMSI_ROAD_SNOW_GRID` | flag | With the roads' snow: a 4 m grid in the track field instead of the lanes' ruts (to see it lie on the world). |
| `OMSI_ROAD_SNOW_LOG` | flag | With the roads' snow: log its cover, the snow fallen, the weather and what it costs a frame (mean, worst, rut texels drawn) every ten seconds. |

### Navigator

| Variable | Values / default | What it does |
| --- | --- | --- |
| `OMSI_NAV_MAP` | flag | Open the navigation map. |
| `OMSI_NAV_MAP_MPP` | number; default `2.5` | Navigation map zoom in metres per pixel. |
| `OMSI_NAV_SCHEDULE` | flag | Navigation map shows the schedule. |
| `OMSI_NAV_LINES` | text | Offscreen: open the city map's lines panel on the line with this number (a route, then a departure time, after semicolons: 5;1 or 5;1;14:07, which opens its timetable). |
| `OMSI_NAV_ORIENT` | default `map` | Offscreen: how the city map is turned: map (as it is drawn), north (real north up, where the package says it lies) or heading (the bus's heading up). |
| `OMSI_NAV_CONTOURS` | flag | Offscreen: the city map in its terrain view (contour lines and their heights, when the map's package has them). |
| `OMSI_NAV_2D` | flag | Navigator pictures with flat houses (the nav_3d setting off). |
| `OMSI_NAV_PROBE` | text | x,y[,r]: the network lanes that start or end within r of that point. |
| `OMSI_NO_MAP_CACHE` | flag | Read the map's navigator layers (houses, ground, paving) again instead of from the cache. |

### Map and model checks

Each check logs what it finds while the map loads or the bus drives.

| Variable | Values / default | What it does |
| --- | --- | --- |
| `OMSI_CHECK_ENTRIES` | flag | Map check: is there ground to stand on where a player is put down. |
| `OMSI_CHECK_GROUND` | flag | Check: people on foot with a walkable surface above their heads, every two seconds. |
| `OMSI_CHECK_OBJECTS` | flag | Map check: report placed objects that float or are out of place while loading tiles. |
| `OMSI_CHECK_OBSTACLES` | flag | Map check: sweep a bus-sized box along every lane and list the obstacle boxes it hits. |
| `OMSI_CHECK_OVERLAP` | flag | Check: every AI vehicle whose body has got into another's or into an obstacle. |
| `OMSI_CHECK_ROADS` | flag | Map check: lanes with no road surface drawn under them, and road points under the ground. |
| `OMSI_CHECK_SPIKES` | flag | Map check: road faces standing taller than profile, gradient and cant allow. |
| `OMSI_CHECK_SPLINES` | flag | Map check: chained splines whose ends do not meet in height. |
| `OMSI_CHECK_TPOSE` | flag | Check: people drawn in the rest (T) pose, wider than 1.3 m hand to hand. |
| `OMSI_CHECK_TRIPS` | text | 1: build every trip's route on the loaded lanes and report breaks; a trip name: log every step of that trip. |
| `OMSI_CHECK_TYPES` | flag | Map check: list the object types per folder that loading leaves out. |
| `OMSI_CHECK_WALLS` | flag | Check: everybody inside a bus who stands away from its walkways. |
| `OMSI_CHECK_WHEELS` | flag | Map check: probe the ground along every lane's wheel tracks (invisible walls, humps, holes). |
| `OMSI_CLICK_ALL` | flag | Check: aim at every clickable item of the cab and report which one would actually be operated. |
| `OMSI_TRIGGER_ALL` | flag | Check: fire every [mouseevent] of the model and compare the variables before and after. |
| `OMSI_ROAD_PHOTO` | flag | Map check: photograph the road network from above and report grass where a carriageway should be. |
| `OMSI_ROAD_PHOTO_N` | number; default `400` | With `OMSI_ROAD_PHOTO`: number of sample points. |
| `OMSI_ROAD_PHOTO_SIDE` | number; default `0` | With `OMSI_ROAD_PHOTO`: metres to either side of the carriageway. |
| `OMSI_ROAD_PHOTO_SLANT` | number | With `OMSI_ROAD_PHOTO`: from a driver's eye this far back instead of from above. |
| `OMSI_HOLE_PHOTO` | flag | With `OMSI_ROAD_PHOTO`: photograph from above down to 25 m under the lane (holes in the world). |
| `OMSI_PROBE` | text | x0,y0,x1,y1[,n]: print terrain and road surface heights along a line. |
| `OMSI_PROBE_GRID` | text | x,y,half,step: the wheels' ground on a grid around a point. |
| `OMSI_GROUND_SAMPLE` | text | Offscreen CSV: what the wheels stand on every metre along the lanes near the start. |
| `OMSI_LANES_NEAR` | text | x,y,r: log the driving lanes passing there. |

### Loading only part of the scene

| Variable | Values / default | What it does |
| --- | --- | --- |
| `OMSI_ONLY_OBJECT` | text | Load only the objects whose file name contains this text. |
| `OMSI_SKIP_OBJECT` | text | Leave out the objects whose file name contains this text. |
| `OMSI_ONLY_MESH` | text | a\|b: draw only the meshes whose file names contain one of the parts. |
| `OMSI_HIDE_MESH` | text | a\|b: leave out the meshes whose file names contain one of the parts. |
| `OMSI_ONLY_SURFACES` | flag | Draw only the surfaces. |
| `OMSI_SKIP_PIPE` | text | Pipeline kinds to leave out of the main pass (comma-separated numbers). |
| `OMSI_SHADER_HOT` | folder, or `1` | Read the renderer's WGSL shaders from this copy of a renderer crate's `src` folder instead of the compiled-in copy, to try shaders without a rebuild. `1`: the source tree the game was built from. |

### Window, input and fault injection

`OMSI_INPUT` takes a script of timed commands separated by `;`, for example
`t=3 move 1045,826; t=3.2 press; t=3.5 drag 0,-80; t=4 release; t=5 key F3`. Coordinates are
logical pixels and `drag` is relative. Other commands: `look yaw,pitch`, `turn dx,dy`,
`set name=value` (a script variable), `trigger name`, `log name`, `dumptex <folder>`,
`shot <file>`, `notify <id> <seconds> <kind> <text>` and `type <text>` (into the LAN chat line
that `key V` opened).

| Variable | Values / default | What it does |
| --- | --- | --- |
| `OMSI_INPUT` | script | Scripted keyboard, mouse and camera input for window runs (syntax above). |
| `OMSI_BACKGROUND` | flag | A test window that does not take the keyboard focus (`OMSI_INPUT` drives the handlers directly); with `OMSI_INPUT` it ignores the real mouse too. |
| `OMSI_HIDDEN_WINDOW` | flag | The game's window is made and never shown (no Dock icon on macOS): a run of the whole game (its frame, the plugins, `--exit-after`) beside whoever works at the screen; its frames are drawn into a texture, for benchmarks. |
| `OMSI_HIDE_WINDOW` | text | from,to: treat the window as hidden between these seconds. |
| `OMSI_RENDER_OCCLUDED` | flag | Draw frames into a texture while the window is hidden (macOS gives none). |
| `OMSI_MOBILE` | flag | The phone layout on a computer (screen cut-outs). |
| `OMSI_TOUCH` | flag | The on-screen touch controls on a computer. |
| `OMSI_MUTE` | flag | Mix everything as usual but play nothing. |
| `OMSI_VOICE_TEST_TONE` | text | Voice chat: the microphone is a synthetic tone (1 = 440 Hz, or a pitch in Hz) and the device is never opened - tests of the whole chain without a person. |
| `OMSI_LAN_SAY` | text | "30=Hallo;45=Bye": LAN chat lines said at these seconds of the session. |
| `OMSI_LAN_AUDIO` | flag | Offscreen LAN: with the other buses' sounds. |
| `OMSI_BRIDGE_ONLY` | flag | LAN: forget our own addresses so that only what the rendezvous bridge reports is tried. |
| `OMSI_CLOUDFLARED_OWN` | flag | LAN tunnel: only the game's own downloaded cloudflared, to test the download. |
| `OMSI_APPLE_EXIT_AFTER` | number | iPhone/iPad: the session ends by itself after this many seconds (checks the way back to the Swift launcher without a person). |
| `OMSI_MX_SOFTWARE` | flag | Let mX Graphics draw on a software renderer (WARP, llvmpipe) on a computer; without it a computer where only one opens starts again on wgpu. |
| `OMSI_GPU_LIMITS` | `default` or `downlevel` | Request only the WebGPU default (or downlevel) limits. Set by the `small_chip` test. |
| `OMSI_FAKE_GPU_ERROR` | `open`, `open-panic`, `pipeline`, `lost-build`, `build`, `lost`, `frame` or `mesh` | Simulate this GPU error. Only in builds with the renderer's `test-hooks` feature (on by default). |
| `OMSI_GUARD_FORCE` | flag | The start guard also watches a development build (one run from a cargo target folder). |
| `OMSI_GUARD_TEST_BREAK` | flag | With it, a file guard-test-break beside the program (panic, exit, hang or nothing) breaks this start before its first frames on purpose (scripts/guard-e2e.sh). |
| `OMSI_GUARD_DIALOG_FILE` | text | The start guard writes the messages it would show the player into this file instead (scripts/guard-e2e.sh). |
| `OMSI_UI_PREVIEW` | default `target/ui-preview.png` | Plugin UI preview test: the picture's path. |

### Test suite only

Read only by `cargo test` and the examples, never by the game.

| Variable | Values / default | What it does |
| --- | --- | --- |
| `OMSI_TEST_CONTENT` | text | Content root for tests that need real OMSI content. |
| `OMSI_ORIGINAL` | text | vehicle_vars example: the original OMSI folder. |
| `OMSI_AUDIT_LINE` | number | bus_audit example: the line (number and letter code) the IBIS typist enters instead of the bus's own. |
| `OMSI_PICK_BUSES` | text | cargo test -p omsi-app installed_switches_stay_reachable_from_inside: more .bus files to check, by commas (under the root, or absolute). |
| `OMSI_API_BLESS` | flag | cargo test -p omsi-plugin api_manifest: write docs/plugin-api.json and the reference tables of docs/PLUGINS.md again from the plugin API registry. |
| `OMSI_TEST_AVATAR` | text | Test only: an avatar FBX for the avatar loading test (`cargo test -p omsi-sim a_rocketbox_avatar_loads -- --ignored`). |
| `OMSI_TEST_CLIPS` | text | Test only: a folder of Rocketbox motion clips (`f_walk_neutral_01.fbx`, ...) for the clip test (`cargo test -p omsi-sim rocketbox_clips -- --ignored`, with `OMSI_TEST_AVATAR`). |
| `OMSI_TEST_PACKAGE_MAP` | text | Test only: the global.cfg of a map that carries a package, for the launcher route map test (`cargo test -p omsi-launcher-model the_package_of_a_real_map_is_exported -- --ignored`). |
| `OMSI_TEST_WINE_DIR` | text | Plugin demo test: folder holding omsi-plugin-host.exe for Wine. |
| `OMSI_TEST_LEGACY_TROLLEY_DRIVE` | flag | Test only: set by the trolley test on the child process it starts to run itself with the realistic physics on. |
| `OMSI_TOBII_RAW` | text | Tests: a file for the Tobii tracker's raw poses (record_the_tracker_here). |
| `OMSI_TOBII_SECS` | number; default `8` | Tests: how long the Tobii tracker is recorded. |

## A/B and kill switches

Each switch turns one feature off or brings back an older way of doing it, to compare pictures
or behaviour, or to work around a problem. "A/B" in a description means the switch exists for
comparing the two.

### Rendering

| Variable | Values / default | What it does |
| --- | --- | --- |
| `OMSI_CUT_PLAIN` | flag | Road-cut textures uploaded without GPU-made mipmaps. |
| `OMSI_FORCE_OPAQUE` | flag | Draw transmap-masked materials opaque. |
| `OMSI_FSR_RAW_INPUT` | flag | FSR 1 reads the scene through a view that is not sRGB: no square roots in EASU (not on OpenGL) - to measure. |
| `OMSI_GL_TEXTURE_UNITS` | flag | Use the texture-unit layout of the OpenGL backend on any device. |
| `OMSI_GPU_ARRAYS` | `textures`, `nostorage` or `lighttextures` | Take that texture array path on any device. |
| `OMSI_LAMP_SHADOWS_EVERY_FRAME` | flag | Redraw the street lamps' shadow tiles every frame (not only when they change). |
| `OMSI_LIGHT_GRID_SQUARES` | flag | A light goes into every cell of the light grid's square round its range, the corners its range misses too (A/B). |
| `OMSI_MAIN_SPLIT` | flag | Split the main pass's bundles in two (encoded apart, the second part loads what the first stored). |
| `OMSI_MIRROR_ENHANCED` | flag | Draw the mirrors with the enhanced shading again. |
| `OMSI_NOZCHECK_BIAS` | flag | The old reading of [matl_noZcheck] (A/B). |
| `OMSI_NO_AO` | flag | No ambient occlusion pass. |
| `OMSI_NO_AO_REACH` | flag | The depth prepass that only the ambient occlusion needs draws the far scenery too, past the occlusion's reach (A/B). |
| `OMSI_NO_BEAM_SHADOWS` | flag | Enhanced: the headlamps' and street lamps' beams in fog, rain and snow are not cut by what stands in them (the old picture). |
| `OMSI_NO_BOUNDS_CACHE` | flag | No cache of instance bounds. |
| `OMSI_NO_BUMP` | flag | No bump maps. |
| `OMSI_NO_BUNDLES` | flag | Record the main pass directly instead of from render bundles. |
| `OMSI_NO_CABIN_SHADOWS` | flag | Enhanced: the lamps of the player's saloon cast no shadows (A/B). |
| `OMSI_NO_CLOUDS` | flag | No clouds. |
| `OMSI_NO_CLOUD_SHADOW_FIELD` | flag | Enhanced: the clouds' shadow is one number for the whole picture again, not a field over the land (A/B). |
| `OMSI_NO_CORONAS` | flag | No light coronas. |
| `OMSI_NO_CULL` | flag | Draw every mesh from both sides (A/B). |
| `OMSI_NO_DRAW_ORDER_DEPTH` | flag | An object's surfaces in one plane, and a spline laid on an earlier one at its height, without the draw order's depth step (A/B). |
| `OMSI_NO_ENHANCED` | flag | Enhanced settings drawn with the plain path. |
| `OMSI_NO_ENVMAP` | flag | No environment maps. |
| `OMSI_NO_FSR_MIP_BIAS` | flag | FSR 1 without its textures' mip bias (to compare). |
| `OMSI_NO_FXAA` | flag | No FXAA. |
| `OMSI_NO_GLARE` | flag | No sun glare. |
| `OMSI_NO_GLASS_PICTURE` | flag | No picture behind the glass (rain films). |
| `OMSI_NO_GROUND_PAINT` | flag | No painted ground layers. |
| `OMSI_NO_GROUND_PASS` | flag | The terrain's paint layers lit one by one again, not laid into one albedo picture first (A/B). |
| `OMSI_NO_GROUND_SPLINE_BATCHING` | flag | No batching of ground splines. |
| `OMSI_NO_LAMP_SIGHT` | flag | Enhanced by night: every lamp near the line of sight dazzles, hidden or not. |
| `OMSI_NO_LEAN` | flag | Enhanced main pass: draw every material with the full scene pipelines, not the lean ones of plain materials on a dry frame. |
| `OMSI_NO_LIGHT_MAP` | flag | No night light maps on the tiles (A/B). |
| `OMSI_NO_MATERIAL_SPLINE_BATCHING` | flag | No batching of splines by material. |
| `OMSI_NO_MODEL_ORDER` | flag | Draw the opaque parts of ordered models first again (A/B). |
| `OMSI_NO_MSAA_PREPASS` | flag | No depth prepass with multisampling. |
| `OMSI_NO_NAV_CULL` | flag | The navigator draws all of its ground and houses every frame, not only what its camera sees (A/B). |
| `OMSI_NO_OWN_VEHICLE_LAMP_BLOCK` | flag | Other vehicles' lamps light the player's vehicle inside and the street beyond it through its body again (A/B). |
| `OMSI_NO_PBR` | flag | No PBR materials. |
| `OMSI_NO_POLL_THREAD` | flag | No device poll thread. |
| `OMSI_NO_POST_SPLIT` | flag | Finish the passes after the main pass in the main pass's own command buffer (A/B). |
| `OMSI_NO_PRECIP_ROOF` | flag | Rain and snow fall through roofs (no roof map). |
| `OMSI_NO_PUDDLE_GLASS_DEPTH` | flag | Puddles without glass depth. |
| `OMSI_NO_PUDDLE_REFLECTIONS` | flag | No puddle reflections. |
| `OMSI_NO_PUDDLE_VEHICLE` | flag | No vehicles in puddle reflections. |
| `OMSI_NO_RAIN` | flag | No rainfall. |
| `OMSI_NO_RENDER_POOL` | flag | No encoding thread pool for rendering. |
| `OMSI_NO_RT` | flag | No ray queries (Enhanced+ draws as Enhanced). |
| `OMSI_NO_RT_FRAME` | flag | Ray queries kept but nothing traced. |
| `OMSI_NO_RT_GRADE` | flag | Enhanced+ without its colour grade. |
| `OMSI_NO_SHADOWS` | flag | No shadows. |
| `OMSI_NO_SMOKE` | flag | No smoke. |
| `OMSI_NO_SNOWFALL` | flag | No snowfall. |
| `OMSI_NO_SPLINE_BATCHING` | flag | No spline batching. |
| `OMSI_NO_SPRAY` | flag | No tyre spray. |
| `OMSI_REPAIR_BODY_DEPTH` | flag | The old guess for [matl_alpha] 2 vehicle bodies (A/B). |
| `OMSI_RT_REFL_HALF` | flag | Trace reflections at half size. |
| `OMSI_SERIAL_PIPELINES` | flag | Compile the renderer's pipelines one after the other on one thread, as before (A/B). |
| `OMSI_SHADOW_FAR_EVERY_FRAME` | flag | Redraw the far shadow map every frame. |
| `OMSI_SHADOW_NEAR_EVERY_FRAME` | flag | Redraw the near shadow map every frame. |
| `OMSI_SKY_CUBE_EVERY_FRAME` | flag | Enhanced: redraw a slice of the sky cube every frame again, not at most 60 a second (A/B). |

### World, traffic and physics

| Variable | Values / default | What it does |
| --- | --- | --- |
| `OMSI_AI_MODEL_LOCK` | flag | AI cars steer no further than their model's own steering lock (no 60 degree allowance for tight turns). |
| `OMSI_AI_WAY_ONLY` | flag | AI vehicles stand on their way with the plain ground sampler, as before (A/B). |
| `OMSI_CROSSING_DEFORM` | flag | Apply the crossings' ground deformation again (A/B; Omsi.exe does not). |
| `OMSI_DEBUG_RAIL` | flag | Log a rail vehicle's speed, requested drive and brake, and adhesion, twice a second. |
| `OMSI_HEIGHTPROFILE_GROUND` | flag | The wheels stand on the splines' [heightprofile]s again (A/B). |
| `OMSI_NO_ANIMPARENT` | flag | Every mesh animated on its own, without [animparent] (A/B). |
| `OMSI_NO_ATTACH_FALLBACK` | flag | Drop attachments that have no fallback instead of placing them (A/B). |
| `OMSI_NO_BEND_CACHE` | flag | AI cars look for the bends ahead anew every step (A/B). |
| `OMSI_NO_INTERP` | flag | The old way without interpolation, for comparing. |
| `OMSI_NO_SPLINE_HOLES` | flag | Keep the unfitted terrain mesh beneath aligned splines. |
| `OMSI_NO_SURF` | flag | Every road as smooth as before (no surface texture) (A/B). |
| `OMSI_NO_WHEEL_SLIP` | flag | Every wheel grips, as before wheels turned on their own (A/B). |
| `OMSI_OLD_WORLD_GRID` | flag | Maps with world coordinates take the one tile size of 371.9 m (comparison). |
| `OMSI_ROAD_CUT` | flag | Take the ground away under every road surface (Omsi.exe does not). |
| `OMSI_TERRAIN_ALIGN` | flag | Align the terrain to the splines again at load (A/B; Omsi.exe does not). |
| `OMSI_TYRE_SUSPENSION` | flag | The old suspension with wheel mass, tyre and bump stops (A/B). |

## Apple and launcher variables

These are read outside the game's Rust code, so the list in `flags.rs` does not hold them.

| Variable | Values / default | What it does |
| --- | --- | --- |
| `OMSI_APPLE_PAGE` | text | iPhone/iPad launcher: open on a page without a person at it: a page name (`settings`, `games`, ...), `tab:more`, `picker:bus`, `sheet:trip` or `onboarding[:step]`. For the Simulator and CI. |
| `OMSI_APPLE_START` | number | iPhone/iPad launcher: press Start after this many seconds. |
| `OMSI_APPLE_ORIENT` | `landscape` or `portrait` | iPhone/iPad: turn the screen that way, to check either layout on a Simulator. |
| `OPENOMSI_LAUNCHER_SELFTEST` | `1` | Desktop launcher: the window stays hidden, the page checks itself (commands, pictures, font, every page), prints a report and the program exits. `0` or empty is off. |
| `OPENOMSI_WEBVIEW_FRESH` | flag | Windows launcher: set the WebView2 profile aside and start with a fresh one. The game sets it when the launcher window could not open. |
| `OPENOMSI_NO_WEBVIEW` | flag | Windows: the desktop launcher could not open for lack of WebView2. The built-in launcher then offers to install it. Set by the game. |
| `OPENOMSI_BIN` | path | The game program the launcher starts. Set by the game when it opens the launcher. |
| `OPENOMSI_CONTAINER` | path | iPhone/iPad: the app's container folder. Set by the app at start. |
| `OPENOMSI_FONTS` | folder | Apple launcher screenshot tool: where to find the Inter font. |

## Build-time

Read when compiling, not at runtime.

| Variable | Values / default | What it does |
| --- | --- | --- |
| `OMSI_BUILD` | set by `build.rs` | The commit the binary was built from (`env!`). Not read at runtime. |

| Variable | What it does |
| --- | --- |
| `OPENOMSI_VERSION` | The version baked into the program. CI sets it; otherwise `build.rs` works it out as `scripts/version.sh` does (see [Versions and releases](VERSIONING.md)). |
| `OPENOMSI_STORE` | A Mac build for the App Store: the game never looks for or offers updates. iPhone and iPad builds always behave so. |
| `OPENOMSI_MARKETING_VERSION`, `OPENOMSI_BUILD_NUMBER`, `OPENOMSI_TEAM_ID` | Xcode project (`apple/project.yml`): the app's version, build number and signing team. |
| `OPENOMSI_RUST_TARGET`, `OPENOMSI_RUST_PROFILE` | Xcode project: where the Rust libraries are (default `../target`, `release`). |
