#!/usr/bin/env python3
"""Run the planned calls of the study one at a time and record only the allowed fields.

Commands:
  run      run the pilot or the approved level, continuing from the raw file
  approve  record an approved level with its window values
  resume   decide whether an aborted run may continue and record the decision

Exit codes: 0 finished or stopped by --max-calls, 1 resume refused, 2 could not
judge, 3 aborted by the failure rules.
"""

import argparse
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path

sys.dont_write_bytecode = True

import check_rules
import policy
import usage_level
from cli_events import parse_claude, parse_codex
from common import DATA_DIR, LEVELS, MODELS, PILOT_RETRY_CAP, REPO_ROOT, SEED
from prompts import build_user_prompt, joined_prompt, prompt_hash

READ_ONLY_LINE = "Read only: do not create, change or delete any file."
DEFAULT_TIMEOUT = 600
RAW_PATHSPEC = ":(exclude)docs/research/child-tree-format/data/raw"


@dataclass
class Config:
    data: Path
    phase: str
    level: str
    provenance: dict
    bins: dict
    timeout: float = DEFAULT_TIMEOUT
    max_calls: int = None
    utilization_unit: str = "fraction"
    low_model_default_effort: bool = False
    codex_bypass_sandbox: bool = False
    repo_root: Path = REPO_ROOT
    versions: dict = field(default_factory=dict)


def now():
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%fZ")


def read_jsonl(path):
    if not path.exists():
        return []
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def append_line(path, row):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(row, sort_keys=True, ensure_ascii=False) + "\n")
        handle.flush()
        os.fsync(handle.fileno())


def raw_path(data, phase):
    return data / "raw" / f"{phase}.jsonl"


def log_path(data):
    return data / "raw/level-changes.jsonl"


def clean_env():
    """Return the environment without TASTY_ names. A hook would report the call as this lane."""
    return {k: v for k, v in os.environ.items() if not k.startswith("TASTY_")}


def schedule_name(cfg):
    return "pilot" if cfg.phase == "pilot" else cfg.level


def retry_cap(cfg):
    return PILOT_RETRY_CAP if cfg.phase == "pilot" else LEVELS[cfg.level]["retry_cap"]


def load_prompt(data, row):
    tree, fmt = row["tree"], row["format"]
    questions = json.loads((data / f"questions/{tree}.json").read_text(encoding="utf-8"))
    rendered = (data / f"rendered/{tree}/{fmt}.txt").read_text(encoding="utf-8").rstrip("\n")
    system = (data / "prompts/system.txt").read_text(encoding="utf-8").rstrip("\n")
    template = (data / "prompts/user.txt").read_text(encoding="utf-8").rstrip("\n")
    user = build_user_prompt(template, fmt, rendered, questions["questions"], questions["silent_minutes"], row["order"])
    if prompt_hash(system, user) != row["prompt_sha256"]:
        raise ValueError(f"prompt of {row['call_id']} does not match the schedule")
    return system, user


def command_for(cfg, row, system, user, folder):
    """Return (argv, stdin text) as the rules write them. The executable is called directly."""
    model = MODELS[row["model"]]
    if model["cli"] == "claude":
        argv = [
            cfg.bins["claude"], "-p", "--safe-mode", "--tools", "", "--strict-mcp-config",
            "--no-session-persistence", "--system-prompt", system, "--model", model["id"],
        ]
        if not (cfg.low_model_default_effort and row["model"] == "claude-low"):
            argv += ["--effort", model["effort"]]
        return argv + ["--output-format", "stream-json", "--verbose"], user
    argv = [
        cfg.bins["codex"], "exec", "--json", "--ephemeral", "--skip-git-repo-check", "--ignore-user-config",
        "-C", folder, "-m", model["id"], "-c", f"model_reasoning_effort={model['effort']}",
        "-c", "features.shell_tool=false", "-c", "project_doc_max_bytes=0",
    ]
    text = joined_prompt(system, user)
    if cfg.codex_bypass_sandbox:
        argv.append("--dangerously-bypass-approvals-and-sandbox")
        text = f"{READ_ONLY_LINE}\n{text}"
    return argv, text


def spawn(argv, text, folder, timeout):
    """Return (stdout, returncode, timed out). Standard error is dropped: it can hold paths."""
    process = subprocess.Popen(
        argv, cwd=folder, env=clean_env(), stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL, start_new_session=True,
    )
    try:
        stdout, _ = process.communicate(text.encode(), timeout=timeout)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        stdout, _ = process.communicate()
        return stdout.decode(errors="replace"), process.returncode, True
    return stdout.decode(errors="replace"), process.returncode, False


