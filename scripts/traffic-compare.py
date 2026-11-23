#!/usr/bin/env python3
"""Compare the AI traffic of two openBus builds over many sessions, not one.

One run of a scene is one draw of a random process: which cars come, where and when
(the session's seed, see `omsi_script::set_session_seed`). Two single runs of two builds
differ by that chance as much as by what changed between them. This runs every scene with
the same N seeds on both builds (OMSI_SEED=1..N; a seed gives both the same start), reads
each run's OMSI_TRAFFIC_STATS summary line and prints, per scene and figure, the mean and
spread of each build and the difference with its 95 % interval (a bootstrap of the paired
differences). A difference whose interval does not hold 0 is marked: only those are more
than chance. The invariants (overlaps, red runs) are summed as well: one of them in any
run is a fault whatever the means say.

    python3 scripts/traffic-compare.py --base <openbus> --new <openbus> --root <OMSI 2> \\
        --out <dir> [--seeds 8] [--secs 300] [--jobs 4] [--scene NAME=MAP@HH:MM[,ARGS]] \\
        [--base-env NAME=VALUE] [--new-env NAME=VALUE] ...

--base-env / --new-env set a variable for one side's runs only: one build against itself with
a switch on (`--base X --new X --new-env OMSI_NO_WHEEL_SLIP=1`).

The default scenes are Berlin-Spandau and Grundorf at 08:00 and at 17:00 with the
timetable. Every run gets a fresh, empty $HOME. Runs already in --out are reused (delete
the folder to measure again).
"""

from __future__ import annotations

import argparse
import concurrent.futures
import os
import random
import re
import shutil
import statistics
import subprocess
import sys
from pathlib import Path

DEFAULT_SCENES = [
    "spandau_08=maps/Berlin-Spandau/global.cfg@08:00",
    "spandau_08_tt=maps/Berlin-Spandau/global.cfg@08:00,--schedule",
    "spandau_17_tt=maps/Berlin-Spandau/global.cfg@17:00,--schedule",
    "grundorf_08=maps/Grundorf/global.cfg@08:00",
    "grundorf_17_tt=maps/Grundorf/global.cfg@17:00,--schedule",
]

# (name in the summary line, short label, higher is better)
FIGURES = [
    (r"([\d.]+) cars", "cars", True),
    (r"mean ([\d.]+) km/h", "km/h", True),
    (r"([\d.]+) junction entries/min", "entries/min", True),
    (r"([\d.]+) km/min", "km/min", True),
    (r"stood >30 s ([\d.]+)", "stood>30s", False),
    (r">60 s ([\d.]+)", "stood>60s", False),
    (r"in waits-for cycles ([\d.]+)", "in cycles", False),
    (r"emergency brakes (\d+)", "emerg.brakes", False),
    (r"close calls (\d+)", "close calls", False),
    (r"gave up (\d+)", "gave up", False),
    (r"overlaps (\d+)", "overlaps", False),
    (r"red runs (\d+)", "red runs", False),
]
INVARIANTS = ("overlaps", "red runs")


def run_one(side: str, binary: str, root: str, scene: str, seed: int, secs: int, out: Path, envs: list[str]) -> Path:
    name, rest = scene.split("=", 1)
    where, _, extra = rest.partition(",")
    map_cfg, _, time = where.partition("@")
    env_tag = "".join("-" + re.sub(r"[^A-Za-z0-9_.]", "_", e) for e in envs)
    # (the side in the name: two binaries in folders of one name - wt/base-core and wt/head-tick -
    # wrote the same files, and the comparison was of one build with itself)
    tag = f"{side}-{Path(binary).resolve().parent.parent.name}{env_tag}-{name}-s{seed}"
    log = out / f"{tag}.log"
    if log.exists() and "traffic stats over" in log.read_text(errors="replace"):
        return log
    home = out / "home" / tag
    home.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, HOME=str(home), OMSI_SEED=str(seed), OMSI_NO_UPDATE="1",
               OMSI_TRAFFIC_STATS=str(out / f"{tag}.csv"), RUST_LOG_STYLE="never")
    env.update(e.split("=", 1) for e in envs)
    cmd = [binary, "--root", root, "--map", map_cfg, "--bus", "Vehicles/MAN_NL_NG/MAN_GN92_main.bus",
           "--time", time or "08:00", "--traffic", "60", "--view", "free", "--cam", "0,0,50,0,-30",
           "--drive", str(secs), "--offscreen", str(out / f"{tag}.png"), "--size", "320x180"]
    cmd += [a for a in extra.split() if a]
    with open(log, "w") as f:
        subprocess.run(cmd, stdout=f, stderr=subprocess.STDOUT, env=env, check=False)
    # (the run's home holds the game's caches, hundreds of MB a run)
    shutil.rmtree(home, ignore_errors=True)
    return log


