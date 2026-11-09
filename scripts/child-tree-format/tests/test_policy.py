"""Retry, abort and resume rules, and the level proposal, without running anything."""

import sys
import unittest

import helpers  # noqa: F401

sys.dont_write_bytecode = True

import policy
import usage_level as ul


def raw(call="a", model="claude-low", error=None, tools=0, status=None, seven=None):
    return {
        "call_id": call, "model": model, "error_kind": error, "tool_violations": tools,
        "windows": {"status": status, "seven_day_percent": seven},
    }


PLAN = ["a", "b", "c"]


def step(records, log=(), cap=5, plan=PLAN):
    return policy.next_action(policy.replay(records, log), plan, cap)


class Schedule(unittest.TestCase):
    def test_plan_runs_in_order_and_ends(self):
        self.assertEqual(step([]), ("run", ("a", 1, "first")))
        self.assertEqual(step([raw("a")]), ("run", ("b", 1, "first")))
        self.assertEqual(step([raw("a"), raw("b"), raw("c")]), ("done", None))

    def test_parse_failure_is_a_valid_call_without_retry(self):
        # The record has a response text that does not parse, which is not an error kind.
        self.assertEqual(step([raw("a")]), ("run", ("b", 1, "first")))

    def test_first_api_failure_retries_at_once(self):
        for error in ("api_error", "timeout"):
            with self.subTest(error=error):
                self.assertEqual(step([raw("a", error=error)]), ("run", ("a", 2, "retry")))

    def test_second_failure_waits_for_the_end_of_the_schedule(self):
        records = [raw("a", error="api_error"), raw("a", error="timeout")]
        self.assertEqual(step(records), ("run", ("b", 1, "first")))
        records += [raw("b"), raw("c")]
        self.assertEqual(step(records), ("run", ("a", 3, "deferred")))

    def test_deferred_success_finishes(self):
        records = [raw("a", error="api_error"), raw("a", error="api_error"), raw("b"), raw("c"), raw("a")]
        self.assertEqual(step(records), ("done", None))

    def test_third_failure_aborts(self):
        records = [raw("a", error="api_error")] * 3
        self.assertEqual(step(records), ("abort", "api_failure_exhausted"))

    def test_violation_retries_then_second_on_the_same_model_aborts(self):
        self.assertEqual(step([raw("a", tools=1)]), ("run", ("a", 2, "retry")))
        self.assertEqual(step([raw("a", tools=1), raw("a"), raw("b", tools=1)]), ("abort", "tool_violation_twice"))

    def test_violations_on_different_models_do_not_add_up(self):
        records = [raw("a", tools=1), raw("a"), raw("b", model="codex", tools=1)]
        self.assertEqual(step(records), ("run", ("b", 2, "retry")))

    def test_violation_retry_ending_in_api_failure_follows_the_api_path(self):
        records = [raw("a", tools=1), raw("a", error="api_error")]
        self.assertEqual(step(records), ("run", ("a", 3, "retry")))
        self.assertEqual(policy.replay(records).api_fails, {"a": 1})

    def test_usage_events_abort_without_retry(self):
        self.assertEqual(step([raw("a", error="rate_limited")]), ("abort", "usage_limit"))

    def test_warning_keeps_the_call_and_aborts(self):
        ledger = policy.replay([raw("a", status="allowed_warning")])
        self.assertIn("a", ledger.done)
        self.assertEqual(ledger.abort, "allowed_warning")

    def test_usage_signals_stop_whatever_else_the_call_did(self):
        cases = [
            (raw("a", error="api_error", status="allowed_warning"), "allowed_warning"),
            (raw("a", error="timeout", status="allowed_warning"), "allowed_warning"),
            (raw("a", tools=1, status="allowed_warning"), "allowed_warning"),
            (raw("a", error="timeout", status="rejected"), "usage_limit"),
            (raw("a", error="api_error", status="rejected"), "usage_limit"),
            (raw("a", tools=1, status="rejected"), "usage_limit"),
            (raw("a", error="rate_limited", tools=1), "usage_limit"),
            (raw("a", status="rejected"), "usage_limit"),
        ]
        for record, want in cases:
            with self.subTest(record=record):
                self.assertEqual(step([record]), ("abort", want))

    def test_a_usage_stop_replaces_an_api_stop_because_it_asks_more_of_a_resume(self):
        records = [raw("a", error="api_error"), raw("a", error="api_error"), raw("a", error="api_error", status="allowed_warning")]
        self.assertEqual(step(records), ("abort", "allowed_warning"))

    def test_a_second_violation_keeps_its_stop_under_a_usage_signal(self):
        records = [raw("a", tools=1), raw("a"), raw("b", tools=1, status="rejected")]
        self.assertEqual(step(records), ("abort", "tool_violation_twice"))
        self.assertFalse(policy.resume_decision(policy.replay(records), 5, True, "someone")[0])

    def test_every_run_after_the_first_counts_against_the_cap(self):
        records = [raw("a", tools=1), raw("a"), raw("b", error="api_error")]
        self.assertEqual(policy.replay(records).retries_used, 1)
        self.assertEqual(step(records, cap=1), ("abort", "retry_limit"))
        self.assertEqual(step(records, cap=2), ("run", ("b", 2, "retry")))

    def test_a_first_run_needs_no_cap(self):
        self.assertEqual(step([], cap=0), ("run", ("a", 1, "first")))

    def test_seven_day_margin_is_inclusive(self):
        log = [{"kind": "level_change", "to": "L0", "approved_u": {"claude": 70.0}, "calls_seen": 0}]
        self.assertEqual(policy.replay([raw("a", seven=74.9)], log).abort, None)
        self.assertEqual(policy.replay([raw("a", seven=75.0)], log).abort, "u_exceeded")
        codex = raw("a", model="codex", seven=99.0)
        self.assertEqual(policy.replay([codex], log).abort, None)

    def test_a_level_change_can_drop_a_call_from_the_plan(self):
        self.assertEqual(step([raw("x", error="api_error")], plan=["a"]), ("run", ("a", 1, "first")))


