"""Compute the questions and their answers from a node list. No model is involved."""

import statistics

from common import (
    REFERENCE_TIME,
    children_map,
    depths,
    descendants,
    parse_time,
    path_to_root,
    rng,
    roots,
    siblings,
    vocabulary_nicknames,
)

KINDS = {
    "q1": ("filter", "id_set"),
    "q2": ("filter", "id_set"),
    "q3": ("filter", "id_set"),
    "q4": ("lookup", "text"),
    "q5": ("lookup", "id"),
    "q6": ("structure", "int"),
    "q7": ("aggregate", "int"),
    "q8": ("aggregate", "int"),
    "q9": ("path", "id_seq"),
    "q10": ("path", "id"),
    "q11": ("time", "id"),
    "q12": ("time", "id"),
    "q13": ("structure", "int"),
    "q14": ("trap", "null"),
    "q15": ("trap", "null"),
    "q16": ("dependency", "id_set"),
    "q17": ("dependency", "id_set"),
    "q18": ("impact", "id_set"),
    "q19": ("cycle", "null"),
}
Q11_TEXT = (
    "Among nodes whose state is not exited and that have a last_report_at, "
    "which id has the oldest last_report_at?"
)


Q19_TEXT = "Does the graph have a cycle? List the ids of the nodes on cycles, or answer null when there is no cycle."
TREE_ONLY = ("q9", "q10")
DAG_ONLY = ("q16", "q17", "q18", "q19")


def _seconds_before_reference(node):
    return int((REFERENCE_TIME - parse_time(node["last_report_at"])).total_seconds())


