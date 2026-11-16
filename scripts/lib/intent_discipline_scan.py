"""등록된 도메인 변경 메서드의 직접 호출을 찾는다. check-intent-discipline.sh가 부른다.

입력은 주석·문자열을 가린 사본과 원문이다. 줄 번호는 두 트리가 같다.
정규식은 공백·줄바꿈을 허용해 rustfmt가 `state` / `.popups` / `.open(`처럼 나눈 체인도 잡는다.
수신자 타입은 해석하지 않는다. 정책과 표지 규칙은 docs/design/flows/action-dispatch.md.

intent-exempt 표지는 호출이 차지하는 줄, 그 바로 위 줄, 바로 아래 줄 중 하나에 있으면 인정한다.
호출이 차지하는 줄은 수신자 체인의 첫 줄부터 메서드 이름이 있는 줄까지다.
체인의 첫 줄은 `.`으로 시작하는 줄을 위로 따라 올라가 찾는다.

출력: 한 줄에 위반 하나, `file:line: [domain] <원문 줄>`. line은 메서드 이름이 있는 줄이다.
"""

import os
import re
import sys

sys.dont_write_bytecode = True
from core_writer_scan import functions, enclosing

# The App applies frozen PresetEdit values; only store-save calls in this leaf are authorized.
EXECUTION_CALLS = {(
    "src/app/preset_editor.rs", "apply", method
) for method in ("save_workspace", "save_workspace_overwrite", "save_tab", "save_tab_overwrite",
                 "save_pane", "save_pane_overwrite")}

POPUP_METHODS = (
    "open|open_centered|open_centered_focused|open_at_top_of_scope"
    "|open_at_focused|close|toggle|toggle_focused"
)
PRESET_METHODS = (
    "save_workspace|save_workspace_overwrite|save_tab|save_tab_overwrite|save_pane"
    "|save_pane_overwrite|apply_workspace_preset|apply_tab_preset|apply_pane_preset"
)
SURFACE_METHODS = (
    "split_surface|close_surface_by_id|close_surface_by_id_no_snapshot"
    "|convert_surface_to_terminal|convert_surface_to_markdown|convert_surface_to_image"
    "|convert_surface_to_html|convert_surface_to_kind"
)
TAB_METHODS = (
    "add_kind_tab|add_kind_tab_to_pane|add_markdown_tab|add_html_tab|add_image_tab"
    "|add_empty_tab|add_tab_to_pane|close_tab_by_tab_id"
)

# 같은 줄에 여러 패턴이 맞으면 앞의 도메인 하나로 보고한다. 그룹 1은 메서드 이름이다.
PATTERNS = [
    ("popup", re.compile(r"\.\s*popups\s*\.\s*(" + POPUP_METHODS + r")\s*\("), False),
    ("preset", re.compile(r"\.\s*(" + PRESET_METHODS + r")\s*\("), False),
    ("preset", re.compile(r"\.\s*(delete|rename)\s*\(\s*PresetKind"), False),
    ("surface", re.compile(r"\.\s*(" + SURFACE_METHODS + r")\s*\("), False),
    ("tab", re.compile(r"\.\s*(" + TAB_METHODS + r")\s*\("), False),
    ("pane", re.compile(r"\.\s*(split_pane)\s*\("), True),
    ("workspace", re.compile(r"\.\s*(add_workspace)\s*\("), False),
]

CFG_TEST_RE = re.compile(r"#\[cfg\(test\)\]")
MOD_OPEN_RE = re.compile(r"^\s*(pub\s+)?mod\s+[A-Za-z0-9_]+\s*\{")
BLANK_RE = re.compile(r"^\s*$")


def rust_files(root, scan_dirs):
    out = []
    for d in scan_dirs:
        for dirpath, _dirs, files in os.walk(os.path.join(root, d)):
            for f in files:
                if f.endswith(".rs"):
                    full = os.path.join(dirpath, f)
                    out.append(os.path.relpath(full, root).replace(os.sep, "/"))
    return sorted(out)


