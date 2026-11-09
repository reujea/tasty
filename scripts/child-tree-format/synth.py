"""Build synthetic graphs with the measured attribute values of the real tree."""

from collections import Counter
from datetime import timedelta

from common import (
    LATEST_REPORT,
    STATES,
    children_map,
    depths,
    descendants,
    format_time,
    largest_remainder,
    ordered,
    parse_time,
    rng,
    validate_nodes,
    vocabulary_nicknames,
)

DAG_LAYERS = (2, 5, 8, 9, 8, 5, 3)
DAG_SECOND_PARENT = 0.45
DAG_FAR_PARENT = 0.15
DAG_EXITED = 13
DAG_MIN_MULTI_PARENT = 8
DAG_MIN_DIAMONDS = 3


def measure_shape(nodes):
    """Return the measured shape of a tree. This is the definition of the same shape."""
    depth = depths(nodes)
    kids = Counter(p for n in nodes for p in n["parents"])
    times = sorted(
        {int(parse_time(n["last_report_at"]).timestamp()) for n in nodes if n["last_report_at"]}
    )
    cwds = {n["cwd"] for n in nodes}
    return {
        "nodes": len(nodes),
        "depth_counts": {str(d): c for d, c in sorted(Counter(depth.values()).items())},
        "max_depth": max(depth.values()),
        "children_per_parent": sorted(kids.values()),
        "state_counts": {s: sum(1 for n in nodes if n["state"] == s) for s in STATES},
        "nickname_missing": sum(1 for n in nodes if n["nickname"] is None),
        "report_time_missing": sum(1 for n in nodes if n["last_report_at"] is None),
        "report_gaps_seconds": [b - a for a, b in zip(times, times[1:])],
        "repositories": len({c.split("/")[2] for c in cwds}),
    }


def forced_items(shape):
    """Return what a synthetic tree must add because the real tree lacks it."""
    forced = []
    if shape["max_depth"] < 3:
        forced.append("max_depth_3")
    forced += [f"state_{s}" for s, c in shape["state_counts"].items() if c == 0]
    return forced


def _split_depths(shape, count):
    weights = {int(d): c for d, c in shape["depth_counts"].items()}
    deepest = max(3, shape["max_depth"])
    for depth in range(deepest + 1):
        weights.setdefault(depth, 0)
    return largest_remainder(weights, count, {d: 1 for d in weights})


def _assign_parents(gen, shape, depth_counts, ids):
    """Give each depth its nodes, drawing batch sizes from the measured child counts."""
    pool = list(ids)
    parent_of = {}
    previous = []
    for depth in sorted(depth_counts):
        current = [pool.pop() for _ in range(depth_counts[depth])]
        if depth == 0:
            for node_id in current:
                parent_of[node_id] = None
        else:
            parents = list(previous)
            gen.shuffle(parents)
            left = list(current)
            index = 0
            while left:
                batch = gen.choice(shape["children_per_parent"])
                for _ in range(min(batch, len(left))):
                    parent_of[left.pop()] = parents[index % len(parents)]
                index += 1
        previous = current
    return parent_of


def _with_attributes(gen, ids, parent_of, state_list, shape):
    """Fill in the attributes drawn from the measured shape and return the ordered graph."""
    count = len(ids)
    missing = round(shape["nickname_missing"] * count / shape["nodes"])
    names = vocabulary_nicknames(count)
    gen.shuffle(names)
    nicknames = names[: count - missing] + [None] * missing
    gen.shuffle(nicknames)

    repo_count = min(26, max(2, round(shape["repositories"] * count / shape["nodes"])))
    letters = "abcdefghijklmnopqrstuvwxyz"[:repo_count]
    counters = Counter()
    cwds = []
    for _ in range(count):
        letter = gen.choice(letters)
        counters[letter] += 1
        cwds.append(f"/work/repo-{letter}/wt-{counters[letter]:02d}")

    time_missing = round(shape["report_time_missing"] * count / shape["nodes"])
    gaps = shape["report_gaps_seconds"] or [60]
    moment = LATEST_REPORT
    times = []
    for _ in range(count - time_missing):
        times.append(format_time(moment))
        moment -= timedelta(seconds=gen.choice(gaps))
    times += [None] * time_missing
    gen.shuffle(times)

    nodes = [
        {
            "id": node_id,
            "parents": parent_of[node_id],
            "nickname": nicknames[i],
            "state": state_list[i],
            "cwd": cwds[i],
            "last_report_at": times[i],
        }
        for i, node_id in enumerate(ids)
    ]
    validate_nodes(nodes)
    return ordered(nodes)