class Resume(unittest.TestCase):
    def decide(self, records, usage=True, by=None, cap=5):
        return policy.resume_decision(policy.replay(records), cap, usage, by)[0]

    def test_tool_violation_never_resumes(self):
        records = [raw("a", tools=1), raw("b", tools=1)]
        self.assertFalse(self.decide(records, by="someone"))

    def test_api_failure_resumes_when_usage_and_cap_allow(self):
        records = [raw("a", error="api_error")] * 3
        self.assertTrue(self.decide(records))
        self.assertFalse(self.decide(records, usage=False))
        self.assertFalse(self.decide(records, cap=2))

    def test_usage_abort_needs_a_person_and_usage(self):
        records = [raw("a", error="rate_limited")]
        self.assertFalse(self.decide(records))
        self.assertFalse(self.decide(records, usage=False, by="someone"))
        self.assertTrue(self.decide(records, by="someone"))

    def test_retry_limit_does_not_resume(self):
        ledger = policy.replay([], [{"kind": "abort", "reason": "retry_limit", "calls_seen": 0}])
        self.assertFalse(policy.resume_decision(ledger, 5, True, "someone")[0])

    def test_nothing_to_resume(self):
        self.assertFalse(self.decide([raw("a")]))

    def test_counts_carry_over_and_a_second_failure_aborts_again(self):
        records = [raw("a", error="api_error")] * 3
        log = [{"kind": "resume", "calls_seen": 3}]
        self.assertEqual(step(records, log), ("run", ("a", 4, "retry")))
        again = [*records, raw("a", error="api_error")]
        self.assertEqual(step(again, log), ("abort", "api_failure_exhausted"))
        self.assertEqual(policy.replay(again, log).retries_used, 3)

    def test_resume_keeps_the_violation_count(self):
        records = [raw("a", tools=1), raw("a"), raw("b", error="rate_limited")]
        log = [{"kind": "resume", "calls_seen": 3}]
        again = [*records, raw("b", tools=1)]
        self.assertEqual(step(again, log), ("abort", "tool_violation_twice"))

    def test_an_abort_after_the_resume_point_is_not_cleared(self):
        log = [{"kind": "resume", "calls_seen": 0}]
        self.assertEqual(step([raw("a", error="rate_limited")], log), ("abort", "usage_limit"))


