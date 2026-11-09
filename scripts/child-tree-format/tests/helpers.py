"""Fixed trees for the self tests. Answers next to them are computed by hand."""

import sys
from pathlib import Path

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))


def node(node_id, parent, nickname, state, at, cwd=None):
    return {
        "id": node_id,
        "parents": [] if parent is None else [parent] if isinstance(parent, str) else parent,
        "nickname": nickname,
        "state": state,
        "cwd": cwd or f"/work/repo-a/wt-{int(node_id[1:]) % 100:02d}",
        "last_report_at": None if at is None else f"2026-01-05T{at}Z",
    }


# n100 (lane-a, active, 11:00)            n500 (exited, no time)
#   n200 (lane-b, idle, 11:30)              n510 (lane-f, active, 12:00)
#     n300 (active, 11:50)
#       n400 (lane-c, exited, 10:00)
#     n310 (lane-d, stale, 11:10)
#   n210 (lane-e, needs_input, no time)
HAND_TREE = [
    node("n100", None, "lane-a", "active", "11:00:00"),
    node("n200", "n100", "lane-b", "idle", "11:30:00"),
    node("n210", "n100", "lane-e", "needs_input", None),
    node("n300", "n200", None, "active", "11:50:00"),
    node("n310", "n200", "lane-d", "stale", "11:10:00"),
    node("n400", "n300", "lane-c", "exited", "10:00:00"),
    node("n500", None, None, "exited", None),
    node("n510", "n500", "lane-f", "active", "12:00:00"),
]
# n100 (exited) is the top of a diamond through n200 and n210 down to n300.
# n100 exited, 09:00            n110 idle, no time (a root is runnable when idle)
#   n200 exited, 10:00
#     n300 idle, 11:00  parents n200, n210: n210 has not exited, so blocked
#     n310 idle, 11:10  parent n200: runnable
#     n320 idle, 11:20  parents n200, n220: both exited, so runnable
#   n210 active, 10:10
#     n300 (above)
#   n220 exited, 10:20
#     n320 (above)
#       n400 idle, 11:30  parent n320: not exited, so blocked
#     n400 is also below n300, so n100 reaches it by several paths
HAND_DAG = [
    node("n100", None, "lane-a", "exited", "09:00:00"),
    node("n110", None, "lane-b", "idle", None),
    node("n200", "n100", None, "exited", "10:00:00"),
    node("n210", "n100", "lane-c", "active", "10:10:00"),
    node("n220", "n100", "lane-d", "exited", "10:20:00"),
    node("n300", ["n200", "n210"], "lane-e", "idle", "11:00:00"),
    node("n310", "n200", None, "idle", "11:10:00"),
    node("n320", ["n200", "n220"], "lane-f", "idle", "11:20:00"),
    node("n400", ["n300", "n320"], "lane-g", "idle", "11:30:00"),
]
EXAMPLE_TREE = [
    {"id": "n104", "parents": [], "nickname": "lane-a", "state": "active", "cwd": "/work/repo-a/wt-01", "last_report_at": "2026-01-05T11:40:00Z"},
    {"id": "n230", "parents": ["n104"], "nickname": None, "state": "idle", "cwd": "/work/repo-a/wt-02", "last_report_at": "2026-01-05T11:58:12Z"},
    {"id": "n587", "parents": ["n104"], "nickname": "lane-b", "state": "exited", "cwd": "/work/repo-b/wt-01", "last_report_at": None},
]
