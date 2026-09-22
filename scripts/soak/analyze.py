#!/usr/bin/env python3
"""soak JSONL 분석 — 메모리 누수 판정.

tests/soak_memory.rs 가 기록한 JSONL 을 읽어 4계층 지표를 판정한다:

- L2 (heap 성장): warmup 제외 후 트리 RSS 에 OLS. 기울기 + 총증가 이중 조건.
- L3 (GPU):       wgpu allocated 카운트·egui-mesh 맵 len 이 기준선으로 복귀해야
                  PASS (정수 엄격 — 1 이라도 순증가면 FAIL). 호스트 explorer view 수도 같은
                  방식으로 본다(GPU 가 아니라 L2 지만 정수라 엄격 판정이 선다).
- L4 (핸들/프로세스): 자식 프로세스 수는 엄격, 핸들 수는 요동 허용치 내 복귀.

사용:
    python scripts/soak/analyze.py <soak-*.jsonl> [--warmup-frac 0.1] [--plot out.png]

exit code: 0=PASS, 1=FLAG(의심 — 재실행/attribution 권장), 2=FAIL(누수 확정).
판정 기준의 근거와 후속 절차: docs/dev-guide/memory-leak-soak.md
"""

import argparse
import json
import sys

# ── 판정 임계값 ──────────────────────────────────────────────────────────
RSS_SLOPE_BYTES_PER_CYCLE = 1024  # 이 이상 지속 증가면 FLAG
RSS_SLOPE_R2 = 0.5                # 기울기의 설명력 하한
RSS_GROWTH_FRAC = 0.05            # 후반 총증가 비율 임계
RSS_GROWTH_ABS = 20 * 1024 * 1024  # 후반 총증가 절대 임계 (20MB)
HANDLE_TOLERANCE = 64             # Windows 핸들 수 자연 요동 허용치


def load(path):
    meta, points = None, []
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            rec = json.loads(line)
            if "meta" in rec:
                meta = rec["meta"]
            else:
                points.append(rec)
    if not points:
        sys.exit(f"error: no checkpoint records in {path}")
    return meta, points


def series(points, getter):
    out = []
    for p in points:
        try:
            v = getter(p)
        except (KeyError, IndexError, TypeError):
            v = None
        if v is not None:
            out.append((p["cycle"], v))
    return out


def ols(pairs):
    """(x, y) 쌍의 OLS 기울기와 R². 점이 부족하면 (0, 0)."""
    n = len(pairs)
    if n < 3:
        return 0.0, 0.0
    xs = [x for x, _ in pairs]
    ys = [y for _, y in pairs]
    mx, my = sum(xs) / n, sum(ys) / n
    sxx = sum((x - mx) ** 2 for x in xs)
    if sxx == 0:
        return 0.0, 0.0
    sxy = sum((x - mx) * (y - my) for x, y in zip(xs, ys))
    slope = sxy / sxx
    syy = sum((y - my) ** 2 for y in ys)
    r2 = 0.0 if syy == 0 else (sxy**2) / (sxx * syy)
    return slope, r2


def mesh_targets_sum(p):
    return sum(
        w["stats"]["egui_mesh_targets"]
        + w["stats"]["egui_mesh_popup_targets"]
        + w["stats"]["egui_mesh_banner_targets"]
        for w in p["gpu"]["windows"]
    )


def explorer_views_sum(p):
    # 창마다의 explorer view 수. main 이 아닌 창은 null 이라 0 으로 센다. 이 칸이 없는 옛
    # JSONL 은 KeyError 로 series() 가 건너뛴다(판정 "insufficient data").
    return sum(w["explorer_views"] or 0 for w in p["gpu"]["windows"])


def gpu_allocated(p, kind):
    return p["gpu"]["wgpu"]["hub"][kind]["allocated"]


# 각 지표: (이름, getter, 판정 방식). strict=기준선 정수 복귀, tolerance=허용 요동.
BASELINE_METRICS = [
    ("gpu.textures", lambda p: gpu_allocated(p, "textures"), 0),
    ("gpu.buffers", lambda p: gpu_allocated(p, "buffers"), 0),
    ("gpu.texture_views", lambda p: gpu_allocated(p, "texture_views"), 0),
    ("gpu.bind_groups", lambda p: gpu_allocated(p, "bind_groups"), 0),
    ("egui_mesh_targets", mesh_targets_sum, 0),
    ("explorer_views", explorer_views_sum, 0),
    ("surfaces", lambda p: p["surfaces"], 0),
    ("proc_count", lambda p: p["tree"]["proc_count"], 0),
    ("handles", lambda p: p["handles"], HANDLE_TOLERANCE),
]


