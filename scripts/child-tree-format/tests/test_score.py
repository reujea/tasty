"""Grading of each answer shape and of broken responses."""

import json
import sys
import unittest

import helpers  # noqa: F401

sys.dont_write_bytecode = True

from score import correct, grade, parse_response

QUESTIONS = [
    {"id": "q1", "shape": "id_set"},
    {"id": "q9", "shape": "id_seq"},
    {"id": "q6", "shape": "int"},
    {"id": "q5", "shape": "id"},
    {"id": "q4", "shape": "text"},
    {"id": "q14", "shape": "null"},
]
ANSWERS = {"q1": ["n1", "n2"], "q9": ["n1", "n2"], "q6": 3, "q5": "n7", "q4": "/work/repo-a/wt-01", "q14": None}


class Shapes(unittest.TestCase):
    def test_table(self):
        cases = [
            ("id_set", ["a", "b"], ["b", " a "], True),
            ("id_set", ["a", "b"], ["a", "a", "b"], True),
            ("id_set", ["a", "b"], ["a"], False),
            ("id_set", ["a"], "a", False),
            ("id_set", ["a"], [1], False),
            ("id_set", [], [], True),
            ("id_set", [], None, False),
            ("id_seq", ["a", "b"], [" a", "b "], True),
            ("id_seq", ["a", "b"], ["b", "a"], False),
            ("id_seq", ["a", "b"], ["a", "a", "b"], False),
            ("int", 3, 3, True),
            ("int", 3, 4, False),
            ("int", 3, "3", False),
            ("int", 3, 3.0, False),
            ("int", 1, True, False),
            ("int", 0, False, False),
            ("id", "n1", " n1 ", True),
            ("id", "n1", "N1", False),
            ("id", "n1", ["n1"], False),
            ("id", "n1", None, False),
            ("text", "/work/x", "/work/x", True),
            ("text", "/work/x", "/WORK/x", False),
            ("null", None, None, True),
            ("null", None, "null", False),
            ("null", None, "", False),
            ("null", None, [], False),
            ("null", None, 0, False),
        ]
        for shape, expected, value, want in cases:
            with self.subTest(shape=shape, value=value):
                self.assertIs(correct(shape, expected, value), want)


class Grading(unittest.TestCase):
    def test_exact_answers_score_everything(self):
        out = grade(json.dumps(ANSWERS), QUESTIONS, ANSWERS)
        self.assertEqual((out["correct"], out["total"]), (6, 6))
        self.assertFalse(out["parse_failed"] or out["fenced"])

    def test_one_changed_value_loses_only_that_question(self):
        changes = {"q1": ["n1"], "q9": ["n2", "n1"], "q6": 4, "q5": "n8", "q4": "/work/x", "q14": "n1"}
        for qid, value in changes.items():
            with self.subTest(qid=qid):
                out = grade(json.dumps({**ANSWERS, qid: value}), QUESTIONS, ANSWERS)
                self.assertEqual(out["correct"], 5)
                self.assertFalse(out["results"][qid])

    def test_a_trap_answered_with_a_made_up_id_is_wrong(self):
        out = grade(json.dumps({**ANSWERS, "q14": "n104"}), QUESTIONS, ANSWERS)
        self.assertFalse(out["results"]["q14"])

    def test_missing_key_is_wrong_even_for_a_null_answer(self):
        trimmed = {k: v for k, v in ANSWERS.items() if k != "q14"}
        out = grade(json.dumps(trimmed), QUESTIONS, ANSWERS)
        self.assertFalse(out["results"]["q14"])
        self.assertFalse(out["parse_failed"])

    def test_extra_keys_are_ignored(self):
        out = grade(json.dumps({**ANSWERS, "note": "x"}), QUESTIONS, ANSWERS)
        self.assertEqual(out["correct"], 6)

    def test_surrounding_space_is_fine(self):
        self.assertEqual(grade("\n " + json.dumps(ANSWERS) + " \n", QUESTIONS, ANSWERS)["correct"], 6)

    def test_a_whole_fence_is_opened_and_counted(self):
        for opening in ("```json", "```"):
            text = f"{opening}\n{json.dumps(ANSWERS)}\n```"
            with self.subTest(opening=opening):
                out = grade(text, QUESTIONS, ANSWERS)
                self.assertEqual(out["correct"], 6)
                self.assertTrue(out["fenced"])

    def test_broken_responses_fail_every_question(self):
        body = json.dumps(ANSWERS)
        broken = [
            "",
            None,
            "not json",
            "Here you go: " + body,
            body + " done",
            f"text\n```json\n{body}\n```",
            f"```json\n{body}\n```\nmore",
            f"```json\n{body}\n```\n```\n{{}}\n```",
            "[1, 2]",
            "```json\n[1]\n```",
            "3",
            "null",
        ]
        for text in broken:
            with self.subTest(text=text):
                out = grade(text, QUESTIONS, ANSWERS)
                self.assertTrue(out["parse_failed"])
                self.assertFalse(out["fenced"])
                self.assertEqual((out["correct"], out["total"]), (0, 6))

    def test_parse_response_keeps_a_plain_empty_object(self):
        self.assertEqual(parse_response("{}"), ({}, False))
        out = grade("{}", QUESTIONS, ANSWERS)
        self.assertFalse(out["parse_failed"])
        self.assertEqual(out["correct"], 0)


if __name__ == "__main__":
    unittest.main()
