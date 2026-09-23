//! ADR 재번호의 **판정** — 매핑 파일을 읽고, 한 파일의 본문에서 ADR 번호가 나타나는 자리를
//! 형태별로 찾는다. 파일을 옮기고 쓰는 일(부수효과)은 `src/bin/adr-renumber.rs` 가 한다.
//!
//! 절차·형태 목록·못 보는 것은 `docs/dev-guide/adr-renumber.md` 가 정본이다. 여기에는 판정
//! 규칙이 코드와 함께 산다.
//!
//! ## 왜 형태를 가르는가
//!
//! 같은 네 자리가 어떤 자리에서는 ADR 좌표이고 어떤 자리에서는 날짜 · 측정값 · 유니코드
//! 코드포인트다. 이 모듈은 **좌표를 스스로 들고 있는 형태**만 고친다 — 파일명(slug 까지
//! 실재 파일과 일치해야 한다) · `ADR-` 접두 · 그 접두 뒤의 목록 · 링크 대상과 번호가 같은
//! 링크 텍스트 · 인덱스 머리말(인덱스 가드와 같은 술어). 나머지 네 자리는
//! [`Form::Bare`] 로 **보고만** 한다. 문맥으로 ADR 인지 추론하지 않는다 — 추론은 양방향으로
//! 틀리고, 틀린 쪽이 조용하다.

use std::collections::{BTreeMap, BTreeSet};

use crate::adr_index::{bare_numbers, lines_by_region};

/// 매핑 한 줄의 오른쪽.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Num(String),
    Delete,
}

/// 번호가 나타난 형태. 앞의 다섯은 고치는 형태, [`Form::Bare`] 는 보고만 하는 형태다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Form {
    /// `NNNN-<slug>` — slug 가 그 번호의 실재 ADR 과 **끝까지** 같다(`.md` · 앵커는 뒤에 붙어도
    /// 된다). 앞이 무엇이든(`](` · `docs/adr/` · `../adr/` · 백틱 · 따옴표) 같은 형태다.
    FileName,
    /// `ADR-NNNN` · `adr-NNNN` · `adr_NNNN` · `ADR NNNN` · `ADR #NNNN` (대소문자 무관).
    Prefixed,
    /// 접두 뒤 목록의 나머지 — `ADR-NNNN/MMMM` · `ADR-NNNN·MMMM` · `ADR-NNNN+MMMM`.
    ListCont,
    /// `[NNNN](…/NNNN-…)` · `` [`NNNN`](…) `` — 링크 대상 파일명의 번호와 같은 링크 텍스트.
    LinkText,
    /// `docs/adr/index.md` 의 생성 구역 밖에서 [`bare_numbers`] 가 번호로 보는 토큰.
    Preamble,
    /// 위 어디에도 안 드는 네 자리. **고치지 않는다.**
    Bare,
    /// `NNNN-<slug>.md` 인데 그 slug 의 파일이 없다(이미 죽은 인용 · 픽스처). **고치지 않는다.**
    UnknownFile,
}

impl Form {
    pub fn rewrites(self) -> bool {
        !matches!(self, Form::Bare | Form::UnknownFile)
    }

    pub fn label(self) -> &'static str {
        match self {
            Form::FileName => "파일명 NNNN-<slug>",
            Form::Prefixed => "접두 ADR-NNNN",
            Form::ListCont => "접두 목록 ADR-NNNN/MMMM",
            Form::LinkText => "링크 텍스트 [NNNN](NNNN-…)",
            Form::Preamble => "인덱스 머리말 번호",
            Form::Bare => "맨 네 자리",
            Form::UnknownFile => "slug 가 실재 파일과 다른 NNNN-<slug>.md",
        }
    }
}

/// 본문 안에서 네 자리 번호 하나가 차지하는 바이트 구간.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub start: usize,
    pub num: String,
    pub form: Form,
}

fn is_num(s: &str) -> bool {
    s.len() == 4 && s.bytes().all(|b| b.is_ascii_digit())
}

