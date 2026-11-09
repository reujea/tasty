"""The runner against the fake CLI. Nothing here reaches a model."""

import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import helpers  # noqa: F401

sys.dont_write_bytecode = True

import check_rules
import runner
import summarize
from cli_events import parse_claude, parse_codex
from common import DATA_DIR, MODELS, REPO_ROOT, RULES_REL, SEED

FAKE = Path(__file__).resolve().parents[1] / "fake_cli.py"
PROVENANCE = {"rules_commit": "c" * 40, "rules_blob": "b" * 40}
ALLOWED = {
    "call_id", "attempt", "attempt_kind", "phase", "level", "tree", "format", "model", "repeat",
    "model_requested", "model_reported", "cli_version", "started_at", "finished_at", "wall_ms", "seed",
    "prompt_sha256", "response_text", "usage", "duration_ms", "duration_api_ms", "total_cost_usd",
    "error_kind", "status_code", "tool_violations", "rules_commit", "rules_blob", "windows",
}


APPROVED = {"claude": 50.0, "codex": None}


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.data = self.root / "data"
        shutil.copytree(DATA_DIR, self.data, ignore=shutil.ignore_patterns("raw", "summary"))
        self.fake_log = self.root / "fake.jsonl"
        env = {"FAKE_CLI_LOG": str(self.fake_log), "TASTY_SURFACE_ID": "7", "TASTY_HOME": "/private"}
        patcher = mock.patch.dict(os.environ, env)
        patcher.start()
        self.addCleanup(patcher.stop)

    def scenarios(self, names):
        path = self.root / "scenarios.json"
        path.write_text(json.dumps(names))
        os.environ["FAKE_CLI_SCENARIOS"] = str(path)
        self.addCleanup(os.environ.pop, "FAKE_CLI_SCENARIOS", None)

    def config(self, phase="pilot", level=None, **extra):
        return runner.Config(
            data=self.data, phase=phase, level=level, provenance=PROVENANCE,
            bins={"claude": str(FAKE), "codex": str(FAKE)}, versions={"claude": "fake", "codex": "fake"},
            **{"timeout": 10, **extra},
        )

    def raw(self, phase="pilot"):
        return runner.read_jsonl(runner.raw_path(self.data, phase))

    def log(self):
        return runner.read_jsonl(runner.log_path(self.data))

    def fake_calls(self):
        return runner.read_jsonl(self.fake_log)

    def rows(self, name="pilot"):
        return runner.read_jsonl(self.data / f"schedules/{name}.jsonl")


class Normal(Base):
    def test_pilot_runs_every_planned_call_in_schedule_order(self):
        self.assertEqual(runner.run(self.config()), 0)
        raw = self.raw()
        self.assertEqual([r["call_id"] for r in raw], [r["call_id"] for r in self.rows()])
        self.assertEqual(len(raw), 6)
        for record in raw:
            self.assertEqual((record["attempt"], record["attempt_kind"], record["seed"]), (1, "first", SEED))
            self.assertEqual(record["model_requested"], MODELS[record["model"]]["id"])
            self.assertIsNone(record["error_kind"])
            self.assertEqual(record["rules_commit"], PROVENANCE["rules_commit"])

    def test_only_allowed_fields_are_stored(self):
        self.scenarios(["normal", "api_error", "normal", "tool_call"])
        self.assertEqual(runner.run(self.config()), 0)
        text = runner.raw_path(self.data, "pilot").read_text()
        for record in self.raw():
            self.assertLessEqual(set(record), ALLOWED)
        self.assertNotIn("session-that-must-not-be-stored", text)
        self.assertNotIn("/private", text)
        self.assertNotIn("/tmp", text)
        self.assertNotIn("server_tool_use", text)

    def test_isolation_of_each_call(self):
        self.assertEqual(runner.run(self.config()), 0)
        calls = self.fake_calls()
        self.assertEqual(len(calls), 6)
        self.assertEqual(len({c["cwd"] for c in calls}), 6)
        for call in calls:
            self.assertEqual(call["cwd_entries"], [])
            self.assertEqual(call["tasty_vars"], [])
            self.assertFalse(Path(call["cwd"]).exists())
            self.assertFalse(Path(call["cwd"]).is_relative_to(REPO_ROOT))

    def test_commands_match_the_rules(self):
        self.assertEqual(runner.run(self.config()), 0)
        system = (self.data / "prompts/system.txt").read_text().rstrip("\n")
        by_call = zip(self.raw(), self.fake_calls())
        for record, call in by_call:
            model = MODELS[record["model"]]
            if model["cli"] == "claude":
                self.assertEqual(call["argv"], [
                    "-p", "--safe-mode", "--tools", "", "--strict-mcp-config", "--no-session-persistence",
                    "--system-prompt", system, "--model", model["id"], "--effort", "medium",
                    "--output-format", "stream-json", "--verbose",
                ])
            else:
                self.assertEqual(call["argv"], [
                    "exec", "--json", "--ephemeral", "--skip-git-repo-check", "--ignore-user-config",
                    "-C", call["cwd"], "-m", "gpt-6.1-sol", "-c", "model_reasoning_effort=medium",
                    "-c", "features.shell_tool=false", "-c", "project_doc_max_bytes=0",
                ])

    def test_stdin_bytes_match_the_scheduled_hash(self):
        self.assertEqual(runner.run(self.config()), 0)
        for row, call in zip(self.rows(), self.fake_calls()):
            system, user = runner.load_prompt(self.data, row)
            sent = user if MODELS[row["model"]]["cli"] == "claude" else f"{system}\n\n{user}"
            self.assertEqual(call["stdin_sha256"], hashlib.sha256(sent.encode()).hexdigest())
            self.assertEqual(hashlib.sha256(f"{system}\n\n{user}".encode()).hexdigest(), row["prompt_sha256"])

    def test_changed_input_cannot_run(self):
        rendered = self.data / "rendered/synthetic_80" / f"{self.rows()[0]['format']}.txt"
        rendered.write_text(rendered.read_text() + "x\n")
        with self.assertRaises(ValueError):
            runner.run(self.config())
        self.assertEqual(self.raw(), [])

    def test_low_model_can_fall_back_to_the_default_effort(self):
        self.assertEqual(runner.run(self.config(low_model_default_effort=True)), 0)
        for record, call in zip(self.raw(), self.fake_calls()):
            if record["model"] == "claude-low":
                self.assertNotIn("--effort", call["argv"])
            elif record["model"] == "claude-high":
                self.assertIn("--effort", call["argv"])


