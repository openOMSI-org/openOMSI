# Versioning and releases

What openOMSI's version numbers mean, which release channel you get, and how maintainers cut a
release. Players need the first two sections; the rest is for maintainers.

## Version numbers

A version is `MAJOR.MINOR.PATCH`, for example `1.2.17`.

| Part | Where it comes from |
| --- | --- |
| `MAJOR` | First number in the `VERSION` file, changed by hand |
| `MINOR` | Second number in `VERSION`, changed by hand |
| `PATCH` | Counted automatically: one more than the last release tagged `v<MAJOR.MINOR>.*`, or `0` if there is none |

- Releases have no gaps. After `1.2.16` the next release is `1.2.17`, however many commits lie between.
- A commit that already has a release keeps its version.
- `PATCH` restarts at `0` when `VERSION` changes: after `1.1.23`, the first release with `VERSION` set to `1.2` is `1.2.0`.
- A build of a commit without a release shows the number the next release will get.
- A pull request's test build shows `<version>-pr<number>`, for example `0.1.1313-pr1192`.

You see the version in the launcher's side bar, in `game.log`, in crash reports and with
`openomsi --version`.

## Release channels

| Channel | What it is | Who gets it |
| --- | --- | --- |
| **Stable** (default) | A beta that maintainers tested and promoted | Everyone else. The updater, the start guard, the website's Download page, `releases/latest` and the README badge only see stable releases. |
| **Beta** | A build of `main` started by a maintainer, published as a GitHub pre-release | Testers |

To switch channel:

1. Open the launcher's **Settings → General → Updates**.
2. Set **Update channel** to **Beta (for testers)** or **Stable**.

The launcher looks for an update again at once.

- On Beta, the updater offers the newest release (beta or stable) that has a file for your
  computer and is not marked as broken. **Choose a version…** also lists betas, marked *Beta*.
- Back on Stable, an installed beta stays until a newer stable release comes out, or until you
  pick an older one in **Choose a version…**.
- If your installed version is marked as broken, the updater can offer the current stable
  release even when it is older. With automatic updates on, it installs when the game is closed.

The website has a separate Betas download page. When no beta is newer than stable, it links
back to the stable download instead of offering an older build.

## Cutting a release

### Start a new MAJOR.MINOR

Change the one line in `VERSION` in a pull request, like any other change:

```bash
git switch -c version-1.2 && echo 1.2 > VERSION
git commit -am "Version 1.2" && git push -u origin version-1.2
```

`scripts/version.sh` prints the version of the checked-out commit. The build
(`crates/omsi-platform/build.rs`) uses the same rule; CI passes the result in `OPENOMSI_VERSION`.

### Publish a beta

A push to `main` builds every platform but publishes nothing.

1. Open **Actions → Release → Run workflow** on GitHub and run it from `main`
   (or `gh workflow run release.yml --ref main`).
2. The workflow (`.github/workflows/release.yml`) works out the version with `scripts/version.sh`.
3. It builds Windows x64 and ARM64, macOS (Apple silicon and Intel), Linux x64 and ARM64 and
   Android, and packs the dedicated server from the Linux and Windows builds.
4. It creates the pre-release `v<version>`, tagged on that commit.

The release carries:

| Files | Platforms |
| --- | --- |
| `openOMSI-<version>-<os>-<arch>.zip` | `windows-x64`, `windows-arm64`, `macos-arm64`, `macos-x64`, `linux-x64`, `linux-arm64` |
| `openOMSI-<version>-windows-<arch>-setup.exe` | Windows installers (x64, ARM64) |
| `openOMSI-<version>-macos-<arch>.dmg` | macOS disk images (arm64, x64) |
| `openOMSI-<version>-android-arm64.apk`, `openOMSI-android.apk` | Android |
| `openOMSI-<version>-server-<os>-<arch>.zip` | Dedicated server: `linux-x64`, `linux-arm64`, `windows-x64`, `windows-arm64` |
| `SHA256SUMS` | SHA-256 of every file above |

