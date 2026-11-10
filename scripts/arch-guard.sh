#!/usr/bin/env bash
# Architecture guard: which workspace crate may depend on which, and how big the app crate
# may get. The rules live in scripts/arch-rules.txt (see there); this script only checks them.
#
#   scripts/arch-guard.sh            check every crates/*/Cargo.toml and what a change adds to omsi-app
#   scripts/arch-guard.sh --graph    print the workspace dependency graph, one crate per line
#
# Normal and build dependencies count, target-specific ones included; dev-dependencies do not
# (a test may pull in whatever helps it). A rule `forbid <from> <to>` (globs allowed) fails
# the check when a crate matching <from> depends on one matching <to>; `allow <from> <to>`
# records an existing edge that breaks a rule and is still to be removed. An allowance that is
# no longer needed fails the check as well, so that the line is deleted and the edge cannot
# come back. `app-growth <n>`: a change may add at most n lines of Rust to crates/omsi-app, its
# tests aside, against $BASE (the pull request's base; locally the merge base with main): the
# app crate is the glue between the others, new subsystems get crates of their own. A cap on the
# crate's total stopped every small pull request once main came near it.
#
# Needs nothing but python3 (3.8+), git and the checkout.
set -euo pipefail

repo="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
exec python3 -I - "$repo" "$@" <<'PY'
import fnmatch, os, re, subprocess, sys

repo = sys.argv[1]
mode = sys.argv[2] if len(sys.argv) > 2 else "--check"
rules_path = os.path.join(repo, "scripts", "arch-rules.txt")
APP = "omsi-app"
TEST_FILE = re.compile(r"(^|/)tests(/|\.rs$)|_tests?\.rs$")


def deps_of(manifest):
    """Workspace crates (omsi-*) named in the normal/build dependency tables of one Cargo.toml."""
    found, section = set(), ""
    for raw in open(manifest, encoding="utf-8"):
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        m = re.match(r"^\[(.+)\]$", line)
        if m:
            section = m.group(1).strip()
            # [dependencies.omsi-x] / [target.'cfg(..)'.dependencies.omsi-x]
            t = re.match(r"^(?:target\..+\.)?(?:build-)?dependencies\.(omsi-[a-z0-9-]+)$", section)
            if t:
                found.add(t.group(1))
            continue
        if not re.match(r"^(?:target\..+\.)?(?:build-)?dependencies$", section):
            continue
        d = re.match(r"^(omsi-[a-z0-9-]+)\s*=", line)
        if d:
            found.add(d.group(1))
        # a renamed dependency: foo = { package = "omsi-x", ... }
        p = re.search(r'package\s*=\s*"(omsi-[a-z0-9-]+)"', line)
        if p:
            found.add(p.group(1))
    return found


def graph():
    g = {}
    crates_dir = os.path.join(repo, "crates")
    for name in sorted(os.listdir(crates_dir)):
        manifest = os.path.join(crates_dir, name, "Cargo.toml")
        if os.path.exists(manifest):
            g[name] = deps_of(manifest)
    return g


def git(*args):
    r = subprocess.run(["git", "-C", repo, *args], capture_output=True, text=True)
    return r.stdout.strip() if r.returncode == 0 else None


def base_rev():
    if os.environ.get("BASE"):
        return os.environ["BASE"]
    for main in ("upstream/main", "origin/main", "main"):
        b = git("merge-base", "HEAD", main)
        if b:
            return b
    return None


def app_growth(base):
    """Lines of Rust a change adds to omsi-app (added minus removed), test files aside."""
    out = git("diff", "--numstat", base, "HEAD", "--", f"crates/{APP}")
    if out is None:
        return None
    n = 0
    for line in out.splitlines():
        added, removed, path = line.split("\t", 2)
        if path.endswith(".rs") and added != "-" and not TEST_FILE.search(path):
            n += int(added) - int(removed)
    return n


def read_rules():
    forbid, allow, cap, lines = [], [], None, []
    for raw in open(rules_path, encoding="utf-8"):
        lines.append(raw)
        line = raw.split("#", 1)[0].split()
        if not line:
            continue
        if line[0] in ("forbid", "allow") and len(line) == 3:
            (forbid if line[0] == "forbid" else allow).append((line[1], line[2]))
        elif line[0] == "app-growth" and len(line) == 2:
            cap = int(line[1])
        else:
            sys.exit(f"{rules_path}: cannot read line: {raw.rstrip()}")
    return forbid, allow, cap, lines


g = graph()
if mode == "--graph":
    for c, ds in g.items():
        print(f"{c}: {' '.join(sorted(ds)) or '-'}")
    sys.exit(0)

forbid, allow, cap, _ = read_rules()
if mode != "--check":
    sys.exit(f"unknown option {mode}")

errors, used_allow = [], set()
for c, ds in g.items():
    for d in sorted(ds):
        if d not in g:
            errors.append(f"crates/{c}/Cargo.toml: depends on {d}, which is not a crate of the workspace")
            continue
        broken = [(f, t) for f, t in forbid if fnmatch.fnmatch(c, f) and fnmatch.fnmatch(d, t)]
        if not broken:
            continue
        allowed = [(f, t) for f, t in allow if fnmatch.fnmatch(c, f) and fnmatch.fnmatch(d, t)]
        if allowed:
            used_allow.update(allowed)
            continue
        f, t = broken[0]
        errors.append(f"crates/{c}/Cargo.toml: {c} depends on {d}, against `forbid {f} {t}` in scripts/arch-rules.txt")
for f, t in allow:
    if (f, t) not in used_allow:
        errors.append(f"scripts/arch-rules.txt: `allow {f} {t}` is no longer needed: delete the line so the edge cannot come back")
base = base_rev() if cap is not None else None
grown = app_growth(base) if base else None
if grown is not None and grown > cap:
    errors.append(f"crates/{APP}: this change adds {grown} lines of Rust (tests aside), more than the {cap} a change may add "
                  "(app-growth in scripts/arch-rules.txt). Put new subsystems into a crate of their own; omsi-app is the glue.")

for e in errors:
    print(f"::error::{e}" if os.environ.get("GITHUB_ACTIONS") else e)
if errors:
    print(f"\narch guard: {len(errors)} problem(s) (scripts/arch-guard.sh explains the rules)")
    sys.exit(1)
growth = f"{APP} {grown:+} lines against {base[:9]}" if grown is not None else f"{APP}'s growth not checked (no base to compare with)"
print(f"arch guard: ok ({len(g)} crates, {growth})")
PY