def run_call(cfg, row, attempt, kind):
    system, user = load_prompt(cfg.data, row)
    model = MODELS[row["model"]]
    with tempfile.TemporaryDirectory(prefix="child-tree-call-") as folder:
        argv, text = command_for(cfg, row, system, user, folder)
        started = now()
        clock = time.monotonic()
        stdout, returncode, timed_out = spawn(argv, text, folder, cfg.timeout)
        wall = int((time.monotonic() - clock) * 1000)
        finished = now()
    if model["cli"] == "claude":
        parsed = parse_claude(stdout, returncode, cfg.utilization_unit)
    else:
        parsed = parse_codex(stdout, returncode)
    if timed_out:
        parsed.update(error_kind="timeout", response_text=None)
    return {
        "call_id": row["call_id"],
        "attempt": attempt,
        "attempt_kind": kind,
        "phase": cfg.phase,
        "level": cfg.level,
        "tree": row["tree"],
        "format": row["format"],
        "model": row["model"],
        "repeat": row["repeat"],
        "model_requested": model["id"],
        "cli_version": cfg.versions.get(model["cli"]),
        "started_at": started,
        "finished_at": finished,
        "wall_ms": wall,
        "seed": SEED,
        "prompt_sha256": row["prompt_sha256"],
        **parsed,
        **cfg.provenance,
    }


def repo_status(root):
    done = subprocess.run(
        ["git", "-C", str(root), "status", "--porcelain", "--", ".", RAW_PATHSPEC],
        capture_output=True, text=True, check=True,
    )
    return done.stdout


def record_abort(cfg, ledger, reason, seen):
    """Write one abort entry per stopping point. A rerun must not add a second one."""
    log = read_jsonl(log_path(cfg.data))
    same = [e for e in log if e.get("kind") == "abort" and e.get("calls_seen") == seen and e.get("phase") == cfg.phase]
    if same:
        return
    append_line(log_path(cfg.data), {
        "kind": "abort", "phase": cfg.phase, "at": now(), "reason": reason, "calls_seen": seen,
        "level": cfg.level, "retries_used": ledger.retries_used, "values": ledger.abort_values,
    })


def run(cfg):
    if cfg.phase == "main":
        approved = policy.replay([], read_jsonl(log_path(cfg.data))).level
        if approved != cfg.level:
            print(f"cannot judge: approved level is {approved}, asked for {cfg.level}", file=sys.stderr)
            return 2
    rows = read_jsonl(cfg.data / f"schedules/{schedule_name(cfg)}.jsonl")
    if not rows:
        print("cannot judge: the schedule is empty", file=sys.stderr)
        return 2
    by_id = {r["call_id"]: r for r in rows}
    plan = list(by_id)
    path = raw_path(cfg.data, cfg.phase)
    before = repo_status(cfg.repo_root) if cfg.codex_bypass_sandbox else None
    ran = 0
    while True:
        records = read_jsonl(path)
        ledger = policy.replay(records, read_jsonl(log_path(cfg.data)), cfg.phase)
        action, value = policy.next_action(ledger, plan, retry_cap(cfg))
        if action == "abort":
            record_abort(cfg, ledger, value, len(records))
            print(f"aborted: {value} after {len(records)} raw lines, this run made {ran} calls")
            return 3
        if action == "done":
            print(f"finished: {len(ledger.done & set(plan))} of {len(plan)} planned, this run made {ran} calls")
            return 0
        if cfg.max_calls is not None and ran >= cfg.max_calls:
            print(f"stopped by the call limit: this run made {ran} calls")
            return 0
        call, attempt, kind = value
        append_line(path, run_call(cfg, by_id[call], attempt, kind))
        ran += 1
        if before is not None and repo_status(cfg.repo_root) != before:
            append_line(log_path(cfg.data), {
                "kind": "abort", "phase": cfg.phase, "at": now(), "reason": "repo_changed",
                "calls_seen": len(records) + 1, "level": cfg.level,
            })
            print("aborted: repo_changed")
            return 3


