# SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
# SPDX-License-Identifier: Apache-2.0
"""Exercise release selection and refusal of unsafe or inconsistent inputs."""

from io import BytesIO
from pathlib import Path
import shutil
import tempfile
import tomllib
import unittest
from unittest.mock import patch
from urllib.error import HTTPError

import plan_release as release


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        source = Path(__file__).resolve().parents[2]
        for name in release.CRATES:
            destination = self.root / "crates" / name
            destination.mkdir(parents=True)
            shutil.copyfile(source / "crates" / name / "Cargo.toml", destination / "Cargo.toml")
        with (self.root / "crates/igs-iec104/Cargo.toml").open("rb") as manifest:
            self.version = tomllib.load(manifest)["package"]["version"]
        self.tag = f"v{self.version}"

    def test_new_release_keeps_dependency_order(self):
        self.assertEqual(release.release_plan(self.root, self.tag, lambda *_: False), list(release.CRATES))

    def test_retry_skips_uploaded_dependencies(self):
        pending = release.release_plan(self.root, self.tag, lambda name, _: name == release.CRATES[0])
        self.assertEqual(pending, list(release.CRATES[1:]))

    def test_completed_release_is_a_noop(self):
        self.assertEqual(release.release_plan(self.root, self.tag, lambda *_: True), [])

    def test_wrong_tag_is_rejected_before_network_access(self):
        with self.assertRaises(ValueError), patch.object(release, "urlopen") as request:
            release.release_plan(self.root, "v9.9.9")
        request.assert_not_called()

    def test_stale_internal_dependency_is_rejected(self):
        manifest = self.root / "crates/igs-iec104/Cargo.toml"
        text = manifest.read_text().replace(
            f'version = "{self.version}" }}', 'version = "0.0.9" }'
        )
        manifest.write_text(text)
        with self.assertRaisesRegex(ValueError, "update the"):
            release.release_plan(self.root, self.tag)

    def test_missing_registry_version_is_pending(self):
        error = HTTPError("https://crates.io", 404, "missing", {}, None)
        with patch.object(release, "urlopen", side_effect=error):
            self.assertFalse(release.published("igs-iec104", "0.1.0"))

    def test_registry_failure_is_not_treated_as_missing(self):
        error = HTTPError("https://crates.io", 503, "unavailable", {}, None)
        with patch.object(release, "urlopen", side_effect=error), self.assertRaises(HTTPError):
            release.published("igs-iec104", "0.1.0")

    def test_yanked_version_requires_manual_intervention(self):
        response = BytesIO(b'{"version":{"num":"0.1.0","yanked":true}}')
        with patch.object(release, "urlopen", return_value=response), self.assertRaises(ValueError):
            release.published("igs-iec104", "0.1.0")
