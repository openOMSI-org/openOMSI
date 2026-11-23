#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import statistics
import subprocess
import sys
from pathlib import Path

USAGE = """Measure several openBus builds in several scenes and print one table.

    python3 scripts/bench-matrix.py OUT_DIR LABEL=BINARY [LABEL=BINARY ...]
        [--scenarios clear,rain,snow,night] [--runs 2] [--seconds 75] [--report-only]

Every run goes through scripts/bench-window.sh (Berlin-Spandau, still camera, sound off,
the same seed), so the original OMSI 2 install is needed ($OMSI_ROOT). Each run's summary
is OUT_DIR/<label>-<scenario>-<run>.json; --report-only prints the table from them again.
The first LABEL is the reference the others are compared with. Compare builds of one
machine only, and close everything else while it runs."""

SCENARIOS = {
    "clear": ["--weather", "Weather/#CAVOK.owt"],
    "rain": ["--weather", "Weather/Schmuddelwetter.owt"],
    "snow": ["--weather", "Weather/Starker Schneefall.owt"],
    "night": ["--weather", "Weather/#CAVOK.owt"],
}
TIMES = {"night": "22:00"}


def run(script: Path, binary: str, out: Path, scenario: str, seconds: int) -> None:
    env = dict(os.environ, BENCH_STILL="1", BENCH_TIME=TIMES.get(scenario, "08:00"))
    out.with_suffix(".json").unlink(missing_ok=True)
    cmd = [str(script), binary, str(out), "plain", str(seconds), *SCENARIOS[scenario]]
    subprocess.run(cmd, env=env, check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def summary(path: Path) -> dict | None:
    try:
        return json.loads(path.read_text())["summary"]
    except (OSError, ValueError, KeyError):
        return None


def mean(values: list[float]) -> float:
    return statistics.fmean(values) if values else float("nan")


def collect(out_dir: Path, label: str, scenario: str) -> list[dict]:
    found = [summary(p) for p in sorted(out_dir.glob(f"{label}-{scenario}-*.json"))]
    return [s for s in found if s]


def report(out_dir: Path, labels: list[str], scenarios: list[str]) -> None:
    print("| scene | build | runs | fps | p50 ms | p95 ms | p99 ms | main thread ms | most changed stages (ms/frame vs first build) |")
    print("| --- | --- | --- | --- | --- | --- | --- | --- | --- |")
    for scenario in scenarios:
        base_stages: dict[str, float] = {}
        for i, label in enumerate(labels):
            runs = collect(out_dir, label, scenario)
            if not runs:
                print(f"| {scenario} | {label} | 0 | - | - | - | - | - | |")
                continue
            stages: dict[str, float] = {}
            for name in {k for r in runs for k in r.get("stage_average_ms", {})}:
                stages[name] = mean([r.get("stage_average_ms", {}).get(name, 0.0) for r in runs])
            if i == 0:
                base_stages = stages
                changed = ""
            else:
                deltas = sorted(((stages.get(k, 0.0) - base_stages.get(k, 0.0), k) for k in set(stages) | set(base_stages)), key=lambda d: -abs(d[0]))
                changed = ", ".join(f"{k} {d:+.2f}" for d, k in deltas[:4] if abs(d) >= 0.05)
            cells = [mean([r.get(m, float("nan")) for r in runs]) for m in ("average_fps", "p50_ms", "p95_ms", "p99_ms", "main_thread_cpu_ms_per_frame")]
            print(f"| {scenario} | {label} | {len(runs)} | {cells[0]:.1f} | {cells[1]:.1f} | {cells[2]:.1f} | {cells[3]:.1f} | {cells[4]:.2f} | {changed} |")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(usage=USAGE)
    parser.add_argument("out_dir", type=Path)
    parser.add_argument("builds", nargs="+")
    parser.add_argument("--scenarios", default="clear,rain,snow,night")
    parser.add_argument("--runs", type=int, default=2)
    parser.add_argument("--seconds", type=int, default=75)
    parser.add_argument("--report-only", action="store_true")
    args = parser.parse_args(argv)
    builds = [b.split("=", 1) for b in args.builds]
    if any(len(b) != 2 for b in builds):
        parser.error("each build is LABEL=BINARY")
    scenarios = args.scenarios.split(",")
    unknown = [s for s in scenarios if s not in SCENARIOS]
    if unknown:
        parser.error(f"unknown scenario {unknown[0]} (known: {', '.join(SCENARIOS)})")
    args.out_dir.mkdir(parents=True, exist_ok=True)
    script = Path(__file__).resolve().parent / "bench-window.sh"
    if not args.report_only:
        for n in range(1, args.runs + 1):
            for scenario in scenarios:
                for label, binary in builds:
                    out = args.out_dir / f"{label}-{scenario}-{n}"
                    print(f"run {n}/{args.runs}: {label} {scenario}", file=sys.stderr, flush=True)
                    run(script, binary, out, scenario, args.seconds)
    report(args.out_dir, [label for label, _ in builds], scenarios)
    return 0


if __name__ == "__main__":
    sys.exit(main())
