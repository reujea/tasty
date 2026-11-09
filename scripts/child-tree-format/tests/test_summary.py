"""The decision formula and the summary built from fake raw data. No model is involved."""

import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

import helpers  # noqa: F401

sys.dont_write_bytecode = True

import summarize as sm
from common import DATA_DIR, FORMATS, LOW_MODEL, MODELS


def scores(**values):
    """Build {fmt: (score, u, questions)} from short keyword form."""
    return {fmt: value for fmt, value in values.items()}


def low(correct, total=100, tokens=(100,)):
    return {"correct": correct, "total": total, "tokens": list(tokens)}


class CandidateFormula(unittest.TestCase):
    def test_minimum_resolution_boundary_is_a_candidate(self):
        # Zero uncertainty: the margin is 1 / questions = 0.25.
        self.assertTrue(sm.is_candidate(1.0, 0.75, 0.0, 0.0, 4))
        self.assertFalse(sm.is_candidate(1.0, 0.74, 0.0, 0.0, 4))

    def test_tolerance_is_1e_12(self):
        self.assertTrue(sm.is_candidate(1.0, 0.75 - 5e-13, 0.0, 0.0, 4))
        self.assertFalse(sm.is_candidate(1.0, 0.75 - 2e-12, 0.0, 0.0, 4))

    def test_uncertainty_margin_boundary(self):
        # 2 * sqrt(0.03**2 + 0.04**2) = 0.1
        self.assertTrue(sm.is_candidate(0.9, 0.8, 0.03, 0.04, 1000))
        self.assertFalse(sm.is_candidate(0.9, 0.79, 0.03, 0.04, 1000))

    def test_reference_factors_change_only_the_margin(self):
        self.assertFalse(sm.is_candidate(0.9, 0.8, 0.03, 0.04, 1000, factor=1))
        self.assertTrue(sm.is_candidate(0.9, 0.76, 0.03, 0.04, 1000, factor=3))

    def test_aggregate_formula(self):
        # Worked out by hand. Cell means 0.5 and 1.0, score 0.75.
        # Variances: 0.02 over 2 repeats = 0.01, and 0. U = sqrt(0.01) / 2 = 0.05.
        score, uncertainty = sm.aggregate({"a": [0.4, 0.6], "b": [1.0, 1.0]})
        self.assertAlmostEqual(score, 0.75)
        self.assertAlmostEqual(uncertainty, 0.05)

    def test_best_tie_uses_the_larger_uncertainty(self):
        table = scores(a=(0.9, 0.0, 1000), b=(0.9, 0.05, 1000), c=(0.8, 0.0, 1000))
        # Margin against b is 2 * 0.05 = 0.1, so c is in whichever tied format is named first.
        self.assertEqual(sm.candidates(table), ["a", "b", "c"])


