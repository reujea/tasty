//! ADR 번호가 **식별자로 성립하는지** 못 박는다.
//!
//! ADR 은 번호로 인용된다 — 소스 주석, 다른 ADR 의 References, `docs/` 본문, 커밋
//! 메시지가 전부 "ADR-0140" 같은 형태로 가리킨다. 그래서 한 번호가 두 문서를 가리키는
//! 순간 그 인용들이 전부 모호해진다. 그런데 번호를 **여러 lane 이 병렬로 집는다** —
//! 각자 자기 브랜치에서 "최대 번호 + 1" 을 보고 고르므로, 둘이 같은 것을 보면 둘 다
//! 옳게 골랐는데 충돌한다. 병합은 서로 다른 줄을 더한 것이라 conflict 없이 통과한다.
//!
//! 실제로 그렇게 됐다(2026-09-05): `0149` 가 서로 다른 두 ADR 에 붙은 채 main 에
//! 얹혔고, 기존 문서 가드 어느 것도 빨개지지 않았다. 이 파일은 그 자리를 메운다.
//!
//! 네 가지를 본다 — **번호 유일성**, 파일↔인덱스 **양방향** 대응, 그리고 문서 안의
//! `# ADR-NNNN` 제목이 자기 파일명과 같은지. 마지막 것은 번호를 옮길 때 파일명만 바꾸고
//! 본문 제목을 안 고치는 형태를 잡는다.
//!
//! ## 인덱스 행이 거울처럼 싣는 값
//!
//! 인덱스 행은 ADR 헤더의 `Status`·`Date` 를 **복사**한다. 값이 두 곳에 있고 함께
//! 움직여야 하는데, 움직이는 것은 대개 한 곳뿐이다 — 재sync 나 부분 개정에서 본문만
//! 올라가고 행은 첫 커밋 값으로 남는다. 실측(2026-09-06, 모수 179): `Date` 는 179/179
//! 가 같았고 `Status` 는 **한 건이 어긋나 있었다**(0042 — 본문 `Superseded by ADR-0162`,
//! 행 `Accepted`). 그 한 건은 이 가드를 켜는 커밋에서 함께 고쳤다.
//!
//! **`Title` 과 `Tags` 도 본다.** 오래 안 봤던 것은 열마다 관계가 달라서였다 — 본문 제목은
//! 강조 마커(`**…**`)를 쓰고 행은 안 쓰며, `Tags` 는 행이 본문의 상위집합인 경우가 있어
//! 등호도 접두도 아니었다. 2026-09-08 에 그 두 정규화 규칙을 세우고(`normalize_title` ·
//! `tag_set`) 상위집합이던 41 개를 갈라 넣었으므로 지금은 둘 다 판정 대상이다.
//!
//! ## 여기에 더 안 짓기로 한 것 — ADR-0243 의 칸으로
//!
//! - **`Tags` 나열 순서**(칸 ㄴ). 집합이 같은데 순서만 다른 행이 11 개 있었고 표기를
//!   맞췄지만 **판정기는 안 짓는다.** 실패문이 낼 수 있는 처방이 "순서를 맞춰라" 뿐이고,
//!   그것을 안 지켜도 잃는 정보가 없다. **되돌아올 조건 없음** — 순서가 값이 되려면 태그를
//!   순서 있는 목록으로 바꾸는 별개 결정이 필요하고, 그때는 이 판정이 아니라 그 결정이
//!   좌변을 새로 정한다.
//! - **`Title`·`Tags` 두 열 자체**(칸 ㄷ). 새 판사를 안 만들고 이 시험을 넓혔다 — 같은
//!   물음(두 자리가 같은 값을 싣는가)이라 판사가 둘이면 답도 둘이 된다. **답하는 시험의
//!   이름**은 `an_index_row_mirrors_its_adr_header` 다. **되돌아올 조건**: 그 시험이
//!   좁아지거나 사라지면 이 물음에 답하는 자리가 없어지므로 그때 다시 묻는다.
//! - **본문 `Tags` ↔ 본문 서술**(칸 ㄴ). 태그가 주장하는 관계를 본문이 한 번도 안 부르는
//!   자리를 센다. 안 짓는 이유는 좌변이 아니라 `adr-NNNN` 태그의 뜻이 **둘**인데 문면에서
//!   안 갈려서다 — (가) 이 결정이 **부르는** 다른 결정 · (나) 이 결정이 **속하는** 축.
//!   이 시험은 **index 행 ↔ 본문 Tags** 를 보고 그쪽은 **본문 Tags ↔ 본문 서술**이라 물음이
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

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const ADR_DIR: &str = "docs/adr";
const INDEX: &str = "docs/adr/index.md";

