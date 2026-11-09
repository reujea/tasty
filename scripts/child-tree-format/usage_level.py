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


def window_tokens(usage):
    """Return input + cache creation + output, without cache reads.

    Codex counts cached tokens inside input_tokens and Claude reports them apart,
    so taking them off here gives both CLIs the same measure.
    """
    fresh = (usage.get("input_tokens") or 0) - (usage.get("cached_input_tokens") or 0)
    return fresh + (usage.get("cache_creation_input_tokens") or 0) + (usage.get("output_tokens") or 0)


def pilot_means(records):
    """Return mean window_tokens per pilot call of each model."""
    totals = {}
    for record in records:
        usage = record.get("usage")
        if call_outcome(record) != "scored" or not usage:
            continue
        totals.setdefault(record["model"], []).append(window_tokens(usage))
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


def _level_from_read(windows, means, result):
    """Return the first level whose limit holds for every read window, or None."""
    for level in LEVEL_ORDER:
        values = {w["name"]: after_run(w, means, level) for w in windows}
        result["values"][level] = values
        if all(v <= LIMITS[level] for v in values.values()):
            return level
    return None


def _level_from_unread(windows, means, result):
    shares = {w["name"]: projected(w, means, "L0") / w["w"] if w.get("w") else None for w in windows}
    result["values"]["share_at_L0"] = shares
    small = all(s is not None and s <= UNKNOWN_SHARE for s in shares.values())
    return "L0" if small else "L2"


def _smaller(first, second):
    return max(first, second, key=LEVEL_ORDER.index)


def select_level(windows, means, allow_warning=False):
    """Propose a level. allow_warning lets a seven day warning through and names it in the result."""
    seven = [w for w in windows if w["span"] == "7d"]
    five = [w for w in windows if w["span"] == "5h"]
    if not seven:
        raise ValueError("no seven day window")
    result = {"proposal": None, "needs_decision": False, "wait_for": [], "values": {}}
    passing = [w["name"] for w in seven if allow_warning and w.get("status") == "allowed_warning"]
    if passing:
        result["warning_windows"] = passing
    stopped = [w["name"] for w in windows if w.get("status") in STOP_STATES and w["name"] not in passing]
    if stopped:
        result.update(proposal="stop", reason=f"window status stops the run: {stopped}")
        return result
    unread = [w for w in seven if w.get("u") is None or not w.get("w")]
    read = [w for w in seven if w not in unread]
    level = _level_from_read(read, means, result)
    if level is None:
        result.update(proposal="stop", reason="every level is over its limit")
        return result
    if unread:
        result["needs_decision"] = True
        level = _smaller(level, _level_from_unread(unread, means, result))
    result["proposal"] = level
    for window in five:
        value = after_run(window, means, level)
        result["values"].setdefault("five_hour", {})[window["name"]] = value
        if value is not None and value > LIMITS[level]:
            result["wait_for"].append(window["name"])
    return result


def level_allowed(level, result, approved_by=None):
    """Return whether the table allows running this level. A smaller level always fits.

    A proposal that rests on an unread window is a person's decision to make.
    """
    proposal = result["proposal"]
    if proposal == "stop" or proposal is None:
        return False
    if result.get("needs_decision") and not approved_by:
        return False
    return LEVEL_ORDER.index(level) >= LEVEL_ORDER.index(proposal)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pilot", required=True, help="raw pilot lines")
    parser.add_argument("--windows", required=True, help="window values as JSON")
    parser.add_argument("--allow-warning", action="store_true", help="do not stop on a seven day warning status")
    args = parser.parse_args(argv)
    try:
        lines = Path(args.pilot).read_text(encoding="utf-8").splitlines()
        means = pilot_means([json.loads(line) for line in lines if line.strip()])
        result = select_level(json.loads(Path(args.windows).read_text(encoding="utf-8")), means, args.allow_warning)
    except (OSError, ValueError, KeyError) as error:
        print(f"cannot judge: {error}", file=sys.stderr)
        return 2
    print(json.dumps(result, indent=1, sort_keys=True))
    return 1 if result["proposal"] == "stop" else 0


if __name__ == "__main__":
    sys.exit(main())
