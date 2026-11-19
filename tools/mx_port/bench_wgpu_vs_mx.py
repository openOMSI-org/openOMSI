#!/usr/bin/env python3
"""wgpu against mX: the same game binary, the same scenes, the mX Graphics setting off and on.

usage: bench_wgpu_vs_mx.py <openomsi binary> <OMSI 2 root> [--frames N] [--runs N] [--size WxH]

Each run is an offscreen render with OMSI_BENCH=N (the final picture drawn N more times,
mirrors included when a bus is driven): the game logs the median drawing time (CPU), the
median wait for the GPU and the mean and worst frame. Every run gets a HOME of its own with
the player's settings.cfg copied in and only `mx_graphics` (and the upscaler: off, render
scale 1, so both draw the same pixels) changed. Runs alternate wgpu / mX so that heat and
background work hit both alike; the table shows the median over the runs.
"""
import argparse, os, re, shutil, statistics, subprocess, sys, tempfile, time

ap = argparse.ArgumentParser()
ap.add_argument("binary")
ap.add_argument("root")
ap.add_argument("--frames", type=int, default=300)
ap.add_argument("--runs", type=int, default=3)
ap.add_argument("--size", default="1920x1080")
ap.add_argument("--out", default=None, help="write the raw results (CSV) here")
a = ap.parse_args()

SCENES = [
    ("Grundorf, editor camera", ["--map", "maps/Grundorf/global.cfg"]),
    ("Grundorf, MAN SD80 cab (mirrors)", ["--map", "maps/Grundorf/global.cfg", "--bus", "Vehicles/MAN_SD200/MAN_SD80.bus", "--view", "driver"]),
    ("Grundorf, MAN SD80 outside", ["--map", "maps/Grundorf/global.cfg", "--bus", "Vehicles/MAN_SD200/MAN_SD80.bus", "--view", "outside"]),
]
GRAPHICS = ["vanilla_plus", "enhanced"]
BENCH = re.compile(r"bench: \d+ frames of \S+: drawing ([\d.]+) ms, GPU wait ([\d.]+) ms \(medians\), frame mean ([\d.]+) ms, worst ([\d.]+) ms")
BACKEND = re.compile(r"graphics: mX on|opening graphics device: .*\((\w+),")

own = os.path.expanduser("~/.openomsi/settings.cfg")
base = open(own).read() if os.path.exists(own) else ""
work = tempfile.mkdtemp(prefix="omsi-bench-")


def home(mx: bool) -> str:
    h = os.path.join(work, "mx" if mx else "wgpu")
    d = os.path.join(h, ".openomsi")
    os.makedirs(d, exist_ok=True)
    keep = [l for l in base.splitlines() if not re.match(r"\s*(mx_graphics|upscaler|render_scale|frame_generation)\s*=", l)]
    keep += [f"mx_graphics={int(mx)}", "upscaler=off", "render_scale=1", "frame_generation=off"]
    open(os.path.join(d, "settings.cfg"), "w").write("\n".join(keep) + "\n")
    return h


def run(mx: bool, scene_args, graphics):
    env = dict(os.environ, HOME=home(mx), OMSI_BENCH=str(a.frames), OMSI_GRAPHICS=graphics, RUST_LOG="info", OMSI_ROOT=a.root)
    png = os.path.join(work, "out.png")
    cmd = [a.binary, "--root", a.root, *scene_args, "--offscreen", png, "--size", a.size]
    p = subprocess.run(cmd, env=env, capture_output=True, text=True, timeout=900)
    log = p.stdout + p.stderr
    m = BENCH.search(log)
    if not m:
        sys.stderr.write(f"no bench line ({'mX' if mx else 'wgpu'}, {graphics}, {scene_args}): exit {p.returncode}\n{log[-2000:]}\n")
        return None
    on_mx = "graphics: mX on" in log or "on mX" in log
    if on_mx != mx and mx:
        sys.stderr.write("warning: asked for mX, the log does not say mX drew\n")
    return tuple(float(x) for x in m.groups())


rows = []
for name, args in SCENES:
    for g in GRAPHICS:
        got = {False: [], True: []}
        for r in range(a.runs):
            for mx in (False, True) if r % 2 == 0 else (True, False):
                v = run(mx, args, g)
                if v:
                    got[mx].append(v)
                time.sleep(2)
        med = {k: tuple(statistics.median(x[i] for x in v) for i in range(4)) if v else None for k, v in got.items()}
        rows.append((name, g, med[False], med[True], got))
        w, m = med[False], med[True]
        if w and m:
            print(f"{name} / {g}: wgpu {w[0]:.2}+{w[1]:.2} ms, mX {m[0]:.2}+{m[1]:.2} ms", flush=True)

print()
print("| Scene | Graphics | wgpu CPU | wgpu GPU wait | wgpu frame | mX CPU | mX GPU wait | mX frame | mX vs wgpu (frame) |")
print("|---|---|---|---|---|---|---|---|---|")
for name, g, w, m, _ in rows:
    if not (w and m):
        print(f"| {name} | {g} | failed | | | | | | |")
        continue
    fw, fm = w[0] + w[1], m[0] + m[1]
    print(f"| {name} | {g} | {w[0]:.2f} | {w[1]:.2f} | {fw:.2f} | {m[0]:.2f} | {m[1]:.2f} | {fm:.2f} | {(fw / fm - 1) * 100:+.0f} % fps |")
if a.out:
    with open(a.out, "w") as f:
        f.write("scene,graphics,backend,run,cpu_ms,gpu_wait_ms,frame_mean_ms,worst_ms\n")
        for name, g, _, _, got in rows:
            for mx, vs in got.items():
                for i, v in enumerate(vs):
                    f.write(f"{name},{g},{'mX' if mx else 'wgpu'},{i},{v[0]},{v[1]},{v[2]},{v[3]}\n")
shutil.rmtree(work, ignore_errors=True)
