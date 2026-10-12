You are finishing a `git merge upstream/main` in this repository: the Omsi-Hub fork of openOMSI
(Rust), branch `beta`. The merge stopped on conflicts. Resolve every one of them so that the
workspace builds, then stop. Do not commit, do not push, do not run `git merge --abort`,
`git reset`, `git checkout` or `git stash`; reading with `git show :1:<path>` (base),
`:2:<path>` (ours, the fork) and `:3:<path>` (theirs, the project), `git log` and `git diff`
is fine.

What the two sides are:
- OURS (HEAD) is the Omsi-Hub fork: its own launcher interface (crates/omsi-app/src/launcher:
  the step flow, the bus company, the line editor, the livery studio, multiplayer by code,
  the hint toast, the bus chosen per duty), game features (head tracking with a native Tobii
  reader and a per-frame filter, the mph navigator, the loading screen, the trip report, the
  company's buses only in its duties via --company, the fork's updater for 4-part versions in
  updater.rs) and many fixes.
- THEIRS (upstream/main) is the openOMSI project's new work.

How to resolve:
- For every conflict understand both sides against the base and keep BOTH intentions. When
  the project refactored, moved or renamed code the fork changed, port the fork's change onto
  the project's new structure. When both added to a list, a match, a struct or a test, keep
  both. When the project fixed something the fork also touched, keep the fix.
- Never drop a feature of either side silently. The updater must keep pointing at the fork
  (Luc-nbr/openOMSI). The fork's launcher look (its theme: `accent()` and the other functions
  and constants of launcher/theme.rs) stays; code of the project that uses theme names the
  fork does not have gets the fork's equivalents.
- Interface texts (crates/omsi-app/locales/*.yml) are merged by a merge driver already; if one
  is still conflicted, keep both sides' texts and languages.
- Keep the codebase's style: British English comments in the plain style around them.
- Remove every conflict marker. Then run `cargo check --workspace --tests` and fix what the
  merge broke (moved fields, changed signatures, renamed items), until it passes. Keep the
  fixes as small as the merge needs; do not refactor beyond it.
- Finish with a short summary of what each conflicted file needed and anything doubtful.
