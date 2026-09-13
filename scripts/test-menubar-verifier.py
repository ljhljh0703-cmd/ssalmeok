"""Verifier failure-path checks. Fake tools only; no app launch or screen proof."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SCRIPT = Path(__file__).with_name("verify-menubar-runtime.sh")


class VerifierTests(unittest.TestCase):
    def run_case(self, scenario, expected_code):
        with tempfile.TemporaryDirectory(prefix="ssalmeok-verifier-") as directory:
            root = Path(directory)
            (root / "scripts").mkdir()
            shutil.copyfile(SCRIPT, root / "scripts" / SCRIPT.name)
            app = root / "test.app"
            binaries = app / "Contents" / "MacOS"
            binaries.mkdir(parents=True)
            for name in ("ssalmeok", "codexbar", "ssalmeok-menubar"):
                (binaries / name).write_text("fixture only\n")
            stub_dir = root / "stub-bin"
            stub_dir.mkdir()
            stubs = {
                "pgrep": 'test -f "$FIXTURE_ROOT/started" && test "$2" = ssalmeok',
                "open": 'touch "$FIXTURE_ROOT/started"',
                "sleep": 'exit 0',
                "screencapture": 'for target in "$@"; do :; done\nprintf "fixture only" > "$target"',
                "osascript": '''case "$*" in
  *Finder*) echo '0, 0, 1710, 1107'; exit 0 ;;
esac
case "$SCENARIO" in
  probe_error) echo 'injected query failure (-10006)' >&2; exit 1 ;;
  one_item) echo '66%|1102|5|71|24' ;;
  offscreen) printf '66%%|1102|5|71|24\\n94%%|-1|1100|71|24\\n' ;;
  two_items) printf '66%%|1102|5|71|24\\n94%%|1033|5|71|24\\n' ;;
esac''',
            }
            for name, body in stubs.items():
                path = stub_dir / name
                path.write_text("#!/bin/sh\n" + body + "\n")
                path.chmod(0o755)
            env = dict(os.environ, PATH=f"{stub_dir}:{os.environ['PATH']}",
                       FIXTURE_ROOT=str(root), SCENARIO=scenario)
            result = subprocess.run(
                ["bash", str(root / "scripts" / SCRIPT.name), str(app)],
                stdin=subprocess.DEVNULL, capture_output=True, text=True,
                env=env, timeout=15,
            )
            self.assertEqual(result.returncode, expected_code, result.stdout + result.stderr)
            evidence = next((root / ".runtime-evidence").glob("menubar-*"))
            saved = {p.name: p.read_text() for p in evidence.iterdir() if p.suffix == ".txt"}
            self.assertNotIn("pixel_gate=pass", "\n".join(saved.values()))
            return saved

    def test_query_error_is_retained_and_not_reported_as_invisible_app(self):
        saved = self.run_case("probe_error", 1)
        self.assertIn("검사 도구 오류", saved["failure.txt"])
        self.assertIn("injected query failure", saved["probe-error.txt"])
        self.assertNotIn("receipt.txt", saved)

    def test_one_item_cannot_pass_two_provider_requirement(self):
        saved = self.run_case("one_item", 1)
        self.assertIn("66%|1102", saved["failure.txt"])
        self.assertNotIn("receipt.txt", saved)

    def test_offscreen_item_retains_the_failed_coordinate(self):
        saved = self.run_case("offscreen", 1)
        self.assertIn("-1|1100", saved["failure.txt"])
        self.assertNotIn("receipt.txt", saved)

    def test_geometry_alone_cannot_pass_without_pixel_review(self):
        saved = self.run_case("two_items", 2)
        self.assertIn("geometry_gate=pass", saved["receipt.txt"])
        self.assertIn("pixel_gate=pending", saved["receipt.txt"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
