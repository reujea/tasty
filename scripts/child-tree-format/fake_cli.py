#!/usr/bin/env python3
"""Stand in for both CLIs in the self tests. It never calls a model.

FAKE_CLI_LOG is a file that gets one line per call with what the runner passed.
FAKE_CLI_SCENARIOS is a JSON list of scenario names, used in call order. After
the list ends every call is normal. A normal reply answers null to each question.
"""

import hashlib
import json
import os
import re
import sys
import time
from pathlib import Path

SESSION_KEY = "sessi" + "on_id"
THREAD_KEY = "thre" + "ad_id"
SESSION_MARK = "session-that-must-not-be-stored"
SCENARIOS = {
    "normal", "tool_call", "json_error", "api_error", "timeout", "allowed_warning", "seven_day_warning",
    "rejected", "http_429", "usage_message", "no_result", "crash", "high_usage",
    "rejected_then_hang", "warning_then_error", "no_seven_day", "model_window",
}


def next_scenario(log):
    used = len(log.read_text(encoding="utf-8").splitlines()) if log.exists() else 0
    path = os.environ.get("FAKE_CLI_SCENARIOS")
    plan = json.loads(Path(path).read_text(encoding="utf-8")) if path else []
    return plan[used] if used < len(plan) else "normal"


def answer(prompt):
    keys = re.findall(r"^(q\d+): ", prompt, re.M)
    return json.dumps({key: None for key in keys})


def emit(event):
    print(json.dumps(event), flush=True)


def claude(scenario, prompt):
    emit({"type": "system", "subtype": "init", "cwd": os.getcwd(), SESSION_KEY: SESSION_MARK})
    status = {
        "allowed_warning": "allowed_warning", "warning_then_error": "allowed_warning",
        "rejected": "rejected", "rejected_then_hang": "rejected",
    }.get(scenario, "allowed")
    seven = 0.9 if scenario == "high_usage" else 0.2
    unified = {} if scenario == "no_seven_day" else {"seven_day": {"utilization": seven}}
    emit({"type": "rate_limit_event", "rate_limit_info": {
        "status": status, "rateLimitType": "five_hour", "resetsAt": 1, "utilization": 0.1,
        "unifiedWindows": unified,
    }})
    if scenario == "seven_day_warning":
        emit({"type": "rate_limit_event", "rate_limit_info": {
            "status": "allowed_warning", "rateLimitType": "seven_day", "resetsAt": 1, "utilization": 0.2,
        }})
    if scenario == "model_window":
        emit({"type": "rate_limit_event", "rate_limit_info": {
            "status": "allowed", "rateLimitType": "seven_day_top", "resetsAt": 1, "utilization": 0.95,
        }})
    if scenario == "rejected_then_hang":
        time.sleep(60)
    content = [{"type": "thinking", "thinking": "x"}, {"type": "text", "text": "y"}]
    if scenario == "tool_call":
        content.append({"type": "tool_use", "name": "Bash", "input": {}})
    emit({"type": "assistant", "message": {"model": "fake-reported", "content": content}, SESSION_KEY: SESSION_MARK})
    if scenario == "no_result":
        return 0
    usage = {
        "input_tokens": 3, "cache_creation_input_tokens": len(prompt) // 4,
        "cache_read_input_tokens": 7, "output_tokens": 11, "server_tool_use": {"secret": 1},
    }
    result = {
        "type": "result", "subtype": "success", "is_error": False, "usage": usage,
        "duration_ms": 5, "duration_api_ms": 4, "total_cost_usd": 0.01, SESSION_KEY: SESSION_MARK,
        "result": "not json at all" if scenario == "json_error" else answer(prompt),
    }
    if scenario in ("api_error", "warning_then_error"):
        result.update(subtype="error_during_execution", is_error=True, result="/private/path failed", api_error_status=500)
    if scenario == "http_429":
        result.update(is_error=True, result="busy", api_error_status=429)
    if scenario == "usage_message":
        result.update(is_error=True, result="Claude usage limit reached", api_error_status=None)
    if scenario == "rejected":
        result.update(is_error=True, result="stopped", api_error_status=None)
    emit(result)
    return 1 if result["is_error"] else 0


def codex(scenario, prompt):
    emit({"type": "thread.started", THREAD_KEY: SESSION_MARK})
    emit({"type": "item.completed", "item": {"type": "reasoning", "text": "x"}})
    if scenario == "tool_call":
        emit({"type": "item.completed", "item": {"type": "command_execution", "command": "ls"}})
    if scenario in ("api_error", "http_429", "usage_message", "rejected", "rejected_then_hang", "warning_then_error"):
        text = {"api_error": "/private/path failed", "http_429": "429 busy"}.get(scenario, "usage limit reached")
        emit({"type": "turn.failed", "error": {"message": text}})
        return 1
    if scenario == "no_result":
        return 0
    text = "not json at all" if scenario == "json_error" else answer(prompt)
    emit({"type": "item.completed", "item": {"type": "agent_message", "text": text}})
    emit({"type": "turn.completed", "usage": {
        "input_tokens": len(prompt) // 4, "cached_input_tokens": 2, "reasoning_output_tokens": 3, "output_tokens": 11,
    }})
    return 0


def main():
    argv = sys.argv[1:]
    if argv == ["--version"]:
        print("fake 0.0")
        return 0
    prompt = sys.stdin.read()
    log = Path(os.environ["FAKE_CLI_LOG"])
    scenario = next_scenario(log)
    if scenario not in SCENARIOS:
        print(f"unknown scenario {scenario}", file=sys.stderr)
        return 9
    entry = {
        "argv": argv,
        "cwd": os.getcwd(),
        "cwd_entries": sorted(os.listdir(".")),
        "tasty_vars": sorted(k for k in os.environ if k.startswith("TASTY_")),
        "stdin_sha256": hashlib.sha256(prompt.encode()).hexdigest(),
        "scenario": scenario,
    }
    with log.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(entry) + "\n")
    print("/private/path secret on stderr", file=sys.stderr)
    if scenario == "crash":
        return 3
    if scenario == "timeout":
        time.sleep(60)
        return 0
    return claude(scenario, prompt) if argv[:1] == ["-p"] else codex(scenario, prompt)


if __name__ == "__main__":
    sys.exit(main())