def build_synthetic(name, count, shape):
    """Build one synthetic tree of exactly count nodes."""
    gen = rng("tree", name)
    ids = [f"n{number}" for number in gen.sample(range(100, 1000), count)]
    assigned = _assign_parents(gen, shape, _split_depths(shape, count), ids)
    parent_of = {i: [] if p is None else [p] for i, p in assigned.items()}

    states = largest_remainder(shape["state_counts"], count, {s: 1 for s in STATES})
    state_list = [s for s in STATES for _ in range(states[s])]
    gen.shuffle(state_list)
    return _with_attributes(gen, ids, parent_of, state_list, shape)


def _dag_layers(gen, ids):
    """Split ids into the fixed layers and give each node its parents."""
    pool = list(ids)
    layers = []
    parent_of = {}
    for size in DAG_LAYERS:
        layer = [pool.pop() for _ in range(size)]
        for node_id in layer:
            chosen = set()
            if layers:
                chosen.add(gen.choice(layers[-1]))
                if gen.random() < DAG_SECOND_PARENT:
                    chosen.add(gen.choice(layers[-1]))
                if len(layers) >= 2 and gen.random() < DAG_FAR_PARENT:
                    chosen.add(gen.choice(layers[-2]))
            parent_of[node_id] = sorted(chosen)
        layers.append(layer)
    return layers, parent_of


def _dag_states(gen, layers, parent_of):
    """Finish the first nodes in layer order. A node can start only after all its parents exit.

    The nodes that could start get the four other states in turn. The rest stay idle behind them.
    """
    states = {}
    can_start = []
    finished = 0
    for layer in layers:
        for node_id in layer:
            if not all(states.get(p) == "exited" for p in parent_of[node_id]):
                states[node_id] = "idle"
            elif finished < DAG_EXITED:
                states[node_id] = "exited"
                finished += 1
            else:
                states[node_id] = "idle"
                can_start.append(node_id)
    gen.shuffle(can_start)
    cycle = [s for s in STATES if s != "exited"]
    for index, node_id in enumerate(can_start):
        states[node_id] = cycle[index % len(cycle)]
    return states


def diamond_nodes(nodes):
    """Return the nodes with two parents that share an ancestor and do not reach each other."""
    above = {n["id"]: set() for n in nodes}
    for node in nodes:
        for below in descendants(nodes, node["id"]):
            above[below].add(node["id"])
    found = []
    for node in nodes:
        parents = node["parents"]
        for i, first in enumerate(parents):
            for second in parents[i + 1:]:
                unrelated = first not in above[second] and second not in above[first]
                if unrelated and above[first] & above[second]:
                    found.append(node["id"])
    return sorted(set(found))


def dag_checks(nodes):
    kids = children_map(nodes)
    by_id = {n["id"]: n for n in nodes}
    waiting = [n for n in nodes if n["state"] == "idle" and len(n["parents"]) > 1]
    exited_parents = [sum(by_id[p]["state"] == "exited" for p in n["parents"]) for n in waiting]
    return {
        "nodes": len(nodes),
        "edges": sum(len(v) for v in kids.values()),
        "multi_parent_nodes": sum(1 for n in nodes if len(n["parents"]) > 1),
        "diamond_nodes": len(diamond_nodes(nodes)),
        "runnable_with_several_parents": sum(
            1 for n, c in zip(waiting, exited_parents) if c == len(n["parents"])
        ),
        "blocked_with_an_exited_parent": sum(
            1 for n, c in zip(waiting, exited_parents) if 0 < c < len(n["parents"])
        ),
    }


def build_dag(name, count, shape):
    """Build one DAG. Edges come from fixed layers, other attributes from the measured shape."""
    if sum(DAG_LAYERS) != count:
        raise ValueError("the layers do not add up to the node count")
    gen = rng("tree", name)
    ids = [f"n{number}" for number in gen.sample(range(100, 1000), count)]
    layers, parent_of = _dag_layers(gen, ids)
    states = _dag_states(gen, layers, parent_of)
    nodes = _with_attributes(gen, ids, parent_of, [states[i] for i in ids], shape)
    checks = dag_checks(nodes)
    if checks["multi_parent_nodes"] < DAG_MIN_MULTI_PARENT or checks["diamond_nodes"] < DAG_MIN_DIAMONDS:
        raise ValueError(f"the graph lacks several parents or diamonds: {checks}")
    if any(not any(n["state"] == s for n in nodes) for s in STATES):
        raise ValueError("a state is missing")
    if not checks["runnable_with_several_parents"] or not checks["blocked_with_an_exited_parent"]:
        raise ValueError(f"a dependency case is missing: {checks}")
    return nodes
