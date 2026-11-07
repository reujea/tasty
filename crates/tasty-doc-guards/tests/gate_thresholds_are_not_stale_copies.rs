//! 설정과 스크립트에 정의한 검사 임계값을 문서·주석의 현재 값과 대조한다.
//!
//! 등록한 파일에서는 현재 값을 적은 줄 수까지 비교해 한 사본만 낡는 경우를 잡는다.
//! 저장소 전체에서 값과 관련 어휘를 함께 찾고 새로 발견한 파일도 분류하도록 요구한다.
//! 과거 측정, 예시, 같은 숫자의 다른 의미는 이유를 적어 제외한다.
//!
//! 어휘 검색은 의미를 완전히 해석하지 못하므로 넓은 검색어와 앞뒤 두 줄을 유지한다.
//! 예외 파일이 사라지거나 분류 이유가 맞지 않게 되면 예외 목록도 수정해야 한다.
//! 수치 기록 원칙은 `docs/documentation-model.md`, 운영 기준은
//! `docs/dev-guide/complexity-gate.md`를 따른다.

use std::collections::BTreeSet;
use std::path::PathBuf;

/// 정본 — (파일, 값이 뒤에 붙는 접두).
const COGNITIVE_SOURCE: (&str, &str) = ("clippy.toml", "cognitive-complexity-threshold = ");
const SLOC_SOURCE: (&str, &str) = ("scripts/check-file-size.sh", "THRESHOLD=");
const SHARED_WALK_SOURCE: (&str, &str) = ("scripts/check-shared-walk-ratchet.sh", "CAP=");
const ALLOW_REASON_SOURCE: (&str, &str) = ("scripts/check-allow-reason.sh", "CAP=");
const FROZEN_SUM_SOURCE: (&str, &str) = (".complexity-file-allowlist", "# frozen-sum-budget: ");
const DOC_BIAS_SOURCE: (&str, &str) = (".complexity-file-allowlist", "# doc-comment-bias: ");

/// 게이트 어휘 — 이 중 하나가 ±2 줄 창에 있어야 그 자리를 "이 게이트에 대한 언급" 으로 센다.
const COGNITIVE_VOCAB: &[&str] = &[
    "cognitive",
    "복잡도",
    "threshold",
    "임계",
    "clippy.toml",
    "complexity",
];
const SLOC_VOCAB: &[&str] = &[
    "SLOC",
    "sloc",
    "tokei",
    "check-file-size",
    "상한",
    "임계",
    "복잡도",
    "complexity",
    "allowlist",
    "THRESHOLD",
];
const SHARED_WALK_VOCAB: &[&str] = &[
    "read_dir",
    "순회",
    "래칫",
    "상한",
    "임계",
    "shared-walk",
    "shared_walk",
    "CAP",
];
const ALLOW_REASON_VOCAB: &[&str] = &[
    "allow",
    "억제",
    "사유",
    "래칫",
    "상한",
    "임계",
    "allow-reason",
    "allow_reason",
    "CAP",
];
const FROZEN_SUM_VOCAB: &[&str] = &[
    "frozen-sum-budget",
    "frozen_sum",
    "예산",
    "동결",
    "총합",
    "래칫",
    "budget",
];
/// tokei만 함께 나온 비율·개수 설명을 편향 값으로 오해하지 않도록 편향에 직접 관련된 어휘만 사용한다.
const DOC_BIAS_VOCAB: &[&str] = &["doc-comment-bias", "doc 주석", "누락", "편향", "bias"];

/// 현재 cognitive 임계값을 설명하는 파일과 해당 줄 수.
const COGNITIVE_CLAIMS: &[Claim] = &[
    ("Cargo.toml", 1),
    // rca와의 비교 설명 및 설정값.
    ("clippy.toml", 2),
    ("docs/dev-guide/clippy-policy.md", 1),
    ("docs/dev-guide/complexity-gate.md", 1),
];

/// 현재 파일 SLOC 임계값을 설명하는 파일과 해당 줄 수.
const SLOC_CLAIMS: &[Claim] = &[
    (".github/workflows/complexity-check.yml", 1),
    // 합성 500줄이 임계 미만이라는 테스트의 전제도 현재 값 설명으로 분류한다.
    ("tests/file_sloc_gate_fails_loudly.rs", 1),
    ("docs/dev-guide/clippy-policy.md", 1),
    ("docs/dev-guide/complexity-gate.md", 3),
    ("scripts/check-file-size.sh", 1),
];

