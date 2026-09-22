//! ADR 인덱스(`docs/adr/index.md`)와 ADR 파일이 **한 카탈로그로 성립하는지** 못 박는다.
//!
//! 인덱스의 행(번호 · 제목 · Status · Date · Tags)은 ADR 헤더에서 **생성**된다 — 규칙은
//! `tasty_doc_guards::adr_index` 하나이고 `adr-index` bin 이 그것으로 파일을 다시 쓴다.
//! 결정과 대안은 `docs/adr/0565-the-adr-index-rows-are-generated-and-the-group-lives-in-the-adr-header.md`.
//!
//! 행이 생성물이 되면서 "행이 헤더와 같은 값을 싣는가" 는 더 이상 열 단위로 물을 필요가
//! 없다 — **파일이 생성 결과와 글자 단위로 같은가** 하나로 접힌다. 그 대신 생성이 답하지
//! 못하는 물음이 남고, 이 파일이 그것들을 잰다:
//!
//! 1. **번호가 식별자로 성립하는가** — 한 번호가 두 문서를 가리키지 않고, 본문 `# ADR-NNNN`
//!    제목이 파일명 번호와 같다. 번호는 여러 lane 이 병렬로 집어 조용히 겹친다(2026-09-05
//!    에 `0149` 가 두 ADR 에 붙은 채 main 에 얹혔다).
//! 2. **생성 구역이 생성 결과와 같은가** — 마커 안을 손으로 고쳤거나, ADR 을 더하고 다시
//!    안 만들었으면 여기서 빨개진다. 처방은 늘 같다: 헤더를 고치고 생성기를 돌린다.
//! 3. **그룹 배치** — 모든 ADR 이 정확히 한 그룹(`- **Group**:`)에 있고 그 그룹이 인덱스에
//!    실재하며, 인덱스의 모든 그룹에 ADR 이 있다. 배치 **판단**은 사람이 하지만, 판단이
//!    빠졌거나 둘로 갈렸거나 없는 그룹을 가리키는 것은 문면으로 갈린다.
//! 4. **결정 사슬이 양 끝에 적혔는가** — `Superseded by S` 면 S 가 이 ADR 을 부르고 같은
//!    그룹에 있다. `개정 대상` ↔ `부분 개정`, `통합 대상` ↔ `중복 조항 통합` 은 짝으로 있다.
//! 5. **머리말이 부르는 번호가 실재하는가** — 머리말은 산문이라 생성 대상이 아니지만, 거기
//!    적힌 네 자리 번호가 없는 ADR 을 가리키면 사슬이 끊긴 것이다(병합에서 실제로 났다).
//!
//! 머리말의 사슬 **서술이 완전한가**(A → B 가 머리말에 적혔는가)는 안 본다 — 어느 사슬을
//! 머리말에 올릴지는 사람의 편집 판단이고, ADR ↔ ADR 의 양방향 기록(4)이 이미 있다.
//!
//! ## 여기에 더 안 짓기로 한 것 — ADR-0243 의 칸으로
//!
//! - **본문 `Tags` ↔ 본문 서술**(칸 ㄴ). 태그가 주장하는 관계를 본문이 한 번도 안 부르는
//!   자리를 센다. 안 짓는 이유는 좌변이 아니라 `adr-NNNN` 태그의 뜻이 **둘**인데 문면에서
//!   안 갈려서다 — (가) 이 결정이 **부르는** 다른 결정 · (나) 이 결정이 **속하는** 축.
//!   이 파일은 **인덱스 ↔ 본문 헤더 · ADR ↔ ADR 의 사슬** 을 보고 그쪽은 **본문 Tags ↔ 본문 서술**이라 물음이
//!   다르므로, 짓게 되면 새 파일이 아니라 여기를 넓힌다.
//!
//!   **전수 판정(2026-09-09, main `12bc0f4b2` 기점): 그 자리 9 건이 전부 (나) 였다 —
//!   위반 0.** 그중 `0220→0142` 하나에만 본문 한 줄을 적어 8 로 줄었다. 왜 하나만인지가
//!   이 판정의 핵심이다 — 아래 "(나) 라고 다 같지 않다" 참조.
//!
//!   | 자리 | 축 | 판정한 lane |
//!   |---|---|---|
//!   | `0035→0020` · `0038→0020`(지금은 0510) | 갤러리 완전성 정책. 그 정책 아래 만든 UI 컴포넌트다 | 823 |
//!   | `0053→0032` | attach. 0053 은 attach 채널·점유 신뢰 위에 서고 0032 는 그 프로필 층이다 | 823 |
//!   | `0220→0142` | 관측 가능성. **낱말이 같고 층이 다르다** — 아래 주의 | 823 |
//!   | `0224→0139` · `0225→0139` · `0226→0139` | 문서에 적는 수의 분류 | 823 |
//!   | `0224→0180` | 흩어진 판정에 이름과 집을 준다 | 823 |
//!   | `0225→0142` | 어느 트리 기준인가. **아홉 중 (가) 에 가장 가깝다** — 아래 | 823 |
//!
//!   `0225→0142` 를 (나) 로 둔 근거: 0142(채널 주장)와 0225(좌변 값)는 같은 원리의 두
//!   적용이라 축이 매우 가깝지만, 0225 의 서술은 0142 를 안 불러도 완결된다 — 미추적
//!   디렉토리가 더하기만 하므로 추적 계수가 하계라는 논거가 자기 안에서 닫힌다.
//!
//!   ★ **이 9 는 positive control 이 아니라 negative control 이다.** 판정기를 지었다면 9 건
//!   전부 오탐이었다. 진짜 위반은 **관계가 실재하지 않는데 태그가 붙은 것**이고, 이 레포에서
//!   그 형태는 두 번 났다 — 0129 와 0158 의 `adr-0105`(둘 다 양방향 인용 0, 주제 무관).
//!   둘 다 2026-09-08 에 지웠으므로 **실물 positive control 은 지금 0 이고, 합성 픽스처로만
//!   만들 수 있다.** 실물만으로 술어를 재면 정상을 위반으로 세는 쪽으로 초록이 난다.
//!
//!   ★ **아홉이 한 방향을 가리킨다: (나) 의 대상은 전부 정책·원리 ADR 이다.**
//!   0020(갤러리 완전성 — 지금은 0510 에 흡수) · 0032(attach 프로필 층) · 0139(수의 분류) · 0142(트리 기준) ·
//!   0180(가드의 판정 물음). 반대로 **자기 회차의 형제는 본문이 부른다** — 0225 와 0226 은
//!   0224 를 본문에서 부르고 태그에도 넣었다(그래서 방향 A 에 안 걸린다).
//!
//!   ★★ **그러나 역은 거짓이다 — "대상이 정책이면 (나)" 가 아니다.** 그 방향으로 읽으면
//!   두 자리에서 틀린다: `0158→0140`(0140 은 모든 plugin 매니페스트에 걸리는 정책인데
//!   0158 은 그 위에 서지 않고 **반대 결론**을 내므로 본문에 적어야 정보다 — (가)) ·
//!   `0129→0105`·`0158→0105`(0105 는 모든 추적 파일에 걸리는 정책인데 그 둘의 **주제 밖**
//!   이라 관계 자체가 없다 — 지웠다). 그러니 대상이 정책인 것은 (나) 의 **필요조건이지
//!   충분조건이 아니다.** 갈래는 셋이다:
//!
//!   1. 정책이고 이 결정이 그 정책의 **적용 사례** → (나). 본문에 적으면 동어반복.
//!   2. 정책이고 이 결정이 그 정책과 **논증 관계**(같은 축의 반대 결론 포함) → (가).
//!   3. 정책이지만 이 결정의 **주제 밖** → 관계 없음. 태그를 지운다.
//!
//!   **셋을 가르는 것은 "그 정책이 이 결정에 어떻게 걸리는가" 이고 문면에 없다.**
//!   문면 후보 넷을 다 재 봤다(2026-09-09, `12bc0f4b2` + 823 커밋 2 개):
//!
//!   | 후보 | 결과 |
//!   |---|---|
//!   | 본문 인용 in-degree | **판별력 0.** (나) 대상 {5,6,7,8,29} · (가) 대상 {1,6} — **6 이 양쪽** |
//!   | `Status` | 여덟 자리 전부 `Accepted` |
//!   | 절 수 | 여덟 자리 전부 6 |
//!   | 낱말 태그 | `guards` 가 (나) 셋과 (가) 하나에 함께 붙는다 |
//!
//!   in-degree 는 **순환은 아니다** — 본문 인용만 세면 판정 대상인 태그가 정의에 안 들어간다.
//!   그러나 태그를 포함한 값과 거의 같고(0139: 본문 29 / 태그 28) 어느 쪽이든 안 갈린다.
//!
//!   ★ **표시를 만들었다면 오히려 틀렸을 것이다.** "정책이니 (나)" 라는 규칙이 서면
//!   `0158→0140` 과 `0129→0105` 를 잘못 판정한다. 표시가 없어서 난 틀린 판단은 **0 건**이고,
//!   표시가 있었다면 났을 틀린 판단은 **셋**이다. 이것이 (ㄴ) 칸의 세 번째 근거다.
//!
//!   **(나) 라고 다 같지 않다 — 그래서 일률 처방이 안 선다.** (나) 태그에 본문 한 줄을
//!   더하면 방향 A 에서 빠지고, 그 한 줄이 정보인 자리도 있다. 다만 자리마다 다르다:
//!   `0220→0142` 는 **"채널" 이라는 낱말이 두 층을 가리켜** 안 적으면 혼동하므로 적었고,
//!   `0035→0020`·`0038→0020`(지금은 0510) 은 0020 이 **모든** UI 컴포넌트에 걸리는 정책이라 각 컴포넌트
//!   ADR 에 "이것도 그 정책 아래다" 를 적는 것이 동어반복이라 안 적었다. 실패문은 이
//!   차이를 못 만든다 — 낼 수 있는 처방이 "한 줄 적어라" 하나뿐이고 그것이 절반에서
//!   동어반복을 시킨다.
//!
//!   **되돌아올 조건**: 태그 어휘가 (가)/(나)를 가르게 되면(예: 축 태그에 다른 접두).
//!   좌변이 두꺼워지는 것은 조건이 아니다 — 새 ADR 이 들어올 때마다 (나) 가 늘어서
//!   방향 A 는 자란다(2026-09-08 마감으로 4 → 9). 자라는 것은 정상 사례다.

