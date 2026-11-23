"""Sorts a new or edited issue, for .github/workflows/issue_triage.yml.

* Topic: the forms' "What is it about?" answer becomes a label (`area: ...`), swapped when
  the answer is edited.
* Milestone: a new issue without one gets the milestone its kind and topic belong to (see
  the milestones' descriptions); a bug that worked in an earlier version is a regression
  and goes to v0.1.x. Only when the issue is opened, so a maintainer's choice stays.
* Crashes: a report with a Rust panic in it is labelled `crash`. When an open issue already
  has the same panic (the same place in the same file, the same message), a report made by
  the game's "Report on GitHub" is closed as its duplicate and the original is told about
  it; when that issue was closed, the report gets a pointer to it and stays open.

Run as a script it reads the issue from $GITHUB_EVENT_PATH and acts through the `gh` CLI
($GH_TOKEN, $GH_REPO). With `--dry-run` it only prints what it would do.
"""
import json
import os
import re
import subprocess
import sys

# The forms' answers (both forms): (label, milestone for a bug, milestone for a request).
# None: no label, or no milestone of its own.
AREAS = {
    "Crash, freeze or the game doesn't start": ("crash", 1, None),
    "Performance (FPS, stutter, loading)": ("area: performance", 1, None),
    "Passengers, doors, tickets and money": ("area: passengers", 2, 6),
    "Driving, controls, input devices and cameras": ("area: controls", 2, 5),
    "AI traffic, timetable and navigation": ("area: AI and timetable", 2, 5),
    "Multiplayer or dedicated server": ("area: multiplayer", 2, 5),
    "One particular bus, DLC or map": ("area: mod compatibility", 3, None),
    "Graphics, weather and effects": ("area: graphics", 4, 6),
    "Sound": ("area: sound", 4, 5),
    "Launcher, menus and HUD": ("area: launcher and HUD", 4, 5),
    "Modding (vehicle, map and script features)": ("area: modding", None, 6),
    "A new platform or game mode (VR, mobile, trams...)": ("area: new platform", None, 7),
}
LABEL_COLOURS = {"crash": "b60205", "regression": "e99695"}
AREA_COLOUR = "c5def5"


def section(body: str, heading: str) -> str:
    """The answer under a form's `### heading`, or "" (also for "_No response_")."""
    m = re.search(r"^###\s*" + re.escape(heading) + r"\s*\n(.*?)(?=^###\s|\Z)", body, re.M | re.S)
    answer = m.group(1).strip() if m else ""
    return "" if answer == "_No response_" else answer


def worked_before(answer: str) -> bool:
    """The answer to "Did it work in an earlier version?" names one or says yes."""
    a = answer.strip().lower()
    if not a or re.match(r"(no|nope|never|not|unknown|i don.?t know|no idea|didn.?t)\b", a):
        return False
    return bool(re.search(r"\b\d+\.\d+\.\d+\b", a) or re.match(r"yes\b", a) or re.search(r"\bworked\b", a))


def panic_key(text: str) -> str | None:
    """Where a Rust panic happened and what it said, the same in every report of it:
    `wgpu-hal-29.0.4/src/gles/device.rs:87 index out of bounds: the len is # but the index is #`.
    The path loses the user's or the build machine's folders, the message its numbers."""
    m = re.search(r"panicked at (\S+?):(\d+):\d+:[ \t]*(.*)", text, re.S)
    if not m:
        return None
    # The message, with what wgpu says caused it on the lines under it, up to the end of the
    # code block, the next log line or the next heading of the form. A bare "Validation
    # Error" is the same line for every one of wgpu's errors; its causes tell them apart.
    lines = []
    for line in m.group(3).splitlines():
        s = line.strip()
        if s.startswith("```") or s.startswith("###") or re.match(r"\[(\d{4}-|\d+\.\d+ )", s):
            break
        if s:
            lines.append(s)
    message = " ".join(lines)
    path = m.group(1).replace("\\", "/")
    for marker in ("/registry/src/", "/crates/", "/src/"):
        i = path.find(marker)
        if i >= 0:
            path = path[i + len(marker):]
            if marker == "/registry/src/":
                path = path.split("/", 1)[1]  # index.crates.io-<hash>/
            break
    message = re.sub(r"\d+", "#", message)[:200]
    return f"{path}:{m.group(2)} {message}"


