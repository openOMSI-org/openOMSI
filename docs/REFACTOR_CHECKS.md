# Refactor checks

A refactor (code moved, functions extracted, duplication removed) must not change what the
game does. These checks show it did not, beside `cargo test`.

## Golden pictures

`scripts/golden.sh` renders a fixed set of offscreen scenes with one `openomsi` binary, and
compares two such sets pixel by pixel. Build the binary of the commit before the change and
the one after it, render both, compare:

```sh
export OMSI_ROOT="/path/to/OMSI 2"          # the original install (default: "OMSI 2 Original" beside the checkout)
scripts/golden.sh render /path/to/before/openomsi /tmp/golden/before
scripts/golden.sh render /path/to/after/openomsi  /tmp/golden/after
scripts/golden.sh compare /tmp/golden/before /tmp/golden/after
```

`compare` prints `identical` for each scene that matches byte for byte; otherwise the share of
pixels that differ and the largest and mean channel difference (0..255), and writes an
amplified difference picture into `<after>/diff/`. It exits 1 when any scene differs (beyond
its allowance, below). `render <bin> <dir> <scene> ...` renders only the named scenes, and
`compare` then compares only the scenes that are there. Each run writes `<scene>.log` beside
the picture; the first line names the build.

The binary needs `libsteam_api.dylib` (macOS) beside it: copy both out of `target/release`.
The pictures are made from licensed content: keep the output directories outside the
repository (the script refuses a directory inside it) and never commit them.

### The scenes

All at 960×540 (`GOLDEN_SIZE`), `OMSI_SEED=1` (`GOLDEN_SEED`), the default date (1989-05-30),
Grundorf unless named otherwise, the camera at the map's editor camera unless a bus is
spawned.

| Scene | Arguments | Covers | Allowance |
|---|---|---|---|
| `grundorf_day` | `--time 12:00` | terrain, splines, objects, sky, sun shadows | none |
| `grundorf_night` | `--time 23:30` | night maps, lamps, coronas | none |
| `grundorf_rain` | `--time 14:00 --weather Weather/Schmuddelwetter.owt` | rain, wet ground, puddles | none |
| `grundorf_fog_dusk` | `--time 20:45 --weather Weather/Daemmerungsnebel.owt` | fog, dusk light | none |
| `grundorf_enhanced` | `--time 18:30 --enhanced` | the Enhanced (physically based) renderer | 0.01 % |
| `grundorf_bus_outside` | `--time 10:00 --bus Vehicles/MAN_SD200/MAN_SD80.bus --view outside` | a bus from outside: model, materials, reflections | none |
| `grundorf_cockpit` | `--time 10:00 --bus Vehicles/MAN_SD202/MAN_D86.bus --view driver` | the cab: interior, gauges, mirrors, HUD | 0.01 % |
| `grundorf_drive_traffic` | SD202 at stop Bauernhof (`--spawn 500,748,339`), started by its switches (`--triggers ...`), `--drive 12 --traffic 15 --view driver` | engine start, gearbox, driving physics, AI traffic, script state after 12 s | 1 % |
| `spandau_day` | `--map maps/Berlin-Spandau/global.cfg --time 11:00` | a second, larger map | 0.01 % |

### Why runs are repeatable, and what is not

* Every scene runs with a fresh, empty `$HOME`, so the settings are the defaults and nothing
  remembered from earlier runs (the settings file, the install path, the driver profile)
  takes part.
* The settings file of each run turns `windy_trees` off: the renderer's animation clock is
  real time since start-up, so swaying trees never come out twice alike.
* What else is animated by that clock is covered by the scene allowances (share of pixels
  allowed to differ): the drive scene's exhaust smoke in the left mirror (about 0.3 % of the
  pixels), a dozen pixels of an instrument in the cab, a few pixels off by one in the Enhanced scene, and once in a while a single pixel off
  by one on Spandau. Everything else matched byte for byte.
* The simulation itself is repeatable: two runs of the drive scene log the same distance,
  speed, engine revolutions and traffic figures.

Measured with the binary of `main` at c85b2e0d (Oct 7 2026): three renders, all scenes
identical or within their allowance, every run.

Worth comparing too: the end of the logs (`drove ... in 12 s, now ... km/h`, `traffic: ...`)
of `grundorf_drive_traffic`; they carry no wall-clock figures.

## Audio traces

The golden pictures say nothing about what is heard. `scripts/audio-trace.sh` runs offscreen
drives with one `openomsi` binary, each writing an audio trace (`OMSI_AUDIO_TRACE=<file>`)
and the sound mixed from it (`OMSI_RECORD`), and compares two such sets:

```sh
scripts/audio-trace.sh render  /path/to/before/openomsi /tmp/audio/before
scripts/audio-trace.sh render  /path/to/after/openomsi  /tmp/audio/after
scripts/audio-trace.sh compare /tmp/audio/before /tmp/audio/after
```

