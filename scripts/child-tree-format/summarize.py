#!/usr/bin/env python3
"""Rebuild the summary and the recommendation from tracked data only.

Inputs are the raw main run, the level log, the schedules, the questions and
the answers. Each response is graded again here. Pilot calls are never read.
With --check the output is compared to the tracked files byte by byte.

When several formats share the best score the uncertainty of the best is the
largest among them, so the candidate set does not depend on which one is named.

Exit codes: 0 written or identical, 1 different, 2 could not judge.
"""

import argparse
import json
import math
import statistics
import sys
import tempfile
from fractions import Fraction
from pathlib import Path

sys.dont_write_bytecode = True

from common import DATA_DIR, FORMATS, LEVELS, LOW_MODEL, MODELS, TREES, dumps
from schedule import call_id
from score import call_outcome, grade

TOLERANCE = 1e-12
TOKEN_FIELDS = ("input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens", "output_tokens")
REFERENCE_FACTORS = (1, 3)


def read_jsonl(path):
    if not path.exists():
        return []
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def approved_level(log, calls):
    """Return the last level approved before the last main call started."""
    if not calls:
        return None
    last_start = max(c["started_at"] for c in calls)
    approved = [e for e in log if e.get("kind") == "level_change" and e["at"] <= last_start]
    return approved[-1]["to"] if approved else None


def planned_cells(level):
    plan = LEVELS[level]
    return {
        (fmt, model, tree): plan["repeats"][model]
        for fmt in FORMATS
        for model in MODELS
        for tree in plan["trees"]
    }


def valid_calls(level, calls):
    """Return {cell: {repeat: first valid record}} for planned cells and repeats only."""
    plan = planned_cells(level)
    cells = {cell: {} for cell in plan}
    for record in calls:
        if record.get("phase") != "main" or call_outcome(record) != "scored":
            continue
        cell = (record["format"], record["model"], record["tree"])
        repeat = record["repeat"]
        if cell not in plan or not 1 <= repeat <= plan[cell]:
            continue
        if record["call_id"] != call_id(record["tree"], record["format"], record["model"], repeat):
            continue
        cells[cell].setdefault(repeat, record)
    return plan, cells


def aggregate(cell_rates):
    """Return (score, uncertainty) from {cell: per repeat rates}: the rules formula."""
    means = []
    spread = 0.0
    for rates in cell_rates.values():
        means.append(statistics.fmean(rates))
        if len(rates) >= 2:
            spread += statistics.variance(rates) / len(rates)
    if not means:
        return None, None
    return statistics.fmean(means), math.sqrt(spread) / len(means)


def is_candidate(best_score, score, best_u, u, questions, factor=2):
    margin = max(factor * math.sqrt(best_u**2 + u**2), 1 / questions)
    return best_score - score <= margin + TOLERANCE


def candidates(scores, factor=2):
    """Return the candidate formats for a factor. scores: {fmt: (score, u, questions)}."""
    best = max(s for s, _, _ in scores.values())
    best_u = max(u for s, u, _ in scores.values() if abs(s - best) <= TOLERANCE)
    return sorted(f for f, (s, u, q) in scores.items() if is_candidate(best, s, best_u, u, q, factor))


def call_tokens(usage):
    """Return the rule 3 token sum of one call, or None when a field is missing."""
    if not usage or any(usage.get(field) is None for field in TOKEN_FIELDS):
        return None
    return sum(usage[field] for field in TOKEN_FIELDS)


def decide(scores, low):
    """Apply rules 2 and 3 to the rule 1 candidates.

    low: {fmt: {"correct": int, "total": int, "tokens": [int or None]}} of the low model.
    """
    picked = candidates(scores)
    rates = {f: Fraction(low[f]["correct"], low[f]["total"]) for f in picked}
    top = max(rates.values())
    remaining = sorted(f for f in picked if rates[f] == top)
    result = {"candidates": picked, "after_rule_2": remaining, "rule_3_applied": False}
    if len(remaining) > 1:
        tokens = {f: low[f]["tokens"] for f in remaining}
        if any(t is None for values in tokens.values() for t in values) or not all(tokens.values()):
            result["recommendation"] = None
            result["tie"] = remaining
            result["reason"] = "tie: a token field is missing, so rule 3 does not apply"
            return result
        means = {f: Fraction(sum(v), len(v)) for f, v in tokens.items()}
        least = min(means.values())
        remaining = sorted(f for f in remaining if means[f] == least)
        result["rule_3_applied"] = True
    if len(remaining) == 1:
        result["recommendation"] = remaining[0]
    else:
        result["recommendation"] = None
        result["tie"] = remaining
        result["reason"] = "tie after rule 3"
    return result


