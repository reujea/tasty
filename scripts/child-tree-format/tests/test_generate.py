"""Inputs regenerate byte for byte and agree with the rules."""

import json
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import helpers  # noqa: F401

sys.dont_write_bytecode = True

import anonymize
import check_rules
from common import DATA_DIR, FORMATS, LEVELS, RULES_PATH, TREES, depths, parse_time, rng, shift_seconds, sha256_bytes
from generate_inputs import compare_trees, generate
from prompts import build_user_prompt, prompt_hash, question_order
from render import check_read_back
from schedule import call_counts, planned_calls, retry_cap_rule

HERE = Path(__file__).resolve().parents[1]


def load(rel):
    return json.loads((DATA_DIR / rel).read_text(encoding="utf-8"))


def raw_snapshot():
    return {
        "registry_surfaces_outside_forest": 1,
        "state_sources": {"children": 2},
        "nodes": [
            {"surface": 50, "parent": None, "external_parent": None, "nickname": None, "cwd": None, "repo_key": None, "state": "exited", "last_state_report_at": None},
            {"surface": 60, "parent": 50, "external_parent": None, "nickname": "Some Secret Job", "cwd": "/srv/hidden/a", "repo_key": "/srv/hidden/.git", "state": "idle", "last_state_report_at": 1794188477999},
            {"surface": 70, "parent": 50, "external_parent": None, "nickname": None, "cwd": "/srv/hidden/a", "repo_key": "/srv/hidden/.git", "state": "active", "last_state_report_at": 1794188477000},
            {"surface": 80, "parent": None, "external_parent": "ext:key", "nickname": "other", "cwd": "/srv/other", "repo_key": None, "state": "active", "last_state_report_at": 1794188417500},
        ],
    }


