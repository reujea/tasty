#!/usr/bin/env python3
"""Capture the live child forest into a local raw snapshot file. Read only.

The registry file path and the output path are arguments so that no local path
is written into a tracked file. The output holds real names and must stay out
of the repository. anonymize.py turns it into the published tree.

Exit codes: 0 captured, 2 the registry does not belong to the queried instance.
"""

import argparse
import json
import os
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

sys.dont_write_bytecode = True


def call_json(tasty, *args):
    """Return (parsed output, None), or (None, error text) when the command fails."""
    env = dict(os.environ)
    done = subprocess.run([tasty, *args], capture_output=True, text=True, env=env, check=False)
    if done.returncode != 0:
        return None, (done.stderr or done.stdout).strip()
    return json.loads(done.stdout), None


def live_children(tasty, parent):
    """Merge the Claude array and the Codex object listing of one parent."""
    merged = {}
    claude, _ = call_json(tasty, "claude", "children", "--surface", str(parent))
    for entry in claude or []:
        merged[int(entry["child_surface_id"])] = entry
    codex, _ = call_json(tasty, "codex", "children", "--surface", str(parent))
    for entry in (codex or {}).get("children", []):
        sid = entry.get("child_surface_id", entry.get("surface_id"))
        merged[int(sid)] = entry
    return merged


def surface_state(tasty, surface):
    """Follow terminal.state. A surface it cannot find is gone, so it has exited."""
    data, error = call_json(tasty, "terminal", "state", "--surface", str(surface))
    if data is not None:
        return data.get("state"), "terminal.state"
    if error and "no live surface" in error:
        return "exited", "terminal.state error: no live surface"
    raise RuntimeError("terminal.state failed for a surface")


def repo_key(cwd):
    """Name the repository a cwd belongs to, so that worktrees of one repo group together."""
    if not cwd or not Path(cwd).is_dir():
        return None
    done = subprocess.run(
        ["git", "-C", cwd, "rev-parse", "--path-format=absolute", "--git-common-dir"],
        capture_output=True, text=True, check=False,
    )
    return done.stdout.strip() if done.returncode == 0 else None


def forest_from_registry(registry):
    """Return {surface: node} walked from the registry's parent relations."""
    nodes = {}

    def node(surface):
        return nodes.setdefault(
            surface,
            {"surface": surface, "parent": None, "external_parent": None, "nickname": None, "cwd": None},
        )

    for child, parent in registry.get("parent_of", {}).items():
        node(int(child))["parent"] = int(parent)
        node(int(parent))
    for child, external in registry.get("external_parent_of", {}).items():
        node(int(child))["external_parent"] = external
    for parent, entries in registry.get("children", {}).items():
        for entry in entries:
            item = node(int(entry["child_surface_id"]))
            item["parent"] = int(parent)
            item["nickname"] = entry.get("nickname")
            item["cwd"] = entry.get("cwd")
        if entries:
            node(int(parent))
    for external, entries in registry.get("external_children", {}).items():
        for entry in entries:
            item = node(int(entry["child_surface_id"]))
            item["external_parent"] = external
            item["nickname"] = entry.get("nickname")
            item["cwd"] = entry.get("cwd")
    return nodes


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--registry", required=True, help="the child relationship file of the instance")
    parser.add_argument("--out", required=True, help="raw snapshot output, outside the repository")
    parser.add_argument("--tasty", default="tasty", help="tasty CLI to query")
    args = parser.parse_args(argv)

    registry = json.loads(Path(args.registry).read_text(encoding="utf-8"))
    nodes = forest_from_registry(registry)
    reports = registry.get("last_state_report_at", {})
    registered = {int(k): int(v) for k, v in registry.get("parent_of", {}).items()}

    parents = sorted({n["parent"] for n in nodes.values() if n["parent"] is not None})
    live = {}
    for parent in parents:
        for child, entry in live_children(args.tasty, parent).items():
            live[child] = entry
            if registered.get(child) != parent:
                print(
                    "cannot judge: a live child is not in this registry file; "
                    "it probably belongs to another instance",
                    file=sys.stderr,
                )
                return 2

    sources = {}
    for surface, item in nodes.items():
        entry = live.get(surface)
        if entry is not None:
            item["state"] = entry.get("state")
            sources["children"] = sources.get("children", 0) + 1
            item["cwd"] = entry.get("cwd") or item["cwd"]
            item["nickname"] = entry.get("nickname") or item["nickname"]
        else:
            item["state"], source = surface_state(args.tasty, surface)
            sources[source] = sources.get(source, 0) + 1
        at = reports.get(str(surface))
        item["last_state_report_at"] = int(at) if at is not None else None
        item["repo_key"] = repo_key(item["cwd"])

    outside = sorted(
        int(k) for k in set(registry.get("idle", {})) | set(reports) if int(k) not in nodes
    )
    snapshot = {
        "captured_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "state_sources": sources,
        "registry_surfaces_outside_forest": len(outside),
        "nodes": [nodes[k] for k in sorted(nodes)],
    }
    out = Path(args.out)
    out.write_text(json.dumps(snapshot, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    print(f"captured nodes={len(nodes)} parents={len(parents)} outside_forest={len(outside)}")
    print(f"state sources: {json.dumps(sources, sort_keys=True)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
