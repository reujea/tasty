"""Failure, retry and resume rules of the study, as pure functions over the stored records.

The state is always rebuilt from the raw file and the level log, so a run that
was cut off continues with the same counts. Log entries carry calls_seen, the
number of raw lines that existed when they were written.
"""

from dataclasses import dataclass, field

from common import MODELS
from score import call_outcome

API_ABORTS = {"api_failure_exhausted"}
USAGE_ABORTS = {"usage_limit", "allowed_warning", "u_exceeded"}
U_MARGIN = 5


@dataclass
class Ledger:
    done: set = field(default_factory=set)
    attempts: dict = field(default_factory=dict)
    api_fails: dict = field(default_factory=dict)
    last_outcome: dict = field(default_factory=dict)
    violations: dict = field(default_factory=dict)
    retries_used: int = 0
    level: str = None
    approved_u: dict = field(default_factory=dict)
    abort: str = None
    abort_values: dict = field(default_factory=dict)
    calls_seen: int = 0


def _apply_record(ledger, record):
    call = record["call_id"]
    ledger.calls_seen += 1
    previous = ledger.attempts.get(call, 0)
    ledger.attempts[call] = previous + 1
    if previous:
        ledger.retries_used += 1
    outcome = call_outcome(record)
    ledger.last_outcome[call] = outcome
    windows = record.get("windows") or {}
    if outcome == "scored":
        ledger.done.add(call)
    elif outcome == "tool_violation":
        model = record["model"]
        ledger.violations[model] = ledger.violations.get(model, 0) + 1
        if ledger.violations[model] >= 2:
            _abort(ledger, "tool_violation_twice", model=model)
    elif outcome in ("api_error", "timeout"):
        ledger.api_fails[call] = ledger.api_fails.get(call, 0) + 1
        if ledger.api_fails[call] >= 3:
            _abort(ledger, "api_failure_exhausted", call=call, failures=ledger.api_fails[call])
    _apply_usage_stop(ledger, record, windows)
    cli = MODELS[record["model"]]["cli"]
    used = windows.get("seven_day_percent")
    allowed = ledger.approved_u.get(cli)
    if used is not None and allowed is not None and used >= allowed + U_MARGIN and not ledger.abort:
        _abort(ledger, "u_exceeded", used=used, approved=allowed, cli=cli)


def _apply_usage_stop(ledger, record, windows):
    """Stop on a usage signal whatever else the call did, so no retry spends more.

    Usage stops replace an API stop because they ask more of a resume. A second
    tool violation stays, because that run must never resume.
    """
    if ledger.abort == "tool_violation_twice":
        return
    call = record["call_id"]
    known = record.get("error_kind") in (None, "api_error", "timeout")
    if windows.get("status") == "rejected" or not known:
        _abort(ledger, "usage_limit", call=call, status_code=record.get("status_code"))
    elif windows.get("status") == "allowed_warning":
        _abort(ledger, "allowed_warning", call=call)


def _abort(ledger, reason, **values):
    ledger.abort = reason
    ledger.abort_values = values


def _apply_entry(ledger, entry):
    kind = entry.get("kind")
    if kind == "level_change":
        ledger.level = entry["to"]
        ledger.approved_u = entry.get("approved_u") or {}
    elif kind == "resume":
        ledger.abort = None
        ledger.abort_values = {}
    elif kind == "abort" and entry.get("reason") in ("retry_limit", "repo_changed"):
        # These two come from the runner, not from one raw line.
        ledger.abort = entry["reason"]
        ledger.abort_values = {}


def replay(records, log=(), phase="main"):
    """Return the ledger after the raw lines and the log entries of one phase."""
    ledger = Ledger()
    entries = [e for e in log if e.get("phase", "main") == phase]
    position = 0
    for index, record in enumerate(records):
        while position < len(entries) and entries[position]["calls_seen"] <= index:
            _apply_entry(ledger, entries[position])
            position += 1
        _apply_record(ledger, record)
    for entry in entries[position:]:
        _apply_entry(ledger, entry)
    return ledger


def next_action(ledger, plan, cap):
    """Return ("abort", reason), ("done", None) or ("run", (call id, attempt, kind))."""
    if ledger.abort:
        return "abort", ledger.abort
    open_calls = [c for c in plan if c not in ledger.done]
    if not open_calls:
        return "done", None
    front, fresh, deferred = [], [], []
    for call in open_calls:
        if not ledger.attempts.get(call):
            fresh.append(call)
        elif ledger.api_fails.get(call) == 2 and ledger.last_outcome.get(call) in ("api_error", "timeout"):
            deferred.append(call)
        else:
            front.append(call)
    if front:
        call, kind = front[0], "retry"
    elif fresh:
        return "run", (fresh[0], 1, "first")
    else:
        call, kind = deferred[0], "deferred"
    if ledger.retries_used >= cap:
        return "abort", "retry_limit"
    return "run", (call, ledger.attempts[call] + 1, kind)


def resume_decision(ledger, cap, usage_allows, approved_by=None):
    """Return (allowed, values). The values go to the level log either way."""
    reason = ledger.abort
    values = {
        "abort_reason": reason,
        "usage_allows": usage_allows,
        "retries_used": ledger.retries_used,
        "retry_cap": cap,
    }
    if reason in API_ABORTS:
        return usage_allows and ledger.retries_used < cap, values
    if reason in USAGE_ABORTS:
        values["approved_by"] = approved_by
        return bool(approved_by) and usage_allows, values
    # A tool violation, the retry limit and a changed repository never resume.
    return False, values
