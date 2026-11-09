"""W measurement from local logs. Every log here is made up in a temporary folder."""

import contextlib
import io
import json
import tempfile
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path

import helpers  # noqa: F401

import measure_w
import usage_level as ul
from common import MODELS

NOW = datetime.now(timezone.utc).replace(microsecond=0)


def stamp(**ago):
    return (NOW - timedelta(**ago)).isoformat().replace("+00:00", "Z")


def claude_line(key, when, inp=0, create=0, read=0, out=0, model="claude-opus-5-5"):
    usage = {
        "input_tokens": inp, "cache_creation_input_tokens": create,
        "cache_read_input_tokens": read, "output_tokens": out,
    }
    return {"type": "assistant", "timestamp": when, "requestId": "r", "message": {"id": key, "model": model, "usage": usage}}


def codex_line(when, total, inp=0, cached=0, out=0):
    usage = {"input_tokens": inp, "cached_input_tokens": cached, "output_tokens": out, "total_tokens": inp + out}
    info = {"last_token_usage": usage, "total_token_usage": {"total_tokens": total}}
    return {"type": "event_msg", "timestamp": when, "payload": {"type": "token_count", "info": info}}


class Measure(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        self.claude = self.root / "claude"
        self.codex = self.root / "codex"

    def write(self, folder, name, rows):
        path = folder / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("".join(json.dumps(r) + "\n" for r in rows) + "not json\n", encoding="utf-8")

    def windows(self, **extra):
        result, _ = measure_w.measure(self.claude, self.codex, NOW, **extra)
        return {w["name"]: w for w in result}

    def test_cache_reads_never_count_for_either_cli(self):
        self.write(self.claude, "a.jsonl", [claude_line("m", stamp(hours=1), inp=1, create=2, read=9000, out=4)])
        self.write(self.codex, "a.jsonl", [codex_line(stamp(hours=1), 100, inp=100, cached=90, out=5)])
        out = self.windows()
        self.assertEqual(out["claude_7d"]["w"], 7)
        self.assertEqual(out["codex_7d"]["w"], 15)

    def test_a_repeated_message_id_counts_once_with_its_largest_output(self):
        rows = [
            claude_line("m", stamp(hours=1), inp=1, out=1),
            claude_line("m", stamp(hours=1), inp=1, out=9),
            claude_line("m", stamp(hours=1), inp=1, out=4),
        ]
        self.write(self.claude, "a.jsonl", rows)
        self.write(self.claude, "sub/agent.jsonl", rows[:1] + [claude_line("n", stamp(hours=1), inp=5)])
        self.assertEqual(self.windows()["claude_7d"]["w"], 15)

    def test_repeated_and_copied_codex_events_count_once(self):
        first = codex_line(stamp(hours=1), 10, inp=10)
        again = codex_line(stamp(hours=1, seconds=-1), 10, inp=10)
        second = codex_line(stamp(minutes=30), 20, inp=10)
        self.write(self.codex, "a.jsonl", [first, again, second])
        self.write(self.codex, "resumed.jsonl", [first, second, codex_line(stamp(minutes=1), 21, inp=1)])
        self.assertEqual(self.windows()["codex_7d"]["w"], 21)

    def test_spans_cut_on_time(self):
        rows = [
            claude_line("old", stamp(days=8), inp=1000),
            claude_line("week", stamp(days=6), inp=100),
            claude_line("hours", stamp(hours=4), inp=10),
            claude_line("future", stamp(hours=-1), inp=5000),
        ]
        self.write(self.claude, "a.jsonl", rows)
        out = self.windows()
        self.assertEqual((out["claude_7d"]["w"], out["claude_5h"]["w"]), (110, 10))

    def test_a_missing_log_stays_unread_and_the_table_reads_it_that_way(self):
        self.write(self.claude, "a.jsonl", [claude_line("m", stamp(hours=1), inp=10)])
        out = self.windows(used={"claude_7d": 10.0, "codex_7d": 3.0})
        self.assertIsNone(out["codex_7d"]["w"])
        result = ul.select_level(list(out.values()), {m: 1.0 for m in MODELS})
        self.assertEqual((result["proposal"], result["needs_decision"]), ("L2", True))

    def test_a_model_window_counts_only_that_model(self):
        rows = [
            claude_line("a", stamp(hours=1), inp=7, model=MODELS["claude-high"]["id"]),
            claude_line("b", stamp(hours=1), inp=30, model=MODELS["claude-low"]["id"]),
        ]
        self.write(self.claude, "a.jsonl", rows)
        out = self.windows(model_windows={"seven_day_top": "claude-high"})
        self.assertEqual((out["seven_day_top"]["w"], out["seven_day_top"]["models"]), (7, ["claude-high"]))
        self.assertEqual(out["claude_7d"]["w"], 37)
        with self.assertRaises(ValueError):
            self.windows(model_windows={"x": "codex"})

    def test_the_output_holds_only_numbers_and_names(self):
        marker = "SECRET-TEXT-IN-A-LOG"
        row = claude_line("m", stamp(hours=1), inp=10)
        row["message"]["content"] = [{"type": "text", "text": marker}]
        self.write(self.claude, "a.jsonl", [row])
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            rc = measure_w.main(["--claude-dir", str(self.claude), "--u", "claude_7d=12.5", "--status", "claude_7d=allowed"])
        self.assertEqual(rc, 0)
        for text in (out.getvalue(), err.getvalue()):
            self.assertNotIn(marker, text)
            self.assertNotIn(str(self.root), text)
        parsed = {w["name"]: w for w in json.loads(out.getvalue())}
        self.assertEqual((parsed["claude_7d"]["u"], parsed["claude_7d"]["status"]), (12.5, "allowed"))
        self.assertEqual(ul.select_level(list(parsed.values()), {m: 1.0 for m in MODELS})["needs_decision"], True)

    def test_bad_arguments_cannot_judge(self):
        for argv in (["--u", "nonsense"], ["--now", "yesterday"], ["--u", "x=abc"]):
            with self.subTest(argv=argv), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(measure_w.main(argv), 2)