class Generation(unittest.TestCase):
    def test_two_generations_are_byte_identical_and_match_the_tracked_files(self):
        real = load("trees/real.json")
        with tempfile.TemporaryDirectory() as a, tempfile.TemporaryDirectory() as b:
            generate(real, DATA_DIR, Path(a))
            generate(real, DATA_DIR, Path(b))
            self.assertEqual(compare_trees(Path(a), Path(b)), [])
            self.assertEqual(compare_trees(Path(a), DATA_DIR), [])
            self.assertGreater(len(list(Path(a).rglob("*.txt"))), 18)

    def test_manifest_hashes_match_the_files(self):
        manifest = load("manifest.json")
        # 3 trees, 18 renderings, 3 question and 3 answer files, 4 schedules, 2 templates.
        self.assertEqual(len(manifest["files"]), 33)
        for rel, digest in manifest["files"].items():
            self.assertEqual(sha256_bytes((DATA_DIR / rel).read_bytes()), digest, rel)

    def test_every_stored_rendering_reads_back_to_its_tree(self):
        checked = 0
        for tree in TREES:
            nodes = load(f"trees/{tree}.json")["nodes"]
            for fmt in FORMATS:
                text = (DATA_DIR / f"rendered/{tree}/{fmt}.txt").read_text(encoding="utf-8").rstrip("\n")
                check_read_back(fmt, nodes, text)
                checked += 1
        self.assertEqual(checked, 18)

    def test_synthetic_sizes_and_forced_items(self):
        manifest = load("manifest.json")
        for tree, count in (("synthetic_40", 40), ("synthetic_80", 80)):
            nodes = load(f"trees/{tree}.json")["nodes"]
            self.assertEqual(len(nodes), count)
            self.assertGreaterEqual(max(depths(nodes).values()), 3)
            for state in ("active", "idle", "needs_input", "stale", "exited"):
                self.assertTrue(any(n["state"] == state for n in nodes), state)
            times = [n["last_report_at"] for n in nodes if n["last_report_at"]]
            self.assertEqual(len(times), len(set(times)), "synthetic times must not overlap")
            self.assertEqual(max(times), "2026-01-05T12:00:00Z")
        self.assertEqual(manifest["real_shape"]["nodes"], len(load("trees/real.json")["nodes"]))

    def test_no_markdown_under_data(self):
        self.assertEqual(list(DATA_DIR.rglob("*.md")), [])

    def test_each_schedule_row_regenerates_its_prompt_hash(self):
        system = (DATA_DIR / "prompts/system.txt").read_text(encoding="utf-8").rstrip("\n")
        template = (DATA_DIR / "prompts/user.txt").read_text(encoding="utf-8").rstrip("\n")
        checked = 0
        for name in ("pilot", *LEVELS):
            for line in (DATA_DIR / f"schedules/{name}.jsonl").read_text(encoding="utf-8").splitlines():
                row = json.loads(line)
                stored = load(f"questions/{row['tree']}.json")
                order = question_order(row["call_id"], stored["questions"])
                self.assertEqual(order, row["order"])
                rendered = (DATA_DIR / f"rendered/{row['tree']}/{row['format']}.txt").read_text(encoding="utf-8").rstrip("\n")
                user = build_user_prompt(template, row["format"], rendered, stored["questions"], stored["silent_minutes"], order)
                self.assertEqual(prompt_hash(system, user), row["prompt_sha256"])
                for placeholder in ("{format_name}", "{missing_note}", "{T}", "{rendered}", "{N}", "{questions}"):
                    self.assertNotIn(placeholder, user)
                checked += 1
        self.assertEqual(checked, 6 + 162 + 126 + 84)

    def test_templates_are_the_text_in_the_rules(self):
        text = RULES_PATH.read_text(encoding="utf-8")
        for label, name in (("시스템", "system"), ("사용자", "user")):
            block = re.search(rf"{label}:\n\n```text\n(.*?)\n```", text, re.S).group(1)
            self.assertEqual((DATA_DIR / f"prompts/{name}.txt").read_text(encoding="utf-8"), block + "\n")

    def test_question_table_matches_the_rules(self):
        text = RULES_PATH.read_text(encoding="utf-8")
        table = dict(re.findall(r"^\| `(q\d+)` \| [^|]+ \| [^|]+ \| `([^`]+)` \|$", text, re.M))
        self.assertEqual(len(table), 15)
        for tree in TREES:
            for question in load(f"questions/{tree}.json")["questions"]:
                wanted = table[question["id"]]
                for key, value in question["params"].items():
                    wanted = wanted.replace("{" + key + "}", str(value))
                if question["id"] == "q11" and question["params"]:
                    wanted = f"Among the descendants of {question['params']['root']} only: {wanted}"
                self.assertEqual(question["text"], wanted)


class Schedules(unittest.TestCase):
    def test_counts_come_from_the_plan_and_match_the_rules(self):
        counts = call_counts()
        self.assertEqual(counts["pilot"]["calls"], 6)
        self.assertEqual({k: counts[k]["calls"] for k in LEVELS}, {"L0": 162, "L1": 126, "L2": 84})
        for level in LEVELS:
            self.assertEqual(counts[level]["calls"], len(planned_calls(level)))
            self.assertEqual(counts[level]["retry_cap"], retry_cap_rule(counts[level]["calls"]))

    def test_order_differs_by_call_and_is_stable(self):
        rows = [json.loads(line) for line in (DATA_DIR / "schedules/L0.jsonl").read_text(encoding="utf-8").splitlines()]
        self.assertEqual(len({r["call_id"] for r in rows}), 162)
        self.assertGreater(len({tuple(r["order"]) for r in rows}), 100)
        self.assertNotEqual([r["call_id"] for r in rows], sorted(r["call_id"] for r in rows))

    def test_lower_levels_reuse_the_same_call_ids_and_prompts(self):
        def table(name):
            rows = (json.loads(line) for line in (DATA_DIR / f"schedules/{name}.jsonl").read_text(encoding="utf-8").splitlines())
            return {r["call_id"]: r["prompt_sha256"] for r in rows}

        l0, l1, l2 = table("L0"), table("L1"), table("L2")
        self.assertTrue(set(l2) <= set(l1) <= set(l0))
        self.assertTrue(all(l0[k] == v for k, v in l1.items()))
        self.assertFalse(any(k.startswith("synthetic_40") for k in l2))

    def test_seed_formula(self):
        # Worked out independently: sha256 of the joined text, first eight bytes, big endian.
        import hashlib
        import random

        text = b"20261109/targets/real/q3"
        seed = int.from_bytes(hashlib.sha256(text).digest()[:8], "big")
        self.assertEqual(rng("targets", "real/q3").random(), random.Random(seed).random())


