# SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
# SPDX-License-Identifier: Apache-2.0
"""Check a release tag and select library versions not yet on crates.io."""

import json
import os
from pathlib import Path
import tomllib
from urllib.error import HTTPError
from urllib.request import Request, urlopen


CRATES = ("igs-iec104-codec", "igs-iec104-link", "igs-iec104")


def published(name, version):
    request = Request(
        f"https://crates.io/api/v1/crates/{name}/{version}",
        headers={"User-Agent": "igs-iec104-release-workflow"},
    )
    try:
        with urlopen(request, timeout=30) as response:
            entry = json.load(response)["version"]
    except HTTPError as error:
        if error.code == 404:
            return False
        raise
    if entry["num"] != version or entry["yanked"]:
        raise ValueError(f"Unexpected or yanked registry version for {name}")
    return True


def release_plan(root, tag, exists=published):
    manifests = {}
    for name in CRATES:
        with (root / "crates" / name / "Cargo.toml").open("rb") as source:
            manifest = tomllib.load(source)
        package = manifest["package"]
        if package["name"] != name or tag != f"v{package['version']}":
            raise ValueError(f"{name}: tag {tag!r} must match its package version")
        manifests[name] = manifest
    version = manifests[CRATES[0]]["package"]["version"]
    for name, manifest in manifests.items():
        for dependency, spec in manifest.get("dependencies", {}).items():
            if dependency in CRATES and spec.get("version") != version:
                raise ValueError(f"{name}: update the {dependency} version to {version}")
    return [name for name in CRATES if not exists(name, version)]


if __name__ == "__main__":
    if os.environ.get("GITHUB_REF_TYPE") != "tag":
        raise SystemExit("Publication requires a version tag")
    pending = release_plan(Path(__file__).resolve().parents[2], os.environ["GITHUB_REF_NAME"])
    with Path(os.environ["GITHUB_OUTPUT"]).open("a") as output:
        output.write(f"packages={' '.join(pending)}\n")
        output.write(f"publish={str(bool(pending)).lower()}\n")
    print("Pending packages:", ", ".join(pending) or "none (already published)")
