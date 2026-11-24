import shutil
import subprocess
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("bump-version.py").resolve()


class VersionBumpTest(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.git("init", "-q", "-b", "main")
        self.git("config", "user.name", "Version test")
        self.git("config", "user.email", "version@example.invalid")
        self.write_version("0.1.0")
        self.commit("Initial version")
        self.commit("History before adoption")
        (self.root / "scripts").mkdir()
        shutil.copyfile(SCRIPT, self.root / "scripts" / SCRIPT.name)
        self.commit("Enable version automation")

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.root, text=True).strip()

    def commit(self, message):
        self.git("add", ".")
        self.git("commit", "-qm", message, "--allow-empty")

    def write_version(self, version):
        (self.root / "Cargo.toml").write_text(f'[package]\nname = "clothcraft"\nversion = "{version}"\n')
        (self.root / "Cargo.lock").write_text(
            f'version = 4\n\n[[package]]\nname = "clothcraft"\nversion = "{version}"\n'
            '\n[[package]]\nname = "other"\nversion = "0.1.0"\n'
        )

    def bump(self):
        return subprocess.check_output([sys.executable, SCRIPT], cwd=self.root, text=True).strip()

    def assert_versions(self, version):
        manifest = tomllib.loads((self.root / "Cargo.toml").read_text())
        lock = tomllib.loads((self.root / "Cargo.lock").read_text())
        self.assertEqual(manifest["package"]["version"], version)
        self.assertEqual([p["version"] for p in lock["package"]], [version, "0.1.0"])

    def test_adoption_multiple_commits_and_retry(self):
        self.assertEqual(self.bump(), "0.1.1")
        self.commit("First automatic bump")
        self.assertEqual(self.bump(), "")
        self.assertEqual(self.git("status", "--porcelain"), "")
        self.commit("First incoming commit")
        self.commit("Second incoming commit")
        self.assertEqual(self.bump(), "0.1.3")
        self.assert_versions("0.1.3")

    def test_merge_counts_once_and_manual_version_sets_new_baseline(self):
        self.write_version("1.2.8")
        self.commit("Set a manual version")
        self.assertEqual(self.bump(), "")
        self.git("switch", "-qc", "feature")
        self.commit("First feature commit")
        self.commit("Second feature commit")
        self.git("switch", "-q", "main")
        self.git("merge", "--no-ff", "feature", "-qm", "Merge feature")
        self.assertEqual(self.bump(), "1.2.9")
        self.assert_versions("1.2.9")

    def test_manual_version_in_merge_is_preserved(self):
        self.git("switch", "-qc", "version-change")
        self.write_version("2.0.0")
        self.commit("Choose version 2.0.0")
        self.git("switch", "-q", "main")
        self.git("merge", "--no-ff", "version-change", "-qm", "Merge new version")
        self.assertEqual(self.bump(), "")
        self.commit("First change after the new version")
        self.assertEqual(self.bump(), "2.0.1")
        self.assert_versions("2.0.1")