class Anonymize(unittest.TestCase):
    def test_transform(self):
        tree, local = anonymize.anonymize(raw_snapshot())
        text = json.dumps(tree)
        for secret in ("Some Secret Job", "/srv", "hidden", "ext:key", "other"):
            self.assertNotIn(secret, text)
        nodes = {n["id"]: n for n in tree["nodes"]}
        ids = local["ids"]
        self.assertEqual(nodes[ids["60"]]["cwd"], nodes[ids["70"]]["cwd"], "same source, same pseudonym")
        self.assertIsNone(nodes[ids["70"]]["nickname"])
        self.assertIsNone(nodes[ids["50"]]["last_report_at"])
        self.assertIsNone(nodes[ids["80"]]["parent"], "an external parent becomes a root")
        self.assertEqual(tree["capture"]["external_parent_roots"], 1)
        self.assertEqual(tree["capture"]["cwd_unknown_nodes"], 1)
        self.assertEqual(set(nodes[ids["60"]]), {"id", "parent", "nickname", "state", "cwd", "last_report_at"})

    def test_milliseconds_floor_before_the_clock_shift(self):
        tree, local = anonymize.anonymize(raw_snapshot())
        nodes = {n["id"]: n for n in tree["nodes"]}
        ids = local["ids"]
        # 477999 ms and 477000 ms floor to the same second and map to the latest time.
        self.assertEqual(nodes[ids["60"]]["last_report_at"], "2026-01-05T12:00:00Z")
        self.assertEqual(nodes[ids["70"]]["last_report_at"], "2026-01-05T12:00:00Z")
        # 417500 ms floors to 417, sixty seconds before 477.
        self.assertEqual(nodes[ids["80"]]["last_report_at"], "2026-01-05T11:59:00Z")

    def test_shift_keeps_intervals(self):
        out = shift_seconds([100, None, 40])
        self.assertEqual(out[0], "2026-01-05T12:00:00Z")
        self.assertIsNone(out[1])
        self.assertEqual((parse_time(out[0]) - parse_time(out[2])).total_seconds(), 60)


class DecisionHash(unittest.TestCase):
    def test_tracked_rules_pass(self):
        done = subprocess.run([sys.executable, "-B", str(HERE / "check_rules.py")], capture_output=True, text=True)
        self.assertEqual(done.returncode, 0, done.stdout + done.stderr)
        self.assertIn("committed versions checked:", done.stdout)

    def test_a_changed_section_is_detected(self):
        data = RULES_PATH.read_bytes()
        self.assertEqual(check_rules.section_hash(data), check_rules.DECISION_SHA256)
        changed = data.replace(b"1e-12", b"1e-11")
        self.assertNotEqual(changed, data)
        self.assertNotEqual(check_rules.section_hash(changed), check_rules.DECISION_SHA256)

    def test_change_outside_the_section_is_allowed(self):
        data = RULES_PATH.read_bytes()
        self.assertEqual(check_rules.section_hash(data + b"\nextra\n"), check_rules.DECISION_SHA256)

    def test_missing_or_doubled_markers_cannot_judge(self):
        data = RULES_PATH.read_bytes()
        self.assertIsNone(check_rules.section_hash(data.replace(b"decision-rules:end", b"x")))


if __name__ == "__main__":
    unittest.main()