/// 현재 공유 순회 상한을 설명하는 파일과 해당 줄 수.
const SHARED_WALK_CLAIMS: &[Claim] = &[
    ("scripts/check-shared-walk-ratchet.sh", 1),
    // 테스트가 상한을 치환할 때 쓰는 문자열도 실제 값과 맞아야 한다.
    ("tests/shared_walk_gate.rs", 2),
];

/// 현재 사유 없는 allow 상한을 설명하는 파일과 해당 줄 수.
const ALLOW_REASON_CLAIMS: &[Claim] = &[
    // 과거 측정은 당시 값으로 남기고 현재 상한 선언만 대조한다.
    ("scripts/check-allow-reason.sh", 1),
    ("tests/allow_reason_gate.rs", 2),
];

/// 현재 동결 총합 예산을 설명하는 파일과 해당 줄 수.
const FROZEN_SUM_CLAIMS: &[Claim] = &[(".complexity-file-allowlist", 1)];

/// 현재 계측 편향을 설명하는 파일과 해당 줄 수. 과거 측정은 별도로 분류한다.
const DOC_BIAS_CLAIMS: &[Claim] = &[(".complexity-file-allowlist", 1)];

/// 파일과 현재 값이 나타나는 줄 수. 파일 안의 한 설명만 낡은 경우도 찾되 무관한 줄 이동에는 영향을 받지 않는다.
type Claim = (&'static str, usize);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    /// 결정·릴리스 당시의 값. 현재 설정에 맞춰 바꾸지 않는다.
    Dated,
    /// 같은 토큰의 다른 뜻 — webhook `threshold: 20` · `wal_autocheckpoint 1000` 등.
    OtherMeaning,
    /// 이 가드 자신 — 명부와 사유를 본문에 들고 있어 스캔에 걸린다.
    SelfRef,
}

/// 검색 결과 중 현재 값 설명이 아닌 파일과 제외 근거.
const EXCLUDED: &[(&str, Kind, &str)] = &[
    (
        "src/app/anomaly_settings.rs",
        Kind::OtherMeaning,
        "SlowLoop 기본 반복 건수 20을 검증하는 시험이며 코드 복잡도 기준이 아니다",
    ),
    (
        "crates/tasty-settings/src/anomaly_detection.rs",
        Kind::OtherMeaning,
        "CallBurst 기본 호출 건수 1000이며 파일 SLOC 기준이 아니다",
    ),
    (
        "crates/tasty-telemetry/src/anomaly_thresholds.rs",
        Kind::OtherMeaning,
        "CallBurst 시간 창 1000ms와 기본 호출 건수이며 파일 SLOC 기준이 아니다",
    ),
    (
        "docs/features/telemetry/index.md",
        Kind::OtherMeaning,
        "anomaly_detection의 SlowLoop 반복 건수 기본 20과 RSS 샘플 범위 3~20이며 코드 복잡도 기준이 아니다",
    ),
    (
        "docs/dev-guide/agent-runner.md",
        Kind::OtherMeaning,
        "task 그래프의 MAX_GRAPH_TASKS 상한이며 파일 SLOC 임계값이 아니다",
    ),
    (
        "crates/tasty-agent/src/task/store/graph_submit.rs",
        Kind::OtherMeaning,
        "task 그래프 하나의 task 수 상한 1000 이며 파일 SLOC 임계값이 아니다",
    ),
    (
        "CHANGELOG.md",
        Kind::Dated,
        "릴리스 시점의 서술 — 파일 전체가 \"그때 무엇이 바뀌었나\" 라서 현재 상태 주장이 아니다. 그래서 파일 단위로 뺀다",
    ),
    (
        "docs/features/webhook/index.md",
        Kind::OtherMeaning,
        "webhook 남용 차단 threshold 기본 20",
    ),
    (
        "src/webhook/abuse.rs",
        Kind::OtherMeaning,
        "webhook 남용 차단 threshold 기본 20",
    ),
    (
        "src/adapters/ui/popup/command_palette.rs",
        Kind::OtherMeaning,
        "목록이 상한에 걸린 뒤 높이가 안 움직이는지 보는 시험의 항목 수 1000 — 줄 수가 아니다",
    ),
    (
        "docs/design/systems/memory.md",
        Kind::OtherMeaning,
        "SQLite wal_autocheckpoint 1000 페이지",
    ),
    (
        "crates/tasty-memory/src/lib.rs",
        Kind::OtherMeaning,
        "SQLite wal_autocheckpoint 1000 페이지",
    ),
    (
        "crates/tasty-memory/src/tests.rs",
        Kind::OtherMeaning,
        "테스트 픽스처의 행 수",
    ),
    (
        "tests/refusals_are_told_apart_by_their_own_words.rs",
        Kind::OtherMeaning,
        "scripts 아래 셸 스크립트 수를 잰 측정값 27 이며 doc 주석 편향이 아니다",
    ),
    (
        "crates/tasty-plugin-agent-stream/src/handlers.rs",
        Kind::OtherMeaning,
        "POLL_MAX_LIMIT = 1000",
    ),
    (
        "crates/tasty-plugin-manifest/src/types.rs",
        Kind::OtherMeaning,
        "HOOK_TIMEOUT_MS_MAX = 1000",
    ),
    (
        "crates/tasty-telemetry/src/anomaly.rs",
        Kind::OtherMeaning,
        "CALL_BURST_THRESHOLD = 1000",
    ),
    (
        "src/view/main/redraw.rs",
        Kind::OtherMeaning,
        "재시도 상한 1000",
    ),
    (
        "crates/tasty-doc-guards/tests/gate_thresholds_are_not_stale_copies.rs",
        Kind::SelfRef,
        "이 가드 자신의 명부·사유·모듈 주석",
    ),
];

