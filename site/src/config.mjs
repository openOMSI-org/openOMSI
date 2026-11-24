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
    id: "play",
    title: "Playing",
    blurb: "Install, set up, drive.",
    docs: [
      { file: "USER_GUIDE", title: "User guide", summary: "Running the game, the launcher, settings, mods, weather, radio, performance and LAN play." },
      { file: "ANDROID", title: "Android", summary: "The full game on phones and tablets: install, controls, storage." },
      { file: "APPLE", title: "iPhone, iPad and Mac", summary: "The SwiftUI launcher and the Apple builds." },
      { file: "VR", title: "Virtual reality", summary: "Drive in an OpenXR headset on Windows." },
      { file: "triple-screen", title: "Triple screen", summary: "Three projections for a three-monitor setup." },
      { file: "THEMES", title: "Interface themes", summary: "Colours and looks of the game's own screens." },
      { file: "ADDONS", title: "Add-ons", summary: "Tools and content collections others make for openOMSI." },
      { file: "WEATHER_REGIONS", title: "Tomorrow.io weather", summary: "Live weather that follows the map's area." },
      { file: "NVIDIA_DLSS", title: "DLSS and FSR 3", summary: "Temporal upscaling: what to switch on and what to install." },
    ],
  },
  {
    id: "multiplayer",
    title: "Multiplayer",
    blurb: "Sessions, servers, voice.",
    docs: [
      { file: "SERVER", title: "Dedicated server", summary: "Run a whole map without a window: configuration, uploads, administration, bans, the public list." },
      { file: "VOICE", title: "Voice chat", summary: "Push-to-talk between players in a session." },
      { file: "TRAFFIC_TABLE", title: "The traffic table", summary: "How the host's AI vehicles go over the LAN." },
      { file: "FREETEX_SYNC", title: "Freetex images", summary: "How a vehicle's own pictures reach other players." },
    ],
  },
  {
    id: "transport",
    title: "Routes and rail",
    blurb: "Timetables, trams, metros, trolleybuses.",
    docs: [
      { file: "ROUTES", title: "How routes work", summary: "Timetables, chrono scenarios, car_use, HOF files and IBIS, as OMSI 2 runs them." },
      { file: "TRANSPORT_MODES", title: "Transport modes", summary: "Which vehicles a map is for: bus, tram, metro, train." },
      { file: "TRANSPORT_FORMATS", title: "Transport folder", summary: "Stations, routes, wires, switches, depots and signals as text files." },
      { file: "RAIL", title: "Trams, metros and trains", summary: "Vehicle classes, track rules and the openomsi-rail.cfg of a map." },
      { file: "TROLLEYBUS", title: "Trolleybuses", summary: "The overhead wire, poles, switches and the gear of a vehicle." },
      { file: "TROLLEY_MODS", title: "Real trolleybus add-ons", summary: "What three real OMSI 2 add-ons do, and how openOMSI runs them." },
    ],
  },
  {
    id: "content",
    title: "Making content",
    blurb: "Maps, objects, vehicles, textures.",
    docs: [
      { file: "OPENMAPPER", title: "openMapper", summary: "The map editor for openOMSI." },
      { file: "MODDING", title: "Modding beyond OMSI 2", summary: "The limits openOMSI lifts, and the keywords that go with them." },
      { file: "PBR", title: "PBR materials", summary: "Normal, roughness, metalness and occlusion maps." },
      { file: "MAP_PACKAGE", title: "Map packages", summary: "A pre-built surface under the roads." },
      { file: "OBJECT_SCRIPTS", title: "Scripts of map objects", summary: "Lua or compiled scripts for scenery objects." },
      { file: "SUSPENSION", title: "Suspension types", summary: "Struts beyond the coil spring." },
      { file: "HTMLTEXTURE", title: "HTML textures", summary: "Draw a texture from an HTML page." },
      { file: "HTMLTEXTURE_MODERN", title: "HTML textures: modern engine", summary: "The opt-in engine with flexbox, positioning and modern CSS." },
      { file: "NAVIGATORTEXTURE", title: "Navigator textures", summary: "A GPS screen of its own in the cab." },
      { file: "PUBLISHER_PACKS", title: "Publisher packs", summary: "Opt-in provenance, dependency references and protected playback for published content." },
    ],
  },
  {
    id: "plugins",
    title: "Plugins",
    blurb: "Lua, WebAssembly and OMSI DLLs.",
    docs: [
      { file: "PLUGINS", title: "Plugins", summary: "Lua plugins, compiled .oop plugins, panels, the whole omsi.* API and OMSI's own plugin DLLs." },
    ],
  },
  {
    id: "reference",
    title: "Reference",
    blurb: "File formats, flags, versions.",
    docs: [
      { file: "FORMATS", title: "Content formats", summary: "Every OMSI 2 file format as openOMSI reads it." },
      { file: "DEBUG_FLAGS", title: "OMSI_* environment variables", summary: "Every switch, with its type, default and where it is read." },
      { file: "VERSIONING", title: "Versioning and releases", summary: "What MAJOR.MINOR.PATCH means and how a release is made." },
    ],
  },
  {
    id: "project",
    title: "The project",
    blurb: "How openOMSI is built.",
    docs: [
      { file: "BUILDING", title: "Building from source", summary: "Toolchains, targets and the commands." },
      { file: "ARCHITECTURE", title: "Architecture", summary: "Crates, threads, memory, coordinate frames and the layering of targets." },
      { file: "MX_PORT", title: "Porting the renderer to mX", summary: "Moving the renderer from wgpu to openOMSI's own graphics library." },
      { file: "HISTORY", title: "History and roadmap", summary: "The round-by-round record of how openOMSI was built." },
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

// openMapper keeps its own documents in its own repository: they are read at build time.
export const MAPPER_DOCS = [
  { file: "TUTORIAL", slug: "mapper-tutorial", title: "openMapper: your first map", summary: "From an empty map to driving it in openOMSI." },
  { file: "FEATURES", slug: "mapper-features", title: "openMapper: features and controls", summary: "Everything the editor can do, and its keys." },
  { file: "ROADMAP", slug: "mapper-roadmap", title: "openMapper: roadmap", summary: "What is planned." },
];
export const MAPPER_GROUP = { id: "mapper", title: "openMapper", blurb: "The map editor.", docs: [] };

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
  "/mapper": "/docs/openmapper/",
};
