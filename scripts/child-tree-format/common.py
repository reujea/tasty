"""Shared definitions of the child tree format study. docs/research/child-tree-format/rules.md is the source."""

import hashlib
import json
import random
import re
from datetime import datetime, timedelta, timezone
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
RESEARCH_DIR = REPO_ROOT / "docs/research/child-tree-format"
DATA_DIR = RESEARCH_DIR / "data"
RULES_PATH = RESEARCH_DIR / "rules.md"
RULES_REL = "docs/research/child-tree-format/rules.md"

SEED = 20261109
FORMATS = ("parent_table", "adjacency", "mermaid", "flat_json", "nested_json", "indented")
FORMAT_NAMES = {
    "parent_table": "parent id table",
    "adjacency": "adjacency list with attributes",
    "mermaid": "Mermaid flowchart",
    "flat_json": "flat JSON array",
    "nested_json": "nested JSON",
    "indented": "indented list",
}
MISSING_MARK = {f: "null" if f.endswith("_json") else "-" for f in FORMATS}
TREES = ("real", "synthetic_40", "synthetic_80")
STATES = ("active", "idle", "needs_input", "stale", "exited")
FIELDS = ("id", "parent", "nickname", "state", "cwd", "last_report_at")

MODELS = {
    "claude-high": {"cli": "claude", "id": "claude-opus-5-5", "effort": "medium"},
    "claude-low": {"cli": "claude", "id": "claude-haiku-5-5", "effort": "medium"},
    "codex": {"cli": "codex", "id": "gpt-6.1-sol", "effort": "medium"},
}
LOW_MODEL = "claude-low"

# Repeats per model, trees and the retry cap of each level in the rules.
LEVELS = {
    "L0": {
        "trees": TREES,
        "repeats": {"claude-high": 3, "claude-low": 3, "codex": 3},
        "retry_cap": 17,
    },
    "L1": {
        "trees": TREES,
        "repeats": {"claude-high": 2, "claude-low": 3, "codex": 2},
        "retry_cap": 13,
    },
    "L2": {
        "trees": ("real", "synthetic_80"),
        "repeats": {"claude-high": 2, "claude-low": 3, "codex": 2},
        "retry_cap": 9,
    },
}
PILOT_RETRY_CAP = 2
PILOT_TREE = "synthetic_80"

LATEST_REPORT = datetime(2026, 1, 5, 12, 0, 0, tzinfo=timezone.utc)
REFERENCE_TIME = datetime(2026, 1, 5, 12, 5, 0, tzinfo=timezone.utc)

ID_RE = re.compile(r"^n\d{3}$")
NICKNAME_RE = re.compile(r"^[a-z][a-z0-9-]*$")
CWD_RE = re.compile(r"^/work/repo-[a-z]/wt-\d{2}$")
TIME_RE = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$")
FORBIDDEN_CHARS = re.compile(r'[\s";|]')


def rng(purpose, key):
    """Return the generator of one purpose and key. Key parts join with a slash."""
    digest = hashlib.sha256(f"{SEED}/{purpose}/{key}".encode()).digest()
    return random.Random(int.from_bytes(digest[:8], "big"))


def format_time(moment):
    return moment.strftime("%Y-%m-%dT%H:%M:%SZ")


def parse_time(text):
    return datetime.strptime(text, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)


def shift_seconds(seconds_list):
    """Map epoch seconds onto the fixed clock, keeping intervals. None stays None."""
    known = [s for s in seconds_list if s is not None]
    latest = max(known) if known else 0
    return [
        None if s is None else format_time(LATEST_REPORT - timedelta(seconds=latest - s))
        for s in seconds_list
    ]