/// ADR 수의 하한 — **연기 검사**다. 목록이 비면 아래 집합 대조는 빈 집합끼리라
/// 그냥 통과한다. 값의 근거: 2026-09-05 실측 153 건.
///
/// **판별식** — 이 상수 하나가 **세 시험의 네 수**를 지킨다. 그 넷은 서로 독립으로 재는데
/// **정상이면 전부 같은 값**이다. 그래서 넷을 나란히 읽는 것이 곧 이 하한의 검사다:
///
/// ```text
/// cargo test -p tasty-doc-guards --test adr_index_parity -- --nocapture
///   → [ADR 인덱스] ADR 파일 <N> · 하한 120        (every_adr_file_has_a_row…)
///   → [ADR 인덱스] 인덱스 행 <N> · 하한 120        (an_adr_number_names_exactly_one_document)
///   → [adr-index-parity] 행 <N> · Status 대조 <N> · Date 대조 <N> · …
///                                                 (an_index_row_carries_the_same_status…)
/// ```
///
/// ★ **넷이 갈리면 하한이 아니라 독법이 고장 난 것이다.** 파일 수와 인덱스 행 수가 다르면
/// 인덱스가 밀린 것이고, 그 둘이 같은데 Status/Date 대조 수만 낮으면 헤더 독법이 죽은 것이다.
/// 그 구분은 이 하한이 못 한다 — 하한은 "넷 다 0 은 아니다" 까지만 말한다.
///
/// 실측 2026-09-07(`de0572359`): **네 수가 전부 190** 이다(09-05 의 153 에서 늘었다).
/// 하한이 120 이라 **여유가 70** 이다 — 술어가 3 분의 1 만 남아도 통과한다는 뜻이다.
/// 값을 올릴지는 하한 조이기라는 별개 축이라 여기서는 실측만 남긴다.
///
/// **이 수를 내려서 초록을 만들지 마라.** 이 자리의 하한은 대조군이 살아 있는지만 보는
/// 연기 검사라, 내리면 아래 집합 대조들이 더 작은 집합에서만 참이 되면서 색은 안 변한다.
///
/// 정당한 수선: ADR 을 실제로 지웠으면 이 수를 함께 내려라. 그때 **위 네 수를 함께 봐라** —
/// 넷이 같이 줄었으면 지운 것이고, 하나만 줄었으면 지운 것이 아니라 독법이 깨진 것이다.
const MIN_ADRS: usize = 120;

/// 본문이 `Superseded by NNNN` 인 ADR 수의 하한 — **연기 검사**다.
///
/// [`tolerated_too_much`] 의 "행이 대체 ADR 번호를 잃었다" 갈래는 본문에 그 형태가
/// 있어야만 밟힌다. 하나도 없으면 그 갈래는 한 번도 안 돌고, 그때의 초록은 "위반이
/// 없다" 가 아니라 **"볼 것이 없었다"** 다 — 두 초록은 값이 같고 뜻이 다르다.
///
/// 값의 근거: 2026-09-06 실측 5 건(0015 · 0018 · 0040 · 0042 · 0066).
/// 나머지 갈래인 "행의 상태 칸이 비었다" 는 실물이 0 이라 하한을 둘 수 없다 —
/// 그쪽은 변이 팔로만 확인된다.
const MIN_SUPERSEDED: usize = 3;

fn repo_root() -> PathBuf {
    // 이 크레이트는 `crates/tasty-doc-guards` 다 — 레포 루트는 두 단계 위.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("crates/<crate> 아래여야 한다")
        .to_path_buf()
}

fn read(root: &Path, rel: &str) -> String {
    let p = root.join(rel);
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("{rel} 을 읽지 못했다: {e}"))
        .replace("\r\n", "\n")
}

/// 앞 네 자리가 숫자인 `.md` 만 ADR 로 센다 — `index.md` · `template.md` 는 빠진다.
fn adr_files(root: &Path) -> BTreeMap<String, String> {
    let dir = root.join(ADR_DIR);
    let mut out = BTreeMap::new();
    for entry in std::fs::read_dir(&dir).expect("docs/adr 를 읽을 수 없다") {
        let name = entry.expect("디렉터리 항목").file_name();
        let name = name.to_string_lossy().to_string();
        if !name.ends_with(".md") {
            continue;
        }
        let num: String = name.chars().take(4).collect();
        if num.len() == 4 && num.chars().all(|c| c.is_ascii_digit()) {
            let prev = out.insert(num.clone(), name.clone());
            assert!(
                prev.is_none(),
                "파일 두 개가 같은 번호를 쓴다: {num} — {prev:?} 와 {name}"
            );
        }
    }
    out
}

/// 인덱스 표의 한 행에서 뽑아낸 값들.
struct IndexRow {
    num: String,
    file: String,
    title: String,
    status: String,
    date: String,
    tags: String,
}

/// 인덱스 표의 `| NNNN | [제목](파일명) | Status | Date | Tags |` 행을 읽는다.
///
/// 독법은 여기 하나다. 열이 더 필요해지면 이 함수를 넓히고, **두 번째 독법을 만들지
/// 않는다** — 같은 표를 두 방법으로 읽으면 답이 갈리고, 갈린 답 중 어느 것이 옳은지는
/// 표를 다시 읽어야 알게 된다.
fn index_rows(root: &Path) -> Vec<IndexRow> {
    let mut out = Vec::new();
    for line in read(root, INDEX).lines() {
        let Some(rest) = line.strip_prefix("| ") else {
            continue;
        };
        let num: String = rest.chars().take(4).collect();
        if num.len() != 4 || !num.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        // 링크 대상은 `](` 와 `)` 사이.
        //
        // ★ **정규식으로 바꾸지 마라.** `\[([^\]]+)\]\(` 로 잡으면 제목 안에 `]` 가 든
        //   행에서 매칭이 끊겨 그 행이 통째로 빠진다 — 실측 2026-09-08: 0114 의
        //   `` `[font]` `` 와 0123 의 `` `#[cfg(test)]` `` 때문에 모수가 206 이 아니라
        //   204 가 된다. 여기 `find("](")` 는 제목 안 `]` 뒤에 `(` 가 오지 않으므로
        //   링크의 것을 정확히 집는다. 모수가 줄어드는 고장은 조용하다 — 빠진 행은
        //   위반도 못 내므로 색이 안 변한다.
        let Some(at) = rest.find("](") else { continue };
        let after = &rest[at + 2..];
        let Some(end) = after.find(')') else { continue };
        let file = after[..end].to_string();
        // 제목은 여는 `[` 와 위 `](` 사이. 번호 칸에는 `[` 가 없으므로 첫 `[` 가 그것이다.
        let title = match rest.find('[') {
            Some(open) if open < at => rest[open + 1..at].trim().to_string(),
            _ => String::new(),
        };
        // 링크를 닫는 `)` 뒤부터가 Status | Date | Tags 다. 제목 안에 `|` 가 들어갈 수
        // 있으므로 앞에서부터 세지 않고 **링크 뒤**에서 센다.
        let tail: Vec<&str> = after[end + 1..].split('|').collect();
        let cell = |i: usize| {
            tail.get(i)
                .map(|c| c.trim().to_string())
                .unwrap_or_default()
        };
        out.push(IndexRow {
            num,
            file,
            title,
            status: cell(1),
            date: cell(2),
            tags: cell(3),
        });
    }
    out
}

