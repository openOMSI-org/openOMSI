#!/usr/bin/env python3
"""Build openOMSI's passenger avatar pack from the Microsoft Rocketbox avatar library (MIT).

    python3 -I tools/avatars/build_pack.py <tree.json> <out dir> [work dir]

<tree.json> is the GitHub tree listing of microsoft/Microsoft-Rocketbox at master
(https://api.github.com/repos/microsoft/Microsoft-Rocketbox/git/trees/master?recursive=1).
Every avatar of Assets/Avatars that suits a city's street and bus is taken (see EXCLUDE),
one at a time: its `_facial.fbx` (the mesh with the facial blend shapes) as it is, its colour
textures as JPEG (the hair and eyelash texture as PNG at half size, it has alpha), its normal maps at
half size as `<colour texture>_normal.jpg`, which openOMSI's PBR lookup finds beside the
colour texture. The 2048x2048 TGAs (about 22 GB for the library) are fetched, converted and
deleted one avatar at a time. The motion clips of CLIPS (walking, standing, sitting) are taken
unchanged from Assets/Animations. Writes pack.json, LICENSE.md and CREDITS.md.
"""
import json, os, shutil, subprocess, sys, urllib.request
from concurrent.futures import ThreadPoolExecutor
from PIL import Image

RAW = "https://raw.githubusercontent.com/microsoft/Microsoft-Rocketbox/master/"

# Not seen on a city's buses: firefighters in turnout gear, soldiers in combat dress, riot
# police, surgeons in masks and gowns, people in swimwear.
EXCLUDE = {
    "Fire_Female_01", "Fire_Female_02", "Fire_Female_03",
    "Fire_Male_01", "Fire_Male_02", "Fire_Male_03", "Fire_Male_04", "Fire_Male_05", "Fire_Male_06", "Fire_Male_07",
    "Military_Female_01", "Military_Female_02",
    "Military_Male_01", "Military_Male_02", "Military_Male_03", "Military_Male_04", "Military_Male_05", "Military_Male_06",
    "Police_Female_01", "Police_Male_02",
    "Medical_Male_04", "Medical_Male_05",
    "Sports_Female_01", "Sports_Male_01",
}

CLIPS = {
    "walk": ["walk_neutral_01", "walk_neutral_02", "walk_neutral_03", "walk_slow_01", "walk_slow_02",
             "walk_stroll_01", "walk_stroll_02", "walk_fast_01", "walk_cool_01"],
    "stand": ["idle_neutral_01", "idle_neutral_02", "idle_neutral_03", "idle_look_around_01", "idle_waiting_01"],
    "sit": ["sit_chair_idle_neutral_01", "sit_chair_idle_neutral_02", "sit_chair_idle_waiting_01"],
}

COLOR_SIZE = 2048
# hair, eyelashes and the like (a PNG for its alpha: four times the bytes of a JPEG)
ALPHA_SIZE = 1024
NORMAL_SIZE = 1024


def fetch(path, dest):
    url = RAW + urllib.request.quote(path)
    for attempt in range(4):
        try:
            with urllib.request.urlopen(url, timeout=120) as r, open(dest, "wb") as f:
                shutil.copyfileobj(r, f, 1 << 20)
            return
        except Exception as e:  # noqa: BLE001 - retried
            if attempt == 3:
                raise RuntimeError(f"{url}: {e}")


def convert(src, out_dir, name):
    """One texture: colour -> jpg (or png with alpha), normal -> <colour>_normal.jpg."""
    stem = os.path.splitext(name)[0]
    low = stem.lower()
    im = Image.open(src)
    if "_normal" in low:
        if "wrinkle" in low or " - wr" in low:
            return None
        # f001_body_normal -> f001_body_color_normal (beside f001_body_color)
        colour = stem.replace("_normal", "_color", 1)
        im = im.convert("RGB").resize((NORMAL_SIZE, NORMAL_SIZE), Image.LANCZOS)
        out = os.path.join(out_dir, colour + "_normal.jpg")
        im.save(out, quality=92)
        return out
    if "_specular" in low:
        return None
    if im.mode in ("RGBA", "LA") and im.getextrema()[-1][0] < 255:
        im = im.convert("RGBA")
        if im.size[0] > ALPHA_SIZE:
            im = im.resize((ALPHA_SIZE, ALPHA_SIZE), Image.LANCZOS)
        out = os.path.join(out_dir, stem + ".png")
        im.save(out, optimize=True)
        return out
    im = im.convert("RGB")
    if im.size[0] > COLOR_SIZE:
        im = im.resize((COLOR_SIZE, COLOR_SIZE), Image.LANCZOS)
    out = os.path.join(out_dir, stem + ".jpg")
    im.save(out, quality=90, optimize=True)
    return out


