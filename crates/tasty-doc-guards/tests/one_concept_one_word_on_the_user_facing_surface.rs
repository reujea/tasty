//! 번역 파일(앱 문구)의 용어 표기를 확인한다.
//! 사용자 가이드의 용어는 tasty-page 레포의 checks가 검사한다.
//! 패인은 페인으로, 일본어 ウインドウ는 ウィンドウ로 쓴다.
//! 한국어 창은 OS 창을 소유한 View를 뜻할 때 윈도우로 쓰고, 다른 의미는 문구별 근거를 등록한다.
//! 기준은 docs/concepts/ubiquitous-language.md의 Window/Popup 구분이다.
//!
//! 정의를 설명하는 docs와 CHANGELOG는 검사하지 않는다.
//! 표기 금지만으로는 빈 스캔이 통과하므로 권장 표기의 수와 파일 종류별 하한도 확인한다.
//! 자동 실행 경로는 docs/dev-guide/ci-gates.md에 있다.

use std::path::{Path, PathBuf};
use tasty_doc_guards::floored_walk::{Descend, Floor, walk_with_floor};

struct Retired {
    needle: &'static str,
    canonical: &'static str,
    /// 검사 대상이 비어도 금지 표기 0개로 통과하지 않게 한다.
    canonical_floor: usize,
    why: &'static str,
}

const RETIRED: &[Retired] = &[
    Retired {
        needle: "패인",
        canonical: "페인",
        canonical_floor: 40,
        why: "Pane의 한국어 표기는 페인이다. 2026-11-08에 번역 파일에서 68회를 측정했다. 문구 정리를 허용하려고 약 40%의 여유를 뒀다.",
    },
    Retired {
        needle: "ウインドウ",
        canonical: "ウィンドウ",
        canonical_floor: 12,
        why: "Window의 일본어 표기를 통일한다. 2026-09-07에 24회를 측정해 하한을 정했고 2026-11-08 측정은 108회다.",
    },
];

/// 창은 문맥별 예외가 있어 권장 표기의 하한을 따로 둔다.
const WINDOW_CANONICAL: (&str, usize) = ("윈도우", 60);

/// 파일·문구·횟수·근거를 등록해 같은 종류의 새 문구까지 자동 면제되지 않게 한다.
struct NotAWindow {
    path: &'static str,
    /// 그 자리를 집는 문구. 파일 안에서 정확히 `count` 번 나와야 한다.
    phrase: &'static str,
    count: usize,
    /// Window가 아님을 확인한 근거.
    evidence: &'static str,
}

const NOT_A_WINDOW: &[NotAWindow] = &[
    NotAWindow {
        path: "lang/ko.toml",
        phrase: "판정 창 안에서",
        count: 4,
        evidence: "settings.anomaly의 slow_loop·call_burst와 각 threshold caption은 호출을 세는 시간 구간을 뜻한다. src/app/anomaly_settings.rs의 thresholds가 slow_loop_window_secs·call_burst_window_secs를 밀리초로 변환해 detector에 전달한다.",
    },
    NotAWindow {
        path: "lang/ko.toml",
        phrase: "판정 창:",
        count: 2,
        evidence: "settings.anomaly.slow_loop_window_secs·call_burst_window_secs의 라벨이다. 두 unit은 초이며 src/view/settings/ui/tabs/anomaly_detection.rs가 crates/tasty-settings/src/anomaly_detection.rs의 해당 기간 필드에 연결한다. OS Window를 여는 항목이 아니다.",
    },
    NotAWindow {
        path: "crates/tasty-plugin-image/lang/ko.toml",
        phrase: "파일 선택 창",
        count: 1,
        evidence: "이미지 찾아보기가 여는 저장 피커다. src/adapters/ui/popup/defs.rs의 host PopupDef id file_picker이며 별도 OS View가 아니다.",
    },
    NotAWindow {
        path: "crates/tasty-plugin-clipboard-viewer/lang/ko.toml",
        phrase: "스냅샷 창은",
        count: 1,
        evidence: "매니페스트 `crates/tasty-plugin-clipboard-viewer/tasty-plugin.toml` 이 \
                   `[[contributes.popup]]` 로 선언한다",
    },
    NotAWindow {
        path: "crates/tasty-plugin-clipboard-viewer/lang/ko.toml",
        phrase: "기존 창을 앞으로",
        count: 1,
        evidence: "같은 popup 을 한 문장 안에서 두 번째로 가리킨다 — 근거는 위와 같은 매니페스트다",
    },
    NotAWindow {
        path: "crates/tasty-plugin-git-viewer/lang/ko.toml",
        phrase: "기존 창을 앞으로",
        count: 1,
        evidence: "매니페스트 `crates/tasty-plugin-git-viewer/tasty-plugin.toml` 이 \
                   `[[contributes.popup]]` 로 선언한다",
    },
    NotAWindow {
        path: "lang/ko.toml",
        phrase: "주소창으로 이동",
        count: 1,
        evidence: "낱말이 다르다 — 단축키 explorer_focus_address 의 라벨이고 탐색기의 address bar 다. Window 를 가리키지 않는다",
    },
    NotAWindow {
        path: "lang/ko.toml",
        phrase: "속성 창",
        count: 2,
        evidence: "설정 props_time_format 의 라벨·caption 이 탐색기 Properties popup 을 가리킨다. host PopupDef id explorer_properties(EXPLORER_PROPERTIES_POPUP_ID)이며 문구는 수령한 디자인의 ko 원문이다.",
    },
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/tasty-doc-guards 위로 두 단계가 레포 루트다")
        .to_path_buf()
}