def input_tokens(record):
    usage = record.get("usage") or {}
    if MODELS[record["model"]]["cli"] == "claude":
        fields = ("input_tokens", "cache_creation_input_tokens", "cache_read_input_tokens")
        return None if any(usage.get(f) is None for f in fields) else sum(usage[f] for f in fields)
    return usage.get("input_tokens")


def mean_and_spread(values):
    known = [v for v in values if v is not None]
    if not known:
        return {"mean": None, "stdev": None, "n": 0}
    return {
        "mean": statistics.fmean(known),
        "stdev": statistics.stdev(known) if len(known) >= 2 else 0.0,
        "n": len(known),
    }


def _load_questions(data, trees):
    questions = {}
    answers = {}
    for tree in trees:
        questions[tree] = json.loads((data / f"questions/{tree}.json").read_text(encoding="utf-8"))["questions"]
        answers[tree] = json.loads((data / f"answers/{tree}.json").read_text(encoding="utf-8"))
    return questions, answers


def _schedule_hashes(data, level):
    rows = read_jsonl(data / f"schedules/{level}.jsonl")
    return {r["call_id"]: r["prompt_sha256"] for r in rows}


def summarize(data):
    """Return the summary object, or raise ValueError when nothing can be judged."""
    calls = read_jsonl(data / "raw/main.jsonl")
    log = read_jsonl(data / "raw/level-changes.jsonl")
    if not calls:
        raise ValueError("the raw main run has no calls")
    level = approved_level(log, calls)
    if level is None:
        raise ValueError("no level was approved before the last call")
    hashes = _schedule_hashes(data, level)
    plan, cells = valid_calls(level, calls)
    questions, answers = _load_questions(data, TREES)

    graded = {}
    for cell, repeats in cells.items():
        for repeat, record in repeats.items():
            if record["prompt_sha256"] != hashes[record["call_id"]]:
                raise ValueError("a raw call does not match its scheduled prompt")
            graded[(cell, repeat)] = grade(record.get("response_text"), questions[cell[2]], answers[cell[2]])

    unfilled = sorted(
        f"{cell[0]}/{cell[1]}/{cell[2]}: {len(cells[cell])} of {r}"
        for cell, r in plan.items()
        if len(cells[cell]) < r
    )
    kinds = {q["kind"] for qs in questions.values() for q in qs}
    kind_of = {tree: {q["id"]: q["kind"] for q in qs} for tree, qs in questions.items()}

    def rates(select, kind=None):
        """Return {cell: per repeat rates} for selected cells, limited to a question kind."""
        chosen = {}
        for cell, repeats in cells.items():
            if not select(cell):
                continue
            values = []
            for repeat in sorted(repeats):
                outcome = graded[(cell, repeat)]
                items = [
                    ok for qid, ok in outcome["results"].items()
                    if kind is None or kind_of[cell[2]][qid] == kind
                ]
                if items:
                    values.append(sum(items) / len(items))
            if values:
                chosen[cell] = values
        return chosen

    def score_row(select, kind=None):
        score, uncertainty = aggregate(rates(select, kind))
        return {"score": score, "uncertainty": uncertainty}

    per_format = {}
    scores = {}
    for fmt in FORMATS:
        row = score_row(lambda c, f=fmt: c[0] == f)
        total = sum(v["total"] for (cell, _), v in graded.items() if cell[0] == fmt)
        row["questions_graded"] = total
        row["questions_correct"] = sum(v["correct"] for (cell, _), v in graded.items() if cell[0] == fmt)
        per_format[fmt] = row
        if total:
            scores[fmt] = (row["score"], row["uncertainty"], total)

    breakdown = {"model": {}, "tree": {}, "kind": {}}
    for fmt in FORMATS:
        breakdown["model"][fmt] = {m: score_row(lambda c, f=fmt, x=m: c[0] == f and c[1] == x) for m in MODELS}
        breakdown["tree"][fmt] = {t: score_row(lambda c, f=fmt, x=t: c[0] == f and c[2] == x) for t in plan_trees(level)}
        breakdown["kind"][fmt] = {k: score_row(lambda c, f=fmt: c[0] == f, k) for k in sorted(kinds)}

    costs = {}
    for fmt in FORMATS:
        costs[fmt] = {}
        for model in MODELS:
            records = [r for cell, reps in cells.items() if cell[:2] == (fmt, model) for _, r in sorted(reps.items())]
            costs[fmt][model] = {
                "input_tokens": mean_and_spread([input_tokens(r) for r in records]),
                "output_tokens": mean_and_spread([(r.get("usage") or {}).get("output_tokens") for r in records]),
                "wall_ms": mean_and_spread([r.get("wall_ms") for r in records]),
            }

    costs_by_tree = {}
    for fmt in FORMATS:
        costs_by_tree[fmt] = {}
        for model in MODELS:
            costs_by_tree[fmt][model] = {}
            for tree in plan_trees(level):
                records = [r for _, r in sorted(cells.get((fmt, model, tree), {}).items())]
                costs_by_tree[fmt][model][tree] = {
                    "input_tokens": mean_and_spread([input_tokens(r) for r in records]),
                    "output_tokens": mean_and_spread([(r.get("usage") or {}).get("output_tokens") for r in records]),
                    "wall_ms": mean_and_spread([r.get("wall_ms") for r in records]),
                }

    summary = {
        "approved_level": level,
        "calls_recorded": len(calls),
        "calls_by_outcome": _count(call_outcome(c) for c in calls),
        "retries_recorded": sum(1 for c in calls if c.get("attempt", 1) > 1),
        "parse_failures": sum(v["parse_failed"] for v in graded.values()),
        "fenced_responses": sum(v["fenced"] for v in graded.values()),
        "unfilled_cells": unfilled,
        "per_format": per_format,
        "breakdown": breakdown,
        "costs": costs,
        "costs_by_tree": costs_by_tree,
    }
    if unfilled:
        summary["decision"] = {"recommendation": None, "reason": "a planned cell is not full, so no recommendation"}
        summary["reference_candidates"] = {}
        return summary

    low = {}
    for fmt in FORMATS:
        low_cells = [(cell, r) for (cell, r) in graded if cell[0] == fmt and cell[1] == LOW_MODEL]
        low[fmt] = {
            "correct": sum(graded[k]["correct"] for k in low_cells),
            "total": sum(graded[k]["total"] for k in low_cells),
            "tokens": [
                call_tokens(cells[cell][repeat].get("usage")) for cell, repeat in low_cells
            ],
        }
    summary["decision"] = decide(scores, low)
    summary["low_model"] = {
        f: {
            "correct": low[f]["correct"],
            "total": low[f]["total"],
            "mean_tokens": None if any(t is None for t in low[f]["tokens"]) or not low[f]["tokens"]
            else sum(low[f]["tokens"]) / len(low[f]["tokens"]),
        }
        for f in FORMATS
    }
    summary["reference_candidates"] = {str(k): candidates(scores, k) for k in REFERENCE_FACTORS}
    return summary


