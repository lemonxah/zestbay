#!/usr/bin/env python3
"""Write pkg/flatpak/cargo-sources.json from Cargo.lock for the offline
Flatpak build: every crates.io package as a .crate archive vendored under
cargo/vendor, plus the Cargo config that points crates.io at that directory.

Self-contained (standard library only, no network): Cargo.lock already has
each crate's checksum. Only crates.io dependencies are supported; ZestBay
has no git dependencies.

    python3 pkg/flatpak/cargo-sources.py [Cargo.lock] [output.json]

For a Flathub submission you may prefer the official generator from
flatpak/flatpak-builder-tools (cargo/flatpak-cargo-generator.py); its output
has the same layout.
"""

import json
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
VENDOR = "cargo/vendor"

CARGO_CONFIG = f"""[source.vendored-sources]
directory = "{VENDOR}"

[source.crates-io]
replace-with = "vendored-sources"
"""


def main() -> None:
    lock_path = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "Cargo.lock"
    out_path = Path(sys.argv[2]) if len(sys.argv) > 2 else ROOT / "pkg/flatpak/cargo-sources.json"
    lock = tomllib.loads(lock_path.read_text())

    sources = []
    unsupported = []
    for pkg in lock.get("package", []):
        source = pkg.get("source")
        if source is None:
            continue  # workspace member, built from the app's own source
        if source != CRATES_IO or "checksum" not in pkg:
            unsupported.append(f"{pkg['name']} {pkg['version']} ({source})")
            continue
        name, version, checksum = pkg["name"], pkg["version"], pkg["checksum"]
        dest = f"{VENDOR}/{name}-{version}"
        sources.append({
            "type": "archive",
            "archive-type": "tar-gzip",
            "url": f"https://static.crates.io/crates/{name}/{name}-{version}.crate",
            "sha256": checksum,
            "dest": dest,
        })
        # Cargo verifies vendored crates against this file.
        sources.append({
            "type": "inline",
            "contents": json.dumps({"package": checksum, "files": {}}),
            "dest": dest,
            "dest-filename": ".cargo-checksum.json",
        })

    if unsupported:
        raise SystemExit("unsupported (non crates.io) dependencies:\n  " + "\n  ".join(unsupported))

    sources.append({
        "type": "inline",
        "contents": CARGO_CONFIG,
        "dest": "cargo",
        "dest-filename": "config.toml",
    })
    out_path.write_text(json.dumps(sources, indent=4) + "\n")
    print(f"{out_path}: {len(sources) // 2} crates")


if __name__ == "__main__":
    main()