class Failures(Base):
    def first_calls(self, count=1):
        return [r["call_id"] for r in self.rows()[:count]]

    def test_parse_failure_is_kept_without_retry(self):
        self.scenarios(["json_error"])
        self.assertEqual(runner.run(self.config()), 0)
        raw = self.raw()
        self.assertEqual(len(raw), 6)
        self.assertEqual(raw[0]["response_text"], "not json at all")
        self.assertIsNone(raw[0]["error_kind"])

    def test_tool_call_retries_the_same_prompt_and_is_kept(self):
        self.scenarios(["tool_call"])
        self.assertEqual(runner.run(self.config()), 0)
        raw = self.raw()
        self.assertEqual(len(raw), 7)
        self.assertEqual(raw[0]["call_id"], raw[1]["call_id"])
        self.assertEqual(raw[0]["prompt_sha256"], raw[1]["prompt_sha256"])
        self.assertEqual((raw[0]["tool_violations"], raw[1]["attempt"], raw[1]["attempt_kind"]), (1, 2, "retry"))
        calls = self.fake_calls()
        self.assertEqual(calls[0]["stdin_sha256"], calls[1]["stdin_sha256"])

    def test_tool_call_twice_on_one_model_aborts(self):
        rows = self.rows()
        low = [i for i, r in enumerate(rows) if r["model"] == "claude-low"]
        plan = ["normal"] * 7
        # Earlier low model call, then its retry succeeds, then the next low call: index moves by one.
        plan[low[0]] = "tool_call"
        plan[low[1] + 1] = "tool_call"
        self.scenarios(plan)
        self.assertEqual(runner.run(self.config()), 3)
        abort = [e for e in self.log() if e["kind"] == "abort"]
        self.assertEqual(abort[0]["reason"], "tool_violation_twice")

    def test_api_failures_retry_defer_then_abort(self):
        for scenario in ("api_error", "timeout", "crash", "no_result"):
            with self.subTest(scenario=scenario):
                shutil.rmtree(self.data / "raw", ignore_errors=True)
                self.fake_log.unlink(missing_ok=True)
                self.scenarios([scenario, scenario])
                self.assertEqual(runner.run(self.config(timeout=1 if scenario == "timeout" else 10)), 0)
                raw = self.raw()
                self.assertEqual(len(raw), 8)
                self.assertEqual(raw[0]["call_id"], raw[1]["call_id"])
                self.assertEqual(raw[-1]["call_id"], raw[0]["call_id"])
                self.assertEqual(raw[-1]["attempt_kind"], "deferred")
                self.assertEqual(raw[-1]["attempt"], 3)
                self.assertIsNone(raw[-1]["error_kind"])
                self.assertIsNone(raw[0]["response_text"])

    def test_third_failure_aborts_and_keeps_failed_lines(self):
        plan = ["api_error", "api_error"] + ["normal"] * 5 + ["api_error"]
        self.scenarios(plan)
        self.assertEqual(runner.run(self.config()), 3)
        raw = self.raw()
        self.assertEqual(len([r for r in raw if r["error_kind"] == "api_error"]), 3)
        self.assertEqual(self.log()[-1]["reason"], "api_failure_exhausted")

    def test_retry_cap_aborts_before_the_extra_call(self):
        # Pilot cap 2: three calls each need a retry.
        self.scenarios(["api_error", "normal", "api_error", "normal", "api_error"])
        self.assertEqual(runner.run(self.config()), 3)
        self.assertEqual(self.log()[-1]["reason"], "retry_limit")
        self.assertEqual(len(self.raw()), 5)
        self.assertEqual(sum(1 for r in self.raw() if r["attempt"] > 1), 2)

    def test_usage_events_abort_at_once(self):
        for scenario in ("rejected", "http_429", "usage_message"):
            with self.subTest(scenario=scenario):
                shutil.rmtree(self.data / "raw", ignore_errors=True)
                self.fake_log.unlink(missing_ok=True)
                self.scenarios([scenario])
                self.assertEqual(runner.run(self.config()), 3)
                raw = self.raw()
                self.assertEqual(len(raw), 1)
                self.assertEqual(raw[0]["error_kind"], "rate_limited")
                self.assertEqual(self.log()[-1]["reason"], "usage_limit")

    def test_warning_keeps_the_response_and_stops(self):
        first = next(i for i, r in enumerate(self.rows()) if MODELS[r["model"]]["cli"] == "claude")
        self.scenarios(["normal"] * first + ["allowed_warning"])
        self.assertEqual(runner.run(self.config()), 3)
        raw = self.raw()
        self.assertEqual(len(raw), first + 1)
        self.assertIsNone(raw[-1]["error_kind"])
        self.assertTrue(raw[-1]["response_text"])
        self.assertEqual(self.log()[-1]["reason"], "allowed_warning")

    def test_a_resume_on_an_unread_window_needs_a_named_person(self):
        self.assertEqual(runner.approve(self.data, "L2", [], APPROVED), 0)
        rows = self.rows("L2")
        self.scenarios(["api_error", "api_error"] + ["normal"] * (len(rows) - 1) + ["api_error"])
        self.assertEqual(runner.run(self.config("main", "L2")), 3)
        usage = {"proposal": "L1", "needs_decision": True}
        self.assertEqual(runner.resume(self.data, "main", "L2", usage), 1)
        self.assertEqual(self.log()[-1]["kind"], "resume_refused")
        stopped = {"proposal": "stop", "needs_decision": True}
        self.assertEqual(runner.resume(self.data, "main", "L2", stopped, "someone"), 1)
        self.assertEqual(runner.resume(self.data, "main", "L2", usage, "someone"), 0)

    def test_a_rejected_call_that_hangs_stops_the_run(self):
        first = next(i for i, r in enumerate(self.rows()) if MODELS[r["model"]]["cli"] == "claude")
        self.scenarios(["normal"] * first + ["rejected_then_hang"])
        self.assertEqual(runner.run(self.config(timeout=1)), 3)
        self.assertEqual(self.raw()[-1]["error_kind"], "rate_limited")
        self.assertEqual(self.log()[-1]["reason"], "usage_limit")
        self.assertEqual(len(self.raw()), first + 1)

    def test_a_warning_on_a_failed_call_stops_without_a_retry(self):
        first = next(i for i, r in enumerate(self.rows()) if MODELS[r["model"]]["cli"] == "claude")
        self.scenarios(["normal"] * first + ["warning_then_error"])
        self.assertEqual(runner.run(self.config()), 3)
        self.assertEqual(self.raw()[-1]["error_kind"], "api_error")
        self.assertEqual(self.log()[-1]["reason"], "allowed_warning")
        self.assertEqual(len(self.raw()), first + 1)

    def test_an_abort_stays_until_a_resume_and_is_logged_once(self):
        self.scenarios(["http_429"])
        self.assertEqual(runner.run(self.config()), 3)
        self.assertEqual(runner.run(self.config()), 3)
        self.assertEqual(len(self.raw()), 1)
        self.assertEqual(len([e for e in self.log() if e["kind"] == "abort"]), 1)

    def test_stop_and_continue_has_no_duplicates(self):
        self.assertEqual(runner.run(self.config(max_calls=2)), 0)
        self.assertEqual(len(self.raw()), 2)
        self.assertEqual(runner.run(self.config(max_calls=0)), 0)
        self.assertEqual(len(self.raw()), 2)
        self.assertEqual(runner.run(self.config()), 0)
        raw = self.raw()
        self.assertEqual(len(raw), 6)
        self.assertEqual(len({r["call_id"] for r in raw}), 6)
        self.assertEqual({r["attempt"] for r in raw}, {1})

    def test_retry_count_survives_a_restart(self):
        self.scenarios(["api_error", "normal", "api_error"])
        self.assertEqual(runner.run(self.config(max_calls=3)), 0)
        self.scenarios(["api_error", "normal", "api_error", "normal", "api_error"])
        self.assertEqual(runner.run(self.config()), 3)
        self.assertEqual(self.log()[-1]["reason"], "retry_limit")

    def test_abort_without_a_log_line_is_found_again(self):
        self.scenarios(["http_429"])
        self.assertEqual(runner.run(self.config()), 3)
        runner.log_path(self.data).unlink()
        self.assertEqual(runner.run(self.config()), 3)
        self.assertEqual(self.log()[-1]["reason"], "usage_limit")


