"""Failure, retry and resume rules of the study, as pure functions over the stored records.

The state is always rebuilt from the raw file and the level log, so a run that
was cut off continues with the same counts. Log entries carry calls_seen, the
number of raw lines that existed when they were written.
"""

from dataclasses import dataclass, field

from common import MODELS
from score import call_outcome

API_ABORTS = {"api_failure_exhausted"}
USAGE_ABORTS = {"usage_limit", "allowed_warning", "u_exceeded", "usage_unread", "usage_unit_suspect"}
CALL_FAILURES = ("api_error", "timeout", "interrupted")
U_MARGIN = 5
UNIT_CHECK_FROM = 5
UNIT_CHECK_RATIO = 10
SEVEN_DAY_WINDOW = "seven_day"


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
    unread_usage_approved: bool = False
    abort: str = None
    abort_values: dict = field(default_factory=dict)
    calls_seen: int = 0
    warning_allowed: bool = False
    warnings_passed: list = field(default_factory=list)


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
    elif outcome in CALL_FAILURES:
        ledger.api_fails[call] = ledger.api_fails.get(call, 0) + 1
        if ledger.api_fails[call] >= 3:
            _abort(ledger, "api_failure_exhausted", call=call, failures=ledger.api_fails[call])
    _apply_usage_stop(ledger, record, windows)
    cli = MODELS[record["model"]]["cli"]
    readings = {cli: windows.get("seven_day_percent"), **(windows.get("seven_day_by_window") or {})}
    readings.pop("seven_day", None)
    _check_readings(ledger, cli, readings)
    unread = windows.get("seven_day_percent") is None
    if outcome == "scored" and record.get("phase") == "main" and cli == "claude" and unread:
        if not ledger.unread_usage_approved and _can_stop_for_usage(ledger):
            _abort(ledger, "usage_unread", call=call)


def _apply_usage_stop(ledger, record, windows):
    """Stop on a usage signal whatever else the call did, so no retry spends more.

    Usage stops replace an API stop because they ask more of a resume. A second
    tool violation stays, because that run must never resume.
    """
    if ledger.abort == "tool_violation_twice":
        return
    call = record["call_id"]
    known = record.get("error_kind") in (None, *CALL_FAILURES)
    if windows.get("status") == "rejected" or not known:
        _abort(ledger, "usage_limit", call=call, status_code=record.get("status_code"))
    elif windows.get("status") == "allowed_warning":
        warned = warned_windows(windows)
        if ledger.warning_allowed and only_seven_day(warned):
            ledger.warnings_passed.append({"call": call, "attempt": ledger.attempts[call], "windows": warned})
        else:
            _abort(ledger, "allowed_warning", call=call, windows=warned)


def warned_windows(windows):
    """Return the stored values of the windows that report the warning, unchanged."""
    by_type = windows.get("by_type") or {}
    return {name: info for name, info in by_type.items() if info.get("status") == "allowed_warning"}


def only_seven_day(warned):
    """Return whether a warning names seven day windows and nothing else. Naming none is not enough."""
    return bool(warned) and all(name.startswith(SEVEN_DAY_WINDOW) for name in warned)


def _can_stop_for_usage(ledger):
    return not ledger.abort or ledger.abort in API_ABORTS


def _check_readings(ledger, cli, readings):
    """Compare seven day readings with the approved u. A window without its own value uses the CLI value.

    A reading far under an approved value of its own means the unit was named
    wrong, and then the rise check could never fire. A window that only borrows
    the CLI value can be small for real, so it is left out of that check.
    """
    for name, used in readings.items():
        allowed = ledger.approved_u.get(name, ledger.approved_u.get(cli))
        if used is None or allowed is None or not _can_stop_for_usage(ledger):
            continue
        if used >= allowed + U_MARGIN:
            _abort(ledger, "u_exceeded", used=used, approved=allowed, cli=cli, window=name)
        elif name in ledger.approved_u and allowed >= UNIT_CHECK_FROM and used < allowed / UNIT_CHECK_RATIO:
            _abort(ledger, "usage_unit_suspect", used=used, approved=allowed, cli=cli, window=name)


def _abort(ledger, reason, **values):
    ledger.abort = reason
    ledger.abort_values = values


def _apply_entry(ledger, entry):
    kind = entry.get("kind")
    if kind == "level_change":
        ledger.level = entry["to"]
        ledger.approved_u = entry.get("approved_u") or {}
        ledger.unread_usage_approved = bool(entry.get("unread_usage_approved"))
    elif kind == "usage_note":
        _check_readings(ledger, entry["cli"], {entry["cli"]: entry["used_percent"]})
    elif kind == "resume":
        ledger.abort = None
        ledger.abort_values = {}
        ledger.warning_allowed = bool(entry.get("allow_warning"))
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
        elif ledger.api_fails.get(call) == 2 and ledger.last_outcome.get(call) in CALL_FAILURES:
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


def resume_decision(ledger, cap, usage_allows, approved_by=None, allow_warning=False):
    """Return (allowed, values). The values go to the level log either way.

    allow_warning continues past a seven day warning only. Every other stop is
    refused under it, so the option can never be the way out of a harder stop.
    """
    reason = ledger.abort
    values = {
        "abort_reason": reason,
        "usage_allows": usage_allows,
        "retries_used": ledger.retries_used,
        "retry_cap": cap,
    }
    if allow_warning:
        warned = ledger.abort_values.get("windows") or {}
        values.update(approved_by=approved_by, allow_warning=True, warned_windows=warned)
        return bool(approved_by) and usage_allows and reason == "allowed_warning" and only_seven_day(warned), values
    if reason in API_ABORTS:
        return usage_allows and ledger.retries_used < cap, values
    if reason in USAGE_ABORTS:
        values["approved_by"] = approved_by
        return bool(approved_by) and usage_allows, values
    # A tool violation, the retry limit and a changed repository never resume.
    return False, values
