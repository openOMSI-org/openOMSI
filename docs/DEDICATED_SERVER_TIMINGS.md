# Native dedicated-server timings and process resources

**This is a measurement procedure, not a performance claim.** Use a licensed
map and at least two matching graphical clients before publishing gameplay
or capacity results. Headless CI alone cannot establish real-game equivalence.

## Collect one comparable Windows scenario

1. Record the exact native-server commit, openOMSI client versions, map/date,
   CPU model, logical core count, RAM and installed OMSI content. Keep
   settings and client count the same when comparing runs.
2. Start the CPU-only omsi-server.exe in the usual private test console.
   Preserve console output in a **local** log. Do not publish raw logs:
   they may include names, IPs, join codes or mod paths.
3. Connect two graphical clients. Warm up for at least two minutes before
   measuring. Do not include world loading or /restart in steady-state numbers.
4. In the server console enter: timings reset, then timings on. In a
   separate PowerShell console, run:

       .\sample-process.ps1 -DurationSeconds 600 -IntervalSeconds 2 -Output .\process.csv

   With multiple host instances, supply -ProcessId and the real server PID.
   The script refuses non-omsi-server processes. Its cpu_one_core_percent
   can exceed 100% on multicore loads; cpu_system_percent divides by cores.
5. Drive for ten minutes across several tiles with AI/passengers actually
   present. Enter tps, timings report, then timings off in the server console,
   and keep their output in the local log. The parser uses the last cumulative
   timings report; it never averages overlapping percentiles.
6. Summarize using Python 3 (standard library, no package install):

       python .\timings-report.py .\server-test.log --process-csv .\process.csv --label "Savoy9-320" --sha "<commit>" --clients 2 --output .\timings.json

   Generated JSON has only recognized numeric telemetry and operator-supplied
   labels/SHA. It never copies raw chat, addresses or join codes. Review
   your labels before sharing. A missing metric is null, not zero.

Both tools live in scripts/server-headless. They do not open network ports,
modify the simulation or access another host.

## What numbers mean

| Source | Measurement | Interpretation |
| --- | --- | --- |
| native load | Actual TPS, AI counts, pedestrian counts and active CPU tiles | Targets are not observed populations; do not combine separate peak samples |
| timings report | Tick avg, p95, p99 and max wall time | At 60 TPS the nominal budget is about 16.67 ms |
| Stage report | network, interest, simulation, world_sync mean/max/total | Stage wall time is not process CPU time |
| Windows sampler | Process CPU and real RSS, MiB | RSS differs from terrain/raster/wheel heap estimates |
| Gameplay notes | Disconnects, boarding, errors, crossings, restarts | Needed to interpret timing regressions |

Keep separate scenarios for traffic targets 0, 30, 160, 320 and 512
(only increase when stable), plus representative people and stops. Run
ten-minute steady-state samples and a separate long-duration soak. The
maximum configurable 512 random cars / 2,000 humans is **not** proof
those counts were reached; record observed population.

For each scenario keep the exact SHA, platform, settings, client count,
sample length, average/minimum TPS, tick p95/p99/max, actual AI/people,
mean/peak RSS, CPU load and disconnect count.

**Do not merge based on a good TPS screenshot.** Acceptance still needs
green CI on the tested commit, two-client real boarding/return/rejoin,
Chrono midnight transition, preserved file-sharing permissions, correct
simulation state, and no unbounded RSS growth.