class Levels(Base):
    def test_main_needs_an_approved_level(self):
        self.assertEqual(runner.run(self.config("main", "L2")), 2)
        self.assertEqual(self.raw("main"), [])
        self.assertEqual(runner.approve(self.data, "L1", [], APPROVED), 0)
        self.assertEqual(runner.run(self.config("main", "L2")), 2)

    def test_approved_u_has_to_guard_the_run(self):
        bad = (
            {}, [], None, {"codex": None}, {"claude": 50.0}, {"claude": None, "codex": None},
            {"claude": True, "codex": None}, {"claude": "50", "codex": None}, {"claude": 50.0, "codex": "x"},
            {"claude": float("nan"), "codex": None}, {"claude": 50.0, "codex": None, "seven_day_fable": None},
        )
        for value in bad:
            with self.subTest(value=value):
                self.assertEqual(runner.approve(self.data, "L2", [], value), 2)
                self.assertEqual(self.log(), [])
        self.assertEqual(runner.approve(self.data, "L2", [], {"claude": 50, "codex": 3.0, "seven_day_fable": 8}), 0)
        self.assertEqual(len(self.log()), 1)

    def test_the_command_line_refuses_empty_approved_u(self):
        windows = self.root / "windows.json"
        windows.write_text("[]", encoding="utf-8")
        argv = ["--data", str(self.data), "approve", "--level", "L2", "--windows", str(windows), "--approved-u", "{}"]
        self.assertEqual(runner.main(argv), 2)
        self.assertEqual(self.log(), [])

    def test_level_log_is_separate_and_holds_the_values(self):
        windows = [{"name": "w", "u": 1.0}]
        self.assertEqual(runner.approve(self.data, "L2", windows, {"claude": 50.0, "codex": None}), 0)
        self.assertEqual(runner.run(self.config("main", "L2", max_calls=1)), 0)
        entry = self.log()[0]
        self.assertEqual((entry["kind"], entry["from"], entry["to"], entry["windows"]), ("level_change", None, "L2", windows))
        self.assertTrue(entry["at"])
        self.assertTrue(all("kind" not in r for r in self.raw("main")))

    def test_lowering_the_level_keeps_finished_calls_and_skips_the_rest(self):
        self.assertEqual(runner.approve(self.data, "L0", [], APPROVED), 0)
        self.assertEqual(runner.run(self.config("main", "L0", max_calls=10)), 0)
        before = {r["call_id"] for r in self.raw("main")}
        self.assertEqual(runner.approve(self.data, "L2", [], APPROVED), 0)
        self.assertEqual(runner.run(self.config("main", "L2")), 0)
        raw = self.raw("main")
        planned = {r["call_id"] for r in self.rows("L2")}
        new = [r["call_id"] for r in raw[10:]]
        self.assertEqual(set(new), planned - before)
        self.assertEqual(len(new), len(set(new)))
        out = summarize.summarize(self.data)
        self.assertEqual(out["approved_level"], "L2")
        self.assertEqual(out["unfilled_cells"], [])

    def test_a_level_cannot_change_after_its_last_call(self):
        self.assertEqual(runner.approve(self.data, "L2", [], APPROVED), 0)
        self.assertEqual(runner.run(self.config("main", "L2")), 0)
        self.assertEqual(runner.approve(self.data, "L1", [], APPROVED), 1)
        self.assertEqual(len([e for e in self.log() if e["kind"] == "level_change"]), 1)

    def test_seven_day_rise_aborts(self):
        self.assertEqual(runner.approve(self.data, "L2", [], {"claude": 70.0, "codex": None}), 0)
        rows = self.rows("L2")
        first_claude = next(i for i, r in enumerate(rows) if MODELS[r["model"]]["cli"] == "claude")
        self.scenarios(["normal"] * first_claude + ["high_usage"])
        self.assertEqual(runner.run(self.config("main", "L2")), 3)
        self.assertEqual(self.raw("main")[-1]["windows"]["seven_day_percent"], 90.0)
        self.assertEqual(self.log()[-1]["reason"], "u_exceeded")

    def test_percent_unit_is_not_scaled(self):
        self.assertEqual(runner.approve(self.data, "L2", [], APPROVED), 0)
        rows = self.rows("L2")
        first = next(i for i, r in enumerate(rows) if MODELS[r["model"]]["cli"] == "claude")
        self.assertEqual(runner.run(self.config("main", "L2", max_calls=first + 1, utilization_unit="percent")), 0)
        self.assertEqual(self.raw("main")[first]["windows"]["seven_day_percent"], 0.2)

    def test_resume_after_api_failures_records_values_and_continues(self):
        self.assertEqual(runner.approve(self.data, "L2", [], APPROVED), 0)
        rows = self.rows("L2")
        self.scenarios(["api_error", "api_error"] + ["normal"] * (len(rows) - 1) + ["api_error"])
        self.assertEqual(runner.run(self.config("main", "L2")), 3)
        self.assertEqual(runner.resume(self.data, "main", "L2", {"proposal": "L1"}), 0)
        entry = self.log()[-1]
        self.assertEqual((entry["kind"], entry["abort_reason"], entry["retries_used"]), ("resume", "api_failure_exhausted", 2))
        self.assertTrue(entry["at"])
        self.assertEqual(runner.run(self.config("main", "L2")), 0)
        last = self.raw("main")[-1]
        self.assertEqual((last["attempt"], last["attempt_kind"], last["error_kind"]), (4, "retry", None))

    def test_pilot_cap_blocks_a_resume_when_no_retry_is_left(self):
        self.scenarios(["api_error", "api_error"] + ["normal"] * 5 + ["api_error"])
        self.assertEqual(runner.run(self.config()), 3)
        self.assertEqual(runner.resume(self.data, "pilot", None, {"proposal": "L0"}), 1)
        self.assertEqual(self.log()[-1]["retries_used"], 2)

    def test_resume_refused_when_the_table_rejects_the_level(self):
        self.assertEqual(runner.approve(self.data, "L0", [], APPROVED), 0)
        rows = self.rows("L0")
        self.scenarios(["api_error", "api_error"] + ["normal"] * (len(rows) - 1) + ["api_error"])
        self.assertEqual(runner.run(self.config("main", "L0")), 3)
        self.assertEqual(runner.resume(self.data, "main", "L0", {"proposal": "L1"}), 1)
        entry = self.log()[-1]
        self.assertEqual((entry["kind"], entry["usage_allows"]), ("resume_refused", False))
        self.assertEqual(entry["usage"], {"proposal": "L1"})
        self.assertEqual(runner.run(self.config("main", "L0")), 3)

    def test_tool_violation_abort_refuses_every_resume(self):
        rows = self.rows()
        low = [i for i, r in enumerate(rows) if r["model"] == "claude-low"]
        plan = ["normal"] * 7
        plan[low[0]] = "tool_call"
        plan[low[1] + 1] = "tool_call"
        self.scenarios(plan)
        self.assertEqual(runner.run(self.config()), 3)
        self.assertEqual(runner.resume(self.data, "pilot", None, {"proposal": "L0"}, "someone"), 1)
        self.assertEqual(self.log()[-1]["kind"], "resume_refused")

    def test_usage_abort_resume_needs_an_approver(self):
        self.scenarios(["http_429"])
        self.assertEqual(runner.run(self.config()), 3)
        self.assertEqual(runner.resume(self.data, "pilot", None, {"proposal": "L0"}), 1)
        self.assertEqual(runner.resume(self.data, "pilot", None, {"proposal": "L0"}, "someone"), 0)
        self.assertEqual(runner.run(self.config()), 0)
        raw = self.raw()
        self.assertEqual(raw[1]["call_id"], raw[0]["call_id"])
        self.assertEqual(raw[1]["attempt"], 2)

    def test_pilot_and_main_logs_do_not_mix(self):
        self.scenarios(["http_429"])
        self.assertEqual(runner.run(self.config()), 3)
        self.assertEqual(runner.approve(self.data, "L2", [], APPROVED), 0)
        self.assertEqual(runner.run(self.config("main", "L2", max_calls=1)), 0)
        self.assertEqual(len(self.raw("main")), 1)