// 이유: 이 타깃은 시험 범위다. `let _` 로 값을 버리는 자리를 여기서 명부에 올리면
//       그 명부가 프로덕션 자리를 가리키는 뜻을 잃는다 —
//       `crates/tasty-doc-guards/tests/let_underscore_documented.rs` 의 명부 순수성 판정이 그것을 막는다.
#![allow(clippy::let_underscore_must_use)]

use std::path::{Path, PathBuf};

use tasty_doc_guards::adr_index::{
    ADR_DIR, Adr, INDEX, chain_violations, collect, duplicate_numbers, group_slugs, parse_adr,
    placement_violations, preamble_violations, render_index, superseder,
};

/// ADR 수의 하한 — **연기 검사**다. 수집이 죽으면 아래 대조는 빈 집합끼리라 그냥 통과한다.
/// 값의 근거: 2026-09-05 실측 153 건, 2026-09-23 실측 396 건.
///
/// **이 수를 내려서 초록을 만들지 마라.** 정당한 수선은 ADR 을 실제로 지웠을 때뿐이고,
/// 그때는 `[adr-index] ADR <N>` 줄의 수가 함께 줄었는지 본다.
const MIN_ADRS: usize = 120;

/// 본문이 `Superseded by NNNN` 인 ADR 수의 하한 — 사슬 판정의 대체 갈래가 한 번도 안 돌면
/// 그 초록은 "위반이 없다" 가 아니라 "볼 것이 없었다" 다. 값의 근거: 2026-09-06 실측 5 건,
/// 2026-09-23 실측 7 건.
const MIN_SUPERSEDED: usize = 3;

