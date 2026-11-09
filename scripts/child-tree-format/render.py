"""Render the four formats from one node list, and read them back field by field."""

import json
import re

from common import FIELDS, children_map, ordered


TABLE_HEAD = "| id | parents | nickname | state | cwd | last_report_at |"


def _dash(value):
    return "-" if value is None else value


def _attrs(node, sep):
    return sep.join(
        f"{key}={_dash(node[key])}" for key in ("nickname", "state", "cwd", "last_report_at")
    )


def render_parent_table(nodes):
    lines = [TABLE_HEAD, "|---|---|---|---|---|---|"]
    for n in nodes:
        cells = (n["id"], ",".join(n["parents"]) or "-", *(_dash(n[f]) for f in FIELDS[2:]))
        lines.append(f"| {' | '.join(cells)} |")
    return "\n".join(lines)


def render_adjacency(nodes):
    kids = children_map(nodes)
    return "\n".join(
        f"{n['id']} {_attrs(n, ' ')} children={','.join(kids[n['id']]) or '-'}" for n in nodes
    )


def _compact(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def render_flat_json(nodes):
    rows = ",\n".join(_compact({f: n[f] for f in FIELDS}) for n in nodes)
    return f"[\n{rows}\n]"


def _dot_attrs(node):
    return ", ".join(f'{key}="{_dash(node[key])}"' for key in ("nickname", "state", "cwd", "last_report_at"))


def render_dot(nodes):
    lines = ["digraph G {", "  rankdir=LR;"]
    lines += [f'  "{n["id"]}" [{_dot_attrs(n)}];' for n in nodes]
    lines += [f'  "{p}" -> "{n["id"]}";' for n in nodes for p in n["parents"]]
    lines.append("}")
    return "\n".join(lines)


RENDERERS = {
    "parent_table": render_parent_table,
    "adjacency": render_adjacency,
    "flat_json": render_flat_json,
    "dot": render_dot,
}


def render(fmt, nodes):
    """Render one format from a node list. Every format uses the same node order."""
    return RENDERERS[fmt](ordered(nodes))


def _undash(value):
    return None if value == "-" else value


ATTRS_SPACE = r"nickname=(\S+) state=(\S+) cwd=(\S+) last_report_at=(\S+)"
ATTRS_DOT = r'nickname="([^"]+)", state="([^"]+)", cwd="([^"]+)", last_report_at="([^"]+)"'


def _node(node_id, parents, match_groups):
    nick, state, cwd, at = match_groups
    return {
        "id": node_id,
        "parents": sorted(parents),
        "nickname": _undash(nick),
        "state": state,
        "cwd": cwd,
        "last_report_at": _undash(at),
    }


def _read_parent_table(text):
    lines = text.split("\n")
    if lines[:2] != [TABLE_HEAD, "|---|---|---|---|---|---|"]:
        raise ValueError("table header")
    result = []
    for line in lines[2:]:
        cells = line.split(" | ")
        if len(cells) != 6 or not line.startswith("| ") or not line.endswith(" |"):
            raise ValueError("table row")
        cells[0], cells[5] = cells[0][2:], cells[5][:-2]
        result.append(_node(cells[0], [] if cells[1] == "-" else cells[1].split(","), cells[2:]))
    return result


def _read_adjacency(text):
    rows = []
    parents = {}
    for line in text.split("\n"):
        match = re.fullmatch(rf"(n\d+) {ATTRS_SPACE} children=(\S+)", line)
        if not match:
            raise ValueError("adjacency row")
        groups = match.groups()
        rows.append((groups[0], groups[1:5]))
        if groups[5] != "-":
            for kid in groups[5].split(","):
                parents.setdefault(kid, []).append(groups[0])
    return [_node(node_id, parents.get(node_id, []), attrs) for node_id, attrs in rows]


def _read_flat_json(text):
    lines = text.split("\n")
    if lines[0] != "[" or lines[-1] != "]":
        raise ValueError("flat brackets")
    if len(lines) - 2 != len(json.loads(text)):
        raise ValueError("flat rows")
    return [{f: row[f] for f in FIELDS} for row in json.loads(text)]


def _read_dot(text):
    lines = text.split("\n")
    if lines[:2] != ["digraph G {", "  rankdir=LR;"] or lines[-1] != "}":
        raise ValueError("dot frame")
    rows = []
    parents = {}
    for line in lines[2:-1]:
        node = re.fullmatch(rf'  "(n\d+)" \[{ATTRS_DOT}\];', line)
        edge = re.fullmatch(r'  "(n\d+)" -> "(n\d+)";', line)
        if node:
            rows.append((node.group(1), node.groups()[1:]))
        elif edge:
            parents.setdefault(edge.group(2), []).append(edge.group(1))
        else:
            raise ValueError("dot row")
    return [_node(node_id, parents.get(node_id, []), attrs) for node_id, attrs in rows]


READERS = {
    "parent_table": _read_parent_table,
    "adjacency": _read_adjacency,
    "flat_json": _read_flat_json,
    "dot": _read_dot,
}


def read_back(fmt, text):
    """Parse a rendering with a parser that shares no code with the renderer."""
    return READERS[fmt](text)


def check_read_back(fmt, nodes, text=None):
    """Raise ValueError unless a rendering holds every field of every node, in order."""
    text = render(fmt, nodes) if text is None else text
    if text != text.strip() or any(line != line.rstrip() or not line for line in text.split("\n")):
        raise ValueError(f"{fmt}: trailing space or empty line")
    try:
        parsed = read_back(fmt, text)
    except (KeyError, TypeError, json.JSONDecodeError) as error:
        raise ValueError(f"{fmt}: {type(error).__name__}") from error
    expected = ordered(nodes)
    if len(parsed) != len(expected):
        raise ValueError(f"{fmt}: node count")
    for got, want in zip(parsed, expected):
        for field in FIELDS:
            if got[field] != want[field]:
                raise ValueError(f"{fmt}: field {field} of {want['id']}")
