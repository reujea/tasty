"""Build synthetic trees with the measured shape of the real tree."""

from collections import Counter
from datetime import timedelta

from common import (
    LATEST_REPORT,
    STATES,
    depths,
    format_time,
    largest_remainder,
    ordered,
    parse_time,
    rng,
    validate_nodes,
    vocabulary_nicknames,
)


def measure_shape(nodes):
    """Return the measured shape of a tree. This is the definition of the same shape."""
    depth = depths(nodes)
    kids = Counter(n["parent"] for n in nodes if n["parent"] is not None)
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


def build_synthetic(name, count, shape):
    """Build one synthetic tree of exactly count nodes."""
    gen = rng("tree", name)
    ids = [f"n{number}" for number in gen.sample(range(100, 1000), count)]
    parent_of = _assign_parents(gen, shape, _split_depths(shape, count), ids)

    states = largest_remainder(shape["state_counts"], count, {s: 1 for s in STATES})
    state_list = [s for s in STATES for _ in range(states[s])]
    gen.shuffle(state_list)

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
            "parent": parent_of[node_id],
            "nickname": nicknames[i],
            "state": state_list[i],
            "cwd": cwds[i],
            "last_report_at": times[i],
        }
        for i, node_id in enumerate(ids)
    ]
    validate_nodes(nodes)
    return ordered(nodes)