/// 사슬 판정이 읽은 끝(대체 Status + 짝 키 줄의 링크)의 하한. 2026-09-23 실측 90.
/// 0 이면 References 판독이 죽은 것이다.
const MIN_CHAIN_ENDS: usize = 40;

/// 머리말에서 읽은 번호 토큰의 하한. 2026-09-23 실측 250. 0 이면 머리말 판정이 미측정이다.
const MIN_PREAMBLE_NUMBERS: usize = 100;

fn repo_root() -> PathBuf {
    tasty_doc_guards::repo_root()
}

fn adrs(root: &Path) -> Vec<Adr> {
    collect(root).unwrap_or_else(|e| panic!("{e}"))
}

fn index_text(root: &Path) -> String {
    std::fs::read_to_string(root.join(INDEX))
        .unwrap_or_else(|e| panic!("{INDEX} 를 못 읽었다: {e}"))
        .replace("\r\n", "\n")
}

/// 한 번호는 한 ADR 만 가리킨다.
#[test]
fn an_adr_number_names_exactly_one_document() {
    let all = adrs(&repo_root());
    println!("[ADR 인덱스] ADR 파일 {} · 하한 {MIN_ADRS}", all.len());
    assert!(
        all.len() >= MIN_ADRS,
        "ADR 파일을 {} 개밖에 못 셌다(하한 {MIN_ADRS}) — 수집이 죽었다",
        all.len()
    );
    let dupes = duplicate_numbers(&all);
    assert!(
        dupes.is_empty(),
        "같은 ADR 번호가 서로 다른 문서를 가리킨다. 번호는 소스 주석·다른 ADR·커밋 \
         메시지가 인용하는 식별자라, 겹치면 그 인용이 전부 모호해진다. 나중에 얹은 쪽이 \
         현재 최대 번호 + 1 로 옮긴다(파일명·본문 제목·참조 링크 셋 다, 그리고 인덱스를 \
         다시 생성한다). 빈 번호는 재사용하지 않는다\
         (docs/adr/0239-an-unused-adr-number-is-retired-not-recycled.md).\n  {}",
        dupes.join("\n  ")
    );
}

