//! atlas의 begin_frame이 append_terminal_viewport보다 먼저, 같은 수신자에 대해 호출되는지 확인한다.
//! begin_frame은 LRU 프레임 번호를 갱신하고 프레임당 eviction 제한을 초기화한다.
//! 호출이 없으면 첫 eviction 뒤의 제거가 막힐 수 있다.
//!
//! 텍스트 위치와 루프 포함 여부를 검사한다. 조건 분기의 실행 여부와 프레임당 정확한 호출 횟수는
//! 입증하지 않는다. 수신자도 문자열로 비교하므로 같은 객체의 별칭을 다른 대상으로 볼 수 있다.
//! 프레임 진입 함수를 루프 안에서 부르는 곳은 한 단계 호출자 검사로 확인한다.
//!
//! append와 begin_frame을 함께 헬퍼로 옮기면 새 헬퍼가 검사 대상이 된다. 함수 명부가 바뀔 때는
//! 그 헬퍼의 호출자에 루프나 중복 호출이 있는지도 확인해야 한다.
//! atlas 접근 함수의 명부·비공개 여부도 검사하지만 append까지의 호출 경로 전체를 계산하지는 않는다.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::{
    CallSite, callers_of, enclosing_fn, fn_spans, line_of, loop_blocks, mask_non_code, rust_sources,
};

const ARM_CALL: &str = ".begin_frame(";
const APPEND_CALL: &str = ".append_terminal_viewport(";
const ATLAS_CALL: &str = ".atlas.get_or_insert(";
const GUARD_DIRS: &[&str] = &["src/source_guards/", "crates/tasty-doc-guards/"];
const FRAME_ENTRY_FNS: &[&str] = &["capture_surface_to_png", "render_terminals"];
const ATLAS_TOUCHING_FNS: &[&str] = &[
    "append_preedit_overlay",
    "push_cell_glyph",
    "push_overlay_glyph",
    "solid_texel",
];

fn rel_str(rel: &Path) -> String {
    rel.to_string_lossy().replace('\\', "/")
}

fn is_guard_file(rel: &str) -> bool {
    GUARD_DIRS.iter().any(|dir| rel.starts_with(dir))
}

/// 식별자와 점으로 된 수신자 경로만 읽는다. 호출 체인은 마지막 조각만 남으므로 객체 동일성을 보장하지 않는다.
fn receiver_before(masked: &str, call_at: usize) -> &str {
    let head = &masked[..call_at];
    let start = head
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
        .map_or(0, |i| i + 1);
    &head[start..]
}

/// 부분 식별자가 수신자로 일치하지 않도록 왼쪽 경계도 확인한다.
fn find_call_on(masked: &str, from: usize, to: usize, recv: &str, call: &str) -> Option<usize> {
    let needle = format!("{recv}{call}");
    masked[from..to]
        .match_indices(&needle)
        .map(|(at, _)| at + from)
        .find(|at| {
            masked[..*at]
                .chars()
                .next_back()
                .is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '.'))
        })
}

fn frame_entry_files() -> Vec<(PathBuf, String)> {
    rust_sources()
        .into_iter()
        .filter(|(rel, _)| !is_guard_file(&rel_str(rel)))
        .map(|(rel, src)| (rel, mask_non_code(&src)))
        .filter(|(_, masked)| masked.contains(APPEND_CALL))
        .collect()
}

fn frame_entry_fns() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (rel, masked) in frame_entry_files() {
        let rel = rel_str(&rel);
        let spans = fn_spans(&masked);
        for (append, _) in masked.match_indices(APPEND_CALL) {
            let (name, ..) = enclosing_fn(&spans, append).unwrap_or_else(|| {
                panic!(
                    "{rel}:{}의 `{APPEND_CALL}`이 속한 함수를 찾지 못했다. 함수 본문 추출을 확인한다.",
                    line_of(&masked, append)
                )
            });
            out.push((rel.clone(), name.clone()));
        }
    }
    out
}

#[test]
fn the_population_is_not_empty_and_names_itself() {
    let files: Vec<String> = frame_entry_files()
        .iter()
        .map(|(rel, _)| rel_str(rel))
        .collect();
    assert!(
        !files.is_empty(),
        "`{APPEND_CALL}`을 포함한 프레임 진입 함수를 찾지 못했다. 이름과 스캔 경로를 확인한다."
    );
    for rel in &files {
        assert!(
            rel.starts_with("src/gfx/"),
            "프레임 진입점이 `src/gfx/` 밖에서 나왔다: {rel}. 진짜 새 진입점이면 이 \
             단정을 넓혀라. 아니면 `{APPEND_CALL}` 이 다른 뜻으로 쓰인 것이다"
        );
    }
    // append 호출이 헬퍼로 이동한 경우를 경로 접두사만으로는 찾지 못하므로 함수 이름도 비교한다.
    let found: BTreeSet<String> = frame_entry_fns().into_iter().map(|(_, f)| f).collect();
    let expected: BTreeSet<String> = FRAME_ENTRY_FNS.iter().map(|s| (*s).to_string()).collect();
    assert_eq!(
        found, expected,
        "프레임 진입 함수 목록이 FRAME_ENTRY_FNS와 다르다. 실제 추가·이름 변경을 반영한다. 호출을 헬퍼로 옮겼다면 그 헬퍼의 호출자에서 반복 여부도 다시 확인한다."
    );
}

