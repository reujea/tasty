"""Fixed trees for the self tests. Answers next to them are computed by hand."""

import sys
from pathlib import Path

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))


def node(node_id, parent, nickname, state, at, cwd=None):
    return {
        "id": node_id,
        "parent": parent,
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
EXAMPLE_TREE = [
    {"id": "n104", "parent": None, "nickname": "lane-a", "state": "active", "cwd": "/work/repo-a/wt-01", "last_report_at": "2026-01-05T11:40:00Z"},
    {"id": "n230", "parent": "n104", "nickname": None, "state": "idle", "cwd": "/work/repo-a/wt-02", "last_report_at": "2026-01-05T11:58:12Z"},
    {"id": "n587", "parent": "n104", "nickname": "lane-b", "state": "exited", "cwd": "/work/repo-b/wt-01", "last_report_at": None},
]