class CommandLine(Base):
    def fake_args(self, *extra):
        return [
            "--data", str(self.data), "run", "--claude-utilization-unit", "fraction",
            "--claude-bin", str(FAKE), "--codex-bin", str(FAKE), *extra,
        ]

    def test_run_records_the_cli_version_and_the_rules_version(self):
        with mock.patch.object(check_rules, "rules_provenance", return_value=PROVENANCE):
            self.assertEqual(runner.main(self.fake_args("--phase", "pilot", "--max-calls", "1")), 0)
        record = self.raw()[0]
        self.assertEqual(record["cli_version"], "fake 0.0")
        self.assertEqual(record["rules_commit"], PROVENANCE["rules_commit"])

    def test_rules_that_cannot_be_used_stop_before_any_call(self):
        with mock.patch.object(check_rules, "rules_provenance", side_effect=ValueError("no")):
            self.assertEqual(runner.main(self.fake_args("--phase", "pilot")), 2)
        self.assertEqual(self.fake_calls(), [])

    def test_a_missing_executable_cannot_be_judged(self):
        args = self.fake_args("--phase", "pilot")
        args[args.index("--codex-bin") + 1] = str(self.root / "missing")
        self.assertEqual(runner.main(args), 2)
        self.assertEqual(self.fake_calls(), [])

    def test_main_phase_needs_a_level(self):
        self.assertEqual(runner.main(self.fake_args("--phase", "main")), 2)

    def test_options_that_change_a_call_are_logged(self):
        with mock.patch.object(check_rules, "rules_provenance", return_value=PROVENANCE):
            args = self.fake_args("--phase", "pilot", "--max-calls", "0", "--low-model-default-effort")
            self.assertEqual(runner.main(args), 0)
        self.assertEqual(self.log()[-1]["kind"], "run_options")
        self.assertTrue(self.log()[-1]["low_model_default_effort"])

    def test_approve_and_resume_commands(self):
        windows = self.root / "windows.json"
        windows.write_text('[{"name": "w", "u": 1.0}]')
        usage = self.root / "usage.json"
        usage.write_text('{"proposal": "L0"}')
        base = ["--data", str(self.data)]
        args = [*base, "approve", "--level", "L2", "--windows", str(windows), "--approved-u", '{"claude": 50.0, "codex": null}']
        self.assertEqual(runner.main(args), 0)
        self.assertEqual(self.log()[-1]["approved_u"], {"claude": 50.0, "codex": None})
        args = [*base, "resume", "--phase", "main", "--level", "L2", "--usage", str(usage)]
        self.assertEqual(runner.main(args), 1)
        self.assertEqual(self.log()[-1]["kind"], "resume_refused")


