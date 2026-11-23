# Governance

How openBus is run: who maintains it, how a change gets in, and how releases reach
players. Changes to these rules are made like any other change, by a pull request.

## People

| Role | Who | What they do |
| --- | --- | --- |
| Maintainer | [@rafaelkaiz](https://github.com/rafaelkaiz) | reviews and merges pull requests, decides what goes into a release, promotes betas, manages the repository's settings, secrets and access |
| Contributors | anyone | issues, pull requests, translations, testing betas |

openBus is based on [openOMSI](https://github.com/openOMSI-org/openOMSI), whose codebase was
created by usonskyyyy. See [TRADEMARKS.md](TRADEMARKS.md) and [LICENSE](LICENSE).

## How a change gets in

1. Every change goes through a pull request: nobody pushes to `main` directly.
2. A pull request is merged when its checks are green and the maintainer has approved it,
   with every review conversation resolved. A red check is fixed before merging.
   [`.github/CODEOWNERS`](.github/CODEOWNERS) asks the maintainer for a review automatically.
3. A change to the release and update workflows (`.github/workflows/`,
   `scripts/release-notes.sh`, the updater), the online protocol, the formats of saves and
   settings, or anything security related gets extra care and a second look when one is
   available.
4. `main` must always be good enough to ship. Work that is not finished stays in a draft pull
   request, or comes in switched off behind a setting.

## Releases

The maintainer publishes `main` as a beta by hand (**Actions → Release → Run workflow**), and
players get stable releases only (see [docs/VERSIONING.md](docs/VERSIONING.md)). A beta is
promoted to stable when:

* it has been on the Beta channel for a few days, or testers have tried it on the platforms
  it changes (Windows, macOS, Linux, Android);
* no regression found in it is still open;
* `CHANGELOG.md` has its section, and says so when online play needs the new version on
  every computer;
* it is promoted from `main` (**Actions → Promote a beta → Run workflow**).

A fix for a security problem or a crash that many players hit is promoted as soon as it is
checked. A stable release that turns out broken is listed in
[`release/health.json`](release/health.json), and the fix is released as a new version: a
published release is never replaced.

## Security

Security problems are reported privately, never as issues: see [SECURITY.md](SECURITY.md).

## Becoming a maintainer

A contributor who has had several pull requests merged and reviews others' work can be
proposed by the maintainer, who grants write access.
