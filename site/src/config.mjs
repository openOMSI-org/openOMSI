// What the site knows about the project: where it lives, how the documentation is grouped, which
// builds exist. Everything else (titles, summaries, page splits) is read from the Markdown itself.

export const REPO = "openOMSI-org/openOMSI";
export const MAPPER_REPO = "openOMSI-org/openMapper";
export const PRESENCE = "https://openomsi.savvabestbrother.workers.dev";
export const DISCORD = "https://discord.gg/openomsi";
export const DONATE = [
  { name: "Buy Me a Coffee", url: "https://buymeacoffee.com/usonskyyy" },
  { name: "Ko-fi", url: "https://ko-fi.com/usonance" },
];
export const SITE_URL = "https://openomsi.org";

// A document is split into one page per `##` chapter when it is longer than this (bytes).
export const SPLIT_BYTES = 26000;
// ... and a chapter that is still longer than this is split again at its `###` sections.
export const SPLIT_CHAPTER_BYTES = 28000;

// Documentation, by who reads it and why. `file` is docs/<file>.md; `slug` the address.
// `title` and `summary` override what the Markdown says when its own are too long or too plain.
export const DOC_GROUPS = [
  {
    id: "start",
    title: "Getting started",
    blurb: "Install, set up, drive.",
    docs: [
      { file: "GETTING_STARTED", title: "Getting started", summary: "Install openOMSI, point it at OMSI 2 and drive your first bus." },
      { file: "DRIVING", title: "Driving", summary: "Keys, cameras, starting a bus, doors, tickets and the timetable." },
      { file: "LAUNCHER", title: "Launcher and settings", summary: "Every screen of the launcher and every setting, page by page." },
      { file: "TROUBLESHOOTING", title: "Troubleshooting", summary: "Slow frames, a version that does not start, logs and bug reports." },
    ],
  },
  {
    id: "play",
    title: "Playing",
    blurb: "Graphics, mods, weather, platforms.",
    docs: [
      { file: "GRAPHICS", title: "Graphics", summary: "Renderers, quality settings, DLSS and FSR, mirrors." },
      { file: "MODS", title: "Mods", summary: "The content folder, installing mods and archives, the object editor." },
      { file: "WEATHER", title: "Weather and seasons", summary: "Season, weather files and live weather from Tomorrow.io." },
      { file: "RADIO", title: "Radio", summary: "Stations, your own music and the radio in the cab." },
      { file: "ANDROID", title: "Android", summary: "The full game on phones and tablets: install, controls, storage." },
      { file: "APPLE", title: "iPhone, iPad and Mac", summary: "The Apple launcher and builds." },
      { file: "VR", title: "Virtual reality", summary: "Drive in an OpenXR headset on Windows." },
      { file: "triple-screen", title: "Triple screen", summary: "Three projections for a three-monitor setup." },
      { file: "THEMES", title: "Interface themes", summary: "Colours and looks of the game's own screens." },
      { file: "ADDONS", title: "Add-ons", summary: "Tools and content collections others make for openOMSI." },
      { file: "COMMAND_LINE", title: "Command line", summary: "Every flag of the game, and the developer tools in the window." },
    ],
  },
  {
    id: "multiplayer",
    title: "Multiplayer",
    blurb: "Sessions, servers, voice.",
    docs: [
      { file: "MULTIPLAYER", title: "Playing together", summary: "Host a session, join one, and what the players share." },
      { file: "SERVER", title: "Dedicated server", summary: "Run a map without a window: setup, configuration, administration, bans." },
      { file: "VOICE", title: "Voice chat", summary: "Push-to-talk between players in a session." },
      { file: "TRAFFIC_TABLE", title: "The traffic table", summary: "How the host's AI vehicles go over the network." },
      { file: "FREETEX_SYNC", title: "Freetex images", summary: "How a vehicle's own pictures reach other players." },
    ],
  },
  {
    id: "openmapper",
    title: "openMapper",
    blurb: "The map editor: install, tutorials, reference.",
    docs: [
      { file: "MAPPER", title: "openMapper", summary: "The map editor: install, first start and a tour of the window." },
      { file: "MAPPER_FIRST_MAP", title: "Tutorial 1: your first map", summary: "From an empty map to driving it in openOMSI." },
      { file: "MAPPER_REAL_PLACE", title: "Tutorial 2: a real place", summary: "OpenStreetMap, satellite imagery and real terrain heights." },
      { file: "MAPPER_LINES", title: "Tutorial 3: lines and traffic", summary: "Stops, lines, timetables, tours, depots and AI traffic." },
      { file: "MAPPER_TERRAIN", title: "Tutorial 4: terrain", summary: "Sculpting, height maps, ground textures, water and tiles." },
      { file: "MAPPER_MODELS", title: "Tutorial 5: your own models", summary: "Import, edit and texture models, with PBR maps." },
      { file: "MAPPER_TOOLS", title: "Tools reference", summary: "Every tool and panel of the editor." },
      { file: "MAPPER_KEYS", title: "Keys and mouse", summary: "Every shortcut and mouse action." },
      { file: "MAPPER_TROUBLESHOOTING", title: "Checks and problems", summary: "Map checks, saving and versions, collaboration, common problems." },
    ],
  },
  {
    id: "transport",
    title: "Routes and rail",
    blurb: "Timetables, trams, metros, trolleybuses.",
    docs: [
      { file: "ROUTES", title: "How routes work", summary: "Timetables, chrono scenarios, HOF files and IBIS, as OMSI 2 runs them." },
      { file: "TRANSPORT_MODES", title: "Transport modes", summary: "Which vehicles a map is for: bus, tram, metro, train." },
      { file: "TRANSPORT_FORMATS", title: "Transport folder", summary: "Stations, routes, wires, switches, depots and signals as text files." },
      { file: "RAIL", title: "Trams, metros and trains", summary: "Vehicle classes, track rules and a map's openomsi-rail.cfg." },
      { file: "TROLLEYBUS", title: "Trolleybuses", summary: "The overhead wire, poles, switches and the gear of a vehicle." },
      { file: "TROLLEY_MODS", title: "Real trolleybus add-ons", summary: "How openOMSI runs three real OMSI 2 trolleybus add-ons." },
    ],
  },
  {
    id: "content",
    title: "Making content",
    blurb: "Objects, vehicles, textures.",
    docs: [
      { file: "MODDING", title: "Modding overview", summary: "The limits openOMSI lifts and the keywords it adds." },
      { file: "PBR", title: "PBR materials", summary: "Normal, roughness, metalness and occlusion maps." },
      { file: "MAP_PACKAGE", title: "Map packages", summary: "A pre-built surface under the roads." },
      { file: "OBJECT_SCRIPTS", title: "Scripts of map objects", summary: "Lua or compiled scripts for scenery objects." },
      { file: "SUSPENSION", title: "Suspension types", summary: "Struts beyond the coil spring." },
      { file: "HTMLTEXTURE", title: "HTML textures", summary: "Draw a texture from an HTML page." },
      { file: "HTMLTEXTURE_MODERN", title: "HTML textures: modern engine", summary: "The opt-in engine with flexbox and modern CSS." },
      { file: "NAVIGATORTEXTURE", title: "Navigator textures", summary: "A GPS screen of its own in the cab." },
      { file: "PUBLISHER_PACKS", title: "Publisher packs", summary: "Provenance, dependencies and protected playback for published content." },
    ],
  },
  {
    id: "plugins",
    title: "Plugins",
    blurb: "Lua, WebAssembly and OMSI DLLs.",
    docs: [
      { file: "PLUGINS", title: "Writing plugins", summary: "Your first Lua plugin, manifests, panels, compiled plugins and OMSI DLLs." },
      { file: "PLUGIN_API", title: "Plugin API reference", summary: "Every omsi.* function, by namespace." },
    ],
  },
  {
    id: "reference",
    title: "Reference",
    blurb: "File formats, flags, versions.",
    docs: [
      { file: "FORMATS", title: "Content formats", summary: "Every OMSI 2 file format as openOMSI reads it." },
      { file: "DEBUG_FLAGS", title: "Environment variables", summary: "Every OMSI_* switch, with its values and default." },
      { file: "VERSIONING", title: "Versions and releases", summary: "What the version numbers mean, and the release channels." },
    ],
  },
  {
    id: "project",
    title: "The project",
    blurb: "How openOMSI is built.",
    docs: [
      { file: "BUILDING", title: "Building from source", summary: "Toolchains, targets and the commands." },
      { file: "ARCHITECTURE", title: "Architecture", summary: "Crates, threads, data flow, memory and coordinate frames." },
      { file: "MX_PORT", title: "Porting the renderer to mX", summary: "Moving the renderer from wgpu to openOMSI's own graphics library." },
      { file: "HISTORY", title: "History", summary: "A timeline of how openOMSI was built." },
      { file: "REFACTOR_CHECKS", title: "Refactor checks", summary: "How to show a refactor changed nothing." },
    ],
  },
  {
    id: "notes",
    title: "Notes and licensing",
    blurb: "Test plans, experiments, third-party terms.",
    docs: [
      { file: "UPSCALING_TESTS", title: "Testing DLSS and FSR 3", summary: "The runs a tester does before upscaling counts as working." },
      { file: "fsr1-experiment", title: "FSR 1 test build", summary: "Spatial upscaling with EASU and RCAS, opt-in." },
      { file: "performance-th-wald", title: "TH_Wald CPU bottleneck", summary: "Where the frame goes on one demanding spawn." },
      { file: "server-fleet-test-plan", title: "Server-owned fleet test plan", summary: "A reproducible regression plan." },
      { file: "UPSCALER_THIRD_PARTY_LICENSING", title: "Upscaler licensing", summary: "Decision record on bundling AMD and NVIDIA runtimes." },
      { file: "NVIDIA_DLL_LICENSE_SCOPE", title: "NVIDIA DLL license scope", summary: "What the optional NVIDIA runtime download covers." },
    ],
  },
];