class Bypass(Base):
    def git_repo(self):
        repo = self.root / "repo"
        repo.mkdir()
        for args in (["init", "-q"], ["config", "user.email", "t"], ["config", "user.name", "t"]):
            subprocess.run(["git", "-C", str(repo), *args], check=True)
        return repo

    def test_bypass_adds_the_read_only_line_and_keeps_the_scheduled_hash(self):
        repo = self.git_repo()
        self.assertEqual(runner.run(self.config(codex_bypass_sandbox=True, repo_root=repo)), 0)
        for record, call, row in zip(self.raw(), self.fake_calls(), self.rows()):
            self.assertEqual(record["prompt_sha256"], row["prompt_sha256"])
            if record["model"] == "codex":
                self.assertIn("--dangerously-bypass-approvals-and-sandbox", call["argv"])
                system, user = runner.load_prompt(self.data, row)
                sent = f"{runner.READ_ONLY_LINE}\n{system}\n\n{user}"
                self.assertEqual(call["stdin_sha256"], hashlib.sha256(sent.encode()).hexdigest())

    def test_a_changed_repository_aborts(self):
        repo = self.git_repo()
        original = runner.run_call

        def writes(cfg, row, attempt, kind):
            (repo / "written.txt").write_text("x")
            return original(cfg, row, attempt, kind)

        with mock.patch.object(runner, "run_call", writes):
            self.assertEqual(runner.run(self.config(codex_bypass_sandbox=True, repo_root=repo)), 3)
        self.assertEqual(self.log()[-1]["reason"], "repo_changed")
        self.assertEqual(runner.run(self.config(codex_bypass_sandbox=True, repo_root=repo)), 3)