const HOST_LANG_FLOOR: Floor = Floor {
    min: 4,
    measured: 4,
    measured_on: "2026-11-10",
    counted_on: tasty_doc_guards::floored_walk::CountedOn::Tree(
        "d4e70f3ef에 zh-Hans 언어 파일을 더한 트리 — lang 바로 아래 toml 파일 4개를 셌다.",
    ),
    why_this_gap: "호스트 번역 en/ko/ja/zh-Hans 네 파일은 모두 필요해 여유를 두지 않았다. 지원 언어가 바뀌면 이 기준도 함께 검토한다.",
};

const PLUGIN_LANG_FLOOR: Floor = Floor {
    min: 20,
    measured: 36,
    measured_on: "2026-11-10",
    counted_on: tasty_doc_guards::floored_walk::CountedOn::Tree(
        "d4e70f3ef에 zh-Hans 언어 파일을 더한 트리 — 번들 플러그인 lang의 toml 파일 36개를 셌다.",
    ),
    why_this_gap: "2026-11-10에 플러그인 9개 × 언어 4개로 36개 파일을 측정했다. 플러그인 4개가 정리되는 경우를 허용한 하한 20이다. 더 줄면 실제 감소인지 수집 오류인지 확인한다.",
};

type Surface = (String, String);

fn user_facing_files() -> Vec<Surface> {
    let root = repo_root();
    let mut out = collect(
        &root.join("lang"),
        &root,
        &HOST_LANG_FLOOR,
        Descend::Everything,
        &|rel| rel.ends_with(".toml"),
    );
    out.extend(collect(
        &root.join("crates"),
        &root,
        &PLUGIN_LANG_FLOOR,
        Descend::SkipBuildCaches,
        &|rel| rel.contains("/lang/") && rel.ends_with(".toml"),
    ));
    out
}

/// 같은 순회를 합성 트리에서도 검사할 수 있도록 루트·하한·필터를 받는다.
fn collect(
    dir: &Path,
    rel_base: &Path,
    floor: &Floor,
    descend: Descend,
    keep: &dyn Fn(&str) -> bool,
) -> Vec<Surface> {
    let walked = walk_with_floor(dir, rel_base, floor, descend, &|w| keep(&w.rel))
        .unwrap_or_else(|why| panic!("{why}"));
    walked
        .into_iter()
        .map(|w| {
            // 수집량 하한을 통과한 뒤에도 본문 읽기 실패를 보고해야 한다.
            let text = std::fs::read_to_string(&w.path)
                .unwrap_or_else(|e| panic!("사용자 표면 파일을 읽지 못했다: {} — {e}", w.rel));
            (w.rel, text)
        })
        .collect()
}

