#!/usr/bin/env python3
"""Summarize native dedicated-server timings without copying private log contents.

Python 3 standard library only. Missing measurements remain null, never zero.
"""
import argparse
import csv
import json
import re
import statistics
import sys
from pathlib import Path

# Match only known numerical telemetry, not names, addresses or join codes.
NATIVE = re.compile(
    r'native load: TPS ([\d.]+)/([\d.]+), (\d+) active CPU tiles, '
    r'(\d+) AI cars, (\d+) AI buses, (\d+) pedestrians '
    r'\((\d+) walking, (\d+) waiting, (\d+) aboard\); '
    r'ground heap est\.: ([\d.]+) MiB terrain, ([\d.]+) MiB raster, '
    r'([\d.]+) MiB wheel; tick p95 ([\d.]+) ms p99 ([\d.]+) ms '
    r'\(profiling (on|off)\)'
)
TIMINGS = re.compile(
    r'server console: timings (running|stopped), (\d+) ticks in ([\d.]+) s, '
    r'tick avg ([\d.]+) ms, p95 ([\d.]+) ms, p99 ([\d.]+) ms, max ([\d.]+) ms'
)
STAGE = re.compile(
    r'server console: (network|interest|simulation|world_sync)\s+'
    r'mean ([\d.]+) ms, max ([\d.]+) ms, total ([\d.]+) ms '
    r'\(([\d.]+)%\), calls (\d+)'
)


def summary(values):
    if not values:
        return None
    return {'min': min(values), 'mean': round(statistics.mean(values), 3),
            'max': max(values), 'samples': len(values)}


def parse_log(path):
    loads, reports = [], []
    report = None
    with path.open('r', encoding='utf-8', errors='replace') as stream:
        for line in stream:
            m = NATIVE.search(line)
            if m:
                values = m.groups()
                fields = (
                    'tps', 'target_tps', 'tiles', 'ai_cars', 'ai_buses', 'pedestrians',
                    'walking', 'waiting', 'aboard', 'terrain_mib_est', 'raster_mib_est',
                    'wheel_mib_est', 'p95_ms', 'p99_ms'
                )
                sample = dict(zip(fields, [float(x) for x in values[:-1]]))
                sample['profiling'] = values[-1]
                loads.append(sample)
            m = TIMINGS.search(line)
            if m:
                status, ticks, secs, avg, p95, p99, maximum = m.groups()
                report = {'status': status, 'ticks': int(ticks),
                          'duration_seconds': float(secs), 'avg_tick_ms': float(avg),
                          'p95_tick_ms': float(p95), 'p99_tick_ms': float(p99),
                          'max_tick_ms': float(maximum), 'stages': {}}
                reports.append(report)
                continue
            m = STAGE.search(line)
            if m and report is not None:
                stage, avg, maximum, total, share, calls = m.groups()
                report['stages'][stage] = {
                    'mean_ms': float(avg), 'max_ms': float(maximum),
                    'total_ms': float(total), 'share_percent': float(share),
                    'calls': int(calls)}
    return loads, reports


def parse_process_csv(path):
    samples = []
    if path is None:
        return samples
    with path.open('r', newline='', encoding='utf-8-sig') as stream:
        for row in csv.DictReader(stream):
            try:
                samples.append({'cpu_one_core_percent': float(row['cpu_one_core_percent']),
                                'rss_mib': float(row['rss_mib'])})
            except (KeyError, ValueError, TypeError):
                continue
    return samples


def make_report(log, process_csv, *, label, sha, clients):
    loads, reports = parse_log(log)
    proc = parse_process_csv(process_csv)
    latest = reports[-1] if reports else None
    # Use the latest cumulative profile, not an average of overlapping p99s.
    return {
        'scenario': label, 'commit': sha, 'clients': clients,
        'log_samples': len(loads), 'process_samples': len(proc),
        'tps': summary([v['tps'] for v in loads]),
        'actual_ai_cars': summary([v['ai_cars'] for v in loads]),
        'actual_pedestrians': summary([v['pedestrians'] for v in loads]),
        'active_tiles': summary([v['tiles'] for v in loads]),
        'rss_mib': summary([v['rss_mib'] for v in proc]),
        'cpu_one_core_percent': summary([v['cpu_one_core_percent'] for v in proc]),
        'latest_timings': latest,
        'limitations': [
            'Ground heap estimates are not whole-process RSS.',
            'Stage wall-clock time is not process CPU consumption.',
            'Latest timings are cumulative since last timings on/reset; never average reported percentiles.',
            'A null field was not measured; synthetic results do not prove in-game parity.',
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('server_log', type=Path, help='host log with a timings report')
    parser.add_argument('--process-csv', type=Path, help='CSV from sample-process.ps1')
    parser.add_argument('--label', default='unlabeled')
    parser.add_argument('--sha', default='unknown')
    parser.add_argument('--clients', type=int, default=None)
    parser.add_argument('--output', type=Path, help='write sanitized JSON; default stdout')
    args = parser.parse_args()
    if args.clients is not None and args.clients < 0:
        parser.error('--clients must be nonnegative')
    result = make_report(args.server_log, args.process_csv,
                         label=args.label, sha=args.sha, clients=args.clients)
    value = json.dumps(result, indent=2, ensure_ascii=False) + '\n'
    if args.output:
        args.output.write_text(value, encoding='utf-8')
    else:
        print(value, end='')
    if result['latest_timings'] is None or result['latest_timings']['ticks'] == 0:
        print('warning: no nonempty timings report; run timings on then timings report',
              file=sys.stderr)


if __name__ == '__main__':
    main()