/// 문서 안의 `# ADR-NNNN` 제목이 자기 파일명 번호와 같다. 번호를 옮길 때 파일명만 바꾸고
/// 본문을 안 고치면 문서를 **열어서** 번호를 읽은 사람만 틀린 값을 갖는다.
#[test]
fn the_heading_number_matches_the_file_name() {
    let mut wrong = Vec::new();
    for a in adrs(&repo_root()) {
        match &a.heading_num {
            None => wrong.push(format!("{} — `# ADR-…` 제목 줄이 없다", a.file)),
            Some(h) if *h != a.num => {
                wrong.push(format!("{} — 제목은 ADR-{h} 라고 말한다", a.file))
            }
            Some(_) => {}
        }
    }
    assert!(
        wrong.is_empty(),
        "파일명 번호와 본문 제목 번호가 다르다:\n  {}",
        wrong.join("\n  ")
    );
}

/// 행을 만드는 네 헤더 값이 다 있다. 없으면 생성된 행에 빈 칸이 생기고, 빈 칸은 읽는
/// 사람에게 "상태 없음" 이라 통과시킬 값이 아니다.
#[test]
fn every_adr_header_carries_the_row_fields() {
    let all = adrs(&repo_root());
    let mut missing = Vec::new();
    for a in &all {
        for (name, v) in [
            ("제목", &a.title),
            ("Status", &a.status),
            ("Date", &a.date),
            ("Tags", &a.tags),
        ] {
            if v.as_deref().is_none_or(|s| s.trim().is_empty()) {
                missing.push(format!("{} — {name} 이 없다", a.file));
            }
        }
    }
    assert!(
        all.len() >= MIN_ADRS,
        "ADR {} 개 — 수집이 죽었다",
        all.len()
    );
    assert!(
        missing.is_empty(),
        "헤더 값이 빠져 생성된 행에 빈 칸이 생긴다:\n  {}",
        missing.join("\n  ")
    );
}

