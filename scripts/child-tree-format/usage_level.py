#!/usr/bin/env python3
"""Propose a run level from pilot tokens and window usage. The result is a proposal only.

--windows reads a JSON list of {name, span ("7d" or "5h"), models, u, w, status}.
u and w may be null when they could not be read.

Exit codes: 0 a level is proposed, 1 the table says stop, 2 could not judge.
"""

import argparse
import json
import sys
from pathlib import Path

sys.dont_write_bytecode = True

from common import LEVELS
from schedule import planned_calls
from score import call_outcome

LIMITS = {"L0": 70, "L1": 85, "L2": 95}
STOP_STATES = ("allowed_warning", "rejected")
UNKNOWN_SHARE = 0.02
LEVEL_ORDER = tuple(LEVELS)


def planned_by_model(level):
    counts = {}
    for _, _, model, _ in planned_calls(level):
        counts[model] = counts.get(model, 0) + 1
    return counts


def pilot_means(records):
    """Return mean input + cache creation + output tokens per pilot call of each model."""
    totals = {}
    for record in records:
        usage = record.get("usage")
        if call_outcome(record) != "scored" or not usage:
            continue
        tokens = sum(usage.get(k) or 0 for k in ("input_tokens", "cache_creation_input_tokens", "output_tokens"))
        totals.setdefault(record["model"], []).append(tokens)
    return {m: sum(v) / len(v) for m, v in totals.items()}


def projected(window, means, level):
    counts = planned_by_model(level)
    return sum(means[m] * counts[m] for m in window["models"])


def after_run(window, means, level):
    """Return u' of a window, or None when u was not read. Zero W keeps u as it is."""
    if window.get("u") is None:
        return None
    u = max(window["u"], 1)
    if not window.get("w"):
        return u
    return u + u * projected(window, means, level) / window["w"]


def select_level(windows, means):
    seven = [w for w in windows if w["span"] == "7d"]
    five = [w for w in windows if w["span"] == "5h"]
    if not seven:
        raise ValueError("no seven day window")
    result = {"proposal": None, "needs_decision": False, "wait_for": [], "values": {}}
    stopped = [w["name"] for w in windows if w.get("status") in STOP_STATES]
    if stopped:
        result.update(proposal="stop", reason=f"window status stops the run: {stopped}")
        return result
    if any(w.get("u") is None or not w.get("w") for w in seven):
        result["needs_decision"] = True
        shares = {w["name"]: projected(w, means, "L0") / w["w"] if w.get("w") else None for w in seven}
        result["values"]["share_at_L0"] = shares
        small = all(s is not None and s <= UNKNOWN_SHARE for s in shares.values())
        result["proposal"] = "L0" if small else "L2"
    else:
        for level in LEVEL_ORDER:
            values = {w["name"]: after_run(w, means, level) for w in seven}
            result["values"][level] = values
            if all(v <= LIMITS[level] for v in values.values()):
                result["proposal"] = level
                break
        else:
            result.update(proposal="stop", reason="every level is over its limit")
            return result
    level = result["proposal"]
    for window in five:
        value = after_run(window, means, level)
        result["values"].setdefault("five_hour", {})[window["name"]] = value
        if value is not None and value > LIMITS[level]:
            result["wait_for"].append(window["name"])
    return result


def level_allowed(level, result):
    """Return whether the table allows running this level. A smaller level always fits."""
    proposal = result["proposal"]
    if proposal == "stop" or proposal is None:
        return False
    return LEVEL_ORDER.index(level) >= LEVEL_ORDER.index(proposal)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pilot", required=True, help="raw pilot lines")
    parser.add_argument("--windows", required=True, help="window values as JSON")
    args = parser.parse_args(argv)
    try:
        lines = Path(args.pilot).read_text(encoding="utf-8").splitlines()
        means = pilot_means([json.loads(line) for line in lines if line.strip()])
        result = select_level(json.loads(Path(args.windows).read_text(encoding="utf-8")), means)
    except (OSError, ValueError, KeyError) as error:
        print(f"cannot judge: {error}", file=sys.stderr)
        return 2
    print(json.dumps(result, indent=1, sort_keys=True))
    return 1 if result["proposal"] == "stop" else 0


if __name__ == "__main__":
    sys.exit(main())