The trace is written in omsi-audio's `AudioEngine`, where every sound of the game arrives:
each voice started (its file, gain, pitch, loop, place to the centimetre, range, filter),
each change of its parameters, each stop and end, the listener once a step, the radio's
effects. Whatever moved around above the mixer, a trace taken there means the same in both
builds. An offscreen recording steps the mixer with the simulation, so its trace does not
depend on how fast the pictures are drawn.

| Scene | What it plays |
|---|---|
| `grundorf_drive_traffic` | golden.sh's drive: the SD202 started by its switches and driven off from the cab, AI traffic |
| `grundorf_rain_outside` | the same drive heard from outside, at night in the rain: the street's rain, the bus's outside sounds |
| `spandau_ambience` | Spandau at 08:00 with traffic, the timetable's buses and passengers, the ambience's one-shots forty times as often (`OMSI_AMBIENCE_DENSITY=40`) |

`compare` names voices by what they play and the step they started in, and sorts the lines
of a step: some sounds are started from hash maps (the ambience's layers, a sound set's
entries), whose order changes from run to run of one binary, and with it the voices'
numbers and the place in its clip a loop starts from (drawn in start order). What is left
is the same from run to run; `compare` exits 1 when it is not, with the first step that
differs and the files started more or less often. The mixes are compared too, but only
reported: the order the voices are summed in moves their samples by one unit (of 32767).

The ambience's recordings are read from `$OMSI_AMBIENCE_DIR`, else `~/.openomsi/ambience`;
without them the ambience is silent in both runs alike.

What an offscreen run does not play - the scenery's sound objects, the bus radio, the
passengers' voices - comes from `scripts/audio-trace.sh window <binary> <dir>`: whole
sessions in a window that is never shown (`OMSI_HIDDEN_WINDOW`, `OMSI_MUTE`), the radio
tuned to a station nobody answers (its voice and volume are traced, not the music). Their
frames follow the wall clock, so these traces are compared only as which sounds were played
at all; take two runs of each binary to see what moves anyway. One thing that does: a
timetable bus due beside the player at the start is put there only while the map is still
loading, so whether it is heard depends on how fast the start-up went.

Measured on 2026-10-11, main 0.2.30 (a8668bf7) against refactor/stages c51b3626, both with
the trace: the three offscreen scenes asked the mixer for the same sounds (37, 43 and 156
voices, every parameter change of every step), the mixes within one unit; the two window
sessions played the same sounds (the scenery's cock and blackbird, the radio, traffic,
ambience slots, the bus), as two runs of one binary do.

## Frame time

A change meant to make the game faster is measured on the window's own frame, without a
window on the screen: `scripts/bench-window.sh <binary> <out prefix> [plain|enhanced]` runs
Spandau with traffic, passengers and the timetable in a window that is never shown
(`OMSI_HIDDEN_WINDOW`), drives off, and writes `OMSI_PROFILE`'s exit summary (frame-time
percentiles, every stage of the frame in milliseconds, the process's CPU time per frame).
`scripts/compare-performance.py before.json after.json` compares two of them. The stages'
times hold still from run to run far better than the frame times; take several runs of
each binary and their medians all the same.

## Tests that need the original install

Tests that read the original OMSI 2 install are `#[ignore]`d, so CI shows them as ignored
instead of passing them without looking at anything. They find the install through
`tools/test-support/original_root.rs` (`include!`d into their test modules): `$OMSI_ROOT`,
else `OMSI 2 Original` beside the checkout. Run them with

```sh
OMSI_ROOT="/path/to/OMSI 2" cargo test -p <crate> -- --ignored
```

Some need add-on content the stock install does not have (the AA-FR bus bundle, the Volvo
Wright family, the MB O530 Facelift): their `ignore` reason names it. An ignored test that is
run and finds its content missing fails, naming the file.

## Size budget

`scripts/size-budget.sh` (in the pull request checks) fails when a Rust file or function
grows more than 5 % past its size in `scripts/size-budget.txt`, or a new one passes 1500
(file) or 300 (function) lines. Functions are recorded by crate and name, so one moved to
another file of its crate keeps its allowance. After a split, `scripts/size-budget.sh
--update` lowers the recorded sizes; it never raises them. `--list` shows every file with
its longest function.

## Crate layers

`scripts/arch-guard.sh` (run by the pull request checks) keeps the crates in their layers:
the file formats at the bottom, then the simulation, the presentation (renderer, audio, UI),
the app's subsystems, and `omsi-app` on top as the glue. `scripts/arch-rules.txt` lists what
must not depend on what (`forbid omsi-sim omsi-render`: the simulation never knows how it is
drawn) and the edges that still break a rule and are to be removed (`allow ...`); once such an
edge is gone, the check asks for its `allow` line to be deleted, so it cannot come back.
The same file limits how many lines of Rust a pull request may add to `crates/omsi-app`, its
tests aside (`app-growth`, against the pull request's base; locally against the merge base
with main): a new subsystem gets a crate of its own instead of growing the app. `--graph`
prints the dependency graph.
