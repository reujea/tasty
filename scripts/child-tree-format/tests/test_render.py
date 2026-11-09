"""All six formats come from one node list and keep every field."""

import copy
import re
import sys
import unittest

import helpers

sys.dont_write_bytecode = True

from common import FIELDS, FORMATS, RULES_PATH, ordered, validate_nodes
from render import check_read_back, read_back, render


def rules_example(fmt):
    text = RULES_PATH.read_text(encoding="utf-8")
    match = re.search(rf"`{fmt}`[^\n]*\n\n```text\n(.*?)\n```", text, re.S)
    block = match.group(1)
    return block.replace("\n", "") if fmt == "nested_json" else block


class Rendering(unittest.TestCase):
    def test_each_format_matches_the_rules_example_exactly(self):
        for fmt in FORMATS:
            with self.subTest(fmt=fmt):
                self.assertEqual(render(fmt, helpers.EXAMPLE_TREE), rules_example(fmt))

    def test_every_format_reads_back_the_same_nodes(self):
        for tree in (helpers.EXAMPLE_TREE, helpers.HAND_TREE):
            want = ordered(tree)
            for fmt in FORMATS:
                with self.subTest(fmt=fmt):
                    check_read_back(fmt, tree)
                    self.assertEqual(read_back(fmt, render(fmt, tree)), want)

    def test_input_order_does_not_change_the_rendering(self):
        shuffled = list(reversed(helpers.HAND_TREE))
        for fmt in FORMATS:
            self.assertEqual(render(fmt, shuffled), render(fmt, helpers.HAND_TREE))

    def test_changed_value_in_any_field_is_detected(self):
        changed = {"nickname": "lane-z", "state": "stale", "cwd": "/work/repo-z/wt-99", "last_report_at": "2026-01-05T09:09:09Z", "parent": "n587", "id": "n999"}
        for fmt in FORMATS:
            for field in FIELDS:
                with self.subTest(fmt=fmt, field=field):
                    other = copy.deepcopy(helpers.EXAMPLE_TREE)
                    other[1][field] = changed[field]
                    with self.assertRaises(ValueError):
                        check_read_back(fmt, other, render(fmt, helpers.EXAMPLE_TREE))

    def test_a_renderer_that_drops_a_field_fails(self):
        values = {"nickname": "lane-a", "state": "active", "cwd": "/work/repo-a/wt-01", "last_report_at": "2026-01-05T11:40:00Z"}
        for field, value in values.items():
            drops = {
                "parent_table": lambda t, v=value: t.replace(f" {v} |", "", 1),
                "adjacency": lambda t, f=field, v=value: t.replace(f" {f}={v}", "", 1),
                "mermaid": lambda t, f=field, v=value: re.sub(rf"(; )?{f}={re.escape(v)}", "", t, count=1),
                "flat_json": lambda t, f=field, v=value: t.replace(f'"{f}":"{v}",', "", 1).replace(f',"{f}":"{v}"', "", 1),
                "nested_json": lambda t, f=field, v=value: t.replace(f'"{f}":"{v}",', "", 1),
                "indented": lambda t, f=field, v=value: t.replace(f" {f}={v}", "", 1),
            }
            for fmt, drop in drops.items():
                with self.subTest(fmt=fmt, field=field):
                    full = render(fmt, helpers.EXAMPLE_TREE)
                    cut = drop(full)
                    self.assertNotEqual(cut, full)
                    with self.assertRaises(ValueError):
                        check_read_back(fmt, helpers.EXAMPLE_TREE, cut)

    def test_a_renderer_that_loses_the_parent_fails(self):
        cuts = {
            "parent_table": lambda t: t.replace("| n230 | n104 |", "| n230 | - |"),
            "adjacency": lambda t: t.replace("children=n230,n587", "children=n587"),
            "mermaid": lambda t: t.replace("  n104 --> n230\n", ""),
            "flat_json": lambda t: t.replace('"parent":"n104"', '"parent":null', 1),
            "indented": lambda t: t.replace("  - n230", "- n230"),
        }
        for fmt, cut in cuts.items():
            with self.subTest(fmt=fmt):
                full = render(fmt, helpers.EXAMPLE_TREE)
                self.assertNotEqual(cut(full), full)
                with self.assertRaises(ValueError):
                    check_read_back(fmt, helpers.EXAMPLE_TREE, cut(full))

    def test_a_wrong_missing_mark_fails(self):
        text = render("adjacency", helpers.EXAMPLE_TREE).replace("nickname=-", "nickname=none")
        with self.assertRaises(ValueError):
            check_read_back("adjacency", helpers.EXAMPLE_TREE, text)
        text = render("flat_json", helpers.EXAMPLE_TREE).replace('"nickname":null', '"nickname":"-"')
        with self.assertRaises(ValueError):
            check_read_back("flat_json", helpers.EXAMPLE_TREE, text)

    def test_trailing_space_or_empty_line_fails(self):
        for fmt in FORMATS:
            full = render(fmt, helpers.EXAMPLE_TREE)
            with self.assertRaises(ValueError):
                check_read_back(fmt, helpers.EXAMPLE_TREE, full + " ")
            if "\n" in full:
                with self.assertRaises(ValueError):
                    check_read_back(fmt, helpers.EXAMPLE_TREE, full.replace("\n", "\n\n", 1))

    def test_invalid_values_are_rejected_at_generation(self):
        for field, bad in (
            ("nickname", "has space"),
            ("nickname", 'quo"te'),
            ("cwd", "/work/repo-a/wt-01;x"),
            ("cwd", "/work/repo|a/wt-01"),
            ("state", "working"),
            ("id", "104"),
            ("parent", "n999"),
            ("last_report_at", "2026-01-05 11:40"),
        ):
            with self.subTest(field=field, bad=bad):
                tree = copy.deepcopy(helpers.EXAMPLE_TREE)
                tree[2][field] = bad
                with self.assertRaises(ValueError):
                    validate_nodes(tree)


if __name__ == "__main__":
    unittest.main()