def plan_trees(level):
    return LEVELS[level]["trees"]


def _count(items):
    counts = {}
    for item in items:
        counts[str(item)] = counts.get(str(item), 0) + 1
    return dict(sorted(counts.items()))


def _pct(value):
    return "-" if value is None else f"{value * 100:.1f}"


def _row(cells, widths):
    return "  ".join(str(c).ljust(w) for c, w in zip(cells, widths)).rstrip()


def _table(title, header, rows):
    widths = [max(len(str(r[i])) for r in [header, *rows]) for i in range(len(header))]
    lines = [title, _row(header, widths)]
    lines += [_row(r, widths) for r in rows]
    return lines + [""]


def tables(summary):
    """Return the plain text tables. Every number comes from the summary object."""
    lines = [
        f"approved level: {summary['approved_level']}",
        f"calls recorded: {summary['calls_recorded']} {json.dumps(summary['calls_by_outcome'], sort_keys=True)}",
        f"retries recorded: {summary['retries_recorded']}",
        f"parse failures: {summary['parse_failures']}, fenced responses: {summary['fenced_responses']}",
        f"unfilled cells: {len(summary['unfilled_cells'])}",
        "",
    ]
    lines += _table(
        "score by format (percent)",
        ["format", "score", "uncertainty", "correct", "graded"],
        [
            [f, _pct(r["score"]), _pct(r["uncertainty"]), r["questions_correct"], r["questions_graded"]]
            for f, r in summary["per_format"].items()
        ],
    )
    for axis in ("model", "tree", "kind"):
        columns = list(next(iter(summary["breakdown"][axis].values())))
        lines += _table(
            f"score by format and {axis} (percent, uncertainty in brackets)",
            ["format", *columns],
            [
                [f, *(f"{_pct(v[c]['score'])} ({_pct(v[c]['uncertainty'])})" for c in columns)]
                for f, v in summary["breakdown"][axis].items()
            ],
        )
    cost_rows = []
    for fmt, models in summary["costs"].items():
        for model, row in models.items():
            cost_rows.append(
                [fmt, model, *(
                    "-" if row[k]["mean"] is None else f"{row[k]['mean']:.1f} ({row[k]['stdev']:.1f})"
                    for k in ("input_tokens", "output_tokens", "wall_ms")
                )]
            )
    lines += _table(
        "mean cost per valid call (standard deviation in brackets)",
        ["format", "model", "input tokens", "output tokens", "wall ms"],
        cost_rows,
    )
    tree_rows = []
    for fmt, models in summary["costs_by_tree"].items():
        for model, trees in models.items():
            for tree, row in trees.items():
                tree_rows.append(
                    [fmt, model, tree, *(
                        "-" if row[k]["mean"] is None else f"{row[k]['mean']:.1f} ({row[k]['stdev']:.1f})"
                        for k in ("input_tokens", "output_tokens", "wall_ms")
                    )]
                )
    lines += _table(
        "mean cost per valid call by tree. Tokens are per call, so question kinds cannot split them",
        ["format", "model", "tree", "input tokens", "output tokens", "wall ms"],
        tree_rows,
    )
    decision = summary["decision"]
    lines.append(f"recommendation: {decision.get('recommendation')}")
    for key in ("candidates", "after_rule_2", "rule_3_applied", "tie", "reason"):
        if key in decision:
            lines.append(f"{key}: {decision[key]}")
    for factor, chosen in summary.get("reference_candidates", {}).items():
        lines.append(f"reference candidates with factor {factor}, not used to decide: {chosen}")
    return "\n".join(lines) + "\n"


def write_summary(summary, out):
    out.mkdir(parents=True, exist_ok=True)
    (out / "summary.json").write_text(dumps(summary), encoding="utf-8", newline="\n")
    (out / "tables.txt").write_text(tables(summary), encoding="utf-8", newline="\n")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", default=str(DATA_DIR), help="data folder")
    parser.add_argument("--check", action="store_true", help="compare with the tracked files")
    args = parser.parse_args(argv)
    data = Path(args.data)
    try:
        summary = summarize(data)
    except (ValueError, OSError, KeyError) as error:
        print(f"cannot judge: {error}", file=sys.stderr)
        return 2
    if args.check:
        with tempfile.TemporaryDirectory() as tmp:
            write_summary(summary, Path(tmp))
            different = [
                name for name in ("summary.json", "tables.txt")
                if not (data / "summary" / name).exists()
                or (data / "summary" / name).read_bytes() != (Path(tmp) / name).read_bytes()
            ]
        for name in different:
            print(f"different: {name}")
        print(f"summary files differing: {len(different)}")
        return 1 if different else 0
    write_summary(summary, data / "summary")
    print(f"recommendation: {summary['decision'].get('recommendation')}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