/// 매핑 파일을 읽는다. 형식: 한 줄에 `<옛 번호> <새 번호|DELETE>`, 빈 줄과 `#` 로 시작하는
/// 줄은 무시. `existing` 은 지금 `docs/adr/` 에 있는 번호 전부다.
///
/// 거절하는 것: 모르는 형식 · 없는 옛 번호 · 옛 번호 중복 · **빠진 ADR**(모든 ADR 이 한 번씩
/// 나와야 한다 — 빠진 것을 제자리로 두면 새 번호와 겹칠 수 있다) · 새 번호 중복 · `0000`.
pub fn parse_mapping(
    text: &str,
    existing: &BTreeSet<String>,
) -> Result<BTreeMap<String, Target>, Vec<String>> {
    let mut map = BTreeMap::new();
    let mut errs = Vec::new();
    let mut targets: BTreeMap<String, String> = BTreeMap::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        let [old, new] = parts[..] else {
            errs.push(format!("{}행: 칸이 둘이 아니다: {line}", i + 1));
            continue;
        };
        if !is_num(old) || !existing.contains(old) {
            errs.push(format!("{}행: {old} 는 지금 있는 ADR 번호가 아니다", i + 1));
            continue;
        }
        let target = if new == "DELETE" {
            Target::Delete
        } else if is_num(new) && new != "0000" {
            if let Some(prev) = targets.insert(new.to_string(), old.to_string()) {
                errs.push(format!("{}행: 새 번호 {new} 를 {prev} 도 받는다", i + 1));
            }
            Target::Num(new.to_string())
        } else {
            errs.push(format!(
                "{}행: 새 번호가 네 자리도 DELETE 도 아니다: {new}",
                i + 1
            ));
            continue;
        };
        if map.insert(old.to_string(), target).is_some() {
            errs.push(format!("{}행: 옛 번호 {old} 가 두 번 나온다", i + 1));
        }
    }
    for n in existing {
        if !map.contains_key(n) {
            errs.push(format!(
                "ADR {n} 이 매핑에 없다 — 제자리면 `{n} {n}` 으로 적는다"
            ));
        }
    }
    if errs.is_empty() { Ok(map) } else { Err(errs) }
}

fn digits_at(b: &[u8], i: usize) -> bool {
    i + 4 <= b.len()
        && b[i..i + 4].iter().all(u8::is_ascii_digit)
        && b.get(i + 4).is_none_or(|c| !c.is_ascii_digit())
}

fn prev_is(b: &[u8], i: usize, pred: impl Fn(u8) -> bool) -> bool {
    i > 0 && pred(b[i - 1])
}

fn slug_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'-'
}

/// `i` 에서 시작하는 `ADR` 접두(구분자 포함)를 넘긴 번호 자리.
fn prefixed_digits(b: &[u8], i: usize) -> Option<usize> {
    if i + 3 > b.len() || !b[i..i + 3].eq_ignore_ascii_case(b"adr") {
        return None;
    }
    if prev_is(b, i, |c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let j = i + 3;
    for sep in [&b" #"[..], b"-", b"_", b" ", b"#"] {
        if b[j..].starts_with(sep) && digits_at(b, j + sep.len()) {
            return Some(j + sep.len());
        }
    }
    None
}

/// 접두 번호(`d` 에서 시작) 뒤의 목록 — `]` 하나를 건너뛴 뒤 `/` · `·` · `+` 로 이은 네 자리들.
fn list_continuation(text: &str, d: usize) -> Vec<usize> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut k = d + 4;
    if b.get(k) == Some(&b']') {
        k += 1;
    }
    loop {
        let mut p = k;
        while b.get(p) == Some(&b' ') {
            p += 1;
        }
        let Some(sep) = ["/", "·", "+"]
            .into_iter()
            .find(|s| text[p..].starts_with(s))
        else {
            break;
        };
        p += sep.len();
        while b.get(p) == Some(&b' ') {
            p += 1;
        }
        if !digits_at(b, p) {
            break;
        }
        out.push(p);
        k = p + 4;
    }
    out
}