fn repo_root() -> PathBuf {
    tasty_doc_guards::repo_root()
}

fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("읽을 수 없다: {} — {e}", path.display()))
}

/// 설정값을 읽지 못하면 오류로 처리한다. 기본값 0으로 대신하면 대조가 잘못 통과할 수 있다.
fn source_value(source: (&str, &str)) -> String {
    let (file, prefix) = source;
    parse_source_value(file, prefix, &read(file))
}

/// 이미 읽은 설정에서 값을 추출한다. 합성 입력으로 오류와 정상 판독을 검증하도록 파일 읽기와 분리한다.
fn parse_source_value(file: &str, prefix: &str, text: &str) -> String {
    let hits = text.match_indices(prefix).count();
    assert_eq!(
        hits, 1,
        "`{file}` 에서 `{prefix}` 가 {hits} 번 나온다 — 정확히 1 번이어야 정본을 읽을 수 있다"
    );
    let rest = &text[text.find(prefix).expect("위에서 1 번을 확인했다") + prefix.len()..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    assert!(
        !digits.is_empty(),
        "`{file}` 의 `{prefix}` 뒤에 숫자가 없다 — 형태가 바뀌었으면 이 판독기를 고쳐라"
    );
    digits
}

/// 숫자가 더 긴 수나 단위 표현의 일부인지 구별한다.
fn claims_value(text: &str, value: &str, vocab: &[&str]) -> bool {
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if !has_bare_token(line, value) {
            continue;
        }
        let lo = i.saturating_sub(2);
        let hi = (i + 3).min(lines.len());
        if lines[lo..hi]
            .iter()
            .any(|w| vocab.iter().any(|v| w.contains(v)))
        {
            return true;
        }
    }
    false
}

/// 문장 끝 마침표는 경계로 보고, 숫자와 이어진 소수점·버전 표기의 점은 경계에서 제외한다.
fn has_bare_token(line: &str, token: &str) -> bool {
    let alnum = |c: char| c.is_ascii_alphanumeric() || c == '_';
    line.match_indices(token).any(|(at, _)| {
        let head = &line[..at];
        let tail = &line[at + token.len()..];
        let before = match head.chars().next_back() {
            Some(c) if alnum(c) => true,
            Some('.') => head[..head.len() - 1]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_digit()),
            _ => false,
        };
        let after = match tail.chars().next() {
            Some(c) if alnum(c) => true,
            Some('.') => tail[1..].chars().next().is_some_and(|c| c.is_ascii_digit()),
            _ => false,
        };
        !before && !after
    })
}