class Decide(unittest.TestCase):
    def test_rule_1_excludes_a_format_outside_the_margin(self):
        table = scores(a=(0.9, 0.0, 100), b=(0.5, 0.0, 100))
        # b is more accurate on the low model but rule 1 removed it first.
        out = sm.decide(table, {"a": low(50), "b": low(99)})
        self.assertEqual(out["candidates"], ["a"])
        self.assertEqual(out["recommendation"], "a")

    def test_rule_2_picks_the_low_model_winner_among_candidates(self):
        table = scores(a=(0.90, 0.02, 100), b=(0.88, 0.02, 100))
        out = sm.decide(table, {"a": low(80), "b": low(85)})
        self.assertEqual(out["candidates"], ["a", "b"])
        self.assertEqual(out["after_rule_2"], ["b"])
        self.assertEqual(out["recommendation"], "b")
        self.assertFalse(out["rule_3_applied"])

    def test_rule_2_compares_exact_fractions_with_different_totals(self):
        table = scores(a=(0.9, 0.05, 100), b=(0.9, 0.05, 100))
        # 2/3 and 4/6 are the same value and must reach rule 3.
        out = sm.decide(table, {"a": low(2, 3, (300,)), "b": low(4, 6, (200,))})
        self.assertEqual(out["after_rule_2"], ["a", "b"])
        self.assertTrue(out["rule_3_applied"])
        self.assertEqual(out["recommendation"], "b")

    def test_rule_3_uses_exact_mean_tokens(self):
        table = scores(a=(0.9, 0.05, 100), b=(0.9, 0.05, 100))
        out = sm.decide(table, {"a": low(80, 100, (10, 11)), "b": low(80, 100, (10, 10, 11))})
        # 21/2 against 31/3: b is smaller by a third of a token over the fraction.
        self.assertEqual(out["recommendation"], "b")

    def test_rule_3_tie_is_reported_without_a_pick(self):
        table = scores(a=(0.9, 0.05, 100), b=(0.9, 0.05, 100))
        out = sm.decide(table, {"a": low(80, 100, (10, 20)), "b": low(80, 100, (15, 15))})
        self.assertIsNone(out["recommendation"])
        self.assertEqual(out["tie"], ["a", "b"])

    def test_missing_token_field_skips_rule_3_and_reports_a_tie(self):
        table = scores(a=(0.9, 0.05, 100), b=(0.9, 0.05, 100))
        out = sm.decide(table, {"a": low(80, 100, (10, None)), "b": low(80, 100, (5,))})
        self.assertIsNone(out["recommendation"])
        self.assertFalse(out["rule_3_applied"])
        self.assertIn("token field", out["reason"])

    def test_token_sum_needs_all_four_fields(self):
        usage = {"input_tokens": 1, "cache_creation_input_tokens": 2, "cache_read_input_tokens": 4, "output_tokens": 8}
        self.assertEqual(sm.call_tokens(usage), 15)
        self.assertEqual(sm.call_tokens({**usage, "cache_read_input_tokens": 0}), 11)
        for field in usage:
            self.assertIsNone(sm.call_tokens({k: v for k, v in usage.items() if k != field}))
        self.assertIsNone(sm.call_tokens({**usage, "output_tokens": None}))
        self.assertIsNone(sm.call_tokens(None))


class Approval(unittest.TestCase):
    def call(self, at):
        return {"started_at": at}

    def test_last_change_before_the_last_call_counts(self):
        log = [
            {"kind": "level_change", "at": "2026-01-01T00:00:00.000000Z", "to": "L0"},
            {"kind": "level_change", "at": "2026-01-01T02:00:00.000000Z", "to": "L1"},
            {"kind": "level_change", "at": "2026-01-01T09:00:00.000000Z", "to": "L2"},
            {"kind": "resume", "at": "2026-01-01T09:30:00.000000Z"},
        ]
        calls = [self.call("2026-01-01T01:00:00.000000Z"), self.call("2026-01-01T03:00:00.000000Z")]
        # The change after the last call cannot throw away repeats that already ran.
        self.assertEqual(sm.approved_level(log, calls), "L1")

    def test_no_approval_means_no_level(self):
        self.assertIsNone(sm.approved_level([], [self.call("2026-01-01T01:00:00.000000Z")]))
        self.assertIsNone(sm.approved_level([{"kind": "level_change", "at": "2026-01-02T00:00:00.000000Z", "to": "L0"}], [self.call("2026-01-01T01:00:00.000000Z")]))


def usage(n=100):
    return {"input_tokens": n, "cache_creation_input_tokens": 10, "cache_read_input_tokens": 5, "output_tokens": 20}


