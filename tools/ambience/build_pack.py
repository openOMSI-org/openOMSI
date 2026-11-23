#!/usr/bin/env python3
"""Build the openBus ambience sound pack from the raw recordings.

    python3 -E build_pack.py <raw dir> <assign.csv> <slots.toml> <out dir> [--bed-seconds 45] [--jobs N]

Every row of assign.csv files one raw recording under one slot of slots.toml. Depending on the
slot's kind the recording is cut into
  bed / point(loop)   the calmest 45 s stretch(es) (--bed-seconds), 0.3 s fades, -27 LUFS integrated, peak <= -1 dBFS
  event / point(event) single calls found by onset detection, peak-normalised to -3 dBFS
  ir                  the whole response, leading silence trimmed, -1 dBFS peak, FLAC
and encoded at 32 kHz (MP3 VBR -q:a 5, IRs FLAC). The result is <out>/<slot path>/<source>_<n>.mp3,
<out>/pack.json (every take with its cut, level and licence) and <out>/CREDITS.md.

Source metadata comes from <raw dir>/manifest.csv (Freesound; titles corrected from titles.csv,
the exact titles of the Freesound pages) and sources_extra.csv next to assign.csv (Wikimedia
Commons and radio aporee files).

assign.csv columns: file, slot, take, note. `take` holds optional hints separated by ';':
  use A-B      only use this stretch (seconds; several allowed)
  avoid A-B    never use this stretch (several allowed)
  max N        at most N cuts (events) or takes (beds)
"""
import csv
import json
import math
import os
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.parse
from concurrent.futures import ProcessPoolExecutor

import numpy as np
from scipy.ndimage import percentile_filter
from scipy.signal import lfilter

FFMPEG = shutil.which("ffmpeg") or "/opt/homebrew/bin/ffmpeg"
FFPROBE = shutil.which("ffprobe") or "/opt/homebrew/bin/ffprobe"
SR = 32000
HOP = 0.05                      # analysis hop, seconds
HOPN = int(SR * HOP)
BED_LUFS = -27.0
BED_PEAK_DB = -1.0
EVENT_PEAK_DB = -3.0
IR_PEAK_DB = -1.0
FADE = 0.3
PREROLL = 0.15
MAX_CUTS = 8

# ------------------------------------------------------------------------------------- slots


def load_slots(path):
    """slots.toml -> {name: {kind, mode, ...}}. Uses tomllib when present, else a small parser
    that understands the flat [slot."name"] / key = "value" layout of that file."""
    try:
        import tomllib  # Python 3.11+
        with open(path, "rb") as f:
            return tomllib.load(f)["slot"]
    except ImportError:
        pass
    slots, cur = {}, None
    for line in open(path, encoding="utf-8"):
        line = line.split("#", 1)[0].strip() if not line.strip().startswith("about") else line.strip()
        if not line:
            continue
        m = re.match(r'^\[slot\."([^"]+)"\]$', line)
        if m:
            cur = slots.setdefault(m.group(1), {})
            continue
        m = re.match(r'^(\w+)\s*=\s*"(.*)"\s*$', line)
        if m and cur is not None:
            cur[m.group(1)] = m.group(2)
    return slots


def slot_class(slot, spec):
    """'bed', 'event' or 'ir' for the processing path."""
    kind = spec.get("kind")
    if kind == "point":
        return "bed" if spec.get("mode") == "loop" else "event"
    return kind


def event_rule(slot):
    """Cut rules per event slot: mode (onset / strike / whole), length limits, bed window."""
    if slot.startswith(("bird.", "animal.")):
        return dict(mode="onset", lo=1.0, hi=15.0, bedwin=5.0, hold=0.5)
    if slot.startswith("thunder."):
        return dict(mode="onset", lo=4.0, hi=30.0, bedwin=20.0, hold=1.5)
    if slot in ("far.aircraft", "far.helicopter", "far.train") or slot.startswith("far.siren"):
        return dict(mode="onset", lo=8.0, hi=45.0, bedwin=30.0, hold=1.5)
    if slot.startswith("church.peal") or slot in ("church.angelus", "church.change_ringing"):
        return dict(mode="whole", lo=10.0, hi=240.0, bedwin=30.0, hold=3.0)
    if slot == "church.strike":
        return dict(mode="strike", lo=1.5, hi=8.0, bedwin=10.0, hold=0.5)
    if slot in ("school.gong", "school.bell"):
        return dict(mode="whole", lo=1.0, hi=60.0, bedwin=10.0, hold=1.0)
    if slot in ("far.lawnmower", "far.leaf_blower", "far.tractor", "farm.tractor"):
        # steady machines: no onsets to speak of, take the strongest stretches
        return dict(mode="stretch", lo=2.0, hi=30.0, bedwin=10.0, hold=1.0)
    if slot in ("far.door", "far.forklift"):
        return dict(mode="onset", lo=1.0, hi=6.0, bedwin=10.0, hold=0.5)
    return dict(mode="onset", lo=2.0, hi=30.0, bedwin=10.0, hold=1.0)