def main():
    tree = json.load(open(sys.argv[1]))["tree"]
    out = sys.argv[2]
    work = sys.argv[3] if len(sys.argv) > 3 else os.path.join(out, "..", "avatars-work")
    os.makedirs(out, exist_ok=True)
    os.makedirs(work, exist_ok=True)
    avatars = {}
    for t in tree:
        p = t["path"]
        if t["type"] != "blob" or not p.startswith("Assets/Avatars/"):
            continue
        parts = p.split("/")
        if len(parts) < 5:
            continue
        avatars.setdefault((parts[2], parts[3]), []).append(p)
    pack = {"version": 2, "source": "https://github.com/microsoft/Microsoft-Rocketbox", "license": "MIT", "avatars": []}
    old = {}
    if os.path.exists(os.path.join(out, "pack.json")):
        old = {a["id"]: a for a in json.load(open(os.path.join(out, "pack.json")))["avatars"]}
    for (group, name), files in sorted(avatars.items()):
        fbx = [f for f in files if f.endswith("_facial.fbx")]
        # (Female_Party_01/02 are listed under Professions too, with the same model)
        if name in EXCLUDE or not fbx or any(a["id"] == name for a in pack["avatars"]):
            continue
        dest = os.path.join(out, name)
        if name in old and os.path.isdir(dest):
            pack["avatars"].append(old[name])
            continue
        os.makedirs(dest, exist_ok=True)
        tmp = os.path.join(work, name)
        os.makedirs(tmp, exist_ok=True)
        fetch(fbx[0], os.path.join(dest, name + ".fbx"))
        textures = [f for f in files if "/Textures/" in f and f.lower().endswith(".tga")]

        def one(f):
            local = os.path.join(tmp, os.path.basename(f))
            fetch(f, local)
            try:
                return convert(local, dest, os.path.basename(f))
            finally:
                os.remove(local)

        with ThreadPoolExecutor(4) as ex:
            made = [m for m in ex.map(one, textures) if m]
        shutil.rmtree(tmp, ignore_errors=True)
        kind = name.split("_")[0]
        entry = {
            "id": name,
            "group": group,
            "file": f"{name}/{name}.fbx",
            "female": "_Female_" in f"_{name}_" or name.startswith("Female"),
            "child": group == "Children",
            "kind": kind,
            "bytes": sum(os.path.getsize(os.path.join(dest, x)) for x in os.listdir(dest)),
        }
        pack["avatars"].append(entry)
        print(f"{name}: {len(made)} textures, {entry['bytes'] >> 20} MB", flush=True)
        json.dump(pack, open(os.path.join(out, "pack.json"), "w"), indent=1)
    clip_files = {p.split("/")[-1][:-len(".max.fbx")]: p for p in (t["path"] for t in tree)
                  if p.startswith("Assets/Animations/all_animations_max_motextr_") and p.endswith(".max.fbx")
                  and "motextr_xyz/" not in p}
    pack["clips"] = []
    os.makedirs(os.path.join(out, "clips"), exist_ok=True)
    for kind, names in CLIPS.items():
        for sex in ("f", "m"):
            for name in names:
                cid = f"{sex}_{name}"
                if cid not in clip_files:
                    continue
                file = f"clips/{cid}.fbx"
                if not os.path.exists(os.path.join(out, file)):
                    fetch(clip_files[cid], os.path.join(out, file))
                pack["clips"].append({"id": cid, "file": file, "kind": kind, "female": sex == "f"})
    print(f"{len(pack['clips'])} clips, {sum(os.path.getsize(os.path.join(out, c['file'])) for c in pack['clips']) >> 20} MB")
    json.dump(pack, open(os.path.join(out, "pack.json"), "w"), indent=1)
    lic = urllib.request.urlopen(RAW + "LICENSE.md", timeout=60).read()
    open(os.path.join(out, "LICENSE.md"), "wb").write(lic)
    with open(os.path.join(out, "CREDITS.md"), "w") as f:
        f.write("# Passenger avatars\n\nThe Microsoft Rocketbox Avatar Library, (c) Microsoft Corporation,\n"
                "MIT License (LICENSE.md), https://github.com/microsoft/Microsoft-Rocketbox.\n"
                "Gonzalez-Franco et al., \"The Rocketbox library and the utility of freely available rigged avatars\",\n"
                "Frontiers in Virtual Reality, 2020, doi:10.3389/frvir.2020.561558.\n\n"
                "Taken unchanged: the `_facial.fbx` meshes with their blend shapes (ARKit blend shapes by Fang Ma and\n"
                "Matias Volonte). The textures are converted from TGA (colour as JPEG / PNG, normal maps at half size).\n\n"
                f"{len(pack['avatars'])} avatars: " + ", ".join(a["id"] for a in pack["avatars"]) + ".\n\n"
                f"{len(pack['clips'])} motion clips from Assets/Animations, taken unchanged: " + ", ".join(c["id"] for c in pack["clips"]) + ".\n")
    print(f"{len(pack['avatars'])} avatars, {sum(a['bytes'] for a in pack['avatars']) >> 20} MB")


if __name__ == "__main__":
    main()