def figures(log: Path) -> dict[str, float] | None:
    text = log.read_text(errors="replace")
    m = re.search(r"traffic stats over[^\n]*", text)
    if not m:
        return None
    line = m.group(0)
    out = {}
    for pattern, label, _ in FIGURES:
        g = re.search(pattern, line)
        if g:
            out[label] = float(g.group(1))
    return out


def interval(diffs: list[float], rounds: int = 4000) -> tuple[float, float]:
    rng = random.Random(12345)
    means = sorted(statistics.fmean(rng.choices(diffs, k=len(diffs))) for _ in range(rounds))
    return means[int(0.025 * rounds)], means[int(0.975 * rounds) - 1]


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--base", required=True)
    ap.add_argument("--new", required=True)
    ap.add_argument("--root", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--seeds", type=int, default=8)
    ap.add_argument("--secs", type=int, default=300)
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--scene", action="append")
    ap.add_argument("--base-env", action="append", default=[])
    ap.add_argument("--new-env", action="append", default=[])
    a = ap.parse_args()
    out = Path(a.out)
    out.mkdir(parents=True, exist_ok=True)
    scenes = a.scene or DEFAULT_SCENES
    seeds = list(range(1, a.seeds + 1))
    sides = {"base": (a.base, a.base_env), "new": (a.new, a.new_env)}
    jobs = [(side, s, k) for s in scenes for k in seeds for side in sides]
    logs: dict[tuple[str, str, int], Path] = {}
    with concurrent.futures.ThreadPoolExecutor(max_workers=a.jobs) as pool:
        futs = {pool.submit(run_one, side, sides[side][0], a.root, s, k, a.secs, out, sides[side][1]): (side, s, k) for side, s, k in jobs}
        for done, fut in enumerate(concurrent.futures.as_completed(futs), 1):
            logs[futs[fut]] = fut.result()
            print(f"\r{done}/{len(jobs)} runs", end="", file=sys.stderr, flush=True)
    print(file=sys.stderr)
    faults = 0
    for s in scenes:
        name = s.split("=", 1)[0]
        print(f"\n== {name}  ({len(seeds)} seeds, {a.secs} s each; base -> new, mean ± sd, difference [95 % interval])")
        rows = []
        for k in seeds:
            fb, fn = figures(logs[("base", s, k)]), figures(logs[("new", s, k)])
            if fb is None or fn is None:
                print(f"   seed {k}: a run has no summary (see its log)")
                continue
            rows.append((fb, fn))
        if not rows:
            continue
        for _, label, higher_better in FIGURES:
            b = [r[0].get(label, 0.0) for r in rows]
            n = [r[1].get(label, 0.0) for r in rows]
            d = [y - x for x, y in zip(b, n)]
            lo, hi = interval(d) if len(d) > 1 else (d[0], d[0])
            mark = ""
            if lo > 0 or hi < 0:
                better = (lo > 0) == higher_better
                mark = "  better" if better else "  WORSE"
            sd = lambda v: statistics.stdev(v) if len(v) > 1 else 0.0
            print(f"   {label:13} {statistics.fmean(b):8.2f} ± {sd(b):6.2f} -> {statistics.fmean(n):8.2f} ± {sd(n):6.2f}"
                  f"   {statistics.fmean(d):+8.2f} [{lo:+.2f}, {hi:+.2f}]{mark}")
            if label in INVARIANTS:
                faults += int(sum(n))
    if faults:
        print(f"\n{faults} invariant fault(s) in the new build's runs (overlaps, red runs): look at them whatever the means say")
    return 0


if __name__ == "__main__":
    sys.exit(main())