/// 앞 줄과 뒤 줄이 처음 갈리는 자리를 몇 줄 보여 준다 — 전문을 찍으면 실패문이 묻힌다.
fn first_difference(want: &str, got: &str) -> String {
    let (w, g): (Vec<&str>, Vec<&str>) = (want.lines().collect(), got.lines().collect());
    let at = w
        .iter()
        .zip(&g)
        .position(|(a, b)| a != b)
        .unwrap_or(w.len().min(g.len()));
    format!(
        "{}행부터 다르다\n  생성: {:?}\n  파일: {:?}",
        at + 1,
        w.get(at).copied().unwrap_or("<끝>"),
        g.get(at).copied().unwrap_or("<끝>")
    )
}

/// **생성 구역이 생성 결과와 같은가** — 행을 손으로 고쳤거나, ADR 헤더를 고치고 다시
/// 안 만들었거나, ADR 을 더하고 안 만들었으면 여기서 빨개진다.
#[test]
fn the_index_rows_are_what_the_generator_renders() {
    let root = repo_root();
    let all = adrs(&root);
    let current = index_text(&root);
    let rendered = render_index(&current, &all)
        .unwrap_or_else(|e| panic!("인덱스의 마커 구조가 깨졌다:\n  {}", e.join("\n  ")));
    let rows = rendered
        .lines()
        .filter(|l| l.starts_with("| ") && l.as_bytes().get(2).is_some_and(u8::is_ascii_digit))
        .count();
    println!(
        "[adr-index-parity] ADR {} · 그룹 {} · 생성 행 {rows}",
        all.len(),
        group_slugs(&current).len()
    );
    assert!(rows >= MIN_ADRS, "생성 행이 {rows} 개다(하한 {MIN_ADRS})");
    assert!(
        rendered == current,
        "{INDEX} 의 생성 구역(`adr-rows:begin` ~ `adr-rows:end`)이 ADR 헤더에서 만든 결과와 \
         다르다. ★ 생성 구역을 손으로 고치지 마라 — ADR 헤더(제목 · Status · Date · Tags · \
         Group)가 정본이다. 헤더를 고친 뒤 \
         `cargo run -p tasty-doc-guards --bin adr-index -- --write` 로 다시 만든다.\n{}",
        first_difference(&rendered, &current)
    );
}

/// **그룹 배치** — 모든 ADR 이 정확히 한 그룹에 있고, 그 그룹이 인덱스에 실재한다.
#[test]
fn every_adr_sits_in_exactly_one_known_group() {
    let root = repo_root();
    let all = adrs(&root);
    let slugs = group_slugs(&index_text(&root));
    let placed = all.iter().filter(|a| a.groups.len() == 1).count();
    println!(
        "[adr-group] ADR {} · 그룹 하나에 든 ADR {placed} · 인덱스 그룹 {}",
        all.len(),
        slugs.len()
    );
    assert!(
        !slugs.is_empty(),
        "인덱스에서 `adr-rows:begin` 마커를 하나도 못 읽었다"
    );
    let v = placement_violations(&all, &slugs);
    assert!(
        v.is_empty(),
        "그룹 배치가 깨졌다 {} 건. 그룹 고르는 기준은 docs/adr/template.md 의 \
         \"인덱스에 행을 추가할 때\" 다:\n  {}",
        v.len(),
        v.join("\n  ")
    );
}

