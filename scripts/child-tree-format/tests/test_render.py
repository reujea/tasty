"""All four formats come from one node list and keep every field."""

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
    return match.group(1)


class Rendering(unittest.TestCase):
    def test_each_format_matches_the_rules_example_exactly(self):
        for fmt in FORMATS:
            with self.subTest(fmt=fmt):
                self.assertEqual(render(fmt, helpers.EXAMPLE_TREE), rules_example(fmt))

    def test_every_format_reads_back_the_same_nodes(self):
        for tree in (helpers.EXAMPLE_TREE, helpers.HAND_TREE, helpers.HAND_DAG):
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
        changed = {"nickname": "lane-z", "state": "stale", "cwd": "/work/repo-z/wt-99", "last_report_at": "2026-01-05T09:09:09Z", "parents": ["n587"], "id": "n999"}
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
                "flat_json": lambda t, f=field, v=value: t.replace(f'"{f}":"{v}",', "", 1).replace(f',"{f}":"{v}"', "", 1),
                "dot": lambda t, f=field, v=value: re.sub(rf'(, )?{f}="{re.escape(v)}"', "", t, count=1),
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
            "flat_json": lambda t: t.replace('"parents":["n104"]', '"parents":[]', 1),
            "dot": lambda t: t.replace('  "n104" -> "n230";\n', ""),
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
            ("parents", ["n999"]),
            ("parents", ["n587", "n104"]),
            ("parents", ["n104", "n104"]),
            ("parents", "n104"),
            ("last_report_at", "2026-01-05 11:40"),
        ):
            with self.subTest(field=field, bad=bad):
                tree = copy.deepcopy(helpers.EXAMPLE_TREE)
                tree[2][field] = bad
                with self.assertRaises(ValueError):
                    validate_nodes(tree)


class SeveralParents(unittest.TestCase):
    """The hand drawn diamond in helpers.py, written out for each format by hand."""

    def test_the_table_lists_both_parents_in_one_cell(self):
        text = render("parent_table", helpers.HAND_DAG)
        self.assertIn("| n300 | n200,n210 | lane-e | idle |", text)
        self.assertIn("| n110 | - | lane-b | idle |", text)

    def test_adjacency_names_the_shared_child_under_each_parent(self):
        lines = render("adjacency", helpers.HAND_DAG).split("\n")
        rows = {line.split()[0]: line.rsplit(" ", 1)[1] for line in lines}
        self.assertEqual(rows["n200"], "children=n300,n310,n320")
        self.assertEqual(rows["n210"], "children=n300")
        self.assertEqual(rows["n220"], "children=n320")

    def test_json_keeps_the_parent_list(self):
        self.assertIn('"id":"n300","parents":["n200","n210"]', render("flat_json", helpers.HAND_DAG))
        self.assertIn('"id":"n100","parents":[]', render("flat_json", helpers.HAND_DAG))

    def test_dot_draws_one_edge_per_parent(self):
        lines = render("dot", helpers.HAND_DAG).split("\n")
        self.assertEqual([l for l in lines if l.endswith('-> "n300";')], ['  "n200" -> "n300";', '  "n210" -> "n300";'])
        self.assertEqual(sum("->" in l for l in lines), 10)

    def test_a_shared_child_stands_once_where_the_walk_first_reaches_it(self):
        order = [n["id"] for n in ordered(helpers.HAND_DAG)]
        self.assertEqual(order, ["n100", "n200", "n300", "n400", "n310", "n320", "n210", "n220", "n110"])
        for fmt in FORMATS:
            text = render(fmt, helpers.HAND_DAG)
            for node_id in order:
                self.assertGreaterEqual(text.count(node_id), 1)
        self.assertEqual(render("parent_table", helpers.HAND_DAG).count("| n300 |"), 1)

    def test_a_lost_second_parent_is_detected_in_every_format(self):
        cuts = {
            "parent_table": lambda t: t.replace("| n200,n210 |", "| n200 |"),
            "adjacency": lambda t: t.replace("children=n300\n", "children=-\n", 1),
            "flat_json": lambda t: t.replace('["n200","n210"]', '["n200"]'),
            "dot": lambda t: t.replace('  "n210" -> "n300";\n', ""),
        }
        for fmt, cut in cuts.items():
            with self.subTest(fmt=fmt):
                full = render(fmt, helpers.HAND_DAG)
                self.assertNotEqual(cut(full), full)
                with self.assertRaises(ValueError):
                    check_read_back(fmt, helpers.HAND_DAG, cut(full))

    def test_a_cycle_is_rejected_at_generation(self):
        tree = copy.deepcopy(helpers.HAND_DAG)
        by_id = {n["id"]: n for n in tree}
        by_id["n200"]["parents"] = ["n100", "n400"]
        with self.assertRaises(ValueError):
            validate_nodes(tree)


if __name__ == "__main__":
    unittest.main()
