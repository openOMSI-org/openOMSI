#!/usr/bin/env python3
"""Inventory an official Streamline zip without extracting or loading any library.

File presence is evidence about packaging, never proof of a supported GPU, valid
signature, legal redistribution permission, successful initialization or inference.
"""

import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import zipfile


def inspect_archive(path):
    with zipfile.ZipFile(path) as archive:
        names = sorted(archive.namelist())
        files = [n for n in names if not n.endswith("/")]
        nr_plugins = [n for n in files if PurePosixPath(n).name.lower() == "sl.dlss_nr.dll"]
        nr_headers = [n for n in files if "dlss_nr" in PurePosixPath(n).name.lower() and n.lower().endswith(".h")]
        core_headers = [n for n in files if PurePosixPath(n).name == "sl_core_types.h"]
        feature_declared = any(b"kFeatureDLSS_NR" in archive.read(n) for n in core_headers)
    digest = hashlib.sha256()
    with open(path, "rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return {
        "archive": str(path),
        "sha256": digest.hexdigest(),
        "feature_declared": feature_declared,
        "nr_plugin_candidates": nr_plugins,
        "nr_header_candidates": nr_headers,
        "candidate_files_present": bool(nr_plugins and nr_headers),
        "runtime_validated": False,
        "note": "No DLL was loaded. An SDK declaration or filename does not prove functional DLSS 5.",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("--require-nr-files", action="store_true", help="Fail if NR header/plugin candidates are absent; does not certify availability")
    args = parser.parse_args()
    report = inspect_archive(args.archive)
    print(json.dumps(report, indent=2))
    return 2 if args.require_nr_files and not report["candidate_files_present"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
