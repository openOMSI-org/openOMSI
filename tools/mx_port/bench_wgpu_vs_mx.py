#!/usr/bin/env python3
"""Frame time and memory of openOMSI builds on the same scenes: wgpu against mX, and against
earlier releases.

usage:
  bench_wgpu_vs_mx.py <OMSI 2 root> --build LABEL=PATH[:mx|:wgpu] [--build ...]
                      [--frames N] [--runs N] [--size WxH] [--out results.csv]
  bench_wgpu_vs_mx.py <openomsi binary> <OMSI 2 root>     (the binary as wgpu and as mX)

A build is a game binary; `:mx` and `:wgpu` set the mX Graphics setting for it (a binary
from before the setting ignores it). Each run is an offscreen render with OMSI_BENCH=N: the
final picture drawn N more times, mirrors included when a bus is driven, and the game logs
the median drawing time (CPU), the median wait for the GPU and the mean and worst frame.
Memory is the process's peak: the peak memory footprint on macOS (it counts the GPU's
memory too, which is shared there), the peak resident set on Linux and the peak working set
on Windows (psutil); the GPU's own memory on a discrete card is not in it.

Every run gets a HOME of its own with the player's settings.cfg copied in and only
`mx_graphics` (and the upscaler: off, render scale 1, so every build draws the same pixels)
changed. The builds take turns in each round so that heat and background work hit all alike;
the table shows the median over the runs.
"""
import argparse, os, platform, re, shutil, statistics, subprocess, sys, tempfile, time

ap = argparse.ArgumentParser()
ap.add_argument("paths", nargs="+", help="<OMSI 2 root>, or <binary> <OMSI 2 root>")
ap.add_argument("--build", action="append", default=[], help="LABEL=PATH[:mx|:wgpu]")
ap.add_argument("--frames", type=int, default=300)
ap.add_argument("--runs", type=int, default=3)
ap.add_argument("--size", default="1920x1080")
ap.add_argument("--out", default=None, help="write the raw runs (CSV) here")
ap.add_argument("--scenes", default=None, help="comma-separated scene numbers (1-based) to run")
ap.add_argument("--set", default="grundorf", choices=["grundorf", "lighting"], help="the scenes: Grundorf's (default) or Berlin-Spandau's lit by day, by night and in fog (the lighting work)")
a = ap.parse_args()

if len(a.paths) == 2:
    root = a.paths[1]
    builds = [("wgpu", a.paths[0], False), ("mX", a.paths[0], True)]
else:
    root = a.paths[0]
    builds = []
for b in a.build:
    label, rest = b.split("=", 1)
    mx = None
    if rest.endswith(":mx") or rest.endswith(":wgpu"):
        rest, mode = rest.rsplit(":", 1)
        mx = mode == "mx"
    builds.append((label, rest, bool(mx)))
if not builds:
    sys.exit("no builds: pass <binary> <root>, or --build LABEL=PATH[:mx|:wgpu]")

SCENES = [
    ("Grundorf, editor camera", ["--map", "maps/Grundorf/global.cfg"]),
    ("Grundorf, MAN SD80 cab (mirrors)", ["--map", "maps/Grundorf/global.cfg", "--bus", "Vehicles/MAN_SD200/MAN_SD80.bus", "--view", "driver"]),
    ("Grundorf, MAN SD80 outside", ["--map", "maps/Grundorf/global.cfg", "--bus", "Vehicles/MAN_SD200/MAN_SD80.bus", "--view", "outside"]),
]
# Berlin-Spandau with the MAN EN92, its headlamps and saloon lamps switched on where it is dark
# (the switches are script triggers, which fire while the bus is driven: `--drive`)
SPANDAU = ["--map", "maps/Berlin-Spandau/global.cfg", "--bus", "Vehicles/MAN_NL_NG/MAN_EN92_main.bus", "--entry", "0"]
LIGHTS = ["--autostart", "--triggers", "kw_scheinwerfer_toggle@0.3,cp_licht_unterdeck_toggle@0.4,cp_licht_oberdeck_toggle@0.5", "--drive", "2"]
if a.set == "lighting":
    SCENES = [
        ("Spandau, day, outside", SPANDAU + ["--view", "outside", "--time", "12:00", "--weather", "Weather/#CAVOK.owt"]),
        ("Spandau, night, cab, headlamps, traffic", SPANDAU + ["--view", "driver", "--time", "23:00", "--weather", "Weather/#CAVOK.owt", "--traffic", "20"] + LIGHTS),
        ("Spandau, night fog, bus front lit", SPANDAU + ["--view", "free", "--cam", "892320.1,4196851.9,34.6,127,-4", "--time", "22:30", "--weather", "Weather/Bodennebel.owt"] + LIGHTS),
    ]
if a.scenes:
    SCENES = [SCENES[int(i) - 1] for i in a.scenes.split(",")]
GRAPHICS = ["vanilla_plus", "enhanced"]
BENCH = re.compile(r"bench: \d+ frames of \S+: drawing ([\d.]+) ms, GPU wait ([\d.]+) ms \(medians\), frame mean ([\d.]+) ms, worst ([\d.]+) ms")

own = os.path.expanduser("~/.openomsi/settings.cfg")
base = open(own).read() if os.path.exists(own) else ""
work = tempfile.mkdtemp(prefix="omsi-bench-")
system = platform.system()


