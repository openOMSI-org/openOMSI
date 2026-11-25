# Refactor checks

A refactor (code moved, functions extracted, duplication removed) must not change what the game
does. This page lists the checks that show it did not, beside `cargo test`, and the checks every
pull request runs.

## Checking a refactor

1. Build `openomsi` from the commit before your change and from your change (release builds).
2. Copy each binary out of `target/release` together with `libsteam_api.dylib` (macOS) or
   `libsteam_api.so` (Linux x86-64). The program does not start without it.
3. Compare the pictures: [Golden pictures](REFACTOR_CHECKS.md#golden-pictures).
4. Compare the sound: [Audio traces](REFACTOR_CHECKS.md#audio-traces).
5. If the change is meant to make the game faster, measure it:
   [Frame time](REFACTOR_CHECKS.md#frame-time).
6. Run the tests that need the original install:
   [Tests with the original install](REFACTOR_CHECKS.md#tests-with-the-original-install).
7. Run the pull request checks locally: size budget, crate layers and formatting
   ([Pull request checks](REFACTOR_CHECKS.md#pull-request-checks)).

All scripts need the original OMSI 2 install: `$OMSI_ROOT`, else `OMSI 2 Original` beside the
checkout.

> **Warning:** The pictures and recordings are made from licensed content. Keep the output
> folders outside the repository (the scripts refuse a folder inside it) and never commit them.

## Golden pictures

`scripts/golden.sh` renders a fixed set of offscreen scenes with one binary and compares two such
sets pixel by pixel.

```bash
export OMSI_ROOT="/path/to/OMSI 2"
scripts/golden.sh render /path/to/before/openomsi /tmp/golden/before
scripts/golden.sh render /path/to/after/openomsi  /tmp/golden/after
scripts/golden.sh compare /tmp/golden/before /tmp/golden/after
```

| Command | What it does |
| --- | --- |
| `render <bin> <dir> [scene ...]` | Renders all scenes, or only the named ones |
| `compare <dir a> <dir b>` | Compares the scenes present in both |
| `list` | Lists the scenes |

`compare` prints `identical` for each scene that matches byte for byte. Otherwise it prints the
share of differing pixels and the largest and mean channel difference (0-255), and writes an
amplified difference picture into `<dir b>/diff/`. It exits 1 when a scene differs beyond its
allowance. Each run also writes `<scene>.log`; its first line names the build.

### The scenes

All scenes are 960×540 (`GOLDEN_SIZE`) with `OMSI_SEED=1` (`GOLDEN_SEED`) and the default date
(1989-05-30). They use Grundorf unless named otherwise, and the map's editor camera unless a bus
is spawned.

| Scene | Covers | Allowance |
| --- | --- | --- |
| `grundorf_day` | Terrain, splines, objects, sky, sun shadows (12:00) | none |
| `grundorf_night` | Night maps, lamps, coronas (23:30) | none |
| `grundorf_rain` | Rain, wet ground, puddles (`Schmuddelwetter.owt`) | none |
| `grundorf_fog_dusk` | Fog, dusk light (`Daemmerungsnebel.owt`, 20:45) | none |
| `grundorf_enhanced` | The Enhanced renderer (18:30) | 0.01 % |
| `grundorf_bus_outside` | A bus from outside: model, materials, reflections (SD80) | none |
| `grundorf_cockpit` | The cab: interior, gauges, mirrors, HUD (SD202 D86) | 0.01 % |
| `grundorf_drive_traffic` | Engine start, gearbox, driving physics, AI traffic, script state | 1 % |
| `spandau_day` | A second, larger map (Berlin-Spandau, 11:00) | 0.01 % |

`grundorf_drive_traffic` spawns the SD202 at stop Bauernhof (`--spawn 500,748,339`), starts it with
its own switches (`--triggers ...`), and drives 12 s with traffic 15 from the driver's view. The
exact arguments of every scene are in the script.

Also compare the ends of the `grundorf_drive_traffic` logs (`drove ... in 12 s, now ... km/h`,
`traffic: ...`). They carry no wall-clock figures.

### What repeats and what does not

- Every scene runs with a fresh, empty `$HOME`, so settings are the defaults and nothing is
  remembered from earlier runs.
- Each run's settings turn `windy_trees` off. The renderer's animation clock is real time since
  start, so swaying trees never come out alike.
- The allowances cover the rest of that clock: the exhaust smoke in the drive scene's left mirror
  (about 0.3 % of pixels), a dozen pixels of a cab instrument, a few pixels in the Enhanced scene,
  and now and then one pixel on Spandau.
- The simulation itself repeats: two runs of the drive scene log the same distance, speed, engine
  speed and traffic figures.

> **Note:** On 2026-10-07 (`main` at c85b2e0d) all scenes were identical or within their
> allowance in three renders. By 2026-11-17, the day, rain and other scenes differed between two
> runs of the same wgpu build (animated particles: 12.8 % of pixels for day, 23.7 % for rain).
> Before you trust a difference, render the "before" binary twice and compare those two runs.

## Audio traces

The golden pictures say nothing about sound. `scripts/audio-trace.sh` runs offscreen drives with
one binary. Each writes an audio trace (`OMSI_AUDIO_TRACE`) and the sound mixed from it
(`OMSI_RECORD`), and two sets are compared:

```bash
scripts/audio-trace.sh render  /path/to/before/openomsi /tmp/audio/before
scripts/audio-trace.sh render  /path/to/after/openomsi  /tmp/audio/after
scripts/audio-trace.sh compare /tmp/audio/before /tmp/audio/after
```

The trace is written in `omsi-audio`'s `AudioEngine`, where every sound arrives: each voice
started (file, gain, pitch, loop, position, range, filter), each parameter change, each stop and
end, the listener once a step, and the radio's effects. An offscreen recording steps the mixer
with the simulation, so its trace does not depend on drawing speed.

| Scene | What it plays |
| --- | --- |
| `grundorf_drive_traffic` | The golden drive: the SD202 started by its switches and driven off, AI traffic |
| `grundorf_rain_outside` | The same drive heard from outside, at night in the rain |
| `spandau_ambience` | Spandau at 08:00 with traffic, timetable buses and passengers; ambience one-shots 40 times as often (`OMSI_AMBIENCE_DENSITY=40`) |

How `compare` reads the traces:

- It names voices by what they play and the step they started in, and sorts the lines of a step.
  Some sounds start from hash maps, whose order changes from run to run.
- It exits 1 when the traces differ, naming the first differing step and the files started more
  or less often.
- It reports differences in the mixes but does not fail on them: the order voices are summed in
  moves samples by one unit (of 32767).

The ambience's recordings come from `$OMSI_AMBIENCE_DIR`, else `~/.openomsi/ambience`. Without
them the ambience is silent in both runs.

`AUDIO_TRACE_SEED` (default 1) changes `OMSI_SEED`.

### Window sessions

An offscreen run does not play the scenery's sound objects, the bus radio or the passengers'
voices. For those, run whole sessions in a window that is never shown:

```bash
scripts/audio-trace.sh window /path/to/openomsi /tmp/audio/window-before
```

These runs use `OMSI_HIDDEN_WINDOW` and are muted. The radio is tuned to a station that never
answers, so its voice and volume are traced, not the music. Their frames follow the wall clock,
so these traces are compared only by which sounds were played. Take two runs of each binary to
see what moves anyway. One known case: a timetable bus due beside the player at the start appears
only while the map is still loading, so whether it is heard depends on start-up speed.

On 2026-10-11 (`main` 0.2.30 a8668bf7 against `refactor/stages` c51b3626) the three offscreen
scenes asked the mixer for the same sounds (37, 43 and 156 voices) with mixes within one unit,
and the window sessions played the same sounds.

## Frame time

Measure a speed change on the window's own frame, without a window on the screen:

```bash
scripts/bench-window.sh /path/to/before/openomsi /tmp/bench/before enhanced
scripts/bench-window.sh /path/to/after/openomsi  /tmp/bench/after  enhanced
scripts/compare-performance.py /tmp/bench/before.json /tmp/bench/after.json
```

The full form is `bench-window.sh <binary> <out prefix> [plain|enhanced] [seconds] [extra args ...]`.
The run is Berlin-Spandau at 08:00 with the MAN EN92, traffic 30, passengers and the timetable,
in the driver's view at 1280×720. At 44 s the bus drives off. The run lasts 110 s by default and
writes `<out prefix>.log` and `<out prefix>.json` (`OMSI_PROFILE`'s exit summary: frame-time
percentiles, every stage in milliseconds, CPU time per frame).

| Variable | Effect |
| --- | --- |
| `BENCH_STILL=1` | The bus never drives off; the camera stands still |
| `BENCH_FPS=<n>` | Limits the frame rate (30 or 60, as most players see it) |
| `BENCH_TRAFFIC=<n>` | Traffic density (default 30) |
| `BENCH_TIME=<HH:MM>` | Time of day (default 08:00) |

> **Tip:** Stage times hold still between runs far better than frame times. Still take several
> runs of each binary, on one machine, and compare the medians.

`tools/mx_port/bench_wgpu_vs_mx.py` compares wgpu and mX builds offscreen (see
[Porting the renderer to mX](MX_PORT.md)).

## Tests with the original install

Tests that read the original OMSI 2 install are `#[ignore]`d, so CI shows them as ignored instead
of passing them without checking anything. They find the install through
`tools/test-support/original_root.rs`: `$OMSI_ROOT`, else `OMSI 2 Original` beside the checkout.

```bash
OMSI_ROOT="/path/to/OMSI 2" cargo test -p <crate> -- --ignored
```

Some need add-on content the stock install lacks (the AA-FR bus bundle, the Volvo Wright family,
the MB O530 Facelift); their ignore reason names it. An ignored test that runs and finds its
content missing fails and names the file.

## Pull request checks

Every pull request runs these (`.github/workflows/pr_checks.yml`). Run them locally before you
push.

### Size budget

```bash
scripts/size-budget.sh            # check
scripts/size-budget.sh --update   # after a split: lower the recorded sizes
scripts/size-budget.sh --list     # every file with its longest function
```

- A file may have at most 1500 lines, or its recorded size in `scripts/size-budget.txt` plus 5 %.
- A function over 300 lines must be recorded and may grow at most 5 %. A new function may not pass
  300 lines.
- Functions are recorded by crate and name, so moving one to another file of its crate keeps its
  allowance.
- `--update` only lowers sizes. Raising one is a reviewer's decision: edit `size-budget.txt` by
  hand in the same pull request and say why.

### Crate layers

```bash
scripts/arch-guard.sh            # check
scripts/arch-guard.sh --graph    # print the dependency graph
```

It keeps the crates in their layers: file formats at the bottom, then the simulation, the
presentation (renderer, audio, UI), the app's subsystems, and `omsi-app` on top.
`scripts/arch-rules.txt` holds the rules:

- `forbid <from> <to>`: for example `forbid omsi-sim omsi-render` (the simulation never knows how
  it is drawn).
- `allow <from> <to>`: an existing edge that still breaks a rule. Once the edge is gone, the check
  asks you to delete the line.
- `app-growth 300`: a pull request may add at most 300 lines of Rust to `crates/omsi-app`, net and
  tests aside. Locally it is measured against the merge base with `main`. A new subsystem gets a
  crate of its own.

Dev-dependencies do not count. See [Architecture](ARCHITECTURE.md#target-layering).

### Formatting

```bash
scripts/format-check.sh
```

It checks the changed `.rs` files one by one (`scripts/format-check-file.sh`) and runs
`cargo fmt --check` on the crates listed in `scripts/formatted-crates.txt`. Format the files you
changed, not whole crates that are not in that list.
