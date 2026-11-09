"""Compute the planned calls of each level and their shuffled run order."""

import math

from common import FORMATS, LEVELS, MODELS, PILOT_CELLS, PILOT_RETRY_CAP, rng


def call_id(tree, fmt, model, repeat, pilot=False):
    prefix = "pilot-" if pilot else ""
    return f"{prefix}{tree}-{fmt}-{model}-r{repeat}"


def planned_calls(level):
    """Return every planned (tree, format, model, repeat) of a level."""
    plan = LEVELS[level]
    return [
        (tree, fmt, model, repeat)
        for tree in plan["trees"]
        for fmt in FORMATS
        for model in MODELS
        for repeat in range(1, plan["repeats"][model] + 1)
    ]


def pilot_calls():
    return [(tree, fmt, model, 1) for tree, fmt in PILOT_CELLS for model in MODELS]


def shuffled(key, calls, pilot=False):
    """Return calls in the run order of a level. The key is the level name or pilot."""
    rows = sorted(calls, key=lambda c: call_id(*c, pilot=pilot))
    rng("schedule", key).shuffle(rows)
    return rows


def call_counts():
    """Return the planned and capped call counts of each level, computed from the plan."""
    result = {
        "pilot": {"calls": len(pilot_calls()), "retry_cap": PILOT_RETRY_CAP},
    }
    for level, plan in LEVELS.items():
        result[level] = {"calls": len(planned_calls(level)), "retry_cap": plan["retry_cap"]}
    for row in result.values():
        row["upper_bound"] = row["calls"] + row["retry_cap"]
    return result


def retry_cap_rule(calls):
    """Return the main run retry cap as the rules derive it: ten percent rounded up."""
    return math.ceil(calls * 0.1)