/// **결정 사슬이 양 끝에 적혔는가.**
#[test]
fn decision_chains_are_written_on_both_ends() {
    let all = adrs(&repo_root());
    let superseded = all
        .iter()
        .filter(|a| a.status.as_deref().and_then(superseder).is_some())
        .count();
    let (v, ends) = chain_violations(&all);
    println!(
        "[adr-chain] ADR {} · 대체된 ADR {superseded} · 사슬 끝 {ends} · 위반 {}",
        all.len(),
        v.len()
    );
    assert!(
        ends >= MIN_CHAIN_ENDS,
        "사슬 끝을 {ends} 개밖에 못 읽었다(하한 {MIN_CHAIN_ENDS}) — References 판독이 죽었다"
    );
    assert!(
        superseded >= MIN_SUPERSEDED,
        "`Superseded by NNNN` 인 ADR 을 {superseded} 건밖에 못 봤다(하한 {MIN_SUPERSEDED}) — \
         대체 갈래가 한 번도 안 돌면 그 초록은 위반이 없다는 뜻이 아니다. ★ 하한을 내려서 \
         통과시키지 마라"
    );
    assert!(
        v.is_empty(),
        "결정 사슬이 한쪽에만 적혔다 {} 건. 짝 문구는 docs/adr/template.md 의 \
         \"부분 개정 예외\" · \"같은 결정이 두 ADR 에 적혀 있을 때\" 가 정한다:\n  {}",
        v.len(),
        v.join("\n  ")
    );
}

/// **머리말이 부르는 번호가 실재하는가.**
#[test]
fn group_preambles_cite_only_existing_adrs() {
    let root = repo_root();
    let (v, seen) = preamble_violations(&index_text(&root), &adrs(&root));
    println!("[adr-preamble] 번호 토큰 {seen} · 위반 {}", v.len());
    assert!(
        seen >= MIN_PREAMBLE_NUMBERS,
        "머리말에서 번호를 {seen} 개밖에 못 읽었다(하한 {MIN_PREAMBLE_NUMBERS}) — 판독이 죽었다"
    );
    assert!(
        v.is_empty(),
        "머리말이 없는 ADR 번호를 부른다:\n  {}",
        v.join("\n  ")
    );
}

/// 생성기 bin 이 라이브러리와 같은 답을 낸다 — 레포에서 `--check` 가 0 으로 끝난다.
#[test]
fn the_generator_bin_agrees_with_the_repo() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_adr-index"))
        .arg("--check")
        .arg(repo_root())
        .output()
        .expect("adr-index 를 못 돌렸다");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "adr-index --check 가 {:?} 로 끝났다:\n{err}",
        out.status.code()
    );
    assert!(
        err.contains("[adr-index] ADR "),
        "모수 줄이 안 나왔다:\n{err}"
    );
}

// ── 합성 코퍼스 — 레포 자신만 읽으면 위 판정이 빨개지는 상태가 한 번도 안 만들어진다 ──

fn adr(num: &str, status: &str, group: &str, refs: &str) -> Adr {
    let group_line = if group.is_empty() {
        String::new()
    } else {
        format!("- **Group**: {group}\n")
    };
    parse_adr(
        &format!("{num}-x.md"),
        format!(
            "# ADR-{num}: 제목 {num}\n\n- **Status**: {status}\n- **Date**: 2026-09-23\n\
             - **Tags**: t\n{group_line}\n## References\n\n{refs}\n"
        ),
    )
}

const TWO_GROUPS: &str = "머리\n\n## A\n\n0001 을 부른다.\n\n<!-- adr-rows:begin a -->\n\
<!-- adr-rows:end a -->\n\n## B\n\n<!-- adr-rows:begin b -->\n<!-- adr-rows:end b -->\n";

