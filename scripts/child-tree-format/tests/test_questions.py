"""Answers are compared with values worked out by hand from the drawing in helpers.py."""

import sys
import unittest

import helpers
from helpers import node

sys.dont_write_bytecode = True

from questions import (
    blocked_nodes,
    build_questions,
    cycle_answer,
    cycle_nodes,
    question_11,
    runnable_nodes,
    silent_minutes,
    silent_nodes,
)
from common import descendants, siblings


class HandTree(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.questions, cls.answers, cls.notes = build_questions("hand", helpers.HAND_TREE)
        cls.params = {q["id"]: q["params"] for q in cls.questions}

    def test_silent_threshold(self):
        # Ages of active or stale nodes: 65, 15, 55, 5 minutes. The median is 35.
        self.assertEqual(self.notes["silent_minutes"], 35)
        self.assertEqual(self.answers["q1"], ["n100", "n310"])

    def test_filters(self):
        self.assertEqual(self.answers["q2"], ["n210"])
        want = {"n100": ["n400"], "n200": ["n400"]}
        self.assertEqual(self.answers["q3"], want[self.params["q3"]["a"]])

    def test_lookups(self):
        cwd = {"lane-a": "/work/repo-a/wt-00", "lane-b": "/work/repo-a/wt-00", "lane-c": "/work/repo-a/wt-00", "lane-d": "/work/repo-a/wt-10", "lane-e": "/work/repo-a/wt-10", "lane-f": "/work/repo-a/wt-10"}
        self.assertEqual(self.answers["q4"], cwd[self.params["q4"]["nick"]])
        parent = {"n300": "n200", "n310": "n200", "n400": "n300"}
        self.assertEqual(self.answers["q5"], parent[self.params["q5"]["b"]])

    def test_counts(self):
        self.assertEqual(self.answers["q6"], {"n100": 2, "n200": 2}[self.params["q6"]["a"]])
        self.assertEqual(self.answers["q7"], {"n100": 5, "n200": 3}[self.params["q7"]["a"]])
        self.assertEqual(self.answers["q8"], {"n100": 1, "n200": 1}[self.params["q8"]["a"]])

    def test_paths(self):
        path = {"n300": ["n100", "n200", "n300"], "n310": ["n100", "n200", "n310"], "n400": ["n100", "n200", "n300", "n400"]}
        self.assertEqual(self.answers["q9"], path[self.params["q9"]["c"]])
        lca = {
            ("n200", "n210"): "n100", ("n210", "n300"): "n100", ("n210", "n310"): "n100",
            ("n210", "n400"): "n100", ("n300", "n310"): "n200", ("n310", "n400"): "n200",
        }
        pair = tuple(sorted((self.params["q10"]["c"], self.params["q10"]["d"])))
        self.assertEqual(self.answers["q10"], lca[pair])

    def test_oldest_ignores_exited(self):
        # n400 is the oldest report of all but it has exited.
        self.assertEqual(self.answers["q11"], "n100")

    def test_siblings(self):
        self.assertEqual(self.answers["q13"], 1)
        self.assertIn(self.params["q13"]["e"], {"n200", "n210", "n300", "n310"})

    def test_question_without_a_target_is_dropped_and_recorded(self):
        # No node here has two siblings with a report time.
        self.assertEqual(self.notes["dropped"], ["q12"])
        self.assertNotIn("q12", self.answers)
        self.assertEqual(len(self.questions), 14)

    def test_a_tree_gets_no_dependency_question(self):
        self.assertEqual(self.notes["not_asked"], ["q16", "q17", "q18", "q19"])
        for qid in self.notes["not_asked"]:
            self.assertNotIn(qid, self.answers)

    def test_traps_ask_about_things_that_do_not_exist(self):
        self.assertIsNone(self.answers["q14"])
        self.assertIsNone(self.answers["q15"])
        ids = {n["id"] for n in helpers.HAND_TREE}
        nicks = {n["nickname"] for n in helpers.HAND_TREE}
        self.assertNotIn(self.params["q14"]["missing_id"], ids)
        self.assertNotIn(self.params["q15"]["missing_nick"], nicks)
        self.assertRegex(self.params["q14"]["missing_id"], r"^n\d{3}$")
        self.assertRegex(self.params["q15"]["missing_nick"], r"^lane-[a-z]+$")


class HandDag(unittest.TestCase):
    """Answers worked out by hand from the drawing of HAND_DAG in helpers.py."""

    @classmethod
    def setUpClass(cls):
        cls.questions, cls.answers, cls.notes = build_questions("hand_dag", helpers.HAND_DAG)
        cls.params = {q["id"]: q["params"] for q in cls.questions}

    def test_blocked_needs_one_parent_that_has_not_exited(self):
        self.assertEqual(self.answers["q16"], ["n300", "n400"])
        self.assertEqual(blocked_nodes(helpers.HAND_DAG), ["n300", "n400"])

    def test_runnable_needs_every_parent_exited_and_a_root_counts(self):
        self.assertEqual(self.answers["q17"], ["n110", "n310", "n320"])
        self.assertEqual(runnable_nodes(helpers.HAND_DAG), ["n110", "n310", "n320"])

    def test_every_idle_node_is_exactly_one_of_the_two(self):
        idle = sorted(n["id"] for n in helpers.HAND_DAG if n["state"] == "idle")
        both = blocked_nodes(helpers.HAND_DAG) + runnable_nodes(helpers.HAND_DAG)
        self.assertEqual(sorted(both), idle)

    def test_failure_impact_counts_a_diamond_bottom_once(self):
        want = {
            "n100": ["n200", "n210", "n220", "n300", "n310", "n320", "n400"],
            "n200": ["n300", "n310", "n320", "n400"],
        }
        self.assertEqual(self.answers["q18"], want[self.params["q18"]["f"]])

    def test_q18_picks_a_node_with_two_paths_to_a_descendant(self):
        # n100, n200 reach n300 or n320 by two paths. n210 and n220 have one way to each.
        self.assertIn(self.params["q18"]["f"], {"n100", "n200"})

    def test_descendant_count_counts_a_diamond_bottom_once(self):
        self.assertEqual(len(descendants(helpers.HAND_DAG, "n100")), 7)

    def test_no_cycle_is_the_null_answer(self):
        self.assertIsNone(self.answers["q19"])
        q19 = next(q for q in self.questions if q["id"] == "q19")
        self.assertEqual(q19["shape"], "null")
        self.assertIn("q19: the graph has no cycle, so a fixed null is right", self.notes["low_discrimination"])

    def test_a_cycle_is_named_by_its_nodes(self):
        graph = [
            node("n100", None, None, "idle", None),
            node("n200", ["n100", "n300"], None, "idle", None),
            node("n300", "n200", None, "idle", None),
            node("n400", "n300", None, "idle", None),
        ]
        self.assertEqual(cycle_nodes(graph), ["n200", "n300"])
        self.assertEqual(cycle_answer(graph), ("id_set", ["n200", "n300"]))
        self.assertEqual(cycle_answer(helpers.HAND_DAG), ("null", None))
        self.assertEqual(cycle_answer(helpers.HAND_TREE), ("null", None))

    def test_path_questions_are_not_asked_on_a_dag(self):
        self.assertEqual(self.notes["not_asked"], ["q9", "q10"])
        self.assertNotIn("q9", self.answers)
        self.assertNotIn("q10", self.answers)
        self.assertEqual(self.notes["dropped"], [])

    def test_q5_asks_only_about_a_single_parent(self):
        b = self.params["q5"]["b"]
        self.assertIn(b, {"n310"})
        self.assertEqual(self.answers["q5"], "n200")

    def test_siblings_join_every_parent(self):
        # n300 shares n200 with n310 and n320, and n210 has no other child.
        self.assertEqual(siblings(helpers.HAND_DAG, "n300"), ["n310", "n320"])
        # n320 shares n200 with n300 and n310, and n220 has no other child.
        self.assertEqual(siblings(helpers.HAND_DAG, "n320"), ["n300", "n310"])
        self.assertEqual(siblings(helpers.HAND_DAG, "n110"), [])

    def test_exited_descendants_count_once(self):
        below = {"n100": ["n200", "n220"], "n200": [], "n210": [], "n220": []}
        self.assertEqual(self.answers["q3"], below[self.params["q3"]["a"]])


class Edges(unittest.TestCase):
    def test_most_recent_sibling(self):
        tree = [
            node("n100", None, None, "idle", None),
            node("n110", "n100", None, "idle", "10:00:00"),
            node("n120", "n100", None, "idle", "11:00:00"),
            node("n130", "n100", None, "idle", "09:00:00"),
            node("n140", "n100", None, "idle", None),
        ]
        questions, answers, _ = build_questions("edge", tree)
        target = next(q for q in questions if q["id"] == "q12")["params"]["e"]
        # Each of these has at least two timed siblings. n140 has three.
        want = {"n110": "n120", "n130": "n120", "n120": "n110", "n140": "n120"}
        self.assertEqual(answers["q12"], want[target])

    def test_q12_skips_a_node_whose_sibling_maximum_is_tied(self):
        tree = [
            node("n100", None, None, "idle", None),
            node("n110", "n100", None, "idle", "11:00:00"),
            node("n120", "n100", None, "idle", "11:00:00"),
            node("n130", "n100", None, "idle", "09:00:00"),
        ]
        questions, answers, _ = build_questions("edge", tree)
        target = next(q for q in questions if q["id"] == "q12")["params"]["e"]
        # n130 sees a tie between n110 and n120, so only those two qualify.
        self.assertIn(target, {"n110", "n120"})
        self.assertEqual(answers["q12"], {"n110": "n120", "n120": "n110"}[target])

    def test_silent_threshold_edges(self):
        self.assertIsNone(silent_minutes([node("n100", None, None, "idle", "10:00:00")]))
        # 30 seconds before the reference time floors to zero and is raised to one.
        tree = [node("n100", None, None, "active", "12:04:30")]
        self.assertEqual(silent_minutes(tree), 1)
        self.assertEqual(silent_nodes(tree, 1), [])
        # Exactly at the threshold counts as silent.
        tree = [node("n100", None, None, "active", "12:04:00")]
        self.assertEqual(silent_nodes(tree, 1), ["n100"])
        _, answers, notes = build_questions("edge", [node("n100", None, None, "idle", "10:00:00"), node("n110", "n100", None, "idle", "10:01:00")])
        self.assertEqual(notes["silent_minutes"], 30)
        self.assertEqual(answers["q1"], [])

    def test_q11_tie_narrows_to_the_largest_root_with_one_minimum(self):
        tree = [
            node("n100", None, None, "idle", "09:00:00"),
            node("n101", "n100", None, "idle", "10:00:00"),
            node("n102", "n100", None, "idle", "10:00:00"),
            node("n103", "n100", None, "idle", "11:00:00"),
            node("n200", None, None, "idle", "09:00:00"),
            node("n201", "n200", None, "idle", "10:30:00"),
            node("n300", None, None, "idle", "11:00:00"),
            node("n301", "n300", None, "idle", "10:40:00"),
            node("n302", "n300", None, "idle", "11:30:00"),
            node("n303", "n300", None, "idle", "11:40:00"),
            node("n304", "n300", None, "idle", "11:45:00"),
        ]
        # n100 ties inside its descendants, n300 has more descendants than n200.
        self.assertEqual(question_11(tree), ("n300", "n301"))
        questions, answers, _ = build_questions("edge", tree)
        q11 = next(q for q in questions if q["id"] == "q11")
        self.assertTrue(q11["text"].startswith("Among the descendants of n300 only: Among nodes whose"))
        self.assertEqual(answers["q11"], "n301")

    def test_q11_equal_descendant_counts_pick_the_smaller_root(self):
        tree = [
            node("n100", None, None, "idle", "09:00:00"),
            node("n101", "n100", None, "idle", "10:00:00"),
            node("n200", None, None, "idle", "09:00:00"),
            node("n201", "n200", None, "idle", "10:30:00"),
        ]
        self.assertEqual(question_11(tree), ("n100", "n101"))

    def test_q11_is_dropped_when_no_root_has_one_minimum(self):
        tree = [
            node("n100", None, None, "idle", "09:00:00"),
            node("n200", None, None, "idle", "09:00:00"),
        ]
        self.assertEqual(question_11(tree), (None, None))
        questions, answers, notes = build_questions("edge", tree)
        self.assertIn("q11", notes["dropped"])
        self.assertNotIn("q11", answers)
        self.assertNotIn("q11", [q["id"] for q in questions])

    def test_q4_without_nicknames_uses_the_node_wording(self):
        tree = [node("n100", None, None, "idle", None), node("n110", "n100", None, "idle", None)]
        questions, answers, _ = build_questions("edge", tree)
        q4 = next(q for q in questions if q["id"] == "q4")
        self.assertEqual(q4["text"], "What is the cwd of the node n110?")
        self.assertEqual(answers["q4"], "/work/repo-a/wt-10")

    def test_shallow_tree_is_marked(self):
        tree = [node("n100", None, None, "idle", None), node("n110", "n100", None, "idle", None)]
        _, _, notes = build_questions("edge", tree)
        self.assertIn("path and aggregate questions: depth below two", notes["low_discrimination"])

    def test_all_silent_is_marked(self):
        tree = [node("n100", None, None, "active", "10:00:00")]
        _, _, notes = build_questions("edge", tree)
        self.assertIn("q1: every candidate is silent", notes["low_discrimination"])


if __name__ == "__main__":
    unittest.main()