// openMapper's documents now live in docs/ (MAPPER*.md). Nothing is read from its repository any more.
export const MAPPER_DOCS = [];
export const MAPPER_GROUP = { id: "mapper", title: "openMapper", blurb: "The map editor.", docs: [] };

// Documents that were renamed or merged: the old address sends the reader to the new one.
export const MOVED_DOCS = {
  "user-guide": "getting-started",
  "nvidia-dlss": "graphics",
  "weather-regions": "weather",
  "openmapper": "mapper",
  "mapper-tutorial": "mapper-first-map",
  "mapper-features": "mapper-tools",
  "mapper-roadmap": "mapper",
};
// ... and the chapters the old user guide was cut into
export const MOVED_PAGES = {
  "user-guide/checking-an-installation": "troubleshooting",
  "user-guide/running": "getting-started",
  "user-guide/the-launcher": "launcher",
  "user-guide/settings-enhanced-graphics-the-navigator": "graphics",
  "user-guide/mods-and-the-content-folder": "mods",
  "user-guide/season-and-weather": "weather",
  "user-guide/radio": "radio",
  "user-guide/performance": "troubleshooting",
  "user-guide/object-editor": "mods",
  "user-guide/developer-tools": "command-line",
  "user-guide/mirror-panels": "graphics",
  "user-guide/debug-and-test-switches": "debug-flags",
  "user-guide/lan-play": "multiplayer",
};