#[test]
fn rendering_touches_only_the_inside_of_the_markers() {
    let all = vec![
        adr("0001", "Accepted", "a", ""),
        adr("0002", "Accepted — 사유", "b", ""),
    ];
    let once = render_index(TWO_GROUPS, &all).expect("구조가 맞다");
    assert!(once.contains("| 0002 | [제목 0002](0002-x.md) | Accepted | 2026-09-23 | t |"));
    assert!(
        once.starts_with("머리\n\n## A\n\n0001 을 부른다.\n"),
        "마커 밖을 바꿨다"
    );
    // 다시 돌려도 같다 — 생성 결과가 고정점이 아니면 가드가 영영 초록이 안 된다.
    assert_eq!(render_index(&once, &all).expect("구조가 맞다"), once);
    // 생성 구역 행을 손으로 고치면 결과와 달라진다.
    let hand = once.replace("| Accepted — 사유 |", "| Accepted |").replace(
        "| 0002 | [제목 0002](0002-x.md) | Accepted |",
        "| 0002 | [손으로 고친 제목](0002-x.md) | Accepted |",
    );
    assert_ne!(render_index(&hand, &all).expect("구조가 맞다"), hand);
}

#[test]
fn a_broken_marker_structure_is_refused() {
    let all = vec![adr("0001", "Accepted", "a", "")];
    for broken in [
        "<!-- adr-rows:begin a -->\n",
        "<!-- adr-rows:begin a -->\n<!-- adr-rows:end b -->\n",
        "<!-- adr-rows:end a -->\n",
        "<!-- adr-rows:begin a -->\n<!-- adr-rows:end a -->\n<!-- adr-rows:begin a -->\n<!-- adr-rows:end a -->\n",
    ] {
        assert!(
            render_index(broken, &all).is_err(),
            "깨진 구조를 받아들였다: {broken:?}"
        );
    }
}

#[test]
fn placement_catches_none_two_and_unknown_groups() {
    let slugs = vec!["a".to_string(), "b".to_string()];
    let ok = vec![
        adr("0001", "Accepted", "a", ""),
        adr("0002", "Accepted", "b", ""),
    ];
    assert!(
        placement_violations(&ok, &slugs).is_empty(),
        "정상을 위반으로 셌다"
    );
    for (bad, why) in [
        (adr("0003", "Accepted", "", ""), "Group 없음"),
        (adr("0003", "Accepted", "a, b", ""), "두 그룹"),
        (adr("0003", "Accepted", "c", ""), "없는 그룹"),
    ] {
        let mut all = ok.clone();
        all.push(bad);
        assert_eq!(
            placement_violations(&all, &slugs).len(),
            1,
            "{why} 를 못 잡았다"
        );
    }
    let only_a = vec![adr("0001", "Accepted", "a", "")];
    assert_eq!(
        placement_violations(&only_a, &slugs).len(),
        1,
        "빈 그룹을 못 잡았다"
    );
}