# ------------------------------------------------------------------------------- loudness


def _biquad_shelf(fs, fc=1500.0, gain_db=4.0, q=1 / math.sqrt(2)):
    a = 10 ** (gain_db / 40)
    w0 = 2 * math.pi * fc / fs
    alpha = math.sin(w0) / (2 * q)
    c = math.cos(w0)
    b = [a * ((a + 1) + (a - 1) * c + 2 * math.sqrt(a) * alpha),
         -2 * a * ((a - 1) + (a + 1) * c),
         a * ((a + 1) + (a - 1) * c - 2 * math.sqrt(a) * alpha)]
    aa = [(a + 1) - (a - 1) * c + 2 * math.sqrt(a) * alpha,
          2 * ((a - 1) - (a + 1) * c),
          (a + 1) - (a - 1) * c - 2 * math.sqrt(a) * alpha]
    return np.array(b) / aa[0], np.array(aa) / aa[0]


def _biquad_highpass(fs, fc=38.0, q=0.5):
    w0 = 2 * math.pi * fc / fs
    alpha = math.sin(w0) / (2 * q)
    c = math.cos(w0)
    b = [(1 + c) / 2, -(1 + c), (1 + c) / 2]
    aa = [1 + alpha, -2 * c, 1 - alpha]
    return np.array(b) / aa[0], np.array(aa) / aa[0]


K_SHELF = _biquad_shelf(SR)
K_HP = _biquad_highpass(SR)


class KFilter:
    """Streaming BS.1770 K-weighting; returns the channel-summed squared signal."""

    def __init__(self, ch):
        self.z1 = np.zeros((ch, 2))
        self.z2 = np.zeros((ch, 2))

    def __call__(self, x):  # x: (n, ch)
        out = np.zeros(x.shape[0])
        for c in range(x.shape[1]):
            y, self.z1[c] = lfilter(K_SHELF[0], K_SHELF[1], x[:, c], zi=self.z1[c])
            y, self.z2[c] = lfilter(K_HP[0], K_HP[1], y, zi=self.z2[c])
            out += y * y
        return out


def to_db(ms):
    return -0.691 + 10 * np.log10(np.maximum(ms, 1e-20))


def hop_energy(x):
    """(n, ch) samples -> per-hop sum of K-weighted squared signal."""
    z = KFilter(x.shape[1])(x)
    n = len(z) // HOPN
    e = z[: n * HOPN].reshape(n, HOPN).sum(axis=1)
    return e