def silent_minutes(nodes):
    """Return N of the silent definition. None means no node qualifies for the median."""
    ages = [
        _seconds_before_reference(n)
        for n in nodes
        if n["state"] in ("active", "stale") and n["last_report_at"]
    ]
    if not ages:
        return None
    return max(1, int(statistics.median(ages) // 60))


def silent_nodes(nodes, minutes):
    return sorted(
        n["id"]
        for n in nodes
        if n["state"] in ("active", "stale")
        and n["last_report_at"]
        and _seconds_before_reference(n) >= minutes * 60
    )


def _unique_oldest(group):
    """Return the only node with the minimum time in a group, or None on a tie or no nodes."""
    eligible = [n for n in group if n["state"] != "exited" and n["last_report_at"]]
    if not eligible:
        return None
    oldest = min(n["last_report_at"] for n in eligible)
    holders = [n["id"] for n in eligible if n["last_report_at"] == oldest]
    return holders[0] if len(holders) == 1 else None


def question_11(nodes):
    """Return (scope root, answer). Scope is None when the whole tree has one minimum.

    On a tie the question narrows to the descendants of one forest root, among
    roots whose descendants have a single minimum. The root with the most
    descendants wins and then the smaller id. With no such root the question
    is dropped and this returns (None, None).
    """
    by_id = {n["id"]: n for n in nodes}
    answer = _unique_oldest(nodes)
    if answer is not None:
        return None, answer
    choices = []
    for root in roots(nodes):
        below = descendants(nodes, root)
        answer = _unique_oldest([by_id[i] for i in below])
        if answer is not None:
            choices.append((-len(below), root, answer))
    if not choices:
        return None, None
    _, root, answer = min(choices)
    return root, answer


def runnable_nodes(nodes):
    """Return the idle nodes whose parents have all exited. A root has no parent to wait for."""
    by_id = {n["id"]: n for n in nodes}
    return sorted(
        n["id"] for n in nodes
        if n["state"] == "idle" and all(by_id[p]["state"] == "exited" for p in n["parents"])
    )


def blocked_nodes(nodes):
    """Return the idle nodes with at least one parent that has not exited."""
    by_id = {n["id"]: n for n in nodes}
    return sorted(
        n["id"] for n in nodes
        if n["state"] == "idle" and any(by_id[p]["state"] != "exited" for p in n["parents"])
    )


def cycle_nodes(nodes):
    """Return the nodes on a cycle: those reachable from themselves."""
    return sorted(n["id"] for n in nodes if n["id"] in descendants(nodes, n["id"]))


def cycle_answer(nodes):
    """Return (shape, answer) of the cycle question. No cycle is the null answer."""
    found = cycle_nodes(nodes)
    return ("id_set", found) if found else ("null", None)


def _impact_candidates(nodes):
    """Prefer nodes that reach a descendant by two paths, so counting once matters."""
    by_id = {n["id"]: n for n in nodes}
    picked = []
    for node_id in sorted(by_id):
        below = descendants(nodes, node_id)
        reach = set(below) | {node_id}
        if any(sum(p in reach for p in by_id[d]["parents"]) >= 2 for d in below):
            picked.append(node_id)
    return picked or _candidates_with_descendants(nodes)


def _lowest_common_ancestor(nodes, first, second):
    above = set(path_to_root(nodes, first))
    for node_id in path_to_root(nodes, second):
        if node_id in above:
            return node_id
    return None


def _missing_id(nodes, gen):
    used = {n["id"] for n in nodes}
    return gen.choice([f"n{i}" for i in range(100, 1000) if f"n{i}" not in used])


def _missing_nickname(nodes, gen):
    used = {n["nickname"] for n in nodes}
    pool = [name for name in vocabulary_nicknames(len(nodes) + 26) if name not in used]
    return gen.choice(pool)


def _candidates_with_descendants(nodes):
    counts = {n["id"]: len(descendants(nodes, n["id"])) for n in nodes}
    picked = sorted(i for i, c in counts.items() if c >= 2)
    return picked or sorted(i for i, c in counts.items() if c >= 1)


def _candidates_deep(nodes):
    depth = depths(nodes)
    picked = sorted(i for i, d in depth.items() if d >= 2)
    return picked or sorted(i for i, d in depth.items() if d >= 1)


def _lca_pairs(nodes):
    pairs = []
    ids = sorted(n["id"] for n in nodes)
    for i, first in enumerate(ids):
        for second in ids[i + 1:]:
            above_first = set(path_to_root(nodes, first))
            above_second = set(path_to_root(nodes, second))
            if first in above_second or second in above_first:
                continue
            if above_first & above_second:
                pairs.append((first, second))
    return pairs


def _q12_candidates(nodes):
    by_id = {n["id"]: n for n in nodes}
    picked = []
    for node_id in sorted(by_id):
        timed = [by_id[s]["last_report_at"] for s in siblings(nodes, node_id) if by_id[s]["last_report_at"]]
        if len(timed) >= 2 and timed.count(max(timed)) == 1:
            picked.append(node_id)
    return picked


def build_questions(tree, nodes):
    """Return (questions, answers, notes) for one graph.

    A graph with a node of several parents is a DAG. It has no single path or
    lowest common ancestor, so q9 and q10 go. The dependency questions fit only a DAG.
    """
    is_dag = any(len(n["parents"]) > 1 for n in nodes)
    by_id = {n["id"]: n for n in nodes}
    by_kids = children_map(nodes)
    questions = []
    answers = {}
    dropped = []
    low_discrimination = []

    def add(qid, text, answer, shape=None, **params):
        kind, default_shape = KINDS[qid]
        shape = shape or default_shape
        questions.append({"id": qid, "kind": kind, "shape": shape, "text": text, "params": params})
        answers[qid] = answer

    def pick(qid, candidates):
        return rng("targets", f"{tree}/{qid}").choice(candidates) if candidates else None

    minutes = silent_minutes(nodes)
    silent_answer = [] if minutes is None else silent_nodes(nodes, minutes)
    minutes = 30 if minutes is None else minutes
    add("q1", "List the ids of all silent nodes.", silent_answer)
    if not silent_answer:
        low_discrimination.append("q1: empty answer")
    elif len(silent_answer) == sum(
        1 for n in nodes if n["state"] in ("active", "stale") and n["last_report_at"]
    ):
        low_discrimination.append("q1: every candidate is silent")

    needs = sorted(n["id"] for n in nodes if n["state"] == "needs_input")
    add("q2", "List the ids of all nodes with state needs_input.", needs)
    if not needs:
        low_discrimination.append("q2: the state is absent")

    with_kids = _candidates_with_descendants(nodes)
    deep = _candidates_deep(nodes)

    a3 = pick("q3", with_kids)
    if a3 is None:
        dropped.append("q3")
    else:
        below = [i for i in descendants(nodes, a3) if by_id[i]["state"] == "exited"]
        add("q3", f"List the ids of the descendants of {a3} with state exited.", below, a=a3)
        if not below:
            low_discrimination.append("q3: empty answer")

    named = sorted(n["nickname"] for n in nodes if n["nickname"])
    if named:
        nick = pick("q4", named)
        answer = next(n["cwd"] for n in nodes if n["nickname"] == nick)
        add("q4", f"What is the cwd of the node with nickname {nick}?", answer, nick=nick)
    else:
        b4 = pick("q4", deep)
        if b4 is None:
            dropped.append("q4")
        else:
            add("q4", f"What is the cwd of the node {b4}?", by_id[b4]["cwd"], b=b4)

    b5 = pick("q5", [i for i in deep if len(by_id[i]["parents"]) == 1])
    if b5 is None:
        dropped.append("q5")
    else:
        add("q5", f"What is the id of the parent of {b5}?", by_id[b5]["parents"][0], b=b5)

    a6 = pick("q6", with_kids)
    if a6 is None:
        dropped.append("q6")
    else:
        add("q6", f"How many direct children does {a6} have?", len(by_kids[a6]), a=a6)

    a7 = pick("q7", with_kids)
    if a7 is None:
        dropped.append("q7")
    else:
        add("q7", f"How many descendants does {a7} have?", len(descendants(nodes, a7)), a=a7)

    a8 = pick("q8", with_kids)
    if a8 is None:
        dropped.append("q8")
    else:
        count = sum(1 for i in descendants(nodes, a8) if by_id[i]["state"] == "active")
        add("q8", f"How many descendants of {a8} have state active?", count, a=a8)

    if not is_dag:
        c9 = pick("q9", deep)
        if c9 is None:
            dropped.append("q9")
        else:
            path = list(reversed(path_to_root(nodes, c9)))
            add("q9", f"Give the ids on the path from the root down to {c9}, including both ends.", path, c=c9)

        pairs = _lca_pairs(nodes)
        pair = pick("q10", pairs)
        if pair is None:
            dropped.append("q10")
        else:
            c10, d10 = pair
            ancestor = _lowest_common_ancestor(nodes, c10, d10)
            add("q10", f"What is the id of the lowest common ancestor of {c10} and {d10}?", ancestor, c=c10, d=d10)

    scope, oldest = question_11(nodes)
    if oldest is None:
        dropped.append("q11")
    elif scope is None:
        add("q11", Q11_TEXT, oldest)
    else:
        add("q11", f"Among the descendants of {scope} only: {Q11_TEXT}", oldest, root=scope)

    e12 = pick("q12", _q12_candidates(nodes))
    if e12 is None:
        dropped.append("q12")
    else:
        timed = [s for s in siblings(nodes, e12) if by_id[s]["last_report_at"]]
        latest = max(timed, key=lambda s: by_id[s]["last_report_at"])
        add("q12", f"Among the siblings of {e12} that have a last_report_at, which id has the most recent last_report_at?", latest, e=e12)

    e13 = pick("q13", sorted(i for i in by_id if siblings(nodes, i)))
    if e13 is None:
        dropped.append("q13")
    else:
        add("q13", f"How many siblings does {e13} have?", len(siblings(nodes, e13)), e=e13)

    missing_id = _missing_id(nodes, rng("targets", f"{tree}/q14"))
    add("q14", f"What is the id of the parent of {missing_id}?", None, missing_id=missing_id)
    missing_nick = _missing_nickname(nodes, rng("targets", f"{tree}/q15"))
    add("q15", f"What is the state of the node with nickname {missing_nick}?", None, missing_nick=missing_nick)

    if is_dag:
        blocked = blocked_nodes(nodes)
        add("q16", "List the ids of all blocked nodes.", blocked)
        if not blocked:
            low_discrimination.append("q16: empty answer")
        runnable = runnable_nodes(nodes)
        add("q17", "List the ids of all runnable nodes.", runnable)
        if not runnable:
            low_discrimination.append("q17: empty answer")
        f18 = pick("q18", _impact_candidates(nodes))
        if f18 is None:
            dropped.append("q18")
        else:
            add("q18", f"List the ids of the nodes in the failure impact of {f18}.", descendants(nodes, f18), f=f18)
        shape19, answer19 = cycle_answer(nodes)
        add("q19", Q19_TEXT, answer19, shape=shape19)
        if answer19 is None:
            low_discrimination.append("q19: the graph has no cycle, so a fixed null is right")

    if max(depths(nodes).values()) < 2:
        low_discrimination.append("path and aggregate questions: depth below two")
    not_asked = list(TREE_ONLY if is_dag else DAG_ONLY)
    notes = {
        "silent_minutes": minutes,
        "dropped": dropped,
        "not_asked": not_asked,
        "low_discrimination": low_discrimination,
    }
    return questions, answers, notes
