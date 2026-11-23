#!/usr/bin/env python3
"""Compare two openBus profile summaries (schema 1).

A summary is OMSI_PROFILE's exit summary written by a run with OMSI_PROFILE=1,
OMSI_PROFILE_JSON=<file> and --exit-after <s>: frame times, stages and GPU passes after
the 15 s warm-up. This compares two of them; it runs nothing. For a regression claim the
runs must come from the same machine, scene, camera and settings (the "run" block, the
adapter and the warm-up are checked; --allow-mismatch to compare anyway).

    python3 scripts/compare-performance.py before.json after.json [--fail-p95-percent 10]
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path
from typing import Any

LATENCY_METRICS = ("p50_ms", "p95_ms", "p99_ms", "worst_frame_ms")
FPS_METRIC = "average_fps"
EVENNESS_METRICS = ("jitter_ms", "stdev_ms", "frames_over_2x_median", "slowest_second_fps", "fastest_second_fps")


def number(value: Any, name: str) -> float:
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        raise ValueError(f"{name} is not numeric")
    value = float(value)
    if not math.isfinite(value):
        raise ValueError(f"{name} is not finite")
    return value


def load_summary(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ValueError(f"{path}: {exc}") from exc
    if value.get("schema_version") != 1:
        raise ValueError(f"{path}: unsupported schema_version {value.get('schema_version')!r}; expected 1")
    summary = value.get("summary")
    if not isinstance(summary, dict):
        raise ValueError(f"{path}: missing summary object")
    for metric in (FPS_METRIC, *LATENCY_METRICS):
        number(summary.get(metric), f"{path}:{metric}")
    stages = summary.get("stage_average_ms", {})
    if not isinstance(stages, dict):
        raise ValueError(f"{path}: summary.stage_average_ms is not an object")
    for key, val in stages.items():
        number(val, f"{path}:stage_average_ms.{key}")
    return value


def percent_change(base: float, candidate: float) -> float | None:
    if base == 0:
        return None
    return (candidate - base) / abs(base) * 100.0


def regression_percent(metric: str, base: float, candidate: float) -> float:
    if base <= 0:
        return 0.0
    if metric == FPS_METRIC:
        return max(0.0, (base - candidate) / base * 100.0)
    return max(0.0, (candidate - base) / base * 100.0)


def fmt_change(change: float | None) -> str:
    return "n/a" if change is None else f"{change:+.1f}%"


def compare(base: dict[str, Any], candidate: dict[str, Any]) -> dict[str, Any]:
    bs = base["summary"]
    cs = candidate["summary"]
    metrics: dict[str, dict[str, float | None]] = {}
    for metric in (FPS_METRIC, *LATENCY_METRICS):
        b = number(bs[metric], metric)
        c = number(cs[metric], metric)
        metrics[metric] = {
            "base": b,
            "candidate": c,
            "change_percent": percent_change(b, c),
            "regression_percent": regression_percent(metric, b, c),
        }

    b_stages = bs.get("stage_average_ms", {})
    c_stages = cs.get("stage_average_ms", {})
    stages = []
    for name in sorted(set(b_stages) & set(c_stages)):
        b = number(b_stages[name], name)
        c = number(c_stages[name], name)
        stages.append(
            {
                "name": name,
                "base_ms": b,
                "candidate_ms": c,
                "delta_ms": c - b,
                "change_percent": percent_change(b, c),
            }
        )
    stages.sort(key=lambda row: row["delta_ms"], reverse=True)

    mismatches = []
    for key in ("warmup_seconds", "adapter", "os", "architecture"):
        if base.get(key) != candidate.get(key):
            mismatches.append(f"{key}: {base.get(key)!r} -> {candidate.get(key)!r}")
    b_run = base.get("run") or {}
    c_run = candidate.get("run") or {}
    for key in sorted(set(b_run) | set(c_run)):
        if b_run.get(key) != c_run.get(key):
            mismatches.append(f"run.{key}: {b_run.get(key)!r} -> {c_run.get(key)!r}")

    # how even the frames were (summaries written before it was measured have none)
    evenness = {}
    for key in EVENNESS_METRICS:
        b = (bs.get("evenness") or {}).get(key)
        c = (cs.get("evenness") or {}).get(key)
        if isinstance(b, (int, float)) and isinstance(c, (int, float)):
            evenness[key] = {"base": float(b), "candidate": float(c), "change_percent": percent_change(float(b), float(c))}

    return {"metrics": metrics, "evenness": evenness, "stages": stages, "mismatches": mismatches}


def print_report(result: dict[str, Any], max_stages: int) -> None:
    print("| Metric | Base | Candidate | Change |")
    print("| --- | ---: | ---: | ---: |")
    labels = {
        FPS_METRIC: "Average FPS",
        "p50_ms": "p50 frame time (ms)",
        "p95_ms": "p95 frame time (ms)",
        "p99_ms": "p99 frame time (ms)",
        "worst_frame_ms": "Worst frame (ms)",
    }
    # (inclusive stages: a stage's sub-stages, "a.b", are part of "a")
    for metric in (FPS_METRIC, *LATENCY_METRICS):
        row = result["metrics"][metric]
        print(
            f"| {labels[metric]} | {row['base']:.3f} | {row['candidate']:.3f} | "
            f"{fmt_change(row['change_percent'])} |"
        )
    for key, row in result.get("evenness", {}).items():
        print(f"| {key} | {row['base']:.3f} | {row['candidate']:.3f} | {fmt_change(row['change_percent'])} |")
    stages = result["stages"][: max(0, max_stages)]
    if stages:
        print()
        print("| Stage | Base ms | Candidate ms | Delta ms | Change |")
        print("| --- | ---: | ---: | ---: | ---: |")
        for row in stages:
            print(
                f"| `{row['name']}` | {row['base_ms']:.3f} | {row['candidate_ms']:.3f} | "
                f"{row['delta_ms']:+.3f} | {fmt_change(row['change_percent'])} |"
            )
    if result["mismatches"]:
        print()
        print("Comparison warnings:")
        for item in result["mismatches"]:
            print(f"- {item}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("base", type=Path)
    parser.add_argument("candidate", type=Path)
    parser.add_argument("--json", action="store_true", dest="json_output",
                        help="emit machine-readable comparison JSON")
    parser.add_argument("--max-stage-rows", type=int, default=15)
    parser.add_argument("--allow-mismatch", action="store_true",
                        help="allow the run, adapter or warm-up to differ")
    parser.add_argument("--fail-p95-percent", type=float)
    parser.add_argument("--fail-p99-percent", type=float)
    parser.add_argument("--fail-fps-drop-percent", type=float)
    args = parser.parse_args(argv)

    try:
        base = load_summary(args.base)
        candidate = load_summary(args.candidate)
        result = compare(base, candidate)
    except ValueError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2

    if args.json_output:
        print(json.dumps(result, indent=2, sort_keys=True))
    else:
        print_report(result, args.max_stage_rows)

    failed = False
    if result["mismatches"] and not args.allow_mismatch:
        print("error: the runs differ (see the warnings); use --allow-mismatch only when intentional",
              file=sys.stderr)
        failed = True
    checks = [
        ("p95_ms", args.fail_p95_percent, "p95 frame-time regression"),
        ("p99_ms", args.fail_p99_percent, "p99 frame-time regression"),
        (FPS_METRIC, args.fail_fps_drop_percent, "average FPS drop"),
    ]
    for metric, limit, label in checks:
        if limit is None:
            continue
        if limit < 0:
            print(f"error: threshold for {label} must be >= 0", file=sys.stderr)
            return 2
        regression = result["metrics"][metric]["regression_percent"]
        if regression > limit:
            print(f"error: {label} {regression:.1f}% exceeds {limit:.1f}%", file=sys.stderr)
            failed = True
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