#[test]
fn chains_must_be_written_on_both_ends() {
    let amend = "- 개정 대상: [ADR-0001](0001-x.md) (조항)";
    let back = "- 부분 개정: [0002](0002-x.md) (조항 개정)";
    let both = vec![
        adr("0001", "Accepted", "a", back),
        adr("0002", "Accepted", "a", amend),
    ];
    assert!(
        chain_violations(&both).0.is_empty(),
        "양쪽에 적힌 사슬을 위반으로 셌다"
    );
    let fwd_only = vec![
        adr("0001", "Accepted", "a", ""),
        adr("0002", "Accepted", "a", amend),
    ];
    assert_eq!(
        chain_violations(&fwd_only).0.len(),
        1,
        "개정 대상만 적힌 사슬"
    );
    let back_only = vec![
        adr("0001", "Accepted", "a", back),
        adr("0002", "Accepted", "a", ""),
    ];
    assert_eq!(
        chain_violations(&back_only).0.len(),
        1,
        "부분 개정만 적힌 사슬"
    );

    // 대체 — 새 ADR 이 옛 ADR 을 부르고, 같은 그룹이어야 한다.
    let sup = "Superseded by [ADR-0002](0002-x.md) — 사유";
    let cited = "- 대체: [ADR-0001](0001-x.md)";
    let good = vec![
        adr("0001", sup, "a", ""),
        adr("0002", "Accepted", "a", cited),
    ];
    assert!(
        chain_violations(&good).0.is_empty(),
        "정상 대체를 위반으로 셌다"
    );
    let silent = vec![adr("0001", sup, "a", ""), adr("0002", "Accepted", "a", "")];
    assert_eq!(
        chain_violations(&silent).0.len(),
        1,
        "옛 ADR 을 안 부르는 대체"
    );
    let moved = vec![
        adr("0001", sup, "a", ""),
        adr("0002", "Accepted", "b", cited),
    ];
    assert_eq!(chain_violations(&moved).0.len(), 1, "그룹이 갈린 대체");
    let dangling = vec![adr("0001", "Superseded by 0009", "a", "")];
    assert_eq!(chain_violations(&dangling).0.len(), 1, "없는 ADR 로의 대체");
    // 개정 대상이 이후 대체됐으면 그 Status 가 끝을 대신한다.
    let amend_sup = vec![
        adr("0001", sup, "a", ""),
        adr("0002", "Accepted", "a", amend),
    ];
    assert!(
        chain_violations(&amend_sup).0.is_empty(),
        "대체가 끝을 대신하지 못했다"
    );
}

#[test]
fn a_preamble_number_without_an_adr_is_caught() {
    let all = vec![adr("0001", "Accepted", "a", "")];
    let (v, seen) = preamble_violations(TWO_GROUPS, &all);
    assert_eq!((v.len(), seen), (0, 1), "정상 머리말");
    let bad = TWO_GROUPS.replace("0001 을", "0001 → 0613 을");
    let (v, seen) = preamble_violations(&bad, &all);
    assert_eq!((v.len(), seen), (1, 2), "없는 번호를 못 잡았다");
}

/// `MIN_ADRS` 의 **양성 대조** — 모수가 하한 아래로 떨어지는 코퍼스를 만들고 수집기가
/// 그것을 말하는지 묻는다. 위 시험들은 레포 자신을 읽으므로 수집이 죽는 상황이 한 번도
/// 안 만들어진다.
#[test]
fn the_adr_floor_sees_a_collapsed_collection() {
    let root =
        std::env::temp_dir().join(format!("tasty-adrfloor-{}-{}", std::process::id(), line!()));
    // 앞선 실행의 잔여를 치운다 — 없는 것이 정상이라 실패가 정보가 아니다.
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join(ADR_DIR);
    std::fs::create_dir_all(&dir).expect("임시 디렉토리를 못 만들었다");
    assert_eq!(
        adrs(&root).len(),
        0,
        "빈 뿌리에서 0 이 아니면 인자를 안 보고 레포를 읽는 것이다"
    );
    for (num, slug) in [("0001", "a"), ("0002", "b"), ("0003", "c")] {
        std::fs::write(dir.join(format!("{num}-{slug}.md")), "").expect("쓰기 실패");
    }
    std::fs::write(dir.join("template.md"), "").expect("쓰기 실패");
    std::fs::write(dir.join("index.md"), "").expect("쓰기 실패");
    assert_eq!(
        adrs(&root).len(),
        3,
        "번호 아닌 `.md` 를 세거나 디렉터리를 안 읽었다"
    );
    assert!(adrs(&root).len() < MIN_ADRS, "이 대조는 하한 아래여야 한다");
    // 뒷정리 실패는 무시한다 — 임시 디렉토리라 다음 실행이 먼저 지운다.
    let _ = std::fs::remove_dir_all(&root);
}