/// 추적 파일 중 텍스트로 훑을 것.
fn tracked_text_files() -> Vec<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root())
        .args(["ls-files"])
        .output()
        .unwrap_or_else(|e| panic!("`git ls-files` 를 실행할 수 없다 — {e}"));
    assert!(
        out.status.success(),
        "`git ls-files` 가 실패했다: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    let exts = [".md", ".rs", ".toml", ".sh", ".yml", ".yaml"];
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|f| exts.iter().any(|e| f.ends_with(e)))
        .map(str::to_string)
        .collect()
}

/// 어휘창을 통과한 파일 전체(과포함 스캔).
fn mentioning_files(value: &str, vocab: &[&str]) -> BTreeSet<String> {
    let root = repo_root();
    let mut out = BTreeSet::new();
    let files = tracked_text_files();
    if let Some(note) = scan_floor_note(files.len()) {
        panic!("{note}");
    }
    for rel in files {
        let path = root.join(rel.split('/').collect::<PathBuf>());
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if claims_value(&text, value, vocab) {
            out.insert(rel);
        }
    }
    out
}

/// 다른 의미의 숫자를 제외할 때 확인할 주제어. 넓은 검색 어휘와 구분한다.
/// 린트 이름은 억제 사유에도 나올 수 있어 제외하고 설정 키·파일명 등을 사용한다.
/// 주제어가 가까이 있다는 것만으로 자연어의 의미를 완전히 판정할 수는 없다.
const COGNITIVE_SUBJECT: &[&str] = &["cognitive-complexity-threshold", "clippy.toml"];
const SLOC_SUBJECT: &[&str] = &["check-file-size", "THRESHOLD=", "tokei"];
const SHARED_WALK_SUBJECT: &[&str] = &["check-shared-walk-ratchet", "shared_walk_gate"];
const ALLOW_REASON_SUBJECT: &[&str] = &["check-allow-reason", "allow_reason_gate"];
const FROZEN_SUM_SUBJECT: &[&str] = &[
    "frozen-sum-budget",
    "check-frozen-sum-ratchet",
    "frozen_sum_ratchet_gate",
];
const DOC_BIAS_SUBJECT: &[&str] = &["doc-comment-bias", "계측 편향"];

/// 설정·주장 명부·검색 어휘·주제어를 함께 등록해 여러 검사가 같은 게이트 목록을 사용한다.
struct Gate {
    label: &'static str,
    source: (&'static str, &'static str),
    claims: &'static [Claim],
    vocab: &'static [&'static str],
    subject: &'static [&'static str],
}

/// 직접 값이 선언된 게이트 목록. 셸 상수뿐 아니라 예외 목록의 예산·편향도 포함한다.
const GATES: &[Gate] = &[
    Gate {
        label: "cognitive",
        source: COGNITIVE_SOURCE,
        claims: COGNITIVE_CLAIMS,
        vocab: COGNITIVE_VOCAB,
        subject: COGNITIVE_SUBJECT,
    },
    Gate {
        label: "파일 SLOC",
        source: SLOC_SOURCE,
        claims: SLOC_CLAIMS,
        vocab: SLOC_VOCAB,
        subject: SLOC_SUBJECT,
    },
    Gate {
        label: "공용 순회 래칫",
        source: SHARED_WALK_SOURCE,
        claims: SHARED_WALK_CLAIMS,
        vocab: SHARED_WALK_VOCAB,
        subject: SHARED_WALK_SUBJECT,
    },
    Gate {
        label: "사유 없는 allow 래칫",
        source: ALLOW_REASON_SOURCE,
        claims: ALLOW_REASON_CLAIMS,
        vocab: ALLOW_REASON_VOCAB,
        subject: ALLOW_REASON_SUBJECT,
    },
    Gate {
        label: "동결 총합 예산",
        source: FROZEN_SUM_SOURCE,
        claims: FROZEN_SUM_CLAIMS,
        vocab: FROZEN_SUM_VOCAB,
        subject: FROZEN_SUM_SUBJECT,
    },
    Gate {
        label: "계측 편향",
        source: DOC_BIAS_SOURCE,
        claims: DOC_BIAS_CLAIMS,
        vocab: DOC_BIAS_VOCAB,
        subject: DOC_BIAS_SUBJECT,
    },
];

