"""The anonymity check must detect each marker kind, also inside response bodies."""

import io
import json
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import check_anonymity

# Markers are assembled here so that the test file itself stays clean.
MARKERS = {
    "unix-home-path": "/ho" + "me/someone/x",
    "drive-path": "D" + ":\\work\\x",
    "email": "someone" + "@" + "example.org",
    "private-ip": "192." + "168.0.3",
    "session-token-name": "TASTY_SESSI" + "ON_TOKEN",
    "session-field": "sessi" + "on_id",
    "request-number": "R" + "-43",
}
PRIVATE_TERM = "acme-hidden-" + "name"
NICKNAME = "fix the " + "secret_widget" + " thing"
CWD_PART = "acme-" + "worktree-7"


def quiet(argv, stdin=None):
    out, err = io.StringIO(), io.StringIO()
    old = sys.stdin
    if stdin is not None:
        sys.stdin = io.StringIO(stdin)
    try:
        with redirect_stdout(out), redirect_stderr(err):
            rc = check_anonymity.main(argv)
    finally:
        sys.stdin = old
    return rc, out.getvalue() + err.getvalue()


class AnonymityCheck(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        base = Path(self.tmp.name)
        self.root = base / "tree"
        self.data = self.root / "docs/research/child-tree-format/data"
        self.data.mkdir(parents=True)
        (self.root / "scripts/child-tree-format").mkdir(parents=True)
        (self.data / "clean.txt").write_text("lane-a /work/repo-a/wt-01\n")
        self.deny = base / "deny.txt"
        self.deny.write_text("# comment\n" + PRIVATE_TERM + "\n")
        self.snap = base / "snap.json"
        self.snap.write_text(
            json.dumps({"nodes": [{"nickname": NICKNAME, "cwd": "/srv/" + CWD_PART, "repo_key": None}]})
        )

    def run_check(self):
        return quiet(
            ["--root", str(self.root), "--denylist", str(self.deny), "--snapshot", str(self.snap)]
        )

    def test_clean_tree_passes(self):
        rc, out = self.run_check()
        self.assertEqual(rc, 0, out)
        self.assertIn("scanned files: 1", out)

    def test_each_public_marker_is_found_in_a_plain_file(self):
        for kind, marker in MARKERS.items():
            with self.subTest(kind=kind):
                path = self.data / "marked.txt"
                path.write_text("ok\n" + marker + "\n")
                rc, out = self.run_check()
                self.assertEqual(rc, 1, out)
                self.assertIn(f"marked.txt:2: {kind}", out)
                self.assertNotIn(marker, out)
                path.unlink()

    def test_each_public_marker_is_found_inside_a_response_body(self):
        for kind, marker in MARKERS.items():
            with self.subTest(kind=kind):
                path = self.data / "raw.jsonl"
                # ensure_ascii and an escaped slash hide the plain text form.
                body = json.dumps({"response_text": "see " + marker}).replace("/", "\\/")
                path.write_text(body + "\n")
                rc, out = self.run_check()
                self.assertEqual(rc, 1, out)
                self.assertIn(f"raw.jsonl:1: {kind}", out)
                path.unlink()

    def test_private_terms_are_found_and_never_printed(self):
        for term in (PRIVATE_TERM, PRIVATE_TERM.upper(), NICKNAME, "secret_widget", CWD_PART):
            with self.subTest(term=term):
                path = self.data / "marked.json"
                path.write_text(json.dumps({"response_text": term}, ensure_ascii=True) + "\n")
                rc, out = self.run_check()
                self.assertEqual(rc, 1, out)
                self.assertIn("marked.json:1: private-term", out)
                self.assertNotIn(term.lower(), out.lower())
                path.unlink()

    def test_non_ascii_nickname_behind_escapes_is_found(self):
        name = "한글 작업"
        self.snap.write_text(json.dumps({"nodes": [{"nickname": name}]}))
        (self.data / "raw.jsonl").write_text(json.dumps({"r": name}, ensure_ascii=True) + "\n")
        rc, out = self.run_check()
        self.assertEqual(rc, 1, out)
        self.assertIn("private-term", out)

    def test_cannot_judge_without_inputs_or_targets(self):
        cases = [
            ["--root", str(self.root)],
            ["--root", str(self.root), "--denylist", str(self.deny)],
            ["--root", str(self.root), "--snapshot", str(self.snap)],
            ["--root", str(self.root), "--denylist", str(self.deny) + ".gone", "--snapshot", str(self.snap)],
        ]
        for argv in cases:
            with self.subTest(argv=argv):
                self.assertEqual(quiet(argv)[0], 2)
        self.deny.write_text("# only a comment\n")
        self.assertEqual(self.run_check()[0], 2)
        self.deny.write_text(PRIVATE_TERM + "\n")
        empty = Path(self.tmp.name) / "empty"
        empty.mkdir()
        rc, _ = quiet(["--root", str(empty), "--denylist", str(self.deny), "--snapshot", str(self.snap)])
        self.assertEqual(rc, 2)

    def test_public_only_says_the_private_check_did_not_run(self):
        rc, out = quiet(["--root", str(self.root), "--public-only"])
        self.assertEqual(rc, 0)
        self.assertIn("private term check: not run", out)
        (self.data / "marked.txt").write_text(MARKERS["email"] + "\n")
        self.assertEqual(quiet(["--root", str(self.root), "--public-only"])[0], 1)

    def test_stdin_reads_deleted_lines_and_skips_git_headers(self):
        args = ["--denylist", str(self.deny), "--snapshot", str(self.snap), "--stdin"]
        header = "commit abc\nAuthor: a <" + MARKERS["email"] + ">\n\n    Co-Authored-By: b <" + MARKERS["email"] + ">\n"
        rc, out = quiet(args, stdin=header + "+clean\n")
        self.assertEqual(rc, 0, out)
        self.assertIn("2 git metadata lines skipped", out)
        rc, out = quiet(args, stdin=header + "-" + PRIVATE_TERM + "\n")
        self.assertEqual(rc, 1, out)
        rc, out = quiet(args, stdin="    subject with " + MARKERS["request-number"] + "\n")
        self.assertEqual(rc, 1, out)
        self.assertEqual(quiet(args, stdin="")[0], 2)

    def test_common_terms_do_not_become_private_terms(self):
        terms = check_anonymity.snapshot_terms({"nodes": [{"cwd": "/work/lane/tasty"}]})
        self.assertEqual(terms, set())


if __name__ == "__main__":
    unittest.main()