fn count_of(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

fn offenders(files: &[Surface], needle: &str) -> Vec<String> {
    files
        .iter()
        .filter_map(|(rel, text)| match count_of(text, needle) {
            0 => None,
            n => Some(format!("  {rel} — {n} 회")),
        })
        .collect()
}

fn total(files: &[Surface], needle: &str) -> usize {
    files.iter().map(|(_, t)| count_of(t, needle)).sum()
}

/// 명부 한 줄이 선언한 횟수와 실제 횟수가 어긋난 자리.
#[derive(Debug, PartialEq, Eq)]
struct Mismatch {
    path: String,
    phrase: String,
    found: usize,
    declared: usize,
}

/// 예외 파일이 수집 목록에 없으면 빈 위반 목록으로 처리하지 않고 오류를 낸다.
fn roster_mismatches(files: &[Surface], roster: &[NotAWindow]) -> Result<Vec<Mismatch>, String> {
    let mut out = Vec::new();
    for e in roster {
        let text = files
            .iter()
            .find(|(rel, _)| rel == e.path)
            .map(|(_, t)| t)
            .ok_or_else(|| e.path.to_string())?;
        let found = count_of(text, e.phrase);
        if found != e.count {
            out.push(Mismatch {
                path: e.path.to_string(),
                phrase: e.phrase.to_string(),
                found,
                declared: e.count,
            });
        }
    }
    Ok(out)
}

struct Uncovered {
    path: String,
    found: usize,
    registered: usize,
}

/// 한 예외 문구에 같은 단어가 여러 번 있으면 그 횟수도 곱해 계산한다.
fn uncovered(files: &[Surface], roster: &[NotAWindow], word: &str) -> Vec<Uncovered> {
    files
        .iter()
        .filter_map(|(rel, text)| {
            let found = count_of(text, word);
            let registered: usize = roster
                .iter()
                .filter(|e| e.path == rel)
                .map(|e| count_of(text, e.phrase) * count_of(e.phrase, word))
                .sum();
            (found != registered).then(|| Uncovered {
                path: rel.clone(),
                found,
                registered,
            })
        })
        .collect()
}

fn shared_evidence(roster: &[NotAWindow]) -> Option<(&NotAWindow, &NotAWindow)> {
    for (i, a) in roster.iter().enumerate() {
        for b in roster.iter().skip(i + 1) {
            if a.path == b.path && a.evidence == b.evidence {
                return Some((a, b));
            }
        }
    }
    None
}

fn population_is_balanced(shape: (usize, usize), min: (usize, usize)) -> bool {
    shape.0 >= min.0 && shape.1 >= min.1
}

/// (호스트 번역 파일 수, 플러그인 번역 파일 수).
fn population_shape(files: &[Surface]) -> (usize, usize) {
    let host = files.iter().filter(|(r, _)| r.starts_with("lang/")).count();
    let plugin = files
        .iter()
        .filter(|(r, _)| r.starts_with("crates/"))
        .count();
    (host, plugin)
}

#[test]
fn no_retired_spelling_survives_on_the_user_facing_surface() {
    let files = user_facing_files();
    for r in RETIRED {
        let found = offenders(&files, r.needle);
        assert!(
            found.is_empty(),
            "사용자 문구에 이전 표기 `{}`가 남았다. `{}`로 고친다. 이 표기는 예외를 허용하지 않는다:\n{}",
            r.needle,
            r.canonical,
            found.join("\n")
        );
    }
}

#[test]
fn each_canonical_word_is_still_present() {
    let files = user_facing_files();

    for r in RETIRED {
        let n = total(&files, r.canonical);
        assert!(
            n >= r.canonical_floor,
            "권장 표기 `{}`가 {}회뿐이다(하한 {}). 검사 대상 파일과 실제 문구 감소를 확인한다.\n근거: {}",
            r.canonical,
            n,
            r.canonical_floor,
            r.why
        );
    }

    let (word, floor) = WINDOW_CANONICAL;
    let n = total(&files, word);
    assert!(
        n >= floor,
        "권장 표기 `{word}`가 {n}회뿐이다(하한 {floor}, 2026-11-08 번역 파일 측정 107회). 번역 파일의 수집·문구를 확인한다."
    );
}

#[test]
fn every_registration_occurs_exactly_as_many_times_as_declared() {
    let files = user_facing_files();
    let mismatches = roster_mismatches(&files, NOT_A_WINDOW).unwrap_or_else(|missing| {
        panic!(
            "예외 파일 `{missing}`을 수집하지 못했다. 파일 이동·삭제와 검사 범위를 확인하고 예외 경로를 갱신한다."
        )
    });
    assert!(
        mismatches.is_empty(),
        "예외에 기록한 횟수와 실제 문구 수가 다르다. 새 문구의 근거와 삭제된 문구를 확인하고 목록을 갱신한다:\n{}",
        mismatches
            .iter()
            .map(|m| format!(
                "  {} — `{}` {} 회 (명부는 {} 회)",
                m.path, m.phrase, m.found, m.declared
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn every_remaining_window_word_is_registered_as_not_a_window() {
    let files = user_facing_files();
    let left = uncovered(&files, NOT_A_WINDOW, "창");
    assert!(
        left.is_empty(),
        "예외로 등록하지 않은 창 표기가 있다:\n{}\nOS Window를 뜻하면 윈도우로 바꾼다. 다른 뜻이면 매니페스트 popup 선언이나 PopupDef 등 확인한 근거와 함께 NOT_A_WINDOW에 등록한다.",
        left.iter()
            .map(|u| format!(
                "  {} — `창` {} 번인데 명부가 덮는 것은 {} 번",
                u.path, u.found, u.registered
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn registrations_in_the_same_file_do_not_share_evidence() {
    // 다른 파일의 같은 근거는 허용하고 같은 파일의 중복 근거만 검출한다.
    if let Some((a, b)) = shared_evidence(NOT_A_WINDOW) {
        panic!(
            "`{}`의 두 문구(`{}` · `{}`)에 같은 근거가 쓰였다. 각 문구가 Window를 뜻하지 않는 이유를 확인해 적는다.",
            a.path, a.phrase, b.phrase
        );
    }
}

#[test]
fn the_population_holds_both_kinds_of_user_facing_file() {
    // 호스트 번역 파일과 플러그인 번역 파일이 모두 수집됐는지 따로 확인한다.
    let (host, plugin) = population_shape(&user_facing_files());
    assert!(
        population_is_balanced((host, plugin), (HOST_LANG_FLOOR.min, PLUGIN_LANG_FLOOR.min)),
        "번역 파일 수집이 부족하다: 호스트 {host}, 플러그인 {plugin}. 2026-11-08 측정은 각각 3개, 27개다."
    );
}

/// 실제 파일을 만들지 않고 경로·본문 쌍으로 단어 집계와 예외 적용을 검사한다.
fn surface(pairs: &[(&str, &str)]) -> Vec<Surface> {
    pairs
        .iter()
        .map(|(p, t)| ((*p).to_string(), (*t).to_string()))
        .collect()
}

#[test]
fn the_offender_scan_names_the_file_and_its_count_and_skips_the_clean_ones() {
    let files = surface(&[
        ("zone/alpha.toml", "패인 을 나눕니다. 패인 이 둘입니다."),
        ("zone/bravo.toml", "페인 을 나눕니다."),
        ("zone/charlie.md", "패인 하나."),
    ]);
    assert_eq!(
        offenders(&files, "패인"),
        vec![
            "  zone/alpha.toml — 2 회".to_string(),
            "  zone/charlie.md — 1 회".to_string()
        ],
        "은퇴 표기가 있는 파일만, 그 파일의 횟수와 함께 나와야 한다"
    );
    assert!(
        offenders(&files, "ウインドウ").is_empty(),
        "없는 표기를 검출했다"
    );
    assert_eq!(total(&files, "페인"), 1, "정본 대조군을 못 셌다");
}

/// 예외 문구에 단어를 두 번 넣어 문구 내부 횟수도 계산하는지 확인한다.
#[test]
fn the_uncovered_scan_multiplies_by_the_word_count_inside_the_phrase() {
    let files = surface(&[("zone/delta.md", "이 창과 저 창은 다릅니다.")]);
    let roster = &[NotAWindow {
        path: "zone/delta.md",
        phrase: "창과 저 창",
        count: 1,
        evidence: "합성 자리 — 이 시험이 짓는다",
    }];
    assert!(
        uncovered(&files, roster, "창").is_empty(),
        "예외 문구 안의 단어 두 개를 모두 집계하지 못했다"
    );

    let files = surface(&[("zone/echo.md", "창 하나 더 있는 창.")]);
    let roster = &[NotAWindow {
        path: "zone/echo.md",
        phrase: "창 하나",
        count: 1,
        evidence: "합성 자리",
    }];
    let left = uncovered(&files, roster, "창");
    assert_eq!(left.len(), 1, "안 덮인 자리를 못 댔다");
    assert_eq!(
        (left[0].found, left[0].registered),
        (2, 1),
        "수를 잘못 셌다"
    );
}

#[test]
fn two_entries_in_one_file_may_not_share_evidence_but_two_files_may() {
    let same_file = &[
        NotAWindow {
            path: "zone/foxtrot.toml",
            phrase: "첫 창",
            count: 1,
            evidence: "같은 문장",
        },
        NotAWindow {
            path: "zone/foxtrot.toml",
            phrase: "둘째 창",
            count: 1,
            evidence: "같은 문장",
        },
    ];
    let hit = shared_evidence(same_file).expect("같은 파일 안의 근거 복사를 못 잡았다");
    assert_eq!(
        (hit.0.phrase, hit.1.phrase),
        ("첫 창", "둘째 창"),
        "겹친 두 줄을 이름으로 대야 한다"
    );

    let two_files = &[
        NotAWindow {
            path: "zone/foxtrot.toml",
            phrase: "첫 창",
            count: 1,
            evidence: "같은 문장",
        },
        NotAWindow {
            path: "zone/golf.toml",
            phrase: "첫 창",
            count: 1,
            evidence: "같은 문장",
        },
    ];
    assert!(
        shared_evidence(two_files).is_none(),
        "서로 다른 파일에는 같은 근거를 허용해야 한다"
    );
}

#[test]
fn the_roster_reports_a_count_that_grew_as_well_as_one_that_shrank() {
    let files = surface(&[
        ("zone/hotel.md", "확인 창이 뜹니다. 확인 창이 뜹니다."),
        ("zone/india.md", "아무것도 없습니다."),
    ]);
    let grew = &[NotAWindow {
        path: "zone/hotel.md",
        phrase: "확인 창이 뜹니다",
        count: 1,
        evidence: "합성 자리",
    }];
    let m = roster_mismatches(&files, grew).expect("모수 안의 파일인데 없다고 했다");
    assert_eq!(
        m.len(),
        1,
        "늘어난 쪽을 안 셌다 — 새 자리가 근거 없이 들어온다"
    );
    assert_eq!((m[0].found, m[0].declared), (2, 1), "수를 잘못 댔다");

    let shrank = &[NotAWindow {
        path: "zone/india.md",
        phrase: "확인 창이 뜹니다",
        count: 1,
        evidence: "합성 자리",
    }];
    let m = roster_mismatches(&files, shrank).expect("모수 안의 파일인데 없다고 했다");
    assert_eq!((m[0].found, m[0].declared), (0, 1), "줄어든 쪽을 안 셌다");

    let exact = &[NotAWindow {
        path: "zone/hotel.md",
        phrase: "확인 창이 뜹니다",
        count: 2,
        evidence: "합성 자리",
    }];
    assert!(
        roster_mismatches(&files, exact)
            .expect("모수 안의 파일이다")
            .is_empty(),
        "맞는 명부에 어긋남을 냈다"
    );
}

#[test]
fn a_roster_line_pointing_outside_the_population_is_an_error_not_a_skip() {
    let files = surface(&[("zone/juliett.md", "창 하나.")]);
    let moved = &[NotAWindow {
        path: "zone/kilo.md",
        phrase: "창 하나",
        count: 1,
        evidence: "합성 자리",
    }];
    assert_eq!(
        roster_mismatches(&files, moved),
        Err("zone/kilo.md".to_string()),
        "모수 밖을 가리키는 줄은 빈 목록이 아니라 그 경로를 대는 오류여야 한다"
    );
}

#[test]
fn the_population_must_clear_both_floors_not_either() {
    assert!(
        population_is_balanced((3, 15), (3, 15)),
        "딱 맞는데 거부했다"
    );
    assert!(
        !population_is_balanced((3, 5), (3, 15)),
        "플러그인 번역이 통째로 안 읽혔는데 호스트 쪽만 보고 통과시켰다"
    );
    assert!(
        !population_is_balanced((0, 27), (3, 15)),
        "호스트 번역이 통째로 안 읽혔는데 플러그인 쪽만 보고 통과시켰다"
    );
}