/// 검색 어휘와 같은 앞뒤 2줄 범위에서 좁은 주제어를 찾는다.
fn subject_near_value(text: &str, value: &str, subject: &[&str]) -> Option<(usize, String)> {
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if !has_bare_token(line, value) {
            continue;
        }
        let lo = i.saturating_sub(2);
        let hi = (i + 3).min(lines.len());
        for w in &lines[lo..hi] {
            if let Some(s) = subject.iter().find(|s| w.contains(**s)) {
                return Some((i + 1, (*s).to_string()));
            }
        }
    }
    None
}

fn mislabeled_meaning_note(
    file: &str,
    label: &str,
    subject: &str,
    line: usize,
    why: &str,
) -> String {
    format!(
        "{file}:{line}은 OtherMeaning으로 제외됐지만(사유: {why}) {label} 주제어 {subject}가 값 근처에 있다. 현재 설정값을 설명한다면 *_CLAIMS로 옮긴다. 정말 다른 의미라면 구분이 드러나게 문장을 다시 써라. 검사를 통과하려고 제외 분류만 바꾸지 않는다."
    )
}

/// 더 이상 검색에 걸리지 않는 제외 항목이 이후의 새 오류를 숨기지 않도록 제거를 안내한다.
fn dead_exclusion_note(file: &str, kind: Kind, why: &str) -> String {
    format!(
        "{file}은 제외 목록에 있지만 등록된 임계값 검색에 걸리지 않는다(분류 {kind:?}, 사유 {why}). 불필요해진 제외 항목을 제거한다. 여전히 해당 값을 설명하는데 검색어만 바뀌었다면 설명과 검색 범위를 먼저 확인한다."
    )
}

/// 값 없음·주변 어휘 없음·설명 줄 수 차이를 구별해 각기 다른 수정 방법을 안내한다.
fn claim_note(
    rel: &str,
    label: &str,
    value: &str,
    source_file: &str,
    text: &str,
    vocab: &[&str],
    copies: usize,
) -> Option<String> {
    if claims_value(text, value, vocab) {
        let seen = text.lines().filter(|l| has_bare_token(l, value)).count();
        return stale_copy_note(rel, label, value, seen, copies);
    }
    if text.lines().any(|l| has_bare_token(l, value)) {
        Some(format!(
            "  {rel}: {label} 값 {value}은 있지만 검색 어휘와 떨어져 있다. 여전히 해당 게이트를 설명한다면 의미를 명확히 하고, 설명을 없앴다면 *_CLAIMS를 갱신한다. 값을 다시 적지 마라."
        ))
    } else {
        Some(format!(
            "  {rel}: {label}의 현재 값 {value}이 아예 없다. 기준 파일 {source_file}과 대조해 오래된 설명을 고친다."
        ))
    }
}

fn unclassified_note(rel: &str, label: &str, value: &str) -> String {
    format!(
        "  {rel}: {label} 임계값 `{value}` 을 게이트 어휘와 함께 드는데 분류가 없다. \
         현재 상태를 주장하면 명부(`*_CLAIMS`)에, 시점 측정·인용·다른 뜻이면 \
         사유와 함께 `EXCLUDED` 에 넣어라"
    )
}

/// 추적 텍스트 수집이 하한보다 작으면 오류 안내를 반환한다.
fn scan_floor_note(seen: usize) -> Option<String> {
    (seen <= 500).then(|| {
        format!("추적 텍스트 파일을 {seen} 개만 읽었다 — 수집 범위를 확인한다. 읽은 파일이 없으면 위반도 발견할 수 없다")
    })
}

fn stale_copy_note(
    rel: &str,
    label: &str,
    value: &str,
    seen: usize,
    expected: usize,
) -> Option<String> {
    if seen == expected {
        return None;
    }
    Some(format!(
        "  {rel}: {label} 값 {value}을 든 줄이 {seen} 개다(명부 {expected}). 줄었다면 낡은 값으로 남아 있을 수 있다. 늘었다면 새 사본이 생긴 것인지 확인한다.\n  [시점] 과거 기록의 값을 고치지 마라. 당시 사실을 유지한다. 이 검사는 문맥과 관계없이 현재 숫자 토큰이 들어간 파일 전체의 줄을 세므로, 과거 기록에 같은 숫자가 있는 줄도 포함한다. 실제 집계 범위를 확인한 뒤 명부의 줄 수를 갱신한다. 시점 안내는 ci_channel_claims_match_workflows.rs의 TIME_NOTE를 따른다."
    ))
}