def test_lines(rel, masked):
    """test로 보는 줄의 집합(1부터). `*_tests.rs` 파일 전체와 cfg(test) 다음 mod 본문만 추적한다."""
    n = len(masked)
    if rel.endswith("_tests.rs"):
        return set(range(1, n + 1))
    out = set()
    i = 0
    while i < n:
        if not CFG_TEST_RE.search(masked[i]):
            i += 1
            continue
        j = i + 1
        while j < n and BLANK_RE.match(masked[j]):
            j += 1
        if j >= n or not MOD_OPEN_RE.match(masked[j]):
            i += 1
            continue
        depth = 0
        k = j
        while k < n:
            depth += masked[k].count("{") - masked[k].count("}")
            out.add(k + 1)
            if depth <= 0 and k > j:
                break
            k += 1
        i = k + 1
    return out


def chain_start(masked, line):
    """메서드 줄(1부터)에서 `.`으로 시작하는 줄을 위로 따라가 수신자 체인의 첫 줄을 돌려준다."""
    first = line
    while first > 1 and masked[first - 1].lstrip().startswith("."):
        first -= 1
    return first


def exempted(orig, start, end):
    lo = max(start - 1, 1)
    hi = min(end + 1, len(orig))
    return any("intent-exempt" in orig[k - 1] for k in range(lo, hi + 1))


def main():
    args = sys.argv[1:]
    if len(args) < 3 or "--all" not in args or "--pane" not in args:
        print("usage: intent_discipline_scan.py <masked-root> <root> <dir>... "
              "--all <file>... --pane <file>...", file=sys.stderr)
        return 2
    masked_root, root = args[0], args[1]
    a = args.index("--all")
    p = args.index("--pane")
    scan_dirs = args[2:min(a, p)]
    exempt_all = set(args[a + 1:p] if a < p else args[a + 1:])
    exempt_pane = set(args[p + 1:a] if p < a else args[p + 1:])

    files = rust_files(masked_root, scan_dirs)
    if not files:
        print("[intent-discipline] 사본에서 .rs를 찾지 못했다.", file=sys.stderr)
        return 2

    for rel in files:
        if rel in exempt_all:
            continue
        with open(os.path.join(masked_root, rel), encoding="utf-8") as fh:
            text = fh.read()
        masked = text.split("\n")
        fns = functions(text)
        # 원문을 읽지 못한 줄은 사본을 사용한다. 이때 사유는 인정되지 않을 수 있다.
        try:
            with open(os.path.join(root, rel), encoding="utf-8") as fh:
                orig = fh.read().split("\n")
        except OSError:
            orig = []
        if len(orig) < len(masked):
            orig = orig + masked[len(orig):]
        tests = test_lines(rel, masked)
        line_starts = [0]
        for m in re.finditer("\n", text):
            line_starts.append(m.end())

        def line_of(pos):
            lo, hi = 0, len(line_starts) - 1
            while lo < hi:
                mid = (lo + hi + 1) // 2
                if line_starts[mid] <= pos:
                    lo = mid
                else:
                    hi = mid - 1
            return lo + 1

        hits = {}
        for dom, rx, pane_only in PATTERNS:
            if pane_only and rel in exempt_pane:
                continue
            for m in rx.finditer(text):
                if (rel, enclosing(fns, m.start()), m.group(1)) in EXECUTION_CALLS:
                    continue
                method_line = line_of(m.start(1))
                if method_line in hits:
                    continue
                first = min(line_of(m.start()), chain_start(masked, method_line))
                if first in tests or method_line in tests:
                    continue
                if exempted(orig, first, method_line):
                    continue
                hits[method_line] = dom
        for line in sorted(hits):
            print("%s:%d: [%s] %s" % (rel, line, hits[line], orig[line - 1]))
    return 0


if __name__ == "__main__":
    sys.exit(main())