def judge_baseline(name, pairs, tolerance):
    """기준선(첫 post-warmup 체크포인트) 대비 최종값 복귀 판정."""
    if len(pairs) < 2:
        return ("PASS", f"{name}: insufficient data ({len(pairs)} points)")
    baseline, final = pairs[0][1], pairs[-1][1]
    delta = final - baseline
    if delta > tolerance:
        return ("FAIL", f"{name}: {baseline} -> {final} (+{delta}, tolerance {tolerance})")
    # 허용치 내라도 단조 증가 추세면 의심 (요동이 아니라 느린 누수일 수 있음)
    values = [v for _, v in pairs]
    increases = sum(1 for a, b in zip(values, values[1:]) if b > a)
    decreases = sum(1 for a, b in zip(values, values[1:]) if b < a)
    if delta > 0 and increases >= 3 and decreases == 0:
        return ("FLAG", f"{name}: monotonic +{delta} within tolerance — slow-leak suspect")
    return ("PASS", f"{name}: {baseline} -> {final} (Δ{delta:+})")


def judge_rss(name, pairs):
    if len(pairs) < 4:
        return ("PASS", f"{name}: insufficient data ({len(pairs)} points)")
    slope, r2 = ols(pairs)
    half = pairs[len(pairs) // 2 :]
    growth = half[-1][1] - half[0][1]
    base = half[0][1] or 1
    grew = growth > max(RSS_GROWTH_ABS, base * RSS_GROWTH_FRAC)
    sloped = slope > RSS_SLOPE_BYTES_PER_CYCLE and r2 > RSS_SLOPE_R2
    desc = f"{name}: slope {slope:.0f} B/cycle (R²={r2:.2f}), last-half growth {growth / 1048576:.1f}MB"
    if sloped and grew:
        return ("FAIL", desc + " — sustained growth")
    if sloped or grew:
        return ("FLAG", desc + " — one growth signal")
    return ("PASS", desc)


def plot(points, warmup_idx, out_path):
    try:
        import matplotlib

        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
    except ImportError:
        print("plot skipped: matplotlib not installed", file=sys.stderr)
        return
    panels = [
        ("tree RSS (MB)", lambda p: p["tree"]["rss_tree_bytes"] / 1048576),
        ("root RSS (MB)", lambda p: p["tree"]["rss_root_bytes"] / 1048576),
        ("handles/fd", lambda p: p["handles"]),
        ("proc count", lambda p: p["tree"]["proc_count"]),
        ("gpu textures", lambda p: gpu_allocated(p, "textures")),
        ("mesh targets", mesh_targets_sum),
    ]
    fig, axes = plt.subplots(3, 2, figsize=(12, 9), sharex=True)
    for ax, (title, getter) in zip(axes.flat, panels):
        pts = series(points, getter)
        ax.plot([x for x, _ in pts], [y for _, y in pts], marker=".")
        if warmup_idx < len(points):
            ax.axvline(points[warmup_idx]["cycle"], color="gray", linestyle="--", alpha=0.5)
        ax.set_title(title, fontsize=10)
        ax.grid(alpha=0.3)
    fig.suptitle("soak metrics (dashed line = warmup boundary)")
    fig.tight_layout()
    fig.savefig(out_path, dpi=100)
    print(f"plot: {out_path}")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("jsonl")
    ap.add_argument("--warmup-frac", type=float, default=0.1)
    ap.add_argument("--plot", metavar="OUT_PNG")
    args = ap.parse_args()

    meta, points = load(args.jsonl)
    warmup_idx = max(1, int(len(points) * args.warmup_frac))
    if len(points) - warmup_idx < 2:
        warmup_idx = 0  # 짧은 런 — warmup 컷 생략하고 전체 사용
    post = points[warmup_idx:]

    if meta:
        print(
            f"run: scenario={meta.get('scenario')} os={meta.get('os')} "
            f"profile={meta.get('profile')} duration={meta.get('duration_secs')}s"
        )
    print(f"checkpoints: {len(points)} total, {len(post)} post-warmup (cut {warmup_idx})\n")

    results = []
    results.append(judge_rss("rss_tree", series(post, lambda p: p["tree"]["rss_tree_bytes"])))
    results.append(judge_rss("rss_root", series(post, lambda p: p["tree"]["rss_root_bytes"])))
    for name, getter, tol in BASELINE_METRICS:
        results.append(judge_baseline(name, series(post, getter), tol))

    worst = "PASS"
    for verdict, desc in results:
        mark = {"PASS": "  ok ", "FLAG": " FLAG", "FAIL": " FAIL"}[verdict]
        print(f"[{mark}] {desc}")
        if verdict == "FAIL" or (verdict == "FLAG" and worst == "PASS"):
            worst = verdict

    print(f"\nverdict: {worst}")
    if worst != "PASS":
        print(
            "next: 같은 시나리오로 재현 확인 후 attribution 도구로 원인 규명 —\n"
            "  Linux heaptrack / macOS Instruments·leaks / Windows UMDH\n"
            "  (docs/dev-guide/memory-leak-soak.md 의 런북 참조)"
        )

    if args.plot:
        plot(points, warmup_idx, args.plot)

    sys.exit({"PASS": 0, "FLAG": 1, "FAIL": 2}[worst])


if __name__ == "__main__":
    main()