class EndToEnd(Base):
    def test_generate_call_grade_and_summarize_with_no_model(self):
        self.assertEqual(runner.approve(self.data, "L2", [], APPROVED), 0)
        self.scenarios(["json_error", "tool_call", "api_error"])
        self.assertEqual(runner.run(self.config("main", "L2")), 0)
        calls = self.fake_calls()
        self.assertGreaterEqual(len(calls), 84)
        self.assertEqual(summarize.main(["--data", str(self.data)]), 0)
        self.assertEqual(summarize.main(["--data", str(self.data), "--check"]), 0)
        out = json.loads((self.data / "summary/summary.json").read_text())
        self.assertEqual(out["unfilled_cells"], [])
        self.assertEqual(out["parse_failures"], 1)
        self.assertEqual(out["calls_by_outcome"]["tool_violation"], 1)
        self.assertEqual(out["calls_by_outcome"]["api_error"], 1)
        self.assertEqual(out["calls_recorded"], 86)


class Parsers(unittest.TestCase):
    def lines(self, *events):
        return "\n".join(json.dumps(e) for e in events)

    def test_claude_items_outside_the_allowed_kinds_count(self):
        out = parse_claude(self.lines(
            {"type": "assistant", "message": {"model": "m", "content": [{"type": "thinking"}, {"type": "redacted_thinking"}, {"type": "text"}, {"type": "tool_use"}, {"type": "server_tool_use"}]}},
            {"type": "result", "subtype": "success", "is_error": False, "result": "{}", "usage": {"input_tokens": 1}},
        ), 0)
        self.assertEqual(out["tool_violations"], 2)
        self.assertEqual(out["model_reported"], "m")

    def test_broken_lines_are_skipped(self):
        out = parse_claude("not json\n" + self.lines({"type": "result", "subtype": "success", "is_error": False, "result": "{}"}), 0)
        self.assertEqual(out["response_text"], "{}")

    def test_success_with_a_failing_exit_is_an_api_failure(self):
        out = parse_claude(self.lines({"type": "result", "subtype": "success", "is_error": False, "result": "{}"}), 1)
        self.assertEqual(out["error_kind"], "api_error")

    def test_worst_window_status_stays(self):
        out = parse_claude(self.lines(
            {"type": "rate_limit_event", "rate_limit_info": {"status": "allowed_warning", "rateLimitType": "seven_day", "utilization": 0.5}},
            {"type": "rate_limit_event", "rate_limit_info": {"status": "allowed", "rateLimitType": "five_hour", "utilization": 0.25}},
            {"type": "result", "subtype": "success", "is_error": False, "result": "{}"},
        ), 0)
        self.assertEqual(out["windows"]["status"], "allowed_warning")
        self.assertEqual(out["windows"]["seven_day_percent"], 50.0)
        self.assertEqual(out["windows"]["by_type"]["five_hour"]["utilization_percent"], 25.0)

    def test_codex_keeps_the_last_message_and_flags_other_items(self):
        out = parse_codex(self.lines(
            {"type": "item.completed", "item": {"type": "agent_message", "text": "first"}},
            {"type": "item.completed", "item": {"type": "mcp_tool_call"}},
            {"type": "item.completed", "item": {"type": "agent_message", "text": "{}"}},
            {"type": "turn.completed", "usage": {"input_tokens": 5, "cached_input_tokens": 1, "output_tokens": 2, "other": 9}},
        ), 0)
        self.assertEqual((out["response_text"], out["tool_violations"]), ("{}", 1))
        self.assertEqual(out["usage"], {"input_tokens": 5, "cached_input_tokens": 1, "output_tokens": 2})

    def test_codex_without_a_finished_turn_fails(self):
        out = parse_codex(self.lines({"type": "item.completed", "item": {"type": "agent_message", "text": "{}"}}), 0)
        self.assertEqual(out["error_kind"], "api_error")

    LIMIT_MESSAGES = (
        "You've hit your weekly limit",
        "You've hit your usage limit",
        "You've reached your Fable limit.",
        "You're out of extra usage",
        "You're out of usage credits",
        "You hit your spend cap",
        "Spend cap exceeded",
        "429 Too Many Requests",
        "Rate limit exceeded",
    )
    OTHER_MESSAGES = ("Internal server error", "Connection reset by peer", "Overloaded", "invalid request")

    def claude_failure(self, message):
        event = {"type": "result", "subtype": "error_during_execution", "is_error": True, "result": message}
        return parse_claude(json.dumps(event), 1)

    def codex_failure(self, message):
        return parse_codex(json.dumps({"type": "turn.failed", "error": {"message": message}}), 1)

    def test_limit_wordings_stop_as_usage_events(self):
        for message in self.LIMIT_MESSAGES:
            with self.subTest(message=message):
                self.assertEqual(self.claude_failure(message)["error_kind"], "rate_limited")
                self.assertEqual(self.codex_failure(message)["error_kind"], "rate_limited")

    def test_other_errors_stay_api_errors(self):
        for message in self.OTHER_MESSAGES:
            with self.subTest(message=message):
                self.assertEqual(self.claude_failure(message)["error_kind"], "api_error")
                self.assertEqual(self.codex_failure(message)["error_kind"], "api_error")

    def test_codex_limit_text_is_a_usage_event_and_is_not_kept(self):
        out = parse_codex(self.lines({"type": "turn.failed", "error": {"message": "Rate limit at /private"}}), 1)
        self.assertEqual(out["error_kind"], "rate_limited")
        self.assertNotIn("/private", json.dumps(out))