fn check(label: &str, source: (&str, &str), claims: &[Claim], vocab: &[&str]) -> Vec<String> {
    let value = source_value(source);
    let mut wrong = Vec::new();

    for (rel, copies) in claims {
        if let Some(note) = claim_note(rel, label, &value, source.0, &read(rel), vocab, *copies) {
            wrong.push(note);
        }
    }

    let known: BTreeSet<&str> = claims
        .iter()
        .map(|(f, _)| *f)
        .chain(EXCLUDED.iter().map(|(f, _, _)| *f))
        .collect();
    for rel in mentioning_files(&value, vocab) {
        if !known.contains(rel.as_str()) {
            wrong.push(unclassified_note(&rel, label, &value));
        }
    }
    wrong
}

#[test]
fn the_gate_thresholds_have_no_stale_copies() {
    let mut wrong = Vec::new();
    for g in GATES {
        wrong.extend(check(g.label, g.source, g.claims, g.vocab));
    }
    let sources: Vec<String> = GATES
        .iter()
        .map(|g| {
            format!(
                "`{}` 의 `{}` = {}",
                g.source.0,
                g.source.1.trim_end_matches(" = ").trim_end_matches('='),
                source_value(g.source)
            )
        })
        .collect();
    assert!(
        wrong.is_empty(),
        "게이트 임계값의 사본이 정본과 갈렸거나, 분류되지 않은 주장 자리가 생겼다.\n\
         정본: {}\n{}",
        sources.join(" · "),
        wrong.join("\n")
    );
}

/// 다른 의미로 제외한 값 근처에 해당 게이트 주제어가 있는지 확인한다. 판정한 항목 수가 0인 경우도 실패시킨다.
#[test]
fn a_different_meaning_exclusion_does_not_carry_the_gate_subject() {
    let values: Vec<String> = GATES.iter().map(|g| source_value(g.source)).collect();
    let mut checked = 0usize;
    let mut wrong = Vec::new();
    for (file, kind, why) in EXCLUDED {
        if *kind != Kind::OtherMeaning {
            continue;
        }
        let text = read(file);
        for (g, value) in GATES.iter().zip(&values) {
            if !claims_value(&text, value, g.vocab) {
                continue;
            }
            checked += 1;
            if let Some((line, hit)) = subject_near_value(&text, value, g.subject) {
                wrong.push(mislabeled_meaning_note(file, g.label, &hit, line, why));
            }
        }
    }
    assert!(
        checked > 0,
        "OtherMeaning으로 판정한 파일·값 쌍이 0개다. 분류 목록과 검색을 확인한다."
    );
    assert!(
        wrong.is_empty(),
        "다른 의미로 제외한 {}쌍 중 {}곳에 해당 게이트 주제어가 있다:\n{}",
        checked,
        wrong.len(),
        wrong.join("\n")
    );
}

/// 제외 파일이 여전히 검색에 걸리는지 확인한다. 숫자 경계는 이 검사의 ASCII 규칙을 사용한다.
#[test]
fn no_exclusion_is_dead_weight() {
    let values: Vec<String> = GATES.iter().map(|g| source_value(g.source)).collect();
    let mut dead = Vec::new();
    for (file, kind, why) in EXCLUDED {
        let text = read(file);
        if !GATES
            .iter()
            .zip(&values)
            .any(|(g, value)| claims_value(&text, value, g.vocab))
        {
            dead.push(dead_exclusion_note(file, *kind, why));
        }
    }
    assert!(
        dead.is_empty(),
        "더 이상 검색에 걸리지 않는 제외 항목이 {}개다:\n{}",
        dead.len(),
        dead.join("\n")
    );
}

mod threshold_mutations {
    use super::*;

    #[test]
    fn the_two_failure_states_are_told_apart() {
        let drifted = "함수 크기 상한(20) 을 넘기지 않도록.\n";
        assert!(!claims_value(drifted, "20", COGNITIVE_VOCAB));
        assert!(drifted.lines().any(|l| has_bare_token(l, "20")));
        let gone = "인지 복잡도 상한(25) 을 넘기지 않도록.\n";
        assert!(!claims_value(gone, "20", COGNITIVE_VOCAB));
        assert!(!gone.lines().any(|l| has_bare_token(l, "20")));
    }