def approve(data, level, windows, approved_u):
    """Record an approved level. After the last planned call the level can no longer change."""
    records = read_jsonl(raw_path(data, "main"))
    log = read_jsonl(log_path(data))
    ledger = policy.replay(records, log)
    if ledger.level:
        rows = read_jsonl(data / f"schedules/{ledger.level}.jsonl")
        if rows and all(r["call_id"] in ledger.done for r in rows):
            print("refused: the approved level already finished its last call", file=sys.stderr)
            return 1
    append_line(log_path(data), {
        "kind": "level_change", "at": now(), "from": ledger.level, "to": level,
        "windows": windows, "approved_u": approved_u, "calls_seen": len(records),
    })
    return 0


def resume(data, phase, level, usage_result, approved_by=None):
    """Record a resume or the values that refuse it. Scores are never an input here."""
    records = read_jsonl(raw_path(data, phase))
    log = read_jsonl(log_path(data))
    ledger = policy.replay(records, log, phase)
    cap = PILOT_RETRY_CAP if phase == "pilot" else LEVELS[level]["retry_cap"]
    allowed_by_table = usage_level.level_allowed(level or "L0", usage_result)
    allowed, values = policy.resume_decision(ledger, cap, allowed_by_table, approved_by)
    values["usage"] = usage_result
    append_line(log_path(data), {
        "kind": "resume" if allowed else "resume_refused", "phase": phase, "at": now(),
        "level": level, "calls_seen": len(records), **values,
    })
    return 0 if allowed else 1


def version_of(binary):
    done = subprocess.run([binary, "--version"], env=clean_env(), capture_output=True, text=True, check=True)
    return done.stdout.strip()


def resolve_bins(args):
    bins = {}
    for name, value in (("claude", args.claude_bin), ("codex", args.codex_bin)):
        found = shutil.which(value)
        if not found:
            raise ValueError(f"{name} executable not found")
        bins[name] = found
    return bins


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--data", default=str(DATA_DIR))
    commands = parser.add_subparsers(dest="command", required=True)
    go = commands.add_parser("run")
    go.add_argument("--phase", choices=("pilot", "main"), required=True)
    go.add_argument("--level", choices=tuple(LEVELS))
    go.add_argument("--max-calls", type=int)
    go.add_argument("--timeout", type=float, default=DEFAULT_TIMEOUT)
    go.add_argument("--claude-bin", default="claude")
    go.add_argument("--codex-bin", default="codex")
    go.add_argument("--claude-utilization-unit", choices=("fraction", "percent"), required=True)
    go.add_argument("--low-model-default-effort", action="store_true")
    go.add_argument("--codex-bypass-sandbox", action="store_true")
    go.add_argument("--main-ref", default="origin/main")
    ok = commands.add_parser("approve")
    ok.add_argument("--level", choices=tuple(LEVELS), required=True)
    ok.add_argument("--windows", required=True, help="window values as JSON")
    ok.add_argument("--approved-u", required=True, help='approved u per CLI, such as {"claude": 69.9}')
    back = commands.add_parser("resume")
    back.add_argument("--phase", choices=("pilot", "main"), default="main")
    back.add_argument("--level", choices=tuple(LEVELS))
    back.add_argument("--usage", required=True, help="output of usage_level.py as JSON")
    back.add_argument("--approved-by")
    args = parser.parse_args(argv)
    data = Path(args.data)
    try:
        if args.command == "approve":
            return approve(
                data, args.level,
                json.loads(Path(args.windows).read_text(encoding="utf-8")), json.loads(args.approved_u),
            )
        if args.command == "resume":
            usage = json.loads(Path(args.usage).read_text(encoding="utf-8"))
            return resume(data, args.phase, args.level, usage, args.approved_by)
        if args.phase == "main" and not args.level:
            raise ValueError("the main run needs a level")
        bins = resolve_bins(args)
        cfg = Config(
            data=data, phase=args.phase, level=args.level, bins=bins, timeout=args.timeout, max_calls=args.max_calls,
            provenance=check_rules.rules_provenance(REPO_ROOT, args.main_ref),
            utilization_unit=args.claude_utilization_unit, low_model_default_effort=args.low_model_default_effort,
            codex_bypass_sandbox=args.codex_bypass_sandbox,
            versions={name: version_of(path) for name, path in bins.items()},
        )
        if args.low_model_default_effort or args.codex_bypass_sandbox:
            append_line(log_path(data), {
                "kind": "run_options", "phase": args.phase, "at": now(),
                "low_model_default_effort": args.low_model_default_effort,
                "codex_bypass_sandbox": args.codex_bypass_sandbox,
                "calls_seen": len(read_jsonl(raw_path(data, args.phase))),
            })
        return run(cfg)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"cannot judge: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