def home(label: str, mx: bool) -> str:
    h = os.path.join(work, re.sub(r"\W", "_", label))
    d = os.path.join(h, ".openomsi")
    os.makedirs(d, exist_ok=True)
    keep = [l for l in base.splitlines() if not re.match(r"\s*(mx_graphics|upscaler|render_scale|frame_generation)\s*=", l)]
    keep += [f"mx_graphics={int(mx)}", "upscaler=off", "render_scale=1", "frame_generation=off"]
    open(os.path.join(d, "settings.cfg"), "w").write("\n".join(keep) + "\n")
    return h


def run_measured(cmd, env):
    """the process's output and its peak memory in MB (None where it cannot be read)"""
    if system == "Darwin":
        p = subprocess.run(["/usr/bin/time", "-l", *cmd], env=env, capture_output=True, text=True, timeout=900)
        m = re.search(r"(\d+)\s+peak memory footprint", p.stderr) or re.search(r"(\d+)\s+maximum resident set size", p.stderr)
        return p, (int(m.group(1)) / 2**20 if m else None)
    if system == "Linux" and os.path.exists("/usr/bin/time"):
        p = subprocess.run(["/usr/bin/time", "-v", *cmd], env=env, capture_output=True, text=True, timeout=900)
        m = re.search(r"Maximum resident set size \(kbytes\): (\d+)", p.stderr)
        return p, (int(m.group(1)) / 1024 if m else None)
    try:
        import psutil
    except ImportError:
        return subprocess.run(cmd, env=env, capture_output=True, text=True, timeout=900), None
    with tempfile.TemporaryFile("w+") as out:
        proc = psutil.Popen(cmd, env=env, stdout=out, stderr=subprocess.STDOUT, text=True)
        peak = 0
        while proc.poll() is None:
            try:
                mi = proc.memory_info()
                peak = max(peak, getattr(mi, "peak_wset", 0) or mi.rss)
            except psutil.Error:
                pass
            time.sleep(0.05)
        out.seek(0)
        return subprocess.CompletedProcess(cmd, proc.returncode, out.read(), ""), (peak / 2**20 if peak else None)


def run(build, scene_args, graphics):
    label, binary, mx = build
    env = dict(os.environ, HOME=home(label, mx), USERPROFILE=home(label, mx), OMSI_BENCH=str(a.frames), OMSI_GRAPHICS=graphics, RUST_LOG="info", OMSI_ROOT=root)
    png = os.path.join(work, "out.png")
    cmd = [binary, "--root", root, *scene_args, "--offscreen", png, "--size", a.size]
    p, mem = run_measured(cmd, env)
    log = p.stdout + p.stderr
    m = BENCH.search(log)
    if not m:
        sys.stderr.write(f"no bench line ({label}, {graphics}, {scene_args}): exit {p.returncode}\n{log[-2000:]}\n")
        return None
    if mx and "graphics: mX" not in log:
        sys.stderr.write(f"warning: {label}: asked for mX, the log does not say mX drew\n")
    return (*(float(x) for x in m.groups()), mem)


def med(vs, i):
    xs = [v[i] for v in vs if v[i] is not None]
    return statistics.median(xs) if xs else None


rows = []
for name, args in SCENES:
    for g in GRAPHICS:
        got = {b[0]: [] for b in builds}
        for r in range(a.runs):
            order = builds if r % 2 == 0 else builds[::-1]
            for b in order:
                v = run(b, args, g)
                if v:
                    got[b[0]].append(v)
                time.sleep(2)
        rows.append((name, g, got))
        print(f"{name} / {g}: " + ", ".join(f"{l} {med(v, 0) or 0:.2f}+{med(v, 1) or 0:.2f} ms" for l, v in got.items() if v), flush=True)

ref = builds[0][0]
print()
print(f"Frame = drawing (CPU) + GPU wait, medians in ms; fps against {ref}; memory = peak MB.")
print()
print("| Scene | Graphics | Build | CPU | GPU wait | Frame | Worst | Peak memory | fps vs " + ref + " |")
print("|---|---|---|---|---|---|---|---|---|")
for name, g, got in rows:
    rv = got[ref]
    rf = (med(rv, 0) + med(rv, 1)) if rv else None
    for label, vs in got.items():
        if not vs:
            print(f"| {name} | {g} | {label} | failed | | | | | |")
            continue
        f = med(vs, 0) + med(vs, 1)
        mem = med(vs, 4)
        rel = f"{(rf / f - 1) * 100:+.0f} %" if rf and label != ref else "-"
        print(f"| {name} | {g} | {label} | {med(vs, 0):.2f} | {med(vs, 1):.2f} | {f:.2f} | {med(vs, 3):.1f} | {f'{mem:.0f}' if mem else '-'} | {rel} |")
if a.out:
    with open(a.out, "w") as f:
        f.write("scene,graphics,build,run,cpu_ms,gpu_wait_ms,frame_mean_ms,worst_ms,peak_mb\n")
        for name, g, got in rows:
            for label, vs in got.items():
                for i, v in enumerate(vs):
                    f.write(f"{name},{g},{label},{i},{','.join('' if x is None else str(x) for x in v)}\n")
shutil.rmtree(work, ignore_errors=True)