    #[test]
    fn a_stale_copy_is_caught() {
        let doc = "임계 20 이다.\n";
        assert!(claims_value(doc, "20", COGNITIVE_VOCAB));
        assert!(
            !claims_value(doc, "25", COGNITIVE_VOCAB),
            "옛 값을 새 값으로 셌다"
        );
    }

    #[test]
    fn a_number_without_gate_vocabulary_is_not_a_claim() {
        assert!(!claims_value("포트 1000 을 연다.\n", "1000", SLOC_VOCAB));
    }

    #[test]
    fn the_vocabulary_window_reaches_two_lines() {
        let md = "파일 SLOC 상한을 말한다.\n중간 줄.\n값은 1000.\n";
        assert!(claims_value(md, "1000", SLOC_VOCAB));
        let far = "파일 SLOC 상한을 말한다.\n한 줄.\n두 줄.\n세 줄.\n값은 1000.\n";
        assert!(!claims_value(far, "1000", SLOC_VOCAB), "창 밖까지 셌다");
    }

    #[test]
    fn a_glued_token_is_not_counted() {
        assert!(!has_bare_token("연도는 2026 이다", "20"));
        assert!(
            has_bare_token("임계는 1000.", "1000"),
            "문장 끝의 마침표를 경계로 안 봤다"
        );
        assert!(!has_bare_token("타임아웃 1000ms", "1000"));
        assert!(!has_bare_token("버전 1.1000.0", "1000"));
        assert!(has_bare_token("임계 1000 이다", "1000"));
    }

    #[test]
    fn every_excluded_entry_carries_a_reason() {
        for (file, _, why) in EXCLUDED {
            assert!(!why.trim().is_empty(), "{file} 의 제외 사유가 비었다");
        }
    }

    #[test]
    fn no_file_is_both_claimed_and_excluded() {
        let excluded: BTreeSet<&str> = EXCLUDED.iter().map(|(f, _, _)| *f).collect();
        for (rel, _) in GATES.iter().flat_map(|g| g.claims.iter()) {
            assert!(
                !excluded.contains(rel),
                "{rel} 가 명부와 제외목록에 둘 다 있다"
            );
        }
    }
}

/// 명부의 현재 수치가 아닌 독립된 합성값으로 오류 안내를 검증한다.
#[cfg(test)]
mod copy_count_wording {
    use super::stale_copy_note;

    #[test]
    fn a_matching_count_says_nothing() {
        assert!(stale_copy_note("a.md", "파일 SLOC", "1000", 3, 3).is_none());
    }

    #[test]
    fn a_shrunk_count_warns_that_one_copy_may_be_stale() {
        let m =
            stale_copy_note("a.md", "파일 SLOC", "1000", 2, 3).expect("오류 안내가 있어야 한다");
        assert!(m.contains("든 줄이 2 개다(명부 3)"), "{m}");
        assert!(m.contains("낡은 값으로 남아 있을 수 있다"), "{m}");
        assert!(m.contains("a.md"), "좌표가 없다: {m}");
    }

    #[test]
    fn a_grown_count_asks_whether_the_new_line_claims_the_present() {
        let m = stale_copy_note("a.md", "cognitive", "20", 4, 3).expect("오류 안내가 있어야 한다");
        assert!(m.contains("든 줄이 4 개다(명부 3)"), "{m}");
        assert!(m.contains("새 사본이 생긴 것"), "{m}");
    }

    #[test]
    fn the_note_forbids_editing_a_dated_line_and_points_at_the_existing_label() {
        let m =
            stale_copy_note("a.md", "파일 SLOC", "1000", 2, 3).expect("오류 안내가 있어야 한다");
        assert!(m.contains("[시점]"), "{m}");
        assert!(m.contains("값을 고치지 마라"), "{m}");
        assert!(m.contains("TIME_NOTE"), "라벨 정의를 안 가리킨다: {m}");
    }
}

/// 파일시스템과 실제 설정을 바꾸지 않고 합성 입력으로 판정별 안내를 확인한다.
#[cfg(test)]
mod judgment_wording {
    use super::{
        claim_note, mislabeled_meaning_note, parse_source_value, scan_floor_note,
        subject_near_value, unclassified_note,
    };

