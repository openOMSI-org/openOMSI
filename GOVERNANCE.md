# Governance

How openOMSI is run: who maintains it, how a change gets in, and how releases reach
players. Changes to these rules are made like any other change, by a pull request.

## People

| Role | Who | What they do |
| --- | --- | --- |
| Lead developers | [@thibautgnt](https://github.com/thibautgnt), [@isaacsa2](https://github.com/isaacsa2) | review and merge pull requests, decide what goes into a release, promote betas |
| Repository administrator | [@turbo-devv](https://github.com/turbo-devv) | the repository's and organization's settings, secrets and access; reviews pull requests |
| Maintainers | everyone with write access to the repository | review pull requests, triage issues, promote betas |
| Contributors | anyone | issues, pull requests, translations, testing betas |

The original openOMSI codebase was created by usonskyyyy. The official organization is
[openOMSI-org](https://github.com/openOMSI-org).

## How a change gets in

1. Every change, a maintainer's included, goes through a pull request: nobody pushes to `main`
   directly.
2. Each pull request gets test builds for Windows, macOS, Linux and Android (linked in a
   comment), so it can be tried before it is merged.
3. It is merged when its checks are green and **one maintainer other than the author** has
   approved it, with every review conversation resolved. A red check is fixed before merging,
   even where GitHub would allow the merge.
   [`.github/CODEOWNERS`](.github/CODEOWNERS) asks the lead developers and the administrator
   for a review automatically.
4. For a change to one of these, the author also asks a **second** maintainer to look at it
   when one is available: the release and update workflows (`.github/workflows/`,
   `scripts/release-notes.sh`, the updater), the online protocol, the formats of saves and
   settings, and anything security related.
5. `main` must always be good enough to ship. Work that is not finished stays in a draft pull
   request, or comes in switched off behind a setting.

## Releases

A maintainer publishes a beta of `main` by hand (**Actions → Release → Run workflow**)
whenever there is something worth testing, and players get stable releases only (see
[docs/VERSIONING.md](docs/VERSIONING.md)). A maintainer promotes a beta, about once a week,
when:

* it has been on the Beta channel for a few days, or testers have tried it on the platforms
  it changes (Windows, macOS, Linux, Android);
* no regression found in it is still open;
* `CHANGELOG.md` has its section, and says so when online play needs the new version on
  every computer;
* it is promoted from `main` (**Actions → Promote a beta → Run workflow**).

A fix for a security problem or a crash that many players hit is promoted as soon as it is
checked, without waiting for the week. A stable release that turns out broken is listed in
[`release/health.json`](release/health.json), and the fix is released as a new version: a
published release is never replaced.

## Security

Security problems are reported privately, never as issues: see [SECURITY.md](SECURITY.md).
The pull request that fixes one is looked at by a second maintainer when one is available, and
the fix is promoted to stable without waiting for the week.

## Decisions and disagreements

Most decisions are made in the pull request or the issue itself. When maintainers disagree,
the discussion moves to the team's Discord channel, and the lead developers decide. A
decision that changes how the project is run is written down here.

## Becoming a maintainer

A contributor who has had several pull requests merged and reviews others' work can be
proposed by a maintainer. The lead developers agree, and the administrator grants write
access.