class Provenance(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.repo = Path(self.tmp.name)
        self.rules = self.repo / RULES_REL
        self.rules.parent.mkdir(parents=True)
        self.rules.write_bytes((REPO_ROOT / RULES_REL).read_bytes())
        self.git("init", "-q", "-b", "main")
        self.git("config", "user.email", "t")
        self.git("config", "user.name", "t")
        self.git("add", ".")
        self.git("commit", "-qm", "rules")

    def git(self, *args):
        return subprocess.run(["git", "-C", str(self.repo), *args], check=True, capture_output=True, text=True).stdout.strip()

    def test_clean_rules_on_main_pass(self):
        out = check_rules.rules_provenance(self.repo, "main")
        self.assertEqual(out["rules_commit"], self.git("rev-parse", "HEAD"))
        self.assertEqual(out["rules_blob"], self.git("rev-parse", f"HEAD:{RULES_REL}"))

    def test_uncommitted_rules_are_refused(self):
        self.rules.write_bytes(self.rules.read_bytes() + b"\nextra\n")
        with self.assertRaises(ValueError):
            check_rules.rules_provenance(self.repo, "main")

    def test_a_rules_commit_not_on_main_is_refused(self):
        self.git("checkout", "-qb", "side")
        self.rules.write_bytes(self.rules.read_bytes() + b"\nextra\n")
        self.git("commit", "-qam", "side")
        with self.assertRaises(ValueError):
            check_rules.rules_provenance(self.repo, "main")

    def test_a_changed_decision_section_is_refused(self):
        self.rules.write_bytes(self.rules.read_bytes().replace(b"1e-12", b"1e-9"))
        self.git("commit", "-qam", "change")
        with self.assertRaises(ValueError):
            check_rules.rules_provenance(self.repo, "main")

    def row(self, **extra):
        out = check_rules.rules_provenance(self.repo, "main")
        return {**out, "started_at": "2999-01-01T00:00:00.000000Z", **extra}

    def problems(self, rows):
        return [kind for kind, _ in check_rules.raw_problems(rows, self.repo, "main")]

    def test_raw_lines_with_a_good_version_pass(self):
        self.assertEqual(self.problems([self.row()]), [])

    def test_raw_file_outside_the_repository_has_no_history_to_compare(self):
        with tempfile.TemporaryDirectory() as other:
            found = check_rules.raw_problems([self.row()], self.repo, "main", [str(Path(other) / "main.jsonl")])
        self.assertEqual(found, [])

    def test_tracked_raw_file_that_descends_from_the_rules_passes(self):
        raw = self.repo / "main.jsonl"
        raw.write_text("{}\n")
        self.git("add", ".")
        self.git("commit", "-qm", "raw")
        self.assertEqual(check_rules.raw_problems([self.row()], self.repo, "main", [str(raw)]), [])

    def test_raw_line_problems_are_named(self):
        self.assertEqual(self.problems([self.row(rules_blob="0" * 40)]), ["blob_differs"])
        self.assertEqual(self.problems([self.row(rules_commit=None)]), ["missing_rules_fields"])
        self.assertEqual(self.problems([self.row(rules_commit="1" * 40)]), ["unknown_rules_commit"])
        self.assertEqual(self.problems([self.row(started_at="2001-01-01T00:00:00.000000Z")]), ["rules_committed_after_first_call"])


if __name__ == "__main__":
    unittest.main()
