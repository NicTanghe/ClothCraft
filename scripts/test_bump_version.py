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
        self.write_version("0.1.2")
        self.commit("Main version")
        self.git("switch", "-qc", "feature")
        (self.root / "feature.txt").write_text("Feature\n")
        self.commit("Implement feature")

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.root, text=True).strip()

    def commit(self, message):
        self.git("add", ".")
        self.git("commit", "-qm", message)

    def write_version(self, version):
        (self.root / "Cargo.toml").write_text(f'[package]\nname = "clothcraft"\nversion = "{version}"\n')
        (self.root / "Cargo.lock").write_text(
            f'version = 4\n\n[[package]]\nname = "clothcraft"\nversion = "{version}"\n'
            '\n[[package]]\nname = "other"\nversion = "0.1.2"\n'
        )

    def run_bump(self, *args):
        return subprocess.run(
            [sys.executable, SCRIPT, *args, "main"], cwd=self.root,
            capture_output=True, text=True
        )

    def assert_versions(self, version):
        manifest = tomllib.loads((self.root / "Cargo.toml").read_text())
        lock = tomllib.loads((self.root / "Cargo.lock").read_text())
        self.assertEqual(manifest["package"]["version"], version)
        self.assertEqual([p["version"] for p in lock["package"]], [version, "0.1.2"])

    def test_prepare_is_idempotent_and_lands_in_one_squash(self):
        self.assertNotEqual(self.run_bump("--check").returncode, 0)
        result = self.run_bump()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.commit("Prepare version")
        self.assertEqual(self.run_bump().returncode, 0)
        self.assertEqual(self.git("status", "--porcelain"), "")
        self.assertEqual(self.run_bump("--check").returncode, 0)
        self.git("switch", "-q", "main")
        self.git("merge", "--squash", "feature")
        self.commit("Merge feature")
        self.assertEqual(self.git("rev-list", "--count", "HEAD^..main"), "1")
        self.assertEqual(set(self.git("diff", "--name-only", "HEAD^").splitlines()),
                         {"feature.txt", "Cargo.toml", "Cargo.lock"})
        self.assert_versions("0.1.3")

    def test_stale_version_fails_and_can_be_refreshed(self):
        self.write_version("0.1.3")
        self.commit("Prepare version")
        self.git("switch", "-q", "main")
        self.write_version("0.1.3")
        self.commit("Another PR lands first")
        self.git("switch", "-q", "feature")
        self.assertNotEqual(self.run_bump("--check").returncode, 0)
        self.assertEqual(self.run_bump().returncode, 0)
        self.assert_versions("0.1.4")

    def test_manual_version_and_mismatched_lockfile(self):
        self.write_version("2.0.0")
        self.assertEqual(self.run_bump().returncode, 0)
        self.assert_versions("2.0.0")
        lock = self.root / "Cargo.lock"
        lock.write_text(lock.read_text().replace("2.0.0", "0.1.2"))
        self.assertNotEqual(self.run_bump("--check").returncode, 0)

    def test_main_cannot_be_prepared(self):
        self.git("switch", "-q", "main")
        self.assertNotEqual(self.run_bump().returncode, 0)
        self.assertEqual(self.git("status", "--porcelain"), "")
