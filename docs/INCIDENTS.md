# Random traffic incidents: the design, and what of it exists

The request (#2745) is for the road events of ETS 2 in a bus simulator: now and then a crash,
a broken-down lorry or a closed lane, holding traffic up for a few minutes, with the player able
to turn them off or make them rare. This page is the design; the first part of it is in
`omsi_sim::incidents`.

## The three questions, kept apart

1. **When** an incident happens. A Poisson process: with a frequency of *r* incidents an hour,
   the wait for the next one is drawn from an exponential distribution of rate *r*. A process,
   not a timer, so that a player who has waited a long time is not "owed" an incident, and two
   in a row is possible but rare.
2. **Where and what.** The caller hands in the places it has — junctions, the midpoints of road
   segments, the map's stops — and one is chosen; then the kind (an accident, a breakdown,
   roadworks) and how long it stays. Roadworks last longest (10–30 min), an accident 2–7 min.
3. **What it does to the world.** Not in this part. A lane that the AI traffic brakes for, a
   mesh and a warning triangle on the road, the player's own route around it.

## Frequencies

| setting | incidents an hour | on the road at once |
|---|---|---|
| `off` (default) | none | — |
| `low` | 0.25 | 1 |
| `medium` | 1 | 2 |
| `high` | 3 | 4 |

`Frequency::parse` reads the words and the numbers `settings.cfg` writes, and anything it does
not understand is `off`: a mistyped setting never fills the road with crashes.

## Determinism

The scheduler is seeded (`Scheduler::new(frequency, seed)`): the same seed and the same calls
give the same incidents. That is what lets a replay show the same road again, and what a
multiplayer session needs to agree on without sending every incident over the network — the
host's seed and clock are enough.

## What exists today, and what does not

**Exists** (`crates/omsi-sim/src/incidents.rs`, with tests): the frequency, the kinds and their
durations, the scheduler's wait/expire logic, the cap on how many are active, turning the
feature off mid-drive (which clears what is in front of the player), and the setting's words.

**Does not exist yet**:

* a place in `settings.cfg` and the options page (the frequency is in the model, not on a
  screen);
* the incident in the world: an object at `Incident::at`, a blocked lane, AI traffic that brakes
  and queues behind it, the player's own bus colliding with it;
* the sound and the HUD ("an accident ahead");
* the choice of candidates from the loaded map, which is where the map and AI code will decide
  what "a place an incident can be" means.

The issue's milestone (v0.7) asks for a design before the code; this is that design, plus the
part of it that is arithmetic and can be tested without a map.