/// 본문 `# ADR-NNNN: <제목>` 을 인덱스 행에 실을 때의 정규형.
///
/// 두 자리가 같은 값을 다르게 적는 지점은 하나뿐이다 — **강조 마커**. 본문 제목은
/// `**…**` 로 낱말을 세울 수 있고 행은 그것을 안 쓴다. 그 차이는 표기이지 값이
/// 아니므로 마커를 지우고 잉여 공백을 접는다.
///
/// **접두 관용을 두지 않는다.** `Status` 쪽은 본문이 ` — ` 뒤에 사유를 붙이고 행이
/// 그것을 버리는 관례가 있어 접두를 관용하지만, 제목에는 그런 관례가 없다 — 실측
/// 2026-09-08 에 어긋난 일곱 중 **행이 더 긴 것과 본문이 더 긴 것이 둘 다 있었다**
/// (0120·0123 은 행에만 부제가 있었고, 0164 는 본문에만 있었다). 절단 방향이 양쪽인
/// 것은 관례가 아니라 표류다. 관용을 넣으면 그 표류가 영구히 안 보이는 구간이 된다.
fn normalize_title(s: &str) -> String {
    // `squeeze` 를 쓰지 않는다. 그쪽은 `Status` 용이라 `ADR-` 접두와 **백틱**까지 지우는데,
    // 제목에서는 둘 다 값이다 — 백틱을 지우면 한쪽에만 코드 표기가 있는 차이를 못 보고,
    // `ADR-` 를 지우면 실패문이 본문에 없는 문자열을 찍어 **복붙으로 고칠 수 없는 처방**이
    // 된다(실측 2026-09-08: 0164 의 본문 `ADR-0075` 가 실패문에 `0075` 로 나왔다).
    s.replace("**", "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// `Tags` 칸을 항목 집합으로 읽는다. 쉼표로 가르고 빈 것은 버린다.
fn tag_set(s: &str) -> BTreeSet<String> {
    s.split(',')
        .map(|x| x.trim())
        .filter(|x| !x.is_empty())
        .map(|x| x.to_string())
        .collect()
}

/// 두 자리의 `Tags` 를 **집합 등호**로 본다. 어긋난 것을 방향별로 갈라 돌려준다.
///
/// 앞 판이 포함(행 ⊇ 본문)이었던 것은 그때 행에만 있는 태그가 41 개(18 행)라 등호를
/// 세울 수 없었기 때문이다. 그 41 개를 이번 회차에 갈랐다 — 40 개는 본문 `Tags` 줄로
/// 옮겼고, 하나(0129 의 `adr-0105`)는 두 본문 어디에도 서로에 대한 인용이 없어 실재하지
/// 않는 관계로 판정해 행에서 지웠다. 그래서 이제 양쪽이 같고 등호가 선다.
///
/// **순서는 값이 아니다.** 실측 2026-09-08(이 워크트리): 집합이 같은데 나열 순서만
/// 다른 행이 11 개다. 순서까지 요구하면 그 11 개가 값이 안 바뀐 채 빨개진다.
/// 그래서 순서에는 판정기를 안 짓는다 — 모듈 주석의 "여기에 더 안 짓기로 한 것" 참조.
fn tag_drift(body: &str, row: &str) -> (Vec<String>, Vec<String>) {
    let in_body = tag_set(body);
    let in_row = tag_set(row);
    let missing = in_body
        .iter()
        .filter(|x| !in_row.contains(*x))
        .cloned()
        .collect();
    let extra = in_row
        .iter()
        .filter(|x| !in_body.contains(*x))
        .cloned()
        .collect();
    (missing, extra)
}

/// 인덱스 행이 본문 헤더의 값을 그대로 싣는지 볼 때 쓰는 정규화.
///
/// 본문은 같은 값을 인라인 링크로 적을 수 있다(`Superseded by [0032](0032-….md)`).
/// 행은 링크 없이 적는다. 그 차이는 표기이지 값이 아니므로 링크를 벗기고, `ADR-`
/// 접두와 잉여 공백도 지운다.
fn normalize_header_value(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find(']') else {
            out.push_str(&rest[open..]);
            return squeeze(&out);
        };
        out.push_str(&after[..close]);
        let tail = &after[close + 1..];
        rest = match tail.strip_prefix('(') {
            // 링크 대상은 통째로 버린다.
            Some(target) => match target.find(')') {
                Some(e) => &target[e + 1..],
                None => "",
            },
            None => tail,
        };
    }
    out.push_str(rest);
    squeeze(&out)
}

/// 접두 일치가 **관용해서는 안 되는** 두 형태.
///
/// 행은 본문 `Status` 의 앞부분만 실을 수 있다 — 본문은 ` — ` 뒤에 사유를 붙이고 행은
/// 그것을 버린다. 그런데 그 절단 지점이 관례로 정해져 있지 않다(실측 2026-09-06):
/// 0015 는 `(부분)` 을 빼고 0040 은 넣으며, 0027 은 괄호절 전체를 뺀다. 그래서 동일성
/// 비교로 바꾸면 그 셋이 오탐이 된다 — 정규형이 실제로 없는 것을 있다고 가정하는 셈이다.
///
/// 관용은 두되, **잃으면 안 되는 것**만 뺀다.
///
/// - 행이 비면 `starts_with("")` 이 항상 참이라 그 행은 무슨 값이든 통과한다. 빈 칸은
///   읽는 사람에게 "상태 없음" 이라 통과시킬 값이 아니다.
/// - 본문이 다른 ADR 을 가리키는데(`Superseded by NNNN`) 행이 그 번호를 잃으면, 인덱스만
///   읽는 사람은 **어디로 갔는지 모른 채** 죽은 결정을 본다. 상태 이름만 남기는 것이
///   정확히 그 형태이고 접두 일치는 그것을 통과시킨다. 이 가드가 막으려는 사고가 바로
///   그것이므로(0042 는 그 반대 방향이었다 — 행이 `Accepted` 로 남았다) 여기서 뺀다.
///
/// 이 두 형태의 실물은 오늘 0 이다(빈 칸 0 · 맨 `Superseded` 0). 모수는 실행마다
/// 바뀌므로 여기 안 적는다 — 스캔 테스트가 `-- --nocapture` 로 그 줄을 싣는다. 0 인 것과
/// 막는 것이 있는 것은 다르다 — 이 함수가 그 차이다.
fn tolerated_too_much(want: &str, got: &str) -> Option<&'static str> {
    if got.trim().is_empty() {
        return Some("행의 상태 칸이 비었다");
    }
    // 상태 **이름 전체**를 실어야 한다. 접두만 요구하면 본문 `Accepted` 에 행 `A` 가
    // 통과한다 — "칸을 채워라" 를 최소로 이행한 결과가 정확히 그것이고, 그러면 이
    // 검사가 지키려던 것이 남지 않는다. 뒤쪽(사유·괄호절)은 여전히 잘라도 된다.
    if first_word(want) != first_word(got) {
        return Some("행이 상태 이름의 일부만 싣고 있다");
    }
    let num = superseder(want)?;
    (!got.contains(&num)).then_some("행이 대체한 ADR 번호를 잃었다")
}