def validate_nodes(nodes):
    """Raise ValueError when a node breaks the rules on values or the forest shape."""
    ids = [n["id"] for n in nodes]
    if len(set(ids)) != len(ids):
        raise ValueError("duplicate id")
    known = set(ids)
    nicknames = [n["nickname"] for n in nodes if n["nickname"] is not None]
    if len(set(nicknames)) != len(nicknames):
        raise ValueError("duplicate nickname")
    for node in nodes:
        if tuple(node) != FIELDS:
            raise ValueError("wrong fields")
        if not ID_RE.match(node["id"]):
            raise ValueError("bad id form")
        if node["parent"] is not None and node["parent"] not in known:
            raise ValueError("unknown parent")
        if node["nickname"] is not None:
            if not NICKNAME_RE.match(node["nickname"]) or FORBIDDEN_CHARS.search(node["nickname"]):
                raise ValueError("bad nickname form")
        if node["state"] not in STATES:
            raise ValueError("unknown state")
        if not CWD_RE.match(node["cwd"]) or FORBIDDEN_CHARS.search(node["cwd"]):
            raise ValueError("bad cwd form")
        if node["last_report_at"] is not None and not TIME_RE.match(node["last_report_at"]):
            raise ValueError("bad time form")
    depths(nodes)


def children_map(nodes):
    result = {n["id"]: [] for n in nodes}
    for node in nodes:
        if node["parent"] is not None:
            result[node["parent"]].append(node["id"])
    for kids in result.values():
        kids.sort()
    return result


def roots(nodes):
    return sorted(n["id"] for n in nodes if n["parent"] is None)


def ordered(nodes):
    """Return nodes in the fixed order: roots by id, depth first, siblings by id."""
    by_id = {n["id"]: n for n in nodes}
    kids = children_map(nodes)
    result = []
    stack = list(reversed(roots(nodes)))
    while stack:
        current = stack.pop()
        result.append(by_id[current])
        stack.extend(reversed(kids[current]))
    if len(result) != len(nodes):
        raise ValueError("a cycle hides nodes from the roots")
    return result


def depths(nodes):
    """Return {id: depth} with roots at zero."""
    by_id = {n["id"]: n for n in nodes}
    result = {}
    for node in ordered(nodes):
        parent = by_id[node["id"]]["parent"]
        result[node["id"]] = 0 if parent is None else result[parent] + 1
    return result


def descendants(nodes, node_id):
    kids = children_map(nodes)
    result = []
    stack = list(kids[node_id])
    while stack:
        current = stack.pop()
        result.append(current)
        stack.extend(kids[current])
    return sorted(result)


def path_to_root(nodes, node_id):
    by_id = {n["id"]: n for n in nodes}
    result = [node_id]
    while by_id[result[-1]]["parent"] is not None:
        result.append(by_id[result[-1]]["parent"])
    return result


def siblings(nodes, node_id):
    by_id = {n["id"]: n for n in nodes}
    parent = by_id[node_id]["parent"]
    if parent is None:
        return []
    return [k for k in children_map(nodes)[parent] if k != node_id]


def vocabulary_nicknames(count):
    """Yield the shared nickname vocabulary: lane-a to lane-z, then lane-aa onward."""
    letters = "abcdefghijklmnopqrstuvwxyz"
    names = [f"lane-{a}" for a in letters] + [f"lane-{a}{b}" for a in letters for b in letters]
    return names[:count]


def largest_remainder(weights, total, minimums=None):
    """Split total by weights; each key keeps at least its minimum."""
    minimums = minimums or {}
    keys = list(weights)
    spare = total - sum(minimums.get(k, 0) for k in keys)
    weight_sum = sum(weights.values())
    exact = {k: weights[k] * spare / weight_sum if weight_sum else 0 for k in keys}
    result = {k: minimums.get(k, 0) + int(exact[k]) for k in keys}
    left = total - sum(result.values())
    for key in sorted(keys, key=lambda k: (-(exact[k] - int(exact[k])), str(k)))[:left]:
        result[key] += 1
    return result


def sha256_bytes(data):
    return hashlib.sha256(data).hexdigest()


def dumps(value):
    """Serialize a tracked data file. The output is stable and ends with a newline."""
    return json.dumps(value, ensure_ascii=False, indent=1, sort_keys=False) + "\n"
