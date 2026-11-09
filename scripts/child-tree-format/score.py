"""Grade one response against the computed answers. The rules section on grading is the source."""

import json
import re

FENCE = re.compile(r"^```[A-Za-z0-9_-]*[ \t]*\n(.*)\n```$", re.S)


def call_outcome(record):
    """Name what one raw line means from its stored fields only.

    scored means a valid call. A tool violation wins over an error on the same
    call because the rules exclude it and count it per model.
    """
    if record.get("tool_violations"):
        return "tool_violation"
    return record.get("error_kind") or "scored"


def parse_response(text):
    """Return (object or None, fenced). None means the whole call counts as a parse failure."""
    stripped = (text or "").strip()
    try:
        value = json.loads(stripped)
    except ValueError:
        value = None
        match = FENCE.match(stripped)
        # A second fence inside means the response is not one fenced block.
        if match and "```" not in match.group(1):
            try:
                value = json.loads(match.group(1))
            except ValueError:
                return None, False
            return (value, True) if isinstance(value, dict) else (None, False)
        return None, False
    return (value, False) if isinstance(value, dict) else (None, False)


def _strings(value):
    if not isinstance(value, list) or not all(isinstance(v, str) for v in value):
        return None
    return [v.strip() for v in value]


def correct(shape, expected, value):
    """Return whether one answer is right for its shape."""
    if shape == "id_set":
        items = _strings(value)
        return items is not None and set(items) == set(expected)
    if shape == "id_seq":
        items = _strings(value)
        return items is not None and items == list(expected)
    if shape == "int":
        # bool is a subclass of int in Python and must stay wrong.
        return type(value) is int and value == expected
    if shape in ("id", "text"):
        return isinstance(value, str) and value.strip() == expected
    if shape == "null":
        return value is None
    raise ValueError(f"unknown shape {shape}")


def grade(text, questions, answers):
    """Return per question results and the parse flags of one response."""
    parsed, fenced = parse_response(text)
    results = {}
    for question in questions:
        qid = question["id"]
        results[qid] = (
            parsed is not None
            and qid in parsed
            and correct(question["shape"], answers[qid], parsed[qid])
        )
    return {
        "results": results,
        "correct": sum(results.values()),
        "total": len(results),
        "parse_failed": parsed is None,
        "fenced": fenced,
    }