/// `[NNNN](대상)` · `` [`NNNN`](대상) `` 의 번호 자리 — 대상 경로의 마지막 마디가 같은
/// `NNNN-` 로 시작할 때만.
fn link_text_digits(text: &str, i: usize) -> Option<usize> {
    let b = text.as_bytes();
    if b[i] != b'[' {
        return None;
    }
    let tick = b.get(i + 1) == Some(&b'`');
    let d = i + 1 + usize::from(tick);
    if !digits_at(b, d) {
        return None;
    }
    let mut k = d + 4;
    if tick {
        if b.get(k) != Some(&b'`') {
            return None;
        }
        k += 1;
    }
    let rest = text.get(k..)?.strip_prefix("](")?;
    let target = &rest[..rest.find(')')?];
    let last = target.rsplit('/').next().unwrap_or(target);
    let num = &text[d..d + 4];
    last.strip_prefix(num)?.starts_with('-').then_some(d)
}

/// 한 파일 본문에서 번호 자리를 전부 찾는다. `stems` 는 번호 → 실재 ADR 파일명(확장자 뺀
/// 것). `is_index` 면 생성 구역은 통째로 건너뛰고(생성기가 다시 쓴다) 머리말에
/// [`Form::Preamble`] 을 더한다.
///
/// 같은 자리를 두 형태가 잡으면 앞 형태가 이긴다(순서: 파일명 · 접두 · 목록 · 링크 텍스트 ·
/// 머리말 · 맨 네 자리).
pub fn scan(text: &str, stems: &BTreeMap<String, String>, is_index: bool) -> Vec<Hit> {
    let b = text.as_bytes();
    let mut hits: BTreeMap<usize, Form> = BTreeMap::new();
    let skip = if is_index {
        index_regions(text)
    } else {
        Vec::new()
    };
    let in_skip = |p: usize| skip.iter().any(|(s, e)| (*s..*e).contains(&p));
    let mut put = |p: usize, f: Form| {
        if !in_skip(p) {
            hits.entry(p).or_insert(f);
        }
    };
    for i in 0..b.len() {
        if !text.is_char_boundary(i) {
            continue;
        }
        if digits_at(b, i)
            && b.get(i + 4) == Some(&b'-')
            && !prev_is(b, i, |c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
        {
            let num = &text[i..i + 4];
            match stems.get(num) {
                Some(stem)
                    if text[i..].starts_with(stem.as_str())
                        && b.get(i + stem.len()).is_none_or(|c| !slug_byte(*c)) =>
                {
                    put(i, Form::FileName);
                }
                _ => {
                    let mut e = i + 5;
                    while b.get(e).is_some_and(|c| slug_byte(*c)) {
                        e += 1;
                    }
                    if e > i + 5 && text[e..].starts_with(".md") {
                        put(i, Form::UnknownFile);
                    }
                }
            }
        }
        if let Some(d) = prefixed_digits(b, i) {
            put(d, Form::Prefixed);
            for p in list_continuation(text, d) {
                put(p, Form::ListCont);
            }
        }
        if let Some(d) = link_text_digits(text, i) {
            put(d, Form::LinkText);
        }
    }
    if is_index {
        for p in preamble_positions(text) {
            put(p, Form::Preamble);
        }
    }
    for i in 0..b.len() {
        if digits_at(b, i)
            && !prev_is(b, i, |c| c.is_ascii_alphanumeric() || b"._-#{".contains(&c))
            && b.get(i + 4)
                .is_none_or(|c| !c.is_ascii_alphanumeric() && *c != b'_')
        {
            put(i, Form::Bare);
        }
    }
    hits.into_iter()
        .map(|(start, form)| Hit {
            start,
            num: text[start..start + 4].to_string(),
            form,
        })
        .collect()
}

/// 줄 번호(1 부터) → 그 줄이 시작하는 바이트 위치.
fn line_starts(text: &str) -> Vec<usize> {
    let mut v = vec![0];
    v.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    v
}

/// 인덱스의 생성 구역(마커 줄 포함) 바이트 구간들.
fn index_regions(text: &str) -> Vec<(usize, usize)> {
    let starts = line_starts(text);
    let mut out = Vec::new();
    let mut open: Option<usize> = None;
    for (i, line) in text.split('\n').enumerate() {
        if line.starts_with("<!-- adr-rows:begin") {
            open = Some(starts[i]);
        } else if line.starts_with("<!-- adr-rows:end")
            && let Some(s) = open.take()
        {
            out.push((s, starts[i] + line.len()));
        }
    }
    out
}

/// 머리말(생성 구역 밖)에서 인덱스 가드와 **같은 술어**([`bare_numbers`])가 번호로 보는 자리.
fn preamble_positions(text: &str) -> Vec<usize> {
    let starts = line_starts(text);
    let mut out = Vec::new();
    for (n, line, inside) in lines_by_region(text) {
        if inside {
            continue;
        }
        let base = starts[n - 1];
        for num in bare_numbers(line) {
            out.push(base + (num.as_ptr() as usize - line.as_ptr() as usize));
        }
    }
    out
}

/// `hits` 중 고칠 것을 `map` 에 따라 적용한 본문. 옛 번호와 새 번호가 같거나 매핑에 없거나
/// DELETE 인 자리는 그대로 둔다. 모든 교체가 **한 번에** 적용된다 — 교환(A↔B)이 연쇄로
/// 되돌아가지 않는다.
pub fn apply(text: &str, hits: &[Hit], map: &BTreeMap<String, Target>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for h in hits.iter().filter(|h| h.form.rewrites()) {
        if let Some(Target::Num(new)) = map.get(&h.num)
            && *new != h.num
        {
            out.push_str(&text[last..h.start]);
            out.push_str(new);
            last = h.start + 4;
        }
    }
    out.push_str(&text[last..]);
    out
}

/// 바이트 위치 → (줄 번호, 그 줄).
pub fn line_of(text: &str, pos: usize) -> (usize, &str) {
    let start = text[..pos].rfind('\n').map_or(0, |i| i + 1);
    let end = text[pos..].find('\n').map_or(text.len(), |i| pos + i);
    (text[..pos].matches('\n').count() + 1, &text[start..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stems() -> BTreeMap<String, String> {
        [("0315", "0315-alpha-beta"), ("0316", "0316-gamma")]
            .into_iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    fn forms(text: &str) -> Vec<(String, Form)> {
        scan(text, &stems(), false)
            .into_iter()
            .map(|h| (h.num, h.form))
            .collect()
    }

    fn swap() -> BTreeMap<String, Target> {
        [
            ("0315", Target::Num("0316".into())),
            ("0316", Target::Num("0315".into())),
        ]
        .into_iter()
        .map(|(a, b)| (a.to_string(), b))
        .collect()
    }

    #[test]
    fn file_name_needs_the_whole_slug() {
        assert_eq!(
            forms("[x](0315-alpha-beta.md)"),
            vec![("0315".into(), Form::FileName)]
        );
        assert_eq!(
            forms("docs/adr/0315-alpha-beta.md#h"),
            vec![("0315".into(), Form::FileName)]
        );
        assert_eq!(
            forms("[`0315-alpha-beta`]"),
            vec![("0315".into(), Form::FileName)]
        );
        // slug 가 더 길거나 다르면 파일명이 아니다.
        assert_eq!(
            forms("0315-alpha-beta-x.md"),
            vec![("0315".into(), Form::UnknownFile)]
        );
        assert_eq!(
            forms("(0315-x.md)"),
            vec![("0315".into(), Form::UnknownFile)]
        );
        // 날짜 꼬리의 네 자리는 파일명이 아니다(앞의 연도는 맨 네 자리로만 보고된다).
        assert_eq!(
            forms("2026-0315-alpha-beta"),
            vec![("2026".into(), Form::Bare)]
        );
    }

    #[test]
    fn prefixed_forms_and_lists() {
        let got = forms("ADR-0315 adr-0316 adr_0315 ADR 0316 ADR #0315 Adr-0316");
        assert!(got.iter().all(|(_, f)| *f == Form::Prefixed), "{got:?}");
        assert_eq!(got.len(), 6);
        assert_eq!(
            forms("ADR-0315/0316 · [ADR-0316]/0315 ADR-0315 · 0316 ADR-0315+0316"),
            vec![
                ("0315".into(), Form::Prefixed),
                ("0316".into(), Form::ListCont),
                ("0316".into(), Form::Prefixed),
                ("0315".into(), Form::ListCont),
                ("0315".into(), Form::Prefixed),
                ("0316".into(), Form::ListCont),
                ("0315".into(), Form::Prefixed),
                ("0316".into(), Form::ListCont),
            ]
        );
        // 쉼표 · 범위는 목록으로 안 본다.
        assert_eq!(
            forms("ADR-0315, 0316 ADR-0315~0316"),
            vec![
                ("0315".into(), Form::Prefixed),
                ("0316".into(), Form::Bare),
                ("0315".into(), Form::Prefixed),
                ("0316".into(), Form::Bare),
            ]
        );
        // 낱말 안의 adr 은 접두가 아니다.
        assert_eq!(forms("xadr-0315"), vec![]);
    }

    #[test]
    fn link_text_only_when_the_target_has_the_same_number() {
        assert_eq!(
            forms(
                "[0315](0315-alpha-beta.md) [`0316`](../adr/0316-gamma.md) [0315](0316-gamma.md)"
            ),
            vec![
                ("0315".into(), Form::LinkText),
                ("0315".into(), Form::FileName),
                ("0316".into(), Form::LinkText),
                ("0316".into(), Form::FileName),
                ("0315".into(), Form::Bare),
                ("0316".into(), Form::FileName),
            ]
        );
    }

    #[test]
    fn bare_numbers_are_reported_not_rewritten() {
        let text = "0315 가 말했다. e\\u{0316} 0315x";
        assert_eq!(forms(text), vec![("0315".into(), Form::Bare)]);
        assert_eq!(apply(text, &scan(text, &stems(), false), &swap()), text);
    }

    #[test]
    fn swap_is_simultaneous() {
        let text = "[ADR-0315](0315-alpha-beta.md) → [ADR-0316](0316-gamma.md)";
        let out = apply(text, &scan(text, &stems(), false), &swap());
        assert_eq!(
            out,
            "[ADR-0316](0316-alpha-beta.md) → [ADR-0315](0315-gamma.md)"
        );
    }

    #[test]
    fn index_skips_generated_rows_and_reads_the_preamble() {
        let idx = "머리말 0315 → 0316.\n<!-- adr-rows:begin g -->\n| 0315 | [t](0315-alpha-beta.md) |\n<!-- adr-rows:end g -->\n";
        let hits = scan(idx, &stems(), true);
        assert_eq!(
            hits.iter()
                .map(|h| (h.num.as_str(), h.form))
                .collect::<Vec<_>>(),
            vec![("0315", Form::Preamble), ("0316", Form::Preamble)]
        );
    }

    #[test]
    fn mapping_must_be_complete_and_injective() {
        let have: BTreeSet<String> = ["0315", "0316"].into_iter().map(String::from).collect();
        assert!(parse_mapping("0315 0316\n0316 0315\n", &have).is_ok());
        assert!(parse_mapping("# c\n\n0315 0001\n0316 DELETE\n", &have).is_ok());
        let errs = |t: &str| parse_mapping(t, &have).unwrap_err();
        assert_eq!(errs("0315 0315\n").len(), 1, "빠진 ADR");
        assert_eq!(errs("0315 0001\n0316 0001\n").len(), 1, "새 번호 중복");
        assert_eq!(
            errs("0315 0315\n0316 0316\n0999 0999\n").len(),
            1,
            "없는 옛 번호"
        );
        assert_eq!(
            errs("0315 0315\n0315 0316\n0316 0316\n").len(),
            2,
            "옛 번호 중복"
        );
        assert_eq!(
            errs("0315 0000\n0316 0316\n").len(),
            2,
            "0000 은 번호가 아니다"
        );
    }
}
