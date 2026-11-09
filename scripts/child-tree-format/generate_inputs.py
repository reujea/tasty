#!/usr/bin/env python3
"""Generate every tracked input from the published real tree, and print the call counts.

The published tree is the only input, so anyone can regenerate the rest. With
--check the output is compared to the tracked files byte by byte.

Exit codes: 0 written or identical, 1 different from the tracked files, 2 could not judge.
"""

import argparse
import json
import shutil
import sys
import tempfile
from pathlib import Path

sys.dont_write_bytecode = True

from check_rules import DECISION_SHA256
from common import DATA_DIR, FORMATS, LEVELS, PILOT_CELLS, SEED, TREES, dumps, sha256_bytes, validate_nodes
from prompts import build_user_prompt, prompt_hash, question_order
from questions import build_questions
from render import check_read_back, render
from schedule import call_counts, call_id, pilot_calls, planned_calls, shuffled
from synth import build_dag, build_synthetic, dag_checks, forced_items, measure_shape

TEMPLATE_FILES = ("prompts/system.txt", "prompts/user.txt")


def _write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8", newline="\n")


def _schedule_rows(key, calls, inputs, system, template, pilot=False):
    rows = []
    for seq, (tree, fmt, model, repeat) in enumerate(shuffled(key, calls, pilot=pilot), 1):
        cid = call_id(tree, fmt, model, repeat, pilot=pilot)
        questions, minutes = inputs[tree]["questions"], inputs[tree]["minutes"]
        order = question_order(cid, questions)
        user = build_user_prompt(template, fmt, inputs[tree]["rendered"][fmt], questions, minutes, order)
        rows.append(
            {
                "seq": seq,
                "call_id": cid,
                "tree": tree,
                "format": fmt,
                "model": model,
                "repeat": repeat,
                "order": order,
                "prompt_sha256": prompt_hash(system, user),
            }
        )
    return "".join(json.dumps(r, separators=(",", ":")) + "\n" for r in rows)


def generate(real_tree, template_dir, out_dir):
    """Write all inputs into out_dir and return the manifest."""
    real_nodes = real_tree["nodes"]
    validate_nodes(real_nodes)
    shape = measure_shape(real_nodes)
    forced = forced_items(shape)
    trees = {
        "real": real_nodes,
        "synthetic_40": build_synthetic("synthetic_40", 40, shape),
        "synthetic_80": build_synthetic("synthetic_80", 80, shape),
        "dag_40": build_dag("dag_40", 40, shape),
    }
    system = (template_dir / "prompts/system.txt").read_text(encoding="utf-8").rstrip("\n")
    template = (template_dir / "prompts/user.txt").read_text(encoding="utf-8").rstrip("\n")

    _write(out_dir / "trees/real.json", dumps(real_tree))
    for rel in TEMPLATE_FILES:
        if template_dir != out_dir:
            _write(out_dir / rel, (template_dir / rel).read_text(encoding="utf-8"))

    inputs = {}
    rendered_bytes = {}
    question_notes = {}
    for name in TREES:
        nodes = trees[name]
        if name != "real":
            _write(out_dir / f"trees/{name}.json", dumps({"tree": name, "nodes": nodes}))
        rendered = {}
        for fmt in FORMATS:
            text = render(fmt, nodes)
            check_read_back(fmt, nodes, text)
            rendered[fmt] = text
            _write(out_dir / f"rendered/{name}/{fmt}.txt", text + "\n")
        rendered_bytes[name] = {f: len(rendered[f].encode()) for f in FORMATS}
        questions, answers, notes = build_questions(name, nodes)
        _write(out_dir / f"questions/{name}.json", dumps({"silent_minutes": notes["silent_minutes"], "questions": questions}))
        _write(out_dir / f"answers/{name}.json", dumps(answers))
        question_notes[name] = {"asked": len(questions), **notes}
        inputs[name] = {"questions": questions, "minutes": notes["silent_minutes"], "rendered": rendered}

    _write(
        out_dir / "schedules/pilot.jsonl",
        _schedule_rows("pilot", pilot_calls(), inputs, system, template, pilot=True),
    )
    for level in LEVELS:
        _write(
            out_dir / f"schedules/{level}.jsonl",
            _schedule_rows(level, planned_calls(level), inputs, system, template),
        )

    files = {
        str(p.relative_to(out_dir)): sha256_bytes(p.read_bytes())
        for p in sorted(out_dir.rglob("*"))
        if p.is_file() and p.name != "manifest.json" and p.parent.name != "raw"
    }
    manifest = {
        "seed": SEED,
        "decision_rules_sha256": DECISION_SHA256,
        "real_capture": real_tree.get("capture", {}),
        "real_shape": shape,
        "synthetic_forced": forced,
        "synthetic_shapes": {n: measure_shape(trees[n]) for n in ("synthetic_40", "synthetic_80", "dag_40")},
        "dag_checks": dag_checks(trees["dag_40"]),
        "rendered_bytes": rendered_bytes,
        "pilot_cells": [list(cell) for cell in PILOT_CELLS],
        "questions": question_notes,
        "call_counts": call_counts(),
        "files": files,
    }
    _write(out_dir / "manifest.json", dumps(manifest))
    return manifest


def compare_trees(expected, actual):
    """Return the relative paths that differ, skipping the raw results folder."""
    def listing(base):
        return {
            str(p.relative_to(base))
            for p in base.rglob("*")
            if p.is_file() and "raw" not in p.relative_to(base).parts and "summary" not in p.relative_to(base).parts
        }

    left, right = listing(expected), listing(actual)
    different = sorted(left ^ right)
    for rel in sorted(left & right):
        if (expected / rel).read_bytes() != (actual / rel).read_bytes():
            different.append(rel)
    return different


def print_counts(counts):
    for name, row in counts.items():
        print(f"{name}: planned calls {row['calls']}, retry cap {row['retry_cap']}, upper bound {row['upper_bound']}")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", default=str(DATA_DIR), help="data folder")
    parser.add_argument("--check", action="store_true", help="compare with the tracked files")
    parser.add_argument("--counts", action="store_true", help="only print the call counts")
    args = parser.parse_args(argv)
    if args.counts:
        print_counts(call_counts())
        return 0
    data = Path(args.data)
    real_path = data / "trees/real.json"
    if not real_path.exists():
        print("cannot judge: the published real tree is missing", file=sys.stderr)
        return 2
    real_tree = json.loads(real_path.read_text(encoding="utf-8"))
    if args.check:
        with tempfile.TemporaryDirectory() as tmp:
            generate(real_tree, data, Path(tmp))
            different = compare_trees(Path(tmp), data)
        for rel in different:
            print(f"different: {rel}")
        print(f"inputs differing: {len(different)}")
        return 1 if different else 0
    with tempfile.TemporaryDirectory() as tmp:
        manifest = generate(real_tree, data, Path(tmp))
        for sub in ("rendered", "questions", "answers", "schedules"):
            shutil.rmtree(data / sub, ignore_errors=True)
        for path in sorted(Path(tmp).rglob("*")):
            if path.is_file():
                target = data / path.relative_to(tmp)
                target.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(path, target)
    print_counts(manifest["call_counts"])
    return 0


if __name__ == "__main__":
    sys.exit(main())