class EndToEnd(unittest.TestCase):
    """Build a data folder copy with a fake raw file and summarize it."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.data = Path(self.tmp.name) / "data"
        shutil.copytree(DATA_DIR, self.data, ignore=shutil.ignore_patterns("raw", "summary"))
        (self.data / "raw").mkdir()

    def rows(self, level):
        return [json.loads(line) for line in (self.data / f"schedules/{level}.jsonl").read_text().splitlines()]

    def answers(self, tree):
        return json.loads((self.data / f"answers/{tree}.json").read_text())

    def record(self, row, seq, response=None, error_kind=None, tool_violations=0, attempt=1):
        answers = self.answers(row["tree"])
        return {
            "call_id": row["call_id"],
            "attempt": attempt,
            "tree": row["tree"],
            "format": row["format"],
            "model": row["model"],
            "repeat": row["repeat"],
            "started_at": f"2026-01-02T00:{seq // 60:02d}:{seq % 60:02d}.000000Z",
            "wall_ms": 1000 + seq,
            "prompt_sha256": row["prompt_sha256"],
            "response_text": json.dumps(answers) if response is None else response,
            "usage": usage(),
            "phase": "main",
            "error_kind": error_kind,
            "tool_violations": tool_violations,
        }

    def write(self, records, level="L0"):
        (self.data / "raw/main.jsonl").write_text("".join(json.dumps(r) + "\n" for r in records))
        (self.data / "raw/level-changes.jsonl").write_text(
            json.dumps({"kind": "level_change", "at": "2026-01-01T00:00:00.000000Z", "from": None, "to": level}) + "\n"
        )

    def full_run(self, level="L0"):
        return [self.record(row, seq) for seq, row in enumerate(self.rows(level))]

    def test_all_right_answers_score_one_and_recommend_by_tokens(self):
        records = self.full_run()
        for record in records:
            if record["format"] == "indented":
                record["usage"] = usage(50)
        self.write(records)
        out = sm.summarize(self.data)
        self.assertEqual(out["unfilled_cells"], [])
        for fmt in FORMATS:
            self.assertEqual(out["per_format"][fmt]["score"], 1.0)
        self.assertEqual(out["decision"]["recommendation"], "indented")
        self.assertTrue(out["decision"]["rule_3_applied"])
        self.assertEqual(out["low_model"]["indented"]["mean_tokens"], 85)
        self.assertEqual(out["low_model"]["adjacency"]["mean_tokens"], 135)

    def test_an_empty_cell_gives_no_recommendation(self):
        records = self.full_run()
        dropped = records.pop(0)
        self.write(records)
        out = sm.summarize(self.data)
        self.assertIsNone(out["decision"]["recommendation"])
        self.assertEqual(len(out["unfilled_cells"]), 1)
        self.assertIn(f"{dropped['format']}/{dropped['model']}/{dropped['tree']}: 2 of 3", out["unfilled_cells"][0])

    def test_an_empty_cell_stops_a_recommendation_that_tokens_would_give(self):
        records = self.full_run()
        for record in records:
            if record["format"] == "indented":
                record["usage"] = usage(50)
        records.pop(0)
        self.write(records)
        out = sm.summarize(self.data)
        self.assertIsNone(out["decision"]["recommendation"])
        self.assertIn("not full", out["decision"]["reason"])
        self.assertNotIn("low_model", out)

    def test_rule_2_reads_the_low_model_only(self):
        records = self.full_run()
        for record in records:
            # mermaid loses only on the high model and indented only on the low model.
            if record["format"] == "mermaid" and record["model"] == "claude-high" and record["repeat"] == 1:
                record["response_text"] = "not json"
            if record["format"] == "indented" and record["model"] == LOW_MODEL and record["repeat"] == 1 and record["tree"] == "real":
                record["response_text"] = "not json"
            if record["format"] != "mermaid":
                record["usage"] = usage(500)
        self.write(records)
        out = sm.summarize(self.data)
        self.assertLess(out["low_model"]["indented"]["correct"], out["low_model"]["adjacency"]["correct"])
        self.assertEqual(out["low_model"]["mermaid"]["correct"], out["low_model"]["adjacency"]["correct"])
        self.assertEqual(out["decision"]["recommendation"], "mermaid")
        self.assertEqual(out["low_model"]["adjacency"]["total"], 3 * 3 * 15)

    def test_failed_calls_are_left_out_and_the_first_valid_call_counts(self):
        records = self.full_run()
        row = self.rows("L0")[0]
        wrong = self.record(row, 1000, response="{}", attempt=2)
        failed = self.record(row, 0, response="", error_kind="api_error")
        violated = self.record(row, 0, response=json.dumps(self.answers(row["tree"])), tool_violations=1)
        self.write([failed, violated, *records, wrong])
        out = sm.summarize(self.data)
        # The later empty response must not replace the first valid one.
        self.assertEqual(out["per_format"][row["format"]]["score"], 1.0)
        self.assertEqual(out["calls_by_outcome"], {"api_error": 1, "scored": 163, "tool_violation": 1})
        self.assertEqual(out["unfilled_cells"], [])

    def test_a_parse_failure_stays_valid_and_scores_zero_in_its_repeat(self):
        records = self.full_run()
        target = next(r for r in records if r["format"] == "mermaid" and r["model"] == LOW_MODEL)
        target["response_text"] = "not json"
        self.write(records)
        out = sm.summarize(self.data)
        self.assertEqual(out["parse_failures"], 1)
        # The mermaid score loses a third of one of its nine cells.
        self.assertAlmostEqual(out["per_format"]["mermaid"]["score"], 1 - (1 / 3) / 9)
        self.assertEqual(out["unfilled_cells"], [])

    def test_calls_outside_the_approved_plan_are_ignored(self):
        records = self.full_run("L1")
        extra_row = next(r for r in self.rows("L0") if r["model"] == "codex" and r["repeat"] == 3)
        records.append(self.record(extra_row, 1500, response="not json"))
        pilot_row = self.rows("pilot")[0]
        records.append(self.record(pilot_row, 1501, response="not json"))
        marked = self.record(self.rows("L1")[0], 1502, response="not json")
        marked["phase"] = "pilot"
        records.insert(0, marked)
        self.write(records, level="L1")
        out = sm.summarize(self.data)
        self.assertEqual(out["approved_level"], "L1")
        self.assertEqual(out["parse_failures"], 0)
        self.assertEqual(out["unfilled_cells"], [])
        self.assertEqual({f: out["per_format"][f]["score"] for f in FORMATS}, {f: 1.0 for f in FORMATS})

    def test_level_two_needs_no_synthetic_40(self):
        self.write(self.full_run("L2"), level="L2")
        out = sm.summarize(self.data)
        self.assertEqual(out["unfilled_cells"], [])
        self.assertNotIn("synthetic_40", out["breakdown"]["tree"]["indented"])

    def test_a_prompt_hash_mismatch_cannot_be_judged(self):
        records = self.full_run()
        records[0]["prompt_sha256"] = "0" * 64
        self.write(records)
        with self.assertRaises(ValueError):
            sm.summarize(self.data)

    def test_no_raw_data_cannot_be_judged(self):
        self.assertEqual(sm.main(["--data", str(self.data)]), 2)

    def test_check_compares_bytes(self):
        self.write(self.full_run())
        self.assertEqual(sm.main(["--data", str(self.data)]), 0)
        self.assertEqual(sm.main(["--data", str(self.data), "--check"]), 0)
        with (self.data / "summary/tables.txt").open("a") as handle:
            handle.write("hand edit\n")
        self.assertEqual(sm.main(["--data", str(self.data), "--check"]), 1)
        (self.data / "summary/tables.txt").unlink()
        self.assertEqual(sm.main(["--data", str(self.data), "--check"]), 1)

    def test_tokens_of_each_tree_are_kept_apart(self):
        records = self.full_run()
        for record in records:
            if record["tree"] == "real":
                record["usage"] = usage(1)
        self.write(records)
        cost = sm.summarize(self.data)["costs_by_tree"]["indented"]["claude-low"]
        self.assertEqual(cost["real"]["input_tokens"]["mean"], 16)
        self.assertEqual(cost["synthetic_80"]["input_tokens"]["mean"], 115)

    def test_tables_carry_every_axis(self):
        self.write(self.full_run())
        out = sm.summarize(self.data)
        text = sm.tables(out)
        for word in ("score by format and model", "score by format and tree", "score by format and kind", "mean cost per valid call", "reference candidates with factor 1", "reference candidates with factor 3", *MODELS):
            self.assertIn(word, text)


if __name__ == "__main__":
    unittest.main()