def plan(issue: dict, action: str, candidates: list[dict]) -> dict:
    """What to do with `issue`: labels to add and remove, the milestone number to set, and
    for a crash the issue it repeats. `candidates` are other issues with a panic in them
    (number, state, body)."""
    body = issue.get("body") or ""
    labels = {l["name"] for l in issue.get("labels", [])}
    is_bug = "bug" in labels or "### What happened?" in body
    out = {"add": [], "remove": [], "milestone": None, "same_crash": None, "key": None}

    area = section(body, "What is it about?")
    label, bug_ms, feature_ms = AREAS.get(area, (None, None, None))
    if label and label not in labels:
        out["add"].append(label)
    out["remove"] = [l for l in labels if l.startswith("area: ") and l != label]

    regression = is_bug and worked_before(section(body, "Did it work in an earlier version?"))
    if regression and "regression" not in labels:
        out["add"].append("regression")

    key = panic_key(body)
    if key or issue["title"].startswith("Crash:"):
        if "crash" not in labels and "crash" not in out["add"]:
            out["add"].append("crash")
    out["key"] = key

    if action == "opened" and not issue.get("milestone"):
        if regression or key:
            out["milestone"] = 1
        else:
            out["milestone"] = bug_ms if is_bug else feature_ms

    if action == "opened" and key:
        same = [c for c in candidates if c["number"] != issue["number"] and panic_key(c.get("body") or "") == key]
        # an open one first (the oldest: the one the others are collected on), else the
        # most recently closed
        open_ = sorted((c for c in same if c["state"] == "OPEN"), key=lambda c: c["number"])
        closed = sorted((c for c in same if c["state"] != "OPEN"), key=lambda c: -c["number"])
        out["same_crash"] = (open_ or closed or [None])[0]
    return out


def gh(*args: str, check: bool = True) -> str:
    return subprocess.run(["gh", *args], check=check, capture_output=True, text=True, encoding="utf-8").stdout


def milestone_number(n: int) -> int | None:
    for m in json.loads(gh("api", "repos/{owner}/{repo}/milestones?state=open&per_page=100")):
        if m["title"].startswith(f"v0.{n}.x"):
            return m["number"]
    return None


def first_line(body: str) -> str:
    """`openBus 0.1.1324 on android` from a crash report: its version and system."""
    line = (body or "").strip().splitlines()[0] if (body or "").strip() else ""
    return line if line.startswith("openBus ") else ""


def main() -> None:
    dry = "--dry-run" in sys.argv
    event = json.load(open(os.environ["GITHUB_EVENT_PATH"], encoding="utf-8"))
    issue, action = event["issue"], event["action"]
    maintainer = issue.get("author_association") in ("OWNER", "MEMBER", "COLLABORATOR")
    candidates = []
    if action == "opened" and panic_key(issue.get("body") or ""):
        # (by the panic in the text, not by the label: reports from before this workflow have none)
        candidates = json.loads(gh("issue", "list", "--search", '"panicked at" in:body', "--state", "all", "--limit", "300", "--json", "number,state,body"))
    p = plan(issue, action, candidates)
    print(json.dumps({k: v if k != "same_crash" else (v or {}).get("number") for k, v in p.items()}), file=sys.stderr)
    if dry:
        return
    n = str(issue["number"])
    for label in p["add"]:
        colour = AREA_COLOUR if label.startswith("area: ") else LABEL_COLOURS.get(label, "ededed")
        gh("label", "create", label, "--color", colour, check=False)
    edit = ["issue", "edit", n]
    for label in p["add"]:
        edit += ["--add-label", label]
    for label in p["remove"]:
        edit += ["--remove-label", label]
    if len(edit) > 3:
        gh(*edit)
    if p["milestone"]:
        number = milestone_number(p["milestone"])
        if number:
            gh("api", "-X", "PATCH", f"repos/{{owner}}/{{repo}}/issues/{n}", "-F", f"milestone={number}")
    same = p["same_crash"]
    if not same:
        return
    where = p["key"].split(" ", 1)[0]
    if same["state"] == "OPEN" and issue["title"].startswith("Crash:") and not maintainer:
        gh("issue", "comment", str(same["number"]), "--body", f"Reported again in #{n}" + (f" ({first_line(issue.get('body'))})." if first_line(issue.get("body")) else "."))
        gh("label", "create", "duplicate", "--color", "cfd3d7", check=False)
        gh("issue", "edit", n, "--add-label", "duplicate")
        gh("issue", "close", n, "--reason", "not planned", "--comment",
           f"Thanks for the report! This is the same crash as #{same['number']} (at `{where}`), so it is followed there. "
           "If you can tell what you were doing when it happened, please add it to that issue - it helps to find the cause.")
    elif same["state"] != "OPEN":
        gh("issue", "comment", n, "--body",
           f"The same crash (at `{where}`) was reported in #{same['number']}, which has been closed. "
           "If you are on a release that came out after that, the crash is back - please say which version you have, "
           "if it is not in the report already.")


if __name__ == "__main__":
    main()