def window(name="w", span="7d", u=50.0, w=1000.0, status="allowed", models=("claude-low",)):
    return {"name": name, "span": span, "u": u, "w": w, "status": status, "models": list(models)}


# The low model runs 54 calls at L0, 54 at L1 and 36 at L2.
def means_for(share_at_l0, w=1000.0):
    return {"claude-low": share_at_l0 * w / 54, "claude-high": 0.0}


class Levels(unittest.TestCase):
    def test_planned_counts(self):
        self.assertEqual(ul.planned_by_model("L0"), {"claude-high": 54, "claude-low": 54, "codex": 54})
        self.assertEqual(ul.planned_by_model("L1"), {"claude-high": 36, "claude-low": 54, "codex": 36})
        self.assertEqual(ul.planned_by_model("L2"), {"claude-high": 24, "claude-low": 36, "codex": 24})

    def test_worked_example_from_the_design_review(self):
        # P/W is 0.7 percent at L0, so 69.6 grows to 70.087 and only L1 fits.
        means = {"claude-high": 7.0 / 54}
        one = window(u=69.6, w=1000.0, models=("claude-high",))
        self.assertAlmostEqual(ul.after_run(one, {**means}, "L0"), 70.0872, places=3)
        self.assertAlmostEqual(ul.after_run(one, means, "L1"), 69.6 + 69.6 * (7.0 * 36 / 54) / 1000.0, places=6)
        self.assertEqual(ul.select_level([one], means)["proposal"], "L1")

    def test_boundaries_are_inclusive(self):
        means = {"claude-low": 0.0}
        for u, want in ((70.0, "L0"), (70.01, "L1"), (85.0, "L1"), (85.01, "L2"), (95.0, "L2"), (95.01, "stop")):
            with self.subTest(u=u):
                self.assertEqual(ul.select_level([window(u=u)], means)["proposal"], want)

    def test_every_seven_day_window_must_fit(self):
        means = {"claude-low": 0.0}
        windows = [window("a", u=10.0), window("b", u=90.0)]
        self.assertEqual(ul.select_level(windows, means)["proposal"], "L2")

    def test_small_u_counts_as_one(self):
        means = means_for(1.0)
        self.assertAlmostEqual(ul.after_run(window(u=0.2), means, "L0"), 2.0)

    def test_status_comes_first(self):
        for status in ("allowed_warning", "rejected"):
            with self.subTest(status=status):
                self.assertEqual(ul.select_level([window(u=1.0, status=status)], {"claude-low": 0.0})["proposal"], "stop")
        five = window("f", span="5h", u=1.0, status="rejected")
        self.assertEqual(ul.select_level([window(u=1.0), five], {"claude-low": 0.0})["proposal"], "stop")

    def test_unread_u_uses_the_share(self):
        small = ul.select_level([window(u=None)], means_for(0.02))
        self.assertEqual((small["proposal"], small["needs_decision"]), ("L0", True))
        big = ul.select_level([window(u=None)], means_for(0.021))
        self.assertEqual((big["proposal"], big["needs_decision"]), ("L2", True))

    def test_zero_w_proposes_the_smallest_level(self):
        result = ul.select_level([window(u=10.0, w=0)], {"claude-low": 5.0})
        self.assertEqual((result["proposal"], result["needs_decision"]), ("L2", True))

    def test_five_hour_window_waits_and_never_lowers_the_level(self):
        means = {"claude-low": 0.0}
        result = ul.select_level([window(u=10.0), window("f", span="5h", u=71.0)], means)
        self.assertEqual((result["proposal"], result["wait_for"]), ("L0", ["f"]))
        result = ul.select_level([window(u=80.0), window("f", span="5h", u=71.0)], means)
        self.assertEqual((result["proposal"], result["wait_for"]), ("L1", []))

    def test_five_hour_zero_w_keeps_u(self):
        result = ul.select_level([window(u=10.0), window("f", span="5h", u=60.0, w=0)], {"claude-low": 9.0})
        self.assertEqual(result["values"]["five_hour"]["f"], 60.0)
        self.assertEqual(result["wait_for"], [])

    def test_no_seven_day_window_cannot_be_judged(self):
        with self.assertRaises(ValueError):
            ul.select_level([window(span="5h")], {"claude-low": 0.0})

    def test_pilot_means_leave_out_cache_reads_and_failed_calls(self):
        usage = {"input_tokens": 1, "cache_creation_input_tokens": 2, "cache_read_input_tokens": 1000, "output_tokens": 4}
        rows = [
            {"model": "claude-low", "usage": usage, "error_kind": None, "tool_violations": 0},
            {"model": "claude-low", "usage": usage, "error_kind": "api_error", "tool_violations": 0},
            {"model": "codex", "usage": {"input_tokens": 5, "cached_input_tokens": 3, "output_tokens": 5}, "error_kind": None, "tool_violations": 0},
        ]
        self.assertEqual(ul.pilot_means(rows), {"claude-low": 7, "codex": 7})

    def mixed_windows(self, claude_u):
        claude = window("claude_7d", u=claude_u, w=1e8, models=("claude-high", "claude-low"))
        return [claude, window("codex_7d", u=None, w=1e9, models=("codex",))]

    def test_an_unread_window_does_not_hide_a_read_one(self):
        means = {"claude-high": 5000.0, "claude-low": 5000.0, "codex": 20000.0}
        for claude_u, want in ((10.0, "L0"), (80.0, "L1"), (90.0, "L2"), (97.0, "stop")):
            with self.subTest(u=claude_u):
                self.assertEqual(ul.select_level(self.mixed_windows(claude_u), means)["proposal"], want)

    def test_an_unread_window_can_lower_a_read_one(self):
        means = {"claude-high": 5000.0, "claude-low": 5000.0, "codex": 20000.0}
        windows = self.mixed_windows(10.0)
        windows[1]["w"] = 0
        result = ul.select_level(windows, means)
        self.assertEqual((result["proposal"], result["needs_decision"]), ("L2", True))
        self.assertIn("claude_7d", result["values"]["L0"])
        self.assertNotIn("codex_7d", result["values"]["L0"])

    def test_a_read_window_keeps_its_level_when_the_unread_one_proposes_more(self):
        means = {"claude-high": 5000.0, "claude-low": 5000.0, "codex": 20000.0}
        result = ul.select_level(self.mixed_windows(80.0), means)
        self.assertEqual((result["proposal"], result["needs_decision"]), ("L1", True))

    def test_a_proposal_on_an_unread_window_needs_a_person(self):
        result = {"proposal": "L1", "needs_decision": True}
        self.assertFalse(ul.level_allowed("L1", result))
        self.assertTrue(ul.level_allowed("L1", result, "someone"))
        self.assertFalse(ul.level_allowed("L0", result, "someone"))
        self.assertFalse(ul.level_allowed("L1", {"proposal": "stop", "needs_decision": True}, "someone"))

    def test_level_allowed_accepts_smaller_levels(self):
        self.assertTrue(ul.level_allowed("L1", {"proposal": "L0"}))
        self.assertTrue(ul.level_allowed("L1", {"proposal": "L1"}))
        self.assertFalse(ul.level_allowed("L0", {"proposal": "L1"}))
        self.assertFalse(ul.level_allowed("L2", {"proposal": "stop"}))


if __name__ == "__main__":
    unittest.main()