def window_loudness(e, win_s, centred=True):
    """Loudness (LUFS) of a sliding window of win_s seconds over hop energies."""
    w = max(1, int(round(win_s / HOP)))
    c = np.concatenate([[0.0], np.cumsum(e)])
    n = len(e)
    if centred:
        idx = np.arange(n)
        a = np.clip(idx - w // 2, 0, n)
        b = np.clip(a + w, 0, n)
        a = np.clip(b - w, 0, n)
    else:
        a = np.arange(n)
        b = np.clip(a + w, 0, n)
    ms = (c[b] - c[a]) / (np.maximum(b - a, 1) * HOPN)
    return to_db(ms)


def integrated(x):
    """BS.1770-4 gated integrated loudness of samples (n, ch)."""
    z = KFilter(x.shape[1])(x)
    blk, step = int(0.4 * SR), int(0.1 * SR)
    if len(z) < blk:
        return float(to_db(z.mean())) if len(z) else -120.0
    c = np.concatenate([[0.0], np.cumsum(z)])
    starts = np.arange(0, len(z) - blk + 1, step)
    ms = (c[starts + blk] - c[starts]) / blk
    l = to_db(ms)
    g = ms[l > -70]
    if not len(g):
        return -120.0
    rel = to_db(g.mean()) - 10
    g2 = ms[(l > -70) & (l > rel)]
    return float(to_db(g2.mean())) if len(g2) else -120.0


def max_short_term(x):
    z = KFilter(x.shape[1])(x)
    w = int(3.0 * SR)
    if len(z) <= w:
        return float(to_db(z.mean()))
    c = np.concatenate([[0.0], np.cumsum(z)])
    starts = np.arange(0, len(z) - w + 1, int(0.1 * SR))
    return float(to_db(((c[starts + w] - c[starts]) / w).max()))


def true_peak_db(x):
    """Peak of the 4x oversampled signal (inter-sample peaks after MP3 decoding)."""
    from scipy.signal import resample_poly
    if not x.size:
        return -120.0
    return peak_db(resample_poly(x, 4, 1, axis=0))


def encode_checked(y, fn, ch, limit_db):
    """Encode, decode again and lower the gain until the decoded true peak is <= limit_db."""
    for _ in range(4):
        encode(y, fn)
        z = decode(fn, ch)
        pk = true_peak_db(z)
        if pk <= limit_db:
            break
        y = y * 10 ** ((limit_db - 0.2 - pk) / 20)
    return z, pk


def peak_db(x):
    p = float(np.abs(x).max()) if x.size else 0.0
    return 20 * math.log10(p) if p > 0 else -120.0


# ------------------------------------------------------------------------------------ I/O


def probe(path):
    out = subprocess.run([FFPROBE, "-v", "error", "-select_streams", "a:0", "-show_entries",
                          "stream=channels:format=duration", "-of", "json", path],
                         capture_output=True, text=True, check=True).stdout
    j = json.loads(out)
    ch = int(j["streams"][0]["channels"])
    return min(ch, 2), float(j["format"].get("duration", 0) or 0)


def analyse(path, ch):
    """Stream-decode the whole file and return per-hop K-weighted energies."""
    cmd = [FFMPEG, "-v", "error", "-i", path, "-vn", "-ac", str(ch), "-ar", str(SR), "-f", "f32le", "-"]
    p = subprocess.Popen(cmd, stdout=subprocess.PIPE)
    kf = KFilter(ch)
    chunk = SR * 30 * ch * 4
    rest = np.zeros(0)
    parts = []
    while True:
        buf = p.stdout.read(chunk)
        if not buf:
            break
        usable = len(buf) - len(buf) % (4 * ch)
        x = np.frombuffer(buf[:usable], dtype="<f4").reshape(-1, ch).astype(np.float64)
        z = np.concatenate([rest, kf(x)])
        n = len(z) // HOPN
        parts.append(z[: n * HOPN].reshape(n, HOPN).sum(axis=1))
        rest = z[n * HOPN:]
    p.wait()
    return np.concatenate(parts) if parts else np.zeros(0)


def decode(path, ch, start=None, dur=None):
    cmd = [FFMPEG, "-v", "error"]
    if start is not None:
        cmd += ["-ss", "%.3f" % max(0.0, start)]
    cmd += ["-i", path, "-vn"]
    if dur is not None:
        cmd += ["-t", "%.3f" % dur]
    cmd += ["-ac", str(ch), "-ar", str(SR), "-f", "f32le", "-"]
    buf = subprocess.run(cmd, capture_output=True, check=True).stdout
    buf = buf[: len(buf) - len(buf) % (4 * ch)]
    return np.frombuffer(buf, dtype="<f4").reshape(-1, ch).astype(np.float64)


def encode(x, out, fmt="mp3"):
    os.makedirs(os.path.dirname(out), exist_ok=True)
    ch = x.shape[1]
    cmd = [FFMPEG, "-v", "error", "-y", "-f", "f32le", "-ar", str(SR), "-ac", str(ch), "-i", "-"]
    if fmt == "mp3":
        cmd += ["-c:a", "libmp3lame", "-q:a", "5", "-ar", str(SR), out]
    else:
        cmd += ["-c:a", "flac", "-sample_fmt", "s32", "-ar", str(SR), out]
    subprocess.run(cmd, input=x.astype("<f4").tobytes(), check=True)


def fades(x, fin=FADE, fout=FADE):
    x = x.copy()
    n = len(x)
    a = min(int(fin * SR), n // 2)
    b = min(int(fout * SR), n // 2)
    if a > 0:
        x[:a] *= np.linspace(0, 1, a)[:, None]
    if b > 0:
        x[n - b:] *= np.linspace(1, 0, b)[:, None]
    return x


# --------------------------------------------------------------------------------- hints


def parse_hints(s):
    h = {"use": [], "avoid": [], "max": None}
    for item in (s or "").split(";"):
        item = item.strip()
        if not item:
            continue
        m = re.match(r"^(use|avoid)\s+([\d.]+)\s*-\s*([\d.]+)$", item)
        if m:
            h[m.group(1)].append((float(m.group(2)), float(m.group(3))))
            continue
        m = re.match(r"^max\s+(\d+)$", item)
        if m:
            h["max"] = int(m.group(1))
            continue
        raise ValueError("bad take hint: %r" % item)
    return h


def allowed_mask(n, hints, edge=0.0):
    """Boolean mask over hops: True where the hints allow taking audio."""
    t = np.arange(n) * HOP
    if hints["use"]:
        ok = np.zeros(n, bool)
        for a, b in hints["use"]:
            ok |= (t >= a) & (t < b)
    else:
        ok = np.ones(n, bool)
    for a, b in hints["avoid"]:
        ok &= ~((t >= a) & (t < b))
    if edge > 0 and n * HOP > 4 * edge + 15:
        ok &= (t >= edge) & (t < n * HOP - edge)
    return ok


# ---------------------------------------------------------------------------------- beds


def pick_bed_windows(e, hints, length, dur):
    """Return [(start, end)] of the calmest stretches (seconds)."""
    n = len(e)
    if n == 0:
        return []
    lm = window_loudness(e, 0.4)
    ok = allowed_mask(n, hints, edge=1.0)
    if not ok.any():
        ok = np.ones(n, bool)
    med = float(np.median(lm[ok]))
    bad = (lm > med + 10) | (lm < med - 20) | ~ok
    avail = ok.sum() * HOP
    L = min(length, avail)
    if L < 15:
        # short recording: use what is allowed (all of it if the whole file is short)
        idx = np.flatnonzero(ok)
        return [(idx[0] * HOP, min(dur, (idx[-1] + 1) * HOP))]
    w = int(round(L / HOP))
    cb = np.concatenate([[0], np.cumsum(bad)])
    cx = np.concatenate([[0.0], np.cumsum(lm)])
    cx2 = np.concatenate([[0.0], np.cumsum(lm * lm)])
    cok = np.concatenate([[0], np.cumsum(~ok)])
    starts = np.arange(0, n - w + 1, 10)
    if not len(starts):
        starts = np.array([0])
        w = n
    nbad = cb[starts + w] - cb[starts]
    forb = cok[starts + w] - cok[starts]
    mean = (cx[starts + w] - cx[starts]) / w
    var = np.maximum((cx2[starts + w] - cx2[starts]) / w - mean ** 2, 0)
    cost = forb * 1e6 + nbad * 1000.0 + np.sqrt(var)
    takes = []
    want = hints["max"] or (2 if avail > 240 else 1)
    for _ in range(want):
        i = int(np.argmin(cost))
        if not np.isfinite(cost[i]) or (takes and forb[i] > 0):
            break
        s = starts[i]
        takes.append((s * HOP, min(dur, (s + w) * HOP)))
        # block overlapping candidates (with a 5 s gap)
        g = int(5 / HOP)
        cost[(starts + w > s - g) & (starts < s + w + g)] = np.inf
    return sorted(takes)


# -------------------------------------------------------------------------------- events


def find_events(e, rule, hints, dur):
    """Return [(start, end, score, how)] in seconds."""
    n = len(e)
    if n == 0:
        return []
    lm = window_loudness(e, 0.4)
    ok = allowed_mask(n, hints)
    if not ok.any():
        ok = np.ones(n, bool)
    lo, hi, mode = rule["lo"], rule["hi"], rule["mode"]
    maxcuts = hints["max"] or MAX_CUTS
    win = max(3, int(2 * rule["bedwin"] / HOP) | 1)
    win = min(win, (n // 2) * 2 + 1)
    bed = percentile_filter(lm, 30, size=win, mode="nearest")
    floor = float(np.percentile(lm[ok], 10))
    t_end = min(dur, n * HOP)

    if mode == "whole":
        # one cut per `use` range (or for the whole file): from the first to the last loud hop
        ranges = hints["use"] or [(0.0, t_end)]
        out = []
        for ra, rb in ranges:
            sub = dict(hints, use=[(ra, rb)])
            okr = allowed_mask(n, sub)
            if not okr.any():
                continue
            fl = float(np.percentile(lm[okr], 10))
            spread = float(np.percentile(lm[okr], 95)) - fl
            if spread < 10:
                # the ringing fills the whole range (recording starts and ends inside the peal)
                act = np.flatnonzero(okr)
            else:
                act = np.flatnonzero(okr & (lm > fl + max(6.0, 0.4 * spread)))
                if len(act) == 0:
                    act = np.flatnonzero(okr)
            s = max(0.0, act[0] * HOP - PREROLL)
            # natural end: after the last loud hop, until the level falls back near the floor
            j = act[-1]
            while j < n - 1 and okr[j + 1] and lm[j] > fl + 3 and (j - act[-1]) * HOP < 8:
                j += 1
            en = min(t_end, (j + 1) * HOP + FADE)
            if en - s > hi:
                en = s + hi
            out.append((s, en, float(lm[act].max() - fl), "whole"))
        return out[:maxcuts]

    if mode == "stretch":
        return best_stretches(lm, ok, min(hi, t_end), min(3, maxcuts), t_end)

    if mode == "strike":
        ls = window_loudness(e, 0.1)
        k = int(0.6 / HOP)
        prevmin = np.array([ls[max(0, i - k): i].min() if i > 0 else ls[0] for i in range(n)])
        rise = ls - prevmin
        cand = np.flatnonzero(ok & (rise > 6) & (ls > floor + 8))
        onsets = []
        for i in cand:
            if not onsets or (i - onsets[-1]) * HOP >= 1.0:
                onsets.append(i)
        evs = []
        for j, i in enumerate(onsets):
            s = max(0.0, i * HOP - PREROLL - 0.05)
            nxt = onsets[j + 1] * HOP - PREROLL - 0.05 if j + 1 < len(onsets) else t_end
            # natural decay back to the bed
            q = i
            while q < n - 1 and lm[q] > bed[q] + 3:
                q += 1
            en = min(nxt, s + hi, max(q * HOP + 1.0, s + lo), t_end)
            if en - s < lo:
                continue
            if not ok[int(s / HOP): max(int(s / HOP) + 1, int(en / HOP))].all():
                continue
            evs.append((s, en, float(rise[i]), "strike"))
        if evs:
            evs = sorted(evs, key=lambda v: -v[2])[:maxcuts]
            return sorted(evs)
        mode = "onset"

    # onset detection: level rising > 8 dB over the local bed
    above = lm > bed + 8
    hold = int(rule["hold"] / HOP)
    evs = []
    i = 0
    while i < n:
        if not (above[i] and ok[i]):
            i += 1
            continue
        on = i
        j = i
        quiet = 0
        while j < n - 1:
            j += 1
            if lm[j] < bed[on] + 3:
                quiet += 1
                if quiet >= hold:
                    j -= quiet - 1
                    break
            else:
                quiet = 0
        s = max(0.0, on * HOP - PREROLL)
        en = min(t_end, j * HOP + FADE)
        if en - s > hi:
            en = s + hi
        if en - s < lo:
            en = min(t_end, s + lo)
        a, b = int(s / HOP), max(int(s / HOP) + 1, int(en / HOP))
        if ok[a:b].all() and en - s >= min(lo, t_end):
            evs.append((s, en, float((lm[on:max(on + 1, j)] - bed[on]).max()), "onset"))
        i = max(j, i + 1)
    # strongest first, no overlaps
    chosen = []
    for v in sorted(evs, key=lambda v: -v[2]):
        if all(v[1] <= c[0] or v[0] >= c[1] for c in chosen):
            chosen.append(v)
        if len(chosen) >= maxcuts:
            break
    if chosen:
        return sorted(chosen)
    # no clean onset: the best (loudest) whole stretch
    return best_stretches(lm, ok, min(hi, t_end), 1, t_end)


def best_stretches(lm, ok, L, count, t_end):
    """Up to `count` non-overlapping windows of L seconds with the most energy."""
    n = len(lm)
    w = max(1, int(L / HOP))
    if w >= n:
        return [(0.0, t_end, 0.0, "stretch")]
    c = np.concatenate([[0.0], np.cumsum(np.where(ok, 10 ** (lm / 10), 0))])
    cok = np.concatenate([[0], np.cumsum(~ok)])
    st = np.arange(0, n - w + 1)
    score = (c[st + w] - c[st]) - (cok[st + w] - cok[st]) * 1e12
    out = []
    for _ in range(count):
        i = int(np.argmax(score))
        if not np.isfinite(score[i]) or (out and cok[st[i] + w] - cok[st[i]] > 0):
            break
        s = int(st[i])
        out.append((s * HOP, min(t_end, (s + w) * HOP), 0.0, "stretch"))
        score[(st + w > s) & (st < s + w)] = -np.inf
    return sorted(out)


# --------------------------------------------------------------------------------- jobs


def source_id(rel):
    top, name = rel.split("/", 1)[0], os.path.basename(rel)
    stem = os.path.splitext(name)[0]
    if top == "commons":
        slug = urllib.parse.unquote(stem)
        slug = re.sub(r"[^A-Za-z0-9]+", "-", slug.encode("ascii", "ignore").decode()).strip("-").lower()
        return "commons-" + slug, "commons:" + name
    if top == "aporee":
        num = stem.split("_", 1)[0]
        return "aporee-" + num, "aporee:" + num
    num = stem.split("_", 1)[0]
    return num, "freesound:" + num


LIMIT_DB = 6.0   # at most this much peak limiting on a bed take; beyond it the take stays quieter


def limit(x, ceiling_db):
    """Look-ahead peak limiter: brief gain dips only where single peaks exceed the ceiling."""
    from scipy.ndimage import minimum_filter1d, uniform_filter1d
    c = 10 ** (ceiling_db / 20)
    a = np.abs(x).max(axis=1)
    need = np.minimum(1.0, c / np.maximum(a, 1e-12))
    if need.min() >= 1.0:
        return x
    g = minimum_filter1d(need, size=int(0.02 * SR) | 1, mode="nearest")
    g = uniform_filter1d(g, size=int(0.01 * SR) | 1, mode="nearest")
    return x * np.minimum(g, need)[:, None]


def finish_bed(x):
    x = fades(x)
    lufs = integrated(x)
    gain = BED_LUFS - lufs if lufs > -100 else 0.0
    pk = peak_db(x)
    ceiling = BED_PEAK_DB - 0.5
    if pk + gain > ceiling + LIMIT_DB:
        gain = ceiling + LIMIT_DB - pk
    return limit(x * 10 ** (gain / 20), ceiling)


def finish_event(x, forced_start):
    x = fades(x, fin=FADE if forced_start else 0.02, fout=FADE)
    pk = peak_db(x)
    return x * 10 ** ((EVENT_PEAK_DB - pk) / 20) if pk > -120 else x


def process_file(job):
    """One raw file -> all its takes over every slot it is assigned to."""
    raw, rel, rows, slots, out, bed_len = job
    path = os.path.join(raw, rel)
    log = []
    takes = []
    try:
        ch, dur = probe(path)
        need_an = any(slot_class(r["slot"], slots[r["slot"]]) != "ir" for r in rows)
        e = analyse(path, ch) if need_an else None
        if e is not None:
            dur = min(dur, len(e) * HOP) if dur > 0 else len(e) * HOP
        sid, src = source_id(rel)
        for r in rows:
            slot = r["slot"]
            cls = slot_class(slot, slots[slot])
            hints = parse_hints(r.get("take", ""))
            sdir = os.path.join(out, *slot.split("."))
            if cls == "ir":
                y = decode(path, ch)
                pk = np.abs(y).max(axis=1)
                top = pk.max()
                first = int(np.argmax(pk > top * 0.01))
                first = max(0, first - int(0.001 * SR))
                quiet = np.flatnonzero(pk > top * 10 ** (-70 / 20))
                last = int(quiet[-1]) + 1 if len(quiet) else len(y)
                y = y[first:last]
                y = fades(y, fin=0.0, fout=0.01)
                y *= 10 ** ((IR_PEAK_DB - peak_db(y)) / 20)
                fn = os.path.join(sdir, "%s_1.flac" % sid)
                encode(y, fn, "flac")
                takes.append(dict(slot=slot, file=os.path.relpath(fn, out), kind="ir",
                                  seconds=round(len(y) / SR, 3), channels=ch,
                                  peak_db=round(peak_db(decode(fn, ch)), 2), source=src,
                                  cut=[round(first / SR, 3), round(last / SR, 3)], rel=rel))
                continue
            if cls == "bed":
                wins = pick_bed_windows(e, hints, bed_len, dur)
                for k, (a, b) in enumerate(wins, 1):
                    y = decode(path, ch, a, b - a)
                    if len(y) < SR:
                        log.append("%s %s: take %d too short (%.1f s)" % (rel, slot, k, len(y) / SR))
                        continue
                    y = finish_bed(y)
                    fn = os.path.join(sdir, "%s_%d.mp3" % (sid, k))
                    z, pk = encode_checked(y, fn, ch, BED_PEAK_DB)
                    takes.append(dict(slot=slot, file=os.path.relpath(fn, out), kind=slots[slot]["kind"],
                                      seconds=round(len(z) / SR, 2), channels=ch,
                                      lufs=round(integrated(z), 1), peak_db=round(pk, 1), source=src,
                                      cut=[round(a, 2), round(b, 2)], rel=rel))
                continue
            rule = event_rule(slot)
            evs = find_events(e, rule, hints, dur)
            for k, (a, b, score, how) in enumerate(evs, 1):
                y = decode(path, ch, a, b - a)
                if len(y) < int(0.2 * SR):
                    continue
                y = finish_event(y, forced_start=(how in ("stretch", "whole") or a <= 0.0))
                fn = os.path.join(sdir, "%s_%d.mp3" % (sid, k))
                z, pk = encode_checked(y, fn, ch, -1.0)
                takes.append(dict(slot=slot, file=os.path.relpath(fn, out), kind=slots[slot]["kind"],
                                  seconds=round(len(z) / SR, 2), channels=ch,
                                  lufs=round(integrated(z), 1), st_max_lufs=round(max_short_term(z), 1),
                                  peak_db=round(pk, 1), source=src,
                                  cut=[round(a, 2), round(b, 2)], cut_how=how, rel=rel))
    except Exception as ex:  # keep going, report at the end
        log.append("%s: FAILED %r" % (rel, ex))
    return takes, log


# ------------------------------------------------------------------------------ metadata


def lic_name(url):
    u = url.lower()
    if "publicdomain/zero" in u:
        return "CC0 1.0", "CC0"
    if "publicdomain/mark" in u:
        return "Public Domain Mark 1.0", "Public domain"
    m = re.search(r"licenses/(by(?:-sa)?)/([\d.]+)", u)
    if m:
        kind = "CC BY-SA" if m.group(1) == "by-sa" else "CC BY"
        return "%s %s" % (kind, m.group(2)), kind
    return url, "Other"


def load_meta(raw, assign_path):
    meta = {}
    for r in csv.DictReader(open(os.path.join(raw, "manifest.csv"), encoding="utf-8")):
        if not r["file"]:
            continue
        lic = r["license"].replace("http://", "https://")
        meta[r["file"]] = dict(title=r["title"], author=urllib.parse.unquote(r["author"]),
                               license=lic, page="https://freesound.org/s/%s/" % r["id"])
    # exact titles as shown on each Freesound page (the manifest holds research labels for some)
    titles = os.path.join(os.path.dirname(os.path.abspath(assign_path)), "titles.csv")
    if os.path.exists(titles):
        for r in csv.DictReader(open(titles, encoding="utf-8")):
            if r["file"] in meta and r["title"].strip():
                meta[r["file"]]["title"] = r["title"].strip()
    extra = os.path.join(os.path.dirname(os.path.abspath(assign_path)), "sources_extra.csv")
    if os.path.exists(extra):
        for r in csv.DictReader(open(extra, encoding="utf-8")):
            meta[r["file"]] = dict(title=r["title"], author=r["author"], license=r["license"],
                                   page=r["page"])
    return meta


def write_credits(path, sources):
    groups = [("CC0", "CC0 1.0 (public domain dedication)"), ("CC BY", "CC BY (attribution)"),
              ("CC BY-SA", "CC BY-SA (attribution, share-alike)"), ("Public domain", "Public domain")]
    lines = ["# Ambience sound credits", "",
             "The openBus ambience pack is cut from the field recordings below. Every file was "
             "trimmed, level-normalised and re-encoded (32 kHz MP3, impulse responses 32 kHz FLAC); "
             "nothing else was changed. The licence of each recording applies to the takes cut from it "
             "(listed in pack.json under `source`).", ""]
    for key, head in groups + [("Other", "Other")]:
        items = [s for s in sources if s["group"] == key]
        if not items:
            continue
        lines += ["## %s" % head, ""]
        if key == "CC BY-SA":
            lines += ["The takes cut from these recordings are adaptations and are shared under the same "
                      "CC BY-SA licence.", ""]
        for s in sorted(items, key=lambda s: (s["author"].lower(), s["title"].lower())):
            lines.append('- "%s" by %s - %s - %s (%s). Trimmed, level-normalised and re-encoded.'
                         % (s["title"], s["author"], s["page"], s["lic_name"], s["license"]))
        lines.append("")
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(lines))


# ---------------------------------------------------------------------------------- main


def main(argv):
    args, opts = [], {}
    i = 0
    while i < len(argv):
        if argv[i].startswith("--"):
            opts[argv[i][2:]] = argv[i + 1]
            i += 2
        else:
            args.append(argv[i])
            i += 1
    if len(args) != 4:
        print(__doc__)
        return 2
    raw, assign, slots_path, out = [os.path.abspath(a) for a in args]
    # 60 s takes made the pack ~356 MB; 45 s keeps it under the 300 MB budget
    bed_len = float(opts.get("bed-seconds", 45))
    jobs = int(opts.get("jobs", max(1, (os.cpu_count() or 2) - 1)))
    only = opts.get("only")
    slots = load_slots(slots_path)
    rows = list(csv.DictReader(open(assign, encoding="utf-8")))
    for r in rows:
        if r["slot"] not in slots:
            raise SystemExit("unknown slot %r for %s" % (r["slot"], r["file"]))
        if not os.path.exists(os.path.join(raw, r["file"])):
            raise SystemExit("missing raw file %s" % r["file"])
        parse_hints(r.get("take", ""))
    meta = load_meta(raw, assign)
    by_file = {}
    for r in rows:
        by_file.setdefault(r["file"], []).append(r)
    if only:
        by_file = {k: v for k, v in by_file.items() if re.search(only, k)}
    missing = [f for f in by_file if f not in meta]
    if missing:
        raise SystemExit("no source metadata for: %s" % ", ".join(missing))

    if os.path.exists(os.path.join(out, "pack.json")) and not only:
        shutil.rmtree(out)
    os.makedirs(out, exist_ok=True)
    work = [(raw, f, rs, slots, out, bed_len) for f, rs in by_file.items()]
    # longest files first so the pool stays busy
    work.sort(key=lambda w: -os.path.getsize(os.path.join(raw, w[1])))
    all_takes, logs = [], []
    with ProcessPoolExecutor(max_workers=jobs) as ex:
        for i, (t, l) in enumerate(ex.map(process_file, work), 1):
            all_takes += t
            logs += l
            if i % 25 == 0:
                print("  %d/%d files" % (i, len(work)), flush=True)
    for t in all_takes:
        m = meta[t.pop("rel")]
        t.update(title=m["title"], author=m["author"], license=m["license"], page=m["page"])
    all_takes.sort(key=lambda t: (t["slot"], t["file"]))
    order = ["slot", "file", "kind", "seconds", "channels", "lufs", "st_max_lufs", "peak_db",
             "source", "title", "author", "license", "page", "cut", "cut_how"]
    all_takes = [{k: t[k] for k in order if k in t} for t in all_takes]
    pack = dict(version=1, sample_rate=SR, takes=all_takes)
    with open(os.path.join(out, "pack.json"), "w", encoding="utf-8") as f:
        json.dump(pack, f, ensure_ascii=False, indent=1)
    # the takes of one country (see regions.csv)
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    import tag_regions
    tag_regions.tag(os.path.join(os.path.dirname(os.path.abspath(__file__)), "regions.csv"), out)
    # credits: every source once
    seen = {}
    for f in by_file:
        m = meta[f]
        name, group = lic_name(m["license"])
        seen[m["page"]] = dict(m, lic_name=name, group=group)
    write_credits(os.path.join(out, "CREDITS.md"), list(seen.values()))
    shutil.copy(os.path.join(out, "CREDITS.md"), os.path.join(os.path.dirname(assign), "CREDITS.md"))
    for l in logs:
        print("WARN", l)
    print("%d takes from %d files -> %s" % (len(all_takes), len(by_file), out))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