    const VOCAB: &[&str] = &["임계", "복잡도"];

    #[test]
    fn a_subject_two_lines_above_the_value_is_found() {
        let text = "check-file-size 를 설명한다\n\n상한은 1000 줄이다\n";
        let hit = subject_near_value(text, "1000", &["check-file-size"]);
        assert_eq!(hit, Some((3, "check-file-size".to_string())));
    }

    #[test]
    fn a_subject_three_lines_away_is_out_of_the_window() {
        let text = "check-file-size 를 설명한다\n\n\n\n상한은 1000 줄이다\n";
        assert_eq!(subject_near_value(text, "1000", &["check-file-size"]), None);
    }

    #[test]
    fn a_glued_value_is_not_a_site_even_next_to_the_subject() {
        let text = "check-file-size\n타임아웃 1000ms\n";
        assert_eq!(subject_near_value(text, "1000", &["check-file-size"]), None);
    }

    #[test]
    fn the_note_offers_the_roster_or_a_rewrite_but_not_a_relabel() {
        let m = mislabeled_meaning_note("a/b.md", "파일 SLOC", "check-file-size", 7, "사유");
        assert!(m.contains("a/b.md:7"), "{m}");
        assert!(m.contains("CLAIMS"), "명부로 옮기라는 안내가 없다: {m}");
        assert!(
            m.contains("문장을 다시 써라"),
            "다시 작성하라는 안내가 없다: {m}"
        );
        assert!(
            m.contains("검사를 통과하려고 제외 분류만 바꾸지 않는다"),
            "제외 분류만 바꾸지 말라는 안내가 없다: {m}"
        );
    }

    #[test]
    fn a_healthy_claim_says_nothing() {
        let text = "이 게이트의 임계는 20 이다.\n";
        assert!(claim_note("a.md", "cognitive", "20", "clippy.toml", text, VOCAB, 1).is_none());
    }

    #[test]
    fn a_value_far_from_the_vocabulary_is_told_not_to_rewrite_the_value() {
        let text = "복잡도를 말한다.\n한 줄.\n두 줄.\n세 줄.\n무관한 문장 20 개.\n";
        let m = claim_note("a.md", "cognitive", "20", "clippy.toml", text, VOCAB, 1)
            .expect("오류 안내가 있어야 한다");
        assert!(m.contains("떨어져"), "{m}");
        assert!(m.contains("값을 다시 적지 마라"), "{m}");
    }

    #[test]
    fn a_missing_value_names_the_source_of_truth() {
        let text = "이 게이트의 임계는 15 이다.\n";
        let m = claim_note("a.md", "cognitive", "20", "clippy.toml", text, VOCAB, 1)
            .expect("오류 안내가 있어야 한다");
        assert!(m.contains("아예 없다"), "{m}");
        assert!(m.contains("clippy.toml"), "정본을 안 가리킨다: {m}");
    }

    #[test]
    fn an_unclassified_site_is_offered_both_bins() {
        let m = unclassified_note("a.md", "파일 SLOC", "1000");
        assert!(m.contains("분류가 없다"), "{m}");
        assert!(m.contains("_CLAIMS"), "{m}");
        assert!(m.contains("EXCLUDED"), "{m}");
    }

    #[test]
    fn a_collapsed_scan_says_zero_would_be_green() {
        assert!(scan_floor_note(9000).is_none());
        let m = scan_floor_note(3).expect("오류 안내가 있어야 한다");
        assert!(m.contains("3 개만 읽었다"), "{m}");
        assert!(m.contains("수집 범위를 확인한다"), "{m}");
    }

    #[test]
    #[should_panic(expected = "정확히 1 번이어야")]
    fn a_source_prefix_that_appears_twice_refuses_to_guess() {
        parse_source_value("clippy.toml", "T = ", "T = 20\nT = 30\n");
    }

    #[test]
    #[should_panic(expected = "숫자가 없다")]
    fn a_source_prefix_without_digits_says_the_reader_is_stale() {
        parse_source_value("clippy.toml", "T = ", "T = abc\n");
    }

    #[test]
    fn a_well_formed_source_line_yields_the_value() {
        assert_eq!(parse_source_value("clippy.toml", "T = ", "T = 20\n"), "20");
    }
}
