"""Render the six formats from one node list, and read them back field by field."""

import json
import re

from common import FIELDS, ordered


def _dash(value):
    return "-" if value is None else value


def _attrs(node, sep):
    return sep.join(
        f"{key}={_dash(node[key])}" for key in ("nickname", "state", "cwd", "last_report_at")
    )


def _kids(nodes):
    result = {n["id"]: [] for n in nodes}
    for node in nodes:
        if node["parent"] is not None:
            result[node["parent"]].append(node["id"])
    for kids in result.values():
        kids.sort()
    return result


def render_parent_table(nodes):
    lines = ["| id | parent | nickname | state | cwd | last_report_at |", "|---|---|---|---|---|---|"]
    for n in nodes:
        cells = " | ".join(_dash(n[f]) for f in FIELDS)
        lines.append(f"| {cells} |")
    return "\n".join(lines)


def render_adjacency(nodes):
    kids = _kids(nodes)
    return "\n".join(
        f"{n['id']} {_attrs(n, ' ')} children={','.join(kids[n['id']]) or '-'}" for n in nodes
    )


def render_mermaid(nodes):
    lines = ["flowchart TD"]
    lines += [f'  {n["id"]}["{_attrs(n, "; ")}"]' for n in nodes]
    lines += [f"  {n['parent']} --> {n['id']}" for n in nodes if n["parent"] is not None]
    return "\n".join(lines)


def _compact(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def render_flat_json(nodes):
    rows = ",\n".join(_compact({f: n[f] for f in FIELDS}) for n in nodes)
    return f"[\n{rows}\n]"


def render_nested_json(nodes):
    by_id = {n["id"]: n for n in nodes}
    kids = _kids(nodes)

    def build(node_id):
        node = by_id[node_id]
        return {
            "id": node["id"],
            "nickname": node["nickname"],
            "state": node["state"],
            "cwd": node["cwd"],
            "last_report_at": node["last_report_at"],
            "children": [build(k) for k in kids[node_id]],
        }

    return _compact([build(n["id"]) for n in nodes if n["parent"] is None])


def render_indented(nodes):
    by_id = {n["id"]: n for n in nodes}
    depth = {}
    lines = []
    for n in nodes:
        depth[n["id"]] = 0 if n["parent"] is None else depth[by_id[n["id"]]["parent"]] + 1
        lines.append(f"{'  ' * depth[n['id']]}- {n['id']} {_attrs(n, ' ')}")
    return "\n".join(lines)


RENDERERS = {
    "parent_table": render_parent_table,
    "adjacency": render_adjacency,
    "mermaid": render_mermaid,
    "flat_json": render_flat_json,
    "nested_json": render_nested_json,
    "indented": render_indented,
}


def render(fmt, nodes):
    """Render one format from a node list. Every format uses the same node order."""
    return RENDERERS[fmt](ordered(nodes))


def _undash(value):
    return None if value == "-" else value


ATTRS_SPACE = r"nickname=(\S+) state=(\S+) cwd=(\S+) last_report_at=(\S+)"
ATTRS_SEMI = r"nickname=([^;]+); state=([^;]+); cwd=([^;]+); last_report_at=([^;\"]+)"


def _node(node_id, parent, match_groups):
    nick, state, cwd, at = match_groups
    return {
        "id": node_id,
        "parent": parent,
        "nickname": _undash(nick),
        "state": state,
        "cwd": cwd,
        "last_report_at": _undash(at),
    }


def _read_parent_table(text):
    lines = text.split("\n")
    if lines[:2] != ["| id | parent | nickname | state | cwd | last_report_at |", "|---|---|---|---|---|---|"]:
        raise ValueError("table header")
    result = []
    for line in lines[2:]:
        cells = line.split(" | ")
        if len(cells) != 6 or not line.startswith("| ") or not line.endswith(" |"):
            raise ValueError("table row")
        cells[0], cells[5] = cells[0][2:], cells[5][:-2]
        result.append(_node(cells[0], _undash(cells[1]), cells[2:]))
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
                parents[kid] = groups[0]
    return [_node(node_id, parents.get(node_id), attrs) for node_id, attrs in rows]


def _read_mermaid(text):
    lines = text.split("\n")
    if lines[0] != "flowchart TD":
        raise ValueError("mermaid header")
    rows = []
    parents = {}
    for line in lines[1:]:
        node = re.fullmatch(rf'  (n\d+)\["{ATTRS_SEMI}"\]', line)
        edge = re.fullmatch(r"  (n\d+) --> (n\d+)", line)
        if node:
            rows.append((node.group(1), node.groups()[1:]))
        elif edge:
            parents[edge.group(2)] = edge.group(1)
        else:
            raise ValueError("mermaid row")
    return [_node(node_id, parents.get(node_id), attrs) for node_id, attrs in rows]


def _read_flat_json(text):
    lines = text.split("\n")
    if lines[0] != "[" or lines[-1] != "]":
        raise ValueError("flat brackets")
    if len(lines) - 2 != len(json.loads(text)):
        raise ValueError("flat rows")
    return [{f: row[f] for f in FIELDS} for row in json.loads(text)]


def _read_nested_json(text):
    if "\n" in text:
        raise ValueError("nested lines")
    result = []

    def walk(item, parent):
        result.append(
            {
                "id": item["id"],
                "parent": parent,
                "nickname": item["nickname"],
                "state": item["state"],
                "cwd": item["cwd"],
                "last_report_at": item["last_report_at"],
            }
        )
        for kid in item["children"]:
            walk(kid, item["id"])

    for root in json.loads(text):
        walk(root, None)
    return result


def _read_indented(text):
    result = []
    stack = []
    for line in text.split("\n"):
        match = re.fullmatch(rf"((?:  )*)- (n\d+) {ATTRS_SPACE}", line)
        if not match:
            raise ValueError("indented row")
        depth = len(match.group(1)) // 2
        if depth > len(stack):
            raise ValueError("indented jump")
        del stack[depth:]
        result.append(_node(match.group(2), stack[-1] if stack else None, match.groups()[2:]))
        stack.append(match.group(2))
    return result


READERS = {
    "parent_table": _read_parent_table,
    "adjacency": _read_adjacency,
    "mermaid": _read_mermaid,
    "flat_json": _read_flat_json,
    "nested_json": _read_nested_json,
    "indented": _read_indented,
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
