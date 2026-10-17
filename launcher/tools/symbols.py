#!/usr/bin/env python3
"""Writes ui/js/symbols.js: the Material Symbols the page uses, by the launcher's names, from
the game's own copies in assets/icons/material (fetch a missing one from
https://fonts.gstatic.com/s/i/short-term/release/materialsymbolsrounded/<name>/fill1/48px.svg
into that folder first).

    python3 launcher/tools/symbols.py
"""
import json
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent
ICONS = HERE.parent.parent / "assets" / "icons" / "material"
OUT = HERE.parent / "ui" / "js" / "symbols.js"

NAMES = {
    "bus": "directions_bus", "map": "map", "users": "groups", "user": "person", "sliders": "tune",
    "keyboard": "keyboard", "puzzle": "extension", "play": "play_arrow", "gamepad": "sports_esports",
    "book": "school", "folder": "folder_open", "search": "search", "star": "star", "x": "close",
    "check": "check", "down": "expand_more", "right": "chevron_right", "left": "chevron_left",
    "refresh": "refresh", "trash": "delete", "plus": "add", "info": "info", "alert": "warning",
    "clock": "schedule", "calendar": "calendar_month", "cloud": "cloud", "external": "open_in_new",
    "download": "download", "stop": "stop", "copy": "content_copy", "route": "route", "home": "home",
    "globe": "public", "link": "link", "power": "power_settings_new", "file": "description",
    "person": "directions_walk", "sun": "wb_sunny", "traffic": "traffic",
    "ticket": "confirmation_number", "save": "save", "history": "history", "badge": "badge", "help": "help",
    "garage": "garage", "fuel": "local_gas_station", "wash": "local_car_wash", "repair": "build", "edit": "edit",
}

paths = {}
for key, name in NAMES.items():
    svg = (ICONS / f"{name}.svg").read_text()
    paths[key] = " ".join(re.findall(r'<path[^>]* d="([^"]+)"', svg))
OUT.write_text(
    "// Google's Material Symbols Rounded (filled, Apache-2.0): the same glyphs the game's own\n"
    "// interface draws (assets/icons/material), by the launcher's names. Made by\n"
    "// launcher/tools/symbols.py; viewBox 0 -960 960 960.\n"
    "export const SYMBOLS = " + json.dumps(paths, indent=1) + ";\n"
)
print(f"{len(paths)} symbols -> {OUT}")