The release notes come from this version's section of `CHANGELOG.md` (`scripts/release-notes.sh`).

The workflow has an **only** input (`windows`, `macos`, `linux` or `android`) to release a beta
of one platform, for a fix only that platform needs. Such a beta never gets the other
platforms' files, so it cannot be promoted.

### Promote a beta to stable

When a beta has been tested and nothing blocks it (about once a week, sooner for an urgent fix):

1. Open **Actions → Promote a beta → Run workflow** from `main`.
2. Enter the beta's version, for example `0.2.55`.

The beta becomes the stable release as it is: same tag, same files, same SHA-256. Nothing is
built again. Its notes are rewritten to list every change since the previous stable release.

Only a beta that meets all of these can be promoted:

- published from `main`;
- newer than the current stable release;
- has the files of every platform;
- not listed in `release/health.json`.

The workflow (`.github/workflows/promote.yml`) reads `release/health.json` and `CHANGELOG.md`
as they are on `main` at that moment, whatever branch it is started from.

### A published release never changes

- The release job refuses a version that already exists, whose tag names another commit, or
  whose commit is older than the last release.
- It never moves a tag or replaces a file. Tags are never deleted: the next version is counted from them.
- A mistake is fixed with a new commit, and so a new version.
- The repository's *immutable releases* setting makes GitHub enforce this too.

To check a download by hand:

```bash
sha256sum -c SHA256SUMS --ignore-missing
```

The launcher's updater checks each archive against the SHA-256 that GitHub's API lists for it.
When the API cannot be reached and it falls back to github.com, it relies on HTTPS alone.

Before any update replaces the program, the new one must pass `openomsi --self-test` (it
starts, prints its version and exits). A release that cannot even load on a computer is
refused there and never offered again.

## Pull request test builds

Pull requests build Windows x64, macOS (Apple silicon), Linux x64 and Android, without
publishing. The label `ci:all-platforms` builds all seven platforms from the next push or run.

- `.github/workflows/pr_builds.yml` comments on the pull request with a download link per
  platform through [nightly.link](https://nightly.link) (no GitHub account needed), and
  updates the comment on every push. The files are kept for 14 days.
- The version is `<version>-pr<number>`. The updater never offers a release to a test build,
  so it stays until you delete it.
- The Android build is a separate app, *openOMSI PR #N* (`org.openomsi.game.pr`). It installs
  beside the release and shares its `openOMSI` folder.

Build output never goes into the repository (`target/` and `dist/` are ignored).

## Handling a broken release

The start guard (see [Architecture](ARCHITECTURE.md)) heals a release that crashes or hangs
at start on its own: it installs a newer release, or puts the previous version back.

For a release that starts but is broken otherwise (saves, physics, a crash a minute in):

1. List it in `release/health.json` on `main`:

   ```json
   { "bad": ["0.2.31"], "message": "0.2.31 loses the timetable after a stop" }
   ```

2. Push the fix and promote its beta as soon as it is checked, so players without a working
   previous build can update.
3. Take the version off the list once nobody runs it any more.

What players see:

- Every start of a listed version notes it.
- On the next start, the guard tries the updater's candidate, then the locally kept previous
  version if there is one, and tells the player why.
- The guard installs the replacement even when automatic updates are off. A download of an
  older stable build needs the archive's SHA-256 from the GitHub API and passes the same
  self-test as a normal update.
- The list is read from GitHub with a short timeout.

See [Troubleshooting](TROUBLESHOOTING.md) for what players can do themselves.

## The website

`.github/workflows/pages.yml` builds `site/` from the Markdown files in `docs/` and publishes
it to GitHub Pages (https://openomsi.org/) whenever they change on `main`.

- A documentation change is one edit in `docs/`. The build splits long documents into
  chapters, makes the menu, the search index and the links, and fails on a broken link.
- The Download page offers the latest stable release. Betas have their own page with the same
  platform cards and install steps.
- The release history has separate Stable and Betas lists. Auxiliary asset packs appear in neither.