#[test]
fn every_frame_entry_arms_the_atlas_clock_before_it_appends() {
    let mut checked = 0usize;
    for (rel, masked) in frame_entry_files() {
        let rel = rel_str(&rel);
        let spans = fn_spans(&masked);
        for (append, _) in masked.match_indices(APPEND_CALL) {
            let (name, open, close, _) = enclosing_fn(&spans, append).unwrap_or_else(|| {
                panic!(
                    "{rel}:{}의 `{APPEND_CALL}`이 속한 함수를 찾지 못했다. 함수 본문 추출을 확인한다.",
                    line_of(&masked, append)
                )
            });
            // 다른 타입의 동명 begin_frame을 인정하지 않도록 append의 수신자와 맞춘다.
            let recv = receiver_before(&masked, append);
            let arm = find_call_on(&masked, *open, *close, recv, ARM_CALL).unwrap_or_else(|| {
                panic!(
                    "{rel}의 fn {name}에 `{recv}{APPEND_CALL}`은 있지만 `{recv}{ARM_CALL}`을 찾지 못했다. atlas 프레임을 시작하지 않으면 eviction 제한이 초기화되지 않는다. 수신자와 호출 위치를 확인하고 헬퍼로 옮겼다면 검사 범위도 갱신한다."
                )
            });
            assert!(
                arm < append,
                "{rel} 의 `fn {name}`: `{recv}{ARM_CALL}`(줄 {}) 이 `{APPEND_CALL}`(줄 {}) \
                 보다 뒤에 있다. 이번 프레임 glyph 가 지난 프레임 stamp 로 찍힌다",
                line_of(&masked, arm),
                line_of(&masked, append),
            );
            for (lo, hi) in loop_blocks(&masked, *open, *close) {
                assert!(
                    !(lo < arm && arm < hi),
                    "{rel}의 fn {name}: `{recv}{ARM_CALL}`(줄 {})이 줄 {}에서 시작하는 루프 안에 있다. 반복마다 eviction 제한을 초기화하면 같은 프레임에 페이지 제거가 반복될 수 있으므로 루프 밖에서 호출한다.",
                    line_of(&masked, arm),
                    line_of(&masked, lo),
                );
            }
            checked += 1;
        }
    }
    assert!(
        checked > 0,
        "append 호출을 검사하지 못했다. 함수 본문 추출을 확인한다."
    );
}

/// atlas 접근 함수가 바뀌면 append까지의 호출 경로를 다시 확인해야 한다. 이 시험 자체는 호출 경로를 계산하지 않는다.
#[test]
fn the_only_paths_into_the_atlas_are_the_four_known_ones() {
    let mut found: BTreeSet<String> = BTreeSet::new();
    let mut public: Vec<String> = Vec::new();
    for (rel, src) in rust_sources() {
        let rel = rel_str(&rel);
        if is_guard_file(&rel) {
            continue;
        }
        let masked = mask_non_code(&src);
        if !masked.contains(ATLAS_CALL) {
            continue;
        }
        let spans = fn_spans(&masked);
        for (pos, _) in masked.match_indices(ATLAS_CALL) {
            let (name, _, _, kw) = enclosing_fn(&spans, pos).unwrap_or_else(|| {
                panic!(
                    "{rel}:{} 의 `{ATLAS_CALL}` 를 품는 함수를 못 찾았다",
                    line_of(&masked, pos)
                )
            });
            let line_start = masked[..*kw].rfind('\n').map_or(0, |i| i + 1);
            if masked[line_start..*kw].contains("pub") {
                public.push(format!("{rel}: fn {name}"));
            }
            found.insert(name.clone());
        }
    }
    let expected: BTreeSet<String> = ATLAS_TOUCHING_FNS
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    assert_eq!(
        found, expected,
        "atlas 접근 함수 목록이 바뀌었다. 새 호출자의 경로를 따라 append를 거치는지 확인하고, 우회 경로가 있으면 frame_entry_files의 검사 범위를 넓힌다."
    );
    assert!(
        public.is_empty(),
        "atlas 에 닿는 함수가 공개로 바뀌었다: {public:?}. 공개면 append 를 안 거치는 \
         호출자가 생길 수 있고, 그러면 위 모수가 프레임 진입점 전체를 안 덮는다"
    );
}

#[test]
fn no_frame_entry_is_called_from_inside_a_loop() {
    let mut seen = 0usize;
    for name in FRAME_ENTRY_FNS {
        let sites = callers_of(name, GUARD_DIRS);
        assert!(
            !sites.is_empty(),
            "`{name}` 호출을 찾지 못했다. 함수 이름과 호출 삭제 여부를 확인한다."
        );
        for CallSite {
            rel,
            caller,
            in_loop,
            line,
            ..
        } in &sites
        {
            let caller = caller.as_deref().unwrap_or_else(|| {
                panic!(
                    "{rel}:{line}의 `{name}` 호출이 속한 함수를 찾지 못해 루프 포함 여부를 확인할 수 없다"
                )
            });
            assert!(
                !in_loop,
                "{rel}:{line}의 fn {caller}가 `{name}`을 루프 안에서 부른다. 반복마다 atlas 프레임 번호와 eviction 제한을 초기화하지 않도록 루프 밖에서 한 번 호출한다."
            );
            seen += 1;
        }
    }
    assert!(seen > 0, "프레임 진입 함수의 호출을 하나도 검사하지 못했다");
}
