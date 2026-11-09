"""Read the event streams of the two CLIs and keep only the allowed values.

Raw events can hold working paths and session ids, and error text can hold
anything, so nothing is copied unless it is named here.
"""

import json
import re

ALLOWED_ITEMS = {
    "claude": {"text", "thinking", "redacted_thinking"},
    "codex": {"agent_message", "reasoning"},
}
USAGE_FIELDS = (
    "input_tokens",
    "cache_creation_input_tokens",
    "cache_read_input_tokens",
    "cached_input_tokens",
    "reasoning_output_tokens",
    "output_tokens",
)
LIMIT_TEXT = re.compile(
    r"usage limit|rate limit|limit reached|quota|\b429\b|hit your|reached your|out of (extra )?usage|spend cap",
    re.I,
)
STATUS_RANK = {None: 0, "allowed": 1, "allowed_warning": 2, "rejected": 3}


def _events(stdout):
    for line in stdout.splitlines():
        try:
            event = json.loads(line)
        except ValueError:
            continue
        if isinstance(event, dict):
            yield event


def _usage(raw):
    if not isinstance(raw, dict):
        return None
    return {k: raw[k] for k in USAGE_FIELDS if k in raw}


def _percent(value, unit):
    if value is None:
        return None
    return value * 100 if unit == "fraction" else value


def _empty():
    return {
        "response_text": None,
        "usage": None,
        "model_reported": None,
        "duration_ms": None,
        "duration_api_ms": None,
        "total_cost_usd": None,
        "error_kind": None,
        "status_code": None,
        "tool_violations": 0,
        "windows": {"status": None, "by_type": {}, "seven_day_percent": None, "seven_day_by_window": {}},
    }


def _failure(out, message, status):
    out["status_code"] = status
    limited = status == 429 or (isinstance(message, str) and LIMIT_TEXT.search(message))
    out["error_kind"] = "rate_limited" if limited else "api_error"


def _window(out, info, unit):
    windows = out["windows"]
    status = info.get("status")
    if STATUS_RANK.get(status, 0) > STATUS_RANK.get(windows["status"], 0):
        windows["status"] = status
    kind = info.get("rateLimitType")
    used = _percent(info.get("utilization"), unit)
    if kind:
        windows["by_type"][kind] = {
            "status": status,
            "utilization_percent": used,
            "resets_at": info.get("resetsAt"),
        }
    unified = info.get("unifiedWindows") or {}
    seven = (unified.get("seven_day") or {}).get("utilization")
    seven = _percent(seven, unit) if seven is not None else (used if kind == "seven_day" else None)
    if seven is not None:
        windows["seven_day_percent"] = seven
    # A model can have its own seven day window, which reaches its limit first.
    if kind and kind.startswith("seven_day") and used is not None:
        windows["seven_day_by_window"][kind] = used
    for name, value in unified.items():
        reading = value.get("utilization") if isinstance(value, dict) else None
        if name.startswith("seven_day") and reading is not None:
            windows["seven_day_by_window"][name] = _percent(reading, unit)


def parse_claude(stdout, returncode, utilization_unit="fraction"):
    out = _empty()
    result = None
    for event in _events(stdout):
        kind = event.get("type")
        if kind == "assistant":
            message = event.get("message") or {}
            out["model_reported"] = message.get("model") or out["model_reported"]
            for item in message.get("content") or []:
                if isinstance(item, dict) and item.get("type") not in ALLOWED_ITEMS["claude"]:
                    out["tool_violations"] += 1
        elif kind == "rate_limit_event":
            _window(out, event.get("rate_limit_info") or {}, utilization_unit)
        elif kind == "result":
            result = event
    if out["windows"]["status"] == "rejected":
        out["error_kind"] = "rate_limited"
    if result is None:
        out["error_kind"] = out["error_kind"] or "api_error"
        return out
    out["usage"] = _usage(result.get("usage"))
    for name in ("duration_ms", "duration_api_ms", "total_cost_usd"):
        out[name] = result.get(name)
    if result.get("is_error") or result.get("subtype") != "success":
        if not out["error_kind"]:
            _failure(out, result.get("result"), result.get("api_error_status"))
        else:
            out["status_code"] = result.get("api_error_status")
        return out
    out["response_text"] = result.get("result")
    if returncode != 0 and not out["error_kind"]:
        out["error_kind"] = "api_error"
    return out


def parse_codex(stdout, returncode):
    out = _empty()
    message = None
    finished = False
    for event in _events(stdout):
        kind = event.get("type")
        if kind == "item.completed":
            item = event.get("item") or {}
            if item.get("type") == "agent_message":
                message = item.get("text")
            elif item.get("type") not in ALLOWED_ITEMS["codex"]:
                out["tool_violations"] += 1
        elif kind == "turn.completed":
            finished = True
            out["usage"] = _usage(event.get("usage"))
        elif kind in ("turn.failed", "error"):
            detail = event.get("error") if kind == "turn.failed" else event
            text = detail.get("message") if isinstance(detail, dict) else None
            status = detail.get("status_code") if isinstance(detail, dict) else None
            _failure(out, text, status)
    if out["error_kind"]:
        return out
    if not finished or returncode != 0 or message is None:
        out["error_kind"] = "api_error"
        return out
    out["response_text"] = message
    return out