// Downloads: the file is openOMSI-<version>-<key><suffix>.
export const PLATFORMS = [
  { key: "windows-x64", setup: "-setup.exe", name: "Windows", arch: "x64", note: "Windows 10 or newer", os: "windows" },
  { key: "windows-arm64", setup: "-setup.exe", name: "Windows", arch: "ARM64", note: "Snapdragon laptops, Windows 11", os: "windows-arm" },
  { key: "macos-arm64", setup: ".dmg", name: "macOS", arch: "Apple silicon", note: "M1 or newer, macOS 11 or newer", os: "mac" },
  { key: "macos-x64", setup: ".dmg", name: "macOS", arch: "Intel", note: "macOS 11 or newer", os: "mac-intel" },
  { key: "linux-x64", name: "Linux", arch: "x86-64", note: "Vulkan or OpenGL, WebKitGTK 4.1", os: "linux" },
  { key: "linux-arm64", name: "Linux", arch: "ARM64", note: "Vulkan or OpenGL, WebKitGTK 4.1", os: "linux-arm" },
  { key: "android-arm64", ext: "apk", name: "Android", arch: "arm64", note: "Android 8.0 or newer, Vulkan 1.1", os: "android" },
];
export const SERVERS = [
  { key: "server-windows-x64", name: "Windows", arch: "x64", note: "no window" },
  { key: "server-windows-arm64", name: "Windows", arch: "ARM64", note: "no window" },
  { key: "server-linux-x64", name: "Linux", arch: "x86-64", note: "no window" },
  { key: "server-linux-arm64", name: "Linux", arch: "ARM64", note: "Raspberry Pi 4/5, ARM VPS" },
];

// Old addresses (https://openomsi.org/#/docs/USER_GUIDE) keep working: the home page sends them on.
export const LEGACY_ROUTES = {
  "/download": "/download/",
  "/beta": "/download/beta/",
  "/releases": "/releases/",
  "/releases/beta": "/releases/beta/",
  "/issues": "/issues/",
  "/mapper": "/docs/mapper/",
};