/// 공백 앞까지. 값이 비면 빈 문자열.
fn first_word(v: &str) -> &str {
    v.split_whitespace().next().unwrap_or("")
}

/// `Superseded by 0162 …` 에서 `0162`. [`normalize_header_value`] 를 거친 값이라
/// 링크와 `ADR-` 는 이미 벗겨져 있다.
fn superseder(want: &str) -> Option<String> {
    let rest = want.strip_prefix("Superseded by ")?;
    let num: String = rest.chars().take_while(char::is_ascii_digit).collect();
    (num.len() == 4).then_some(num)
}

fn squeeze(s: &str) -> String {
    s.replace("ADR-", "")
        .replace('`', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// 본문 헤더의 `- **<이름>**: <값>` 한 줄을 읽는다.
fn header_field(body: &str, name: &str) -> Option<String> {
    let want = format!("- **{name}**:");
    body.lines()
        .find_map(|l| l.trim_start().strip_prefix(&want))
        .map(|v| v.trim().to_string())
}

/// 한 번호는 한 ADR 만 가리킨다.
#[test]
fn an_adr_number_names_exactly_one_document() {
    let rows = index_rows(&repo_root());
    // 측정값은 단정보다 앞에. 이 파일의 세 시험이 같은 하한을 쓰고 서로 다른 수를
    // 재므로, 그 수들을 나란히 읽는 것이 곧 하한의 판별식이다(상수 doc 참조).
    println!("[ADR 인덱스] 인덱스 행 {} · 하한 {MIN_ADRS}", rows.len());
    assert!(
        rows.len() >= MIN_ADRS,
        "인덱스에서 ADR 행을 {} 개밖에 못 뽑았다(하한 {MIN_ADRS}, 2026-09-05 실측 153). \
         행 형태가 바뀌었으면 이 추출기를 고쳐라 — 지금은 대조군이 죽은 상태다",
        rows.len()
    );

    let mut seen: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for IndexRow { num, file, .. } in &rows {
        seen.entry(num).or_default().push(file);
    }
    let dupes: Vec<String> = seen
        .iter()
        .filter(|(_, files)| files.len() > 1)
        .map(|(num, files)| format!("{num} → {}", files.join(" / ")))
        .collect();
    assert!(
        dupes.is_empty(),
        "같은 ADR 번호가 서로 다른 문서를 가리킨다. 번호는 소스 주석·다른 ADR·커밋 \
         메시지가 인용하는 식별자라, 겹치면 그 인용이 전부 모호해진다. 나중에 얹은 쪽이 \
         현재 최대 번호 + 1 로 옮긴다(파일명·본문 제목·인덱스 행·참조 링크 넷 다). 빈 번호는 \
         재사용하지 않는다(docs/adr/0239-an-unused-adr-number-is-retired-not-recycled.md).\n  {}",
        dupes.join("\n  ")
    );
}

/// 파일과 인덱스 행이 **양방향으로** 대응한다.
#[test]
fn every_adr_file_has_a_row_and_every_row_has_a_file() {
    let root = repo_root();
    let files = adr_files(&root);
    println!("[ADR 인덱스] ADR 파일 {} · 하한 {MIN_ADRS}", files.len());
    assert!(
        files.len() >= MIN_ADRS,
        "ADR 파일이 {} 개뿐이다(하한 {MIN_ADRS})",
        files.len()
    );
    let rows = index_rows(&root);
    let row_files: BTreeSet<&str> = rows.iter().map(|r| r.file.as_str()).collect();
    let disk: BTreeSet<&str> = files.values().map(|f| f.as_str()).collect();

    let missing_row: Vec<&&str> = disk.difference(&row_files).collect();
    assert!(
        missing_row.is_empty(),
        "ADR 파일은 있는데 인덱스에 행이 없다 — 인덱스가 카탈로그 구실을 못 한다: {missing_row:?}"
    );
    let missing_file: Vec<&&str> = row_files.difference(&disk).collect();
    assert!(
        missing_file.is_empty(),
        "인덱스 행이 없는 파일을 가리킨다 — 링크가 죽었다: {missing_file:?}"
    );
}

/// 본문의 `# ADR-NNNN: <제목>` 줄에서 **제목 부분**을 뽑는다.
///
/// 독법은 여기 하나다 — [`the_heading_number_matches_the_file_name`] 이 같은 줄에서
/// **번호**를 읽고 이 함수가 **제목**을 읽는다. 두 함수가 각자 그 줄을 찾으면 한쪽이
/// 못 찾는 형태가 생겨도 다른 쪽이 통과해, 무엇이 고장 났는지가 안 갈린다.
fn adr_heading(body: &str) -> Option<&str> {
    body.lines().find(|l| l.starts_with("# ADR-"))
}

/// 그 제목 줄의 `:` 뒤. 콜론이 없으면 제목이 없는 것으로 본다.
fn adr_title(body: &str) -> Option<String> {
    let head = adr_heading(body)?;
    let (_, title) = head.split_once(':')?;
    let title = title.trim();
    (!title.is_empty()).then(|| title.to_string())
}

/// 두 열의 정규화가 **무엇을 같다고 보고 무엇을 다르다고 보는지** 못 박는다.
///
/// 입력은 이 레포의 실제 값이 아니라 합성 문자열이다 — 가드 자신의 상수나 실물에서
/// 뽑으면 그 값에 대해 항진명제가 되고, 그때 초록은 규칙이 아니라 오늘의 데이터를
/// 재는 것이 된다.
#[test]
fn the_title_rule_folds_emphasis_and_nothing_else() {
    // 마커는 표기라 접는다.
    assert_eq!(
        normalize_title("가 **나** 다"),
        normalize_title("가 나 다"),
        "강조 마커는 값이 아니다"
    );
    // 공백 폭은 표기라 접는다.
    assert_eq!(normalize_title(" 가   나 "), "가 나");
    // 백틱은 **값이다** — `squeeze` 를 쓰면 이 단정이 깨진다.
    assert_ne!(
        normalize_title("`가`"),
        normalize_title("가"),
        "코드 표기의 유무는 제목의 값이다"
    );
    // `ADR-` 접두도 값이다 — 실패문이 본문에 없는 문자열을 찍으면 처방이 못 쓰인다.
    assert_ne!(normalize_title("ADR-0075"), normalize_title("0075"));
    // 부제 절단은 관용하지 않는다 — 접두여도 다른 값이다.
    assert_ne!(normalize_title("주제 — 부제"), normalize_title("주제"));
}

#[test]
fn the_tag_rule_is_set_equality_and_order_is_not_a_value() {
    let none: Vec<String> = Vec::new();
    // 같은 집합이면 나열 순서가 달라도 통과 — 순서는 값이 아니다.
    assert_eq!(tag_drift("가, 나", "나, 가"), (none.clone(), none.clone()));
    // 본문에 있는데 행에 없으면 첫 칸에 나온다.
    assert_eq!(
        tag_drift("가, 나", "가"),
        (vec!["나".to_string()], none.clone())
    );
    // ★ 행에만 있어도 이제 걸린다 — 앞 판이 안 보던 구간이 여기다.
    assert_eq!(
        tag_drift("가", "가, 다"),
        (none.clone(), vec!["다".to_string()])
    );
    // 공백과 빈 항목은 항목이 아니다.
    assert_eq!(
        tag_drift(" 가 ,, 나 ", "나,가"),
        (none.clone(), none.clone())
    );
    // 한쪽이 비면 다른 쪽 전부가 그 방향의 어긋남이다.
    assert_eq!(tag_drift("", "가"), (none.clone(), vec!["가".to_string()]));
}

/// 문서 안의 `# ADR-NNNN` 제목이 자기 파일명 번호와 같다.
///
/// 번호를 옮길 때 파일명만 바꾸고 본문을 안 고치면, 문서를 **열어서** 번호를 읽은
/// 사람만 틀린 값을 갖게 된다. 파일 목록만 보는 판정으로는 안 잡힌다.
#[test]
fn the_heading_number_matches_the_file_name() {
    let root = repo_root();
    let mut wrong = Vec::new();
    for (num, name) in adr_files(&root) {
        let body = read(&root, &format!("{ADR_DIR}/{name}"));
        let Some(head) = adr_heading(&body) else {
            wrong.push(format!("{name} — `# ADR-…` 제목 줄이 없다"));
            continue;
        };
        let in_head: String = head.trim_start_matches("# ADR-").chars().take(4).collect();
        if in_head != num {
            wrong.push(format!("{name} — 제목은 ADR-{in_head} 라고 말한다"));
        }
    }
    assert!(
        wrong.is_empty(),
        "파일명 번호와 본문 제목 번호가 다르다:\n  {}",
        wrong.join("\n  ")
    );
}

/// 인덱스 행의 `Status`·`Date` 가 그 ADR 본문 헤더의 값과 같은지 본다.
///
/// 두 곳에 있고 함께 움직여야 하는 값인데, 재sync·부분 개정에서는 본문만 올라가고
/// 행이 첫 커밋 값으로 남는다. 그때 인덱스만 읽는 사람은 죽은 결정을 살아 있는 것으로
/// 읽는다 — 실제로 0042 가 그 상태였다(본문 Superseded, 행 Accepted).
///
/// 표기 차이는 값 차이가 아니다: 본문은 `Superseded by [0032](0032-….md)` 처럼 인라인
/// 링크를 쓸 수 있고 행은 안 쓴다. 그래서 링크를 벗기고 `ADR-` 접두를 지운 뒤 비교한다.
/// 그러고도 본문이 사유를 덧붙이는 경우가 있어(`Superseded by 0052 (부분) — …`)
/// **행이 본문의 접두인지**를 묻는다 — 행은 본문의 짧은 형태다.
/// 판정 술어를 **직접** 부르는 대조. 위의 스캔 테스트만으로는 이 갈래들이 지켜지지
/// 않는다 — 실측 2026-09-06: `tolerated_too_much` 를 항상 `None` 으로 바꿔도 스캔은
/// 초록이었다(rc=0 · F=0). 코퍼스(`docs/adr/`)를 흔들면 갈래가 밟히지만 그 변이는
/// 원복하면 사라져 회귀에 안 남는다. 그래서 갈래마다 여기서 한 번 더 묻는다.
///
/// 같은 크레이트의 `temp_path` 는 이 형태를 12 개 갖고 있고, 그래서 그쪽은 판정을
/// 느슨하게 바꾸면 유닛이 빨개진다. 이 파일에는 0 이었다 — 그 차이가 구멍이었다.
#[test]
fn the_status_verdicts_each_have_their_own_reason() {
    // 빈 칸 — 최소 이행이 "아무거나 채운다" 가 되면 안 된다.
    assert_eq!(
        tolerated_too_much("Accepted", ""),
        Some("행의 상태 칸이 비었다")
    );
    assert_eq!(
        tolerated_too_much("Accepted", "   "),
        Some("행의 상태 칸이 비었다")
    );

    // 한 글자 — 접두이긴 하지만 상태 **이름**이 아니다. 빈 칸 처방의 최소 이행이
    // 정확히 이 형태였다.
    assert_eq!(
        tolerated_too_much("Accepted", "A"),
        Some("행이 상태 이름의 일부만 싣고 있다")
    );
    assert_eq!(
        tolerated_too_much("Superseded by 0162", "Sup"),
        Some("행이 상태 이름의 일부만 싣고 있다")
    );

    // 이름은 맞는데 대체한 번호를 잃었다 — 인덱스만 읽는 사람이 어디로 갔는지 모른다.
    assert_eq!(
        tolerated_too_much("Superseded by 0162", "Superseded"),
        Some("행이 대체한 ADR 번호를 잃었다")
    );

    // 뒤쪽을 자르는 것은 허용이다. 이름이 같고 번호가 남아 있으면 통과한다.
    assert_eq!(tolerated_too_much("Accepted", "Accepted"), None);
    assert_eq!(tolerated_too_much("Accepted (부분 적용)", "Accepted"), None);
    assert_eq!(
        tolerated_too_much("Superseded by 0162 — 사유", "Superseded by 0162"),
        None
    );
}

/// `superseder` 는 네 자리 번호만 인정한다. 이 갈래가 죽으면 위의 "번호를 잃었다" 가
/// 영영 안 밟히고, `MIN_SUPERSEDED` 하한이 그것을 대신 잡아 주지 않는다 — 하한은
/// **본문에 그 형태가 몇 개 있나**를 볼 뿐 판정이 사는지는 안 본다.
#[test]
fn a_superseder_is_a_four_digit_number_or_nothing() {
    assert_eq!(superseder("Superseded by 0162"), Some("0162".to_string()));
    assert_eq!(
        superseder("Superseded by 0162 — 사유가 뒤에 붙는다"),
        Some("0162".to_string())
    );
    assert_eq!(superseder("Accepted"), None);
    assert_eq!(superseder("Superseded by 162"), None); // 세 자리는 아니다
    assert_eq!(superseder("superseded by 0162"), None); // 대소문자가 다르면 아니다
}

/// 첫 낱말은 공백 앞까지다. 값이 비면 빈 문자열이고, 그때는 빈 칸 갈래가 먼저 잡는다.
#[test]
fn the_first_word_stops_at_whitespace() {
    assert_eq!(first_word("Accepted"), "Accepted");
    assert_eq!(first_word("Superseded by 0162"), "Superseded");
    assert_eq!(first_word("  Accepted  (부분)"), "Accepted");
    assert_eq!(first_word(""), "");
    assert_eq!(first_word("   "), "");
}

#[test]
fn an_index_row_mirrors_its_adr_header() {
    let root = repo_root();
    let rows = index_rows(&root);
    let files = adr_files(&root);
    let mut checked_status = 0usize;
    let mut checked_date = 0usize;
    let mut checked_title = 0usize;
    let mut checked_tags = 0usize;
    let mut superseded_seen = 0usize;
    let mut prefix_ok = 0usize;
    let mut drift: Vec<String> = Vec::new();

    for row in &rows {
        let Some(name) = files.get(&row.num) else {
            // 파일 없는 행은 `every_adr_file_has_a_row_and_every_row_has_a_file` 이 잡는다.
            continue;
        };
        let body = read(&root, &format!("{ADR_DIR}/{name}"));

        if let Some(v) = header_field(&body, "Status") {
            checked_status += 1;
            let want = normalize_header_value(&v);
            let got = normalize_header_value(&row.status);
            if superseder(&want).is_some() {
                superseded_seen += 1;
            }
            let verdict = if want.starts_with(&got) {
                prefix_ok += 1;
                tolerated_too_much(&want, &got)
            } else {
                Some("행이 본문의 접두가 아니다")
            };
            if let Some(why) = verdict {
                drift.push(format!(
                    "{} Status — {why}: 본문 {:?} · 행 {:?}",
                    row.num,
                    want.chars().take(60).collect::<String>(),
                    got
                ));
            }
        }
        if let Some(v) = adr_title(&body) {
            checked_title += 1;
            let want = normalize_title(&v);
            let got = normalize_title(&row.title);
            if want != got {
                drift.push(format!(
                    "{} Title — 본문 {want:?} · 행 {got:?}\n    \
                     처방: 행을 본문 제목에 맞춰라(본문이 정본이다). 마커 `**` 는 행에 안 싣는다",
                    row.num
                ));
            }
        }
        if let Some(v) = header_field(&body, "Tags") {
            checked_tags += 1;
            let (missing, extra) = tag_drift(&v, &row.tags);
            if !missing.is_empty() {
                drift.push(format!(
                    "{} Tags — 본문에 있는데 행에 없다: {}\n    \
                     처방: 그 항목을 행의 Tags 칸에 더해라",
                    row.num,
                    missing.join(", ")
                ));
            }
            if !extra.is_empty() {
                drift.push(format!(
                    "{} Tags — 행에 있는데 본문에 없다: {}\n    \
                     처방: 그 관계가 실재하면 본문 `- **Tags**:` 줄에 더하고, 본문이 그 ADR 을\n    \
                     한 번도 인용하지 않으면 실재하지 않는 관계이니 행에서 지워라",
                    row.num,
                    extra.join(", ")
                ));
            }
        }
        if let Some(v) = header_field(&body, "Date") {
            checked_date += 1;
            let want = normalize_header_value(&v);
            let got = normalize_header_value(&row.date);
            if want != got {
                drift.push(format!("{} Date — 본문 {want:?} · 행 {got:?}", row.num));
            }
        }
    }

    // ★ 아래 하한들은 **하한이지 모수가 아니다.** "표류 0" 이 안 봐서 0 인지 정말
    // 없어서 0 인지는 그 초록만으로 안 갈린다(조용한 0 의 형태). 그래서 모수를 여기서 싣는다.
    // libtest 는 통과한 테스트의 출력을 삼키므로 `-- --nocapture` 로 읽는다.
    // `tracing` 은 여기서 못 쓴다 — 이 크레이트는 의존이 0 인 것이 존재 이유라
    // (ADR-0138) subscriber 자체가 없다.
    // 단정보다 **앞**에 둔다: 빨간 경로에서도 모수가 남아야 한다.
    eprintln!(
        "[adr-index-parity] 행 {} · Status 대조 {checked_status} · Date 대조 {checked_date} \
         · Title 대조 {checked_title} · Tags 대조 {checked_tags} \
         · 접두 통과 {prefix_ok} · 대체 형태 {superseded_seen} · 표류 {}",
        rows.len(),
        drift.len()
    );

    // 0 을 통과로 만들지 않는다 — 열 하나가 안 읽히면 그 갈래는 조용히 빈다.
    // 두 열을 따로 센다: 한 열의 독법이 죽어도 다른 열의 수가 그것을 안 가린다.
    assert!(
        checked_status >= MIN_ADRS,
        "Status 를 {checked_status} 건밖에 못 읽었다(하한 {MIN_ADRS}) — \
         행이나 헤더 독법이 죽었으면 아래 초록은 거짓이다"
    );
    assert!(
        checked_date >= MIN_ADRS,
        "Date 를 {checked_date} 건밖에 못 읽었다(하한 {MIN_ADRS}) — \
         행이나 헤더 독법이 죽었으면 아래 초록은 거짓이다"
    );
    assert!(
        superseded_seen >= MIN_SUPERSEDED,
        "본문이 다른 ADR 을 가리키는 행을 {superseded_seen} 건밖에 못 봤다 \
         (하한 {MIN_SUPERSEDED}) — 그 형태가 없으면 '행이 대체 번호를 잃었다' \
         갈래가 한 번도 안 밟히고, 그때의 초록은 위반이 없다는 뜻이 아니다. \
         ★ 이 하한을 내려서 통과시키지 마라 — 내리면 그 갈래가 밟히는지를 아무도 \
         안 지킨다. 본문에서 그 형태가 정말 사라졌으면 갈래도 함께 지워라"
    );

    assert!(
        drift.is_empty(),
        "인덱스 행이 본문 헤더와 다른 값을 싣고 있다 {} 건. \
         ★ 본문이 정본이다 — 행을 본문에 맞춰라. 본문을 행에 맞추지 마라: \
         그렇게 해도 초록은 되지만, 인덱스만 읽는 사람은 죽은 결정을 살아 있는 것으로 \
         읽고 그 결정이 어디로 갔는지까지 잃는다:\n  {}",
        drift.len(),
        drift.join("\n  ")
    );
}

/// `MIN_ADRS` 의 **양성 대조** — 모수가 하한 아래로 떨어지는 코퍼스를 만들고, 이 파일의
/// 두 수집기가 실제로 그것을 말하는지 묻는다.
///
/// 하한이 있다는 것과 그 하한이 옳다는 것은 다르다. 위 세 시험은 전부 **레포 자신**을
/// 읽으므로 수집이 죽는 상황이 여기서는 한 번도 안 만들어진다 — 그 초록은 "수집이
/// 산다" 가 아니라 "레포에 ADR 이 많다" 만 말한다.
///
/// 상수 doc 이 말하는 **네 수가 갈리는 형태**를 칸으로 만든다. 두 수집기가 독립이라
/// 한쪽만 죽는 것이 실제 사고 모양이고(인덱스가 밀림 / 디렉터리를 못 읽음), 한쪽만
/// 보는 대조는 그것을 못 가른다.
///
/// 비영 대조를 함께 세운다 — 3/3 을 세는 칸이 있어야 앞 칸의 0 이 **구조의 0**(코퍼스가
/// 비었다)이지 **술어의 0**(수집기가 입력을 안 본다)이 아님이 갈린다.
#[test]
fn the_adr_floor_sees_a_collapsed_collection() {
    let root =
        std::env::temp_dir().join(format!("tasty-adrfloor-{}-{}", std::process::id(), line!()));
    // 앞선 실행의 잔여를 치운다 — 없는 것이 정상이라 실패가 정보가 아니다.
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join(ADR_DIR);
    std::fs::create_dir_all(&dir).expect("임시 디렉토리를 못 만들었다");
    std::fs::write(root.join(INDEX), "").expect("쓰기 실패");

    // ① 둘 다 비었다 — 하한이 잡아야 하는 상태.
    assert_eq!(
        adr_files(&root).len(),
        0,
        "뿌리가 비었는데 0 이 아니면 이 수집기는 인자를 안 보고 레포를 읽는 것이다"
    );
    assert_eq!(
        index_rows(&root).len(),
        0,
        "인덱스가 비었는데 0 이 아니면 이 독법은 인자를 안 보고 레포를 읽는 것이다"
    );

    // ② 표 머리글은 행이 아니다. 이것을 세면 인덱스가 통째로 밀려도 수가 안 떨어진다.
    std::fs::write(
        root.join(INDEX),
        "| 번호 | 제목 | Status | Date | Tags |\n|---|---|---|---|---|\n",
    )
    .expect("쓰기 실패");
    assert_eq!(
        index_rows(&root).len(),
        0,
        "머리글·구분줄을 ADR 행으로 세면 빈 인덱스가 하한을 통과한다"
    );

    // ③ 파일만 있고 행이 없다 — 두 수가 **갈리는** 칸. 상수 doc 의 "넷이 갈리면 독법이
    //    고장 난 것" 이 실제로 갈리는지를 여기서 본다.
    for (num, slug) in [("0001", "a"), ("0002", "b"), ("0003", "c")] {
        std::fs::write(dir.join(format!("{num}-{slug}.md")), "").expect("쓰기 실패");
    }
    assert_eq!(
        adr_files(&root).len(),
        3,
        "디렉터리를 안 읽으면 파일 수가 0 에 머문다"
    );
    assert_eq!(
        index_rows(&root).len(),
        0,
        "행이 없는데 행 수가 늘면 두 수가 서로를 가려 준다 — 그러면 한쪽이 죽어도 하한이 안 걸린다"
    );

    // ④ 비영 대조 — 행을 넣으면 두 수가 같이 3 이 된다. 앞 칸들의 0 이 구조의 0 이었음이
    //    여기서 갈린다.
    let mut index = String::from("| 번호 | 제목 | Status | Date | Tags |\n|---|---|---|---|---|\n");
    for (num, slug) in [("0001", "a"), ("0002", "b"), ("0003", "c")] {
        index.push_str(&format!(
            "| {num} | [제목]({num}-{slug}.md) | Accepted | 2026-09-07 | tag |\n"
        ));
    }
    std::fs::write(root.join(INDEX), &index).expect("쓰기 실패");
    assert_eq!(adr_files(&root).len(), 3, "파일 수가 흔들리면 안 된다");
    assert_eq!(
        index_rows(&root).len(),
        3,
        "행이 셋인데 3 이 아니면 독법이 죽은 것이다 — 그때 앞 칸의 0 은 코퍼스가 아니라 독법 탓이다"
    );

    // ⑤ 반대 방향 — 번호가 아닌 `.md` 를 세면 모수가 부풀고, 부푼 만큼 하한이 무뎌진다.
    //    `index.md` 는 이미 이 디렉터리에 있고 `template.md` 를 하나 더 놓는다.
    std::fs::write(dir.join("template.md"), "").expect("쓰기 실패");
    assert_eq!(
        adr_files(&root).len(),
        3,
        "앞 네 자리가 숫자가 아닌 `.md` 를 ADR 로 세면 하한이 그만큼 헐거워진다"
    );

    // ⑥ 그리고 이 코퍼스는 하한 아래다 — 위 세 시험이 이 뿌리를 읽었다면 빨개진다.
    assert!(
        adr_files(&root).len() < MIN_ADRS && index_rows(&root).len() < MIN_ADRS,
        "이 대조가 하한 위에 있으면 '하한이 무너진 상태' 를 한 번도 안 만든 것이다"
    );

    // 뒷정리 실패는 무시한다 — 임시 디렉토리라 남아도 다음 실행이 먼저 지우고, 여기서
    // 죽으면 위 단정의 결과가 정리 오류에 가린다.
    let _ = std::fs::remove_dir_all(&root);
}
