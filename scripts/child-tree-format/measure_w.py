#!/usr/bin/env python3
"""Measure W, the tokens of the last 7 days and 5 hours, from local CLI logs.

W is input + cache creation + output. Cache reads are left out on purpose, the
same way pilot_means counts a call, so W and P can be compared. There is no
switch for it. Log locations are arguments because they belong to one computer.

Only numbers reach stdout, as a JSON list for usage_level.py --windows. Paths and
message text are never printed. A log that could not be read leaves w null,
which the level table treats as unread.

Exit codes: 0 written, 2 could not judge.
"""

import argparse
import json
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

sys.dont_write_bytecode = True

from common import MODELS
from usage_level import window_tokens

SPANS = {"7d": timedelta(days=7), "5h": timedelta(hours=5)}


def _time(value):
    try:
        stamp = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except (AttributeError, ValueError):
        return None
    return stamp if stamp.tzinfo else None


def _lines(root, since):
    """Yield parsed lines of each log file, one list per file, skipping files too old to matter."""
    for path in sorted(root.rglob("*.jsonl")):
        if datetime.fromtimestamp(path.stat().st_mtime, timezone.utc) < since:
            continue
        rows = []
        with path.open(encoding="utf-8", errors="replace") as handle:
            for line in handle:
                try:
                    row = json.loads(line)
                except ValueError:
                    continue
                if isinstance(row, dict):
                    rows.append(row)
        yield rows


def claude_events(root, since):
    """Return [(time, tokens, model)], one per message id.

    Streaming writes the same message on several lines with growing output, so
    the largest count of an id stands for that message.
    """
    best = {}
    for rows in _lines(root, since):
        for row in rows:
            message = row.get("message")
            if row.get("type") != "assistant" or not isinstance(message, dict):
                continue
            usage, when, key = message.get("usage"), _time(row.get("timestamp")), message.get("id")
            if not isinstance(usage, dict) or when is None or not key:
                continue
            event = (when, window_tokens(usage), message.get("model"))
            if key not in best or event[1] > best[key][1]:
                best[key] = event
    return list(best.values())


def codex_events(root, since):
    """Return [(time, tokens, None)]. A repeated count event adds nothing, and a resumed log copies old lines."""
    seen = set()
    events = []
    for rows in _lines(root, since):
        previous = None
        for row in rows:
            payload = row.get("payload")
            if row.get("type") != "event_msg" or not isinstance(payload, dict) or payload.get("type") != "token_count":
                continue
            info = payload.get("info")
            when = _time(row.get("timestamp"))
            if not isinstance(info, dict) or when is None:
                continue
            total = (info.get("total_token_usage") or {}).get("total_tokens")
            usage = info.get("last_token_usage")
            if not isinstance(usage, dict) or total == previous or (row["timestamp"], total) in seen:
                continue
            previous = total
            seen.add((row["timestamp"], total))
            events.append((when, window_tokens(usage), None))
    return events


def _sum(events, since, now, model=None):
    return sum(t for when, t, name in events if since <= when <= now and (model is None or name == model))


def _pairs(values):
    pairs = {}
    for item in values:
        name, separator, value = item.partition("=")
        if not separator or not name or not value:
            raise ValueError(f"expected name=value: {item}")
        pairs[name] = value
    return pairs


def measure(claude_dir, codex_dir, now, used=None, status=None, model_windows=None):
    used, status, model_windows = used or {}, status or {}, model_windows or {}
    oldest = now - SPANS["7d"]
    claude = claude_events(claude_dir, oldest) if claude_dir and claude_dir.is_dir() else []
    codex = codex_events(codex_dir, oldest) if codex_dir and codex_dir.is_dir() else []
    groups = {
        "claude": (claude, sorted(m for m, v in MODELS.items() if v["cli"] == "claude")),
        "codex": (codex, sorted(m for m, v in MODELS.items() if v["cli"] == "codex")),
    }
    windows = []

    def add(name, span, events, models, model=None):
        tokens = _sum(events, now - SPANS[span], now, model)
        windows.append({
            "name": name, "span": span, "models": models, "u": used.get(name), "status": status.get(name),
            "w": tokens if events else None,
        })

    for cli, (events, models) in groups.items():
        for span in SPANS:
            add(f"{cli}_{span}", span, events, models)
    for name, study_model in model_windows.items():
        if study_model not in MODELS or MODELS[study_model]["cli"] != "claude":
            raise ValueError(f"not a claude study model: {study_model}")
        add(name, "7d", claude, [study_model], MODELS[study_model]["id"])
    return windows, {"claude_messages": len(claude), "codex_events": len(codex)}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--claude-dir", help="folder of Claude Code session logs")
    parser.add_argument("--codex-dir", help="folder of Codex session logs")
    parser.add_argument("--now", help="ISO time with an offset. Defaults to the current time")
    parser.add_argument("--u", action="append", default=[], metavar="NAME=PERCENT")
    parser.add_argument("--status", action="append", default=[], metavar="NAME=STATE")
    parser.add_argument("--model-window", action="append", default=[], metavar="NAME=STUDY_MODEL",
                        help="a seven day window that only counts one Claude study model")
    args = parser.parse_args(argv)
    try:
        now = _time(args.now) if args.now else datetime.now(timezone.utc)
        if now is None:
            raise ValueError("--now needs an ISO time with an offset")
        used = {k: float(v) for k, v in _pairs(args.u).items()}
        windows, counts = measure(
            Path(args.claude_dir) if args.claude_dir else None, Path(args.codex_dir) if args.codex_dir else None,
            now, used, _pairs(args.status), _pairs(args.model_window),
        )
    except (OSError, ValueError) as error:
        print(f"cannot judge: {error}", file=sys.stderr)
        return 2
    print(json.dumps(windows, indent=1, sort_keys=True))
    print(json.dumps(counts, sort_keys=True), file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
