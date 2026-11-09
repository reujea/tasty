//! 줄 단위 diff. 줄은 줄바꿈을 포함한 원문 조각이다 — 이어 붙이면 원문이 그대로 나온다.
//!
//! 공통 앞·뒤를 먼저 잘라 내고 가운데만 LCS 로 견준다. 공동 편집의 변경은 대개 몇 줄이라
//! 가운데가 작다. 가운데가 너무 크면(곱이 [`LCS_CELL_LIMIT`] 초과) 가운데 전체를 hunk 하나로
//! 본다 — 틀린 답이 아니라 거친 답이다(병합에서는 conflict 로 드러난다).

use std::ops::Range;

/// LCS 표 칸 수 상한. 4 M 칸 × 4 B = 16 MB.
const LCS_CELL_LIMIT: usize = 4_000_000;

/// 원문을 줄 조각으로 나눈다(줄바꿈 포함).
pub fn split_lines(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find('\n') {
        out.push(&rest[..=i]);
        rest = &rest[i + 1..];
    }
    if !rest.is_empty() {
        out.push(rest);
    }
    out
}

/// `a[a_range]` 를 `b[b_range]` 로 바꾸는 변경 하나. 둘 중 하나는 비어 있을 수 있다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub a: Range<usize>,
    pub b: Range<usize>,
}

/// `a` → `b` 의 hunk 들(겹치지 않고 오름차순).
pub fn diff_lines(a: &[&str], b: &[&str]) -> Vec<Hunk> {
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let max_suffix = a.len().min(b.len()) - prefix;
    let suffix = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take(max_suffix)
        .take_while(|(x, y)| x == y)
        .count();
    let am = &a[prefix..a.len() - suffix];
    let bm = &b[prefix..b.len() - suffix];
    if am.is_empty() && bm.is_empty() {
        return Vec::new();
    }
    if am.is_empty() || bm.is_empty() || am.len().saturating_mul(bm.len()) > LCS_CELL_LIMIT {
        return vec![Hunk {
            a: prefix..prefix + am.len(),
            b: prefix..prefix + bm.len(),
        }];
    }
    lcs_hunks(am, bm)
        .into_iter()
        .map(|h| Hunk {
            a: h.a.start + prefix..h.a.end + prefix,
            b: h.b.start + prefix..h.b.end + prefix,
        })
        .collect()
}

fn lcs_hunks(a: &[&str], b: &[&str]) -> Vec<Hunk> {
    let (n, m) = (a.len(), b.len());
    let w = m + 1;
    // dp[i][j] = a[i..] 와 b[j..] 의 LCS 길이.
    let mut dp = vec![0u32; (n + 1) * w];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i * w + j] = if a[i] == b[j] {
                dp[(i + 1) * w + j + 1] + 1
            } else {
                dp[(i + 1) * w + j].max(dp[i * w + j + 1])
            };
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    let (mut hs_a, mut hs_b) = (0, 0);
    let mut open = false;
    while i < n || j < m {
        if i < n && j < m && a[i] == b[j] {
            if open {
                out.push(Hunk {
                    a: hs_a..i,
                    b: hs_b..j,
                });
                open = false;
            }
            i += 1;
            j += 1;
            continue;
        }
        if !open {
            hs_a = i;
            hs_b = j;
            open = true;
        }
        if j < m && (i == n || dp[i * w + j + 1] >= dp[(i + 1) * w + j]) {
            j += 1;
        } else {
            i += 1;
        }
    }
    if open {
        out.push(Hunk {
            a: hs_a..n,
            b: hs_b..m,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// hunk 들을 `a` 에 적용해 `b` 를 다시 만든다(시험·검증용 왕복).
    pub fn apply_hunks(a: &[&str], b: &[&str], hunks: &[Hunk]) -> String {
        let mut out = String::new();
        let mut at = 0;
        for h in hunks {
            out.extend(a[at..h.a.start].iter().copied());
            out.extend(b[h.b.clone()].iter().copied());
            at = h.a.end;
        }
        out.extend(a[at..].iter().copied());
        out
    }

    fn roundtrip(a: &str, b: &str) -> Vec<Hunk> {
        let (la, lb) = (split_lines(a), split_lines(b));
        let h = diff_lines(&la, &lb);
        assert_eq!(apply_hunks(&la, &lb, &h), b, "round trip {a:?} -> {b:?}");
        h
    }

    #[test]
    fn identical_has_no_hunks() {
        assert!(roundtrip("a\nb\n", "a\nb\n").is_empty());
        assert!(roundtrip("", "").is_empty());
    }

    #[test]
    fn single_line_change_is_one_hunk() {
        let h = roundtrip("a\nb\nc\n", "a\nB\nc\n");
        assert_eq!(h, vec![Hunk { a: 1..2, b: 1..2 }]);
    }

    #[test]
    fn separate_changes_are_separate_hunks() {
        let h = roundtrip("1\n2\n3\n4\n5\n", "1\nX\n3\n4\nY\n5\n");
        assert_eq!(h.len(), 2);
        roundtrip("a\nb\n", "");
        roundtrip("", "x\n");
        roundtrip("a\nb", "a\nb\n");
        roundtrip("x\ny\nz\n", "z\ny\nx\n");
    }
}
