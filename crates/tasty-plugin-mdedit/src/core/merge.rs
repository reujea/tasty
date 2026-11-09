//! 줄 단위 3-way 병합(diff3). base 에서 갈라진 두 쪽(ours = 편집 버퍼, theirs = 디스크)을
//! 합친다. 두 쪽이 **서로 다른 줄**을 고쳤으면 둘 다 남고, 같은 줄(겹치는 base 범위)을 다르게
//! 고쳤으면 conflict 로 남긴다 — 조용히 한쪽을 고르지 않는다(docs/surfaces/mdedit/index.md#방어선은-쓰기-이후에-둔다).
//!
//! 겹침 판정:
//! - 두 변경의 base 범위가 겹치면 겹친다.
//! - 같은 자리에 둘 다 끼워 넣으면 겹친다(순서를 정할 근거가 없다).
//! - 한쪽이 끼워 넣은 자리가 다른 쪽 변경 범위의 **안쪽**이면 겹친다. 경계(바로 앞·뒤)면
//!   겹치지 않는다 — 섹션 끝에 붙이는 것과 그 앞 줄을 고치는 것은 둘 다 남는다.

use super::diff::{Hunk, diff_lines, split_lines};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Ours,
    Theirs,
}

/// 겹친 한 덩어리.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// base 에서의 1 기준 시작 줄.
    pub base_line: usize,
    pub base: String,
    pub ours: String,
    pub theirs: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chunk {
    Clean(String),
    Conflict(Conflict),
}

/// conflict 하나를 어떻게 풀지.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Ours,
    Theirs,
    /// ours 다음에 theirs.
    Both,
}

impl Choice {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "ours" | "mine" => Self::Ours,
            "theirs" | "disk" => Self::Theirs,
            "both" => Self::Both,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Merge {
    pub chunks: Vec<Chunk>,
}

impl Merge {
    pub fn conflicts(&self) -> impl Iterator<Item = &Conflict> {
        self.chunks.iter().filter_map(|c| match c {
            Chunk::Conflict(x) => Some(x),
            Chunk::Clean(_) => None,
        })
    }

    pub fn conflict_count(&self) -> usize {
        self.conflicts().count()
    }

    /// conflict 가 없을 때의 결과.
    pub fn clean_text(&self) -> Option<String> {
        (self.conflict_count() == 0).then(|| self.resolve(&[]))
    }

    /// conflict 를 순서대로 `choices` 로 푼 결과. 모자라면 ours 로 본다.
    pub fn resolve(&self, choices: &[Choice]) -> String {
        let mut out = String::new();
        let mut k = 0;
        for c in &self.chunks {
            match c {
                Chunk::Clean(s) => out.push_str(s),
                Chunk::Conflict(x) => {
                    match choices.get(k).copied().unwrap_or(Choice::Ours) {
                        Choice::Ours => out.push_str(&x.ours),
                        Choice::Theirs => out.push_str(&x.theirs),
                        Choice::Both => {
                            out.push_str(&x.ours);
                            if !x.ours.is_empty() && !x.ours.ends_with('\n') {
                                out.push('\n');
                            }
                            out.push_str(&x.theirs);
                        }
                    }
                    k += 1;
                }
            }
        }
        out
    }
}

fn overlaps(lo: usize, hi: usize, h: &Hunk) -> bool {
    if lo == hi {
        // 그룹이 한 점(끼워 넣기)
        if h.a.is_empty() {
            h.a.start == lo
        } else {
            h.a.start < lo && lo < h.a.end
        }
    } else if h.a.is_empty() {
        lo < h.a.start && h.a.start < hi
    } else {
        h.a.start < hi && lo < h.a.end
    }
}

/// base 줄 `lo..hi` 에 한쪽 hunk 들을 적용한 텍스트.
fn region(base: &[&str], side: &[&str], lo: usize, hi: usize, hunks: &[&Hunk]) -> String {
    let mut out = String::new();
    let mut at = lo;
    for h in hunks {
        out.extend(base[at..h.a.start].iter().copied());
        out.extend(side[h.b.clone()].iter().copied());
        at = h.a.end;
    }
    out.extend(base[at..hi].iter().copied());
    out
}

pub fn merge3(base: &str, ours: &str, theirs: &str) -> Merge {
    let (bl, ol, tl) = (split_lines(base), split_lines(ours), split_lines(theirs));
    let mut all: Vec<(Side, Hunk)> = diff_lines(&bl, &ol)
        .into_iter()
        .map(|h| (Side::Ours, h))
        .chain(diff_lines(&bl, &tl).into_iter().map(|h| (Side::Theirs, h)))
        .collect();
    all.sort_by_key(|(_, h)| (h.a.start, h.a.end));

    let mut chunks = Vec::new();
    let mut clean = String::new();
    let mut at = 0;
    let mut i = 0;
    while i < all.len() {
        let (lo, mut hi) = (all[i].1.a.start, all[i].1.a.end);
        let mut group = vec![&all[i]];
        i += 1;
        while i < all.len() && overlaps(lo, hi, &all[i].1) {
            hi = hi.max(all[i].1.a.end);
            group.push(&all[i]);
            i += 1;
        }
        clean.extend(bl[at..lo].iter().copied());
        at = hi;
        let ours_h: Vec<&Hunk> = group
            .iter()
            .filter(|(s, _)| *s == Side::Ours)
            .map(|(_, h)| h)
            .collect();
        let theirs_h: Vec<&Hunk> = group
            .iter()
            .filter(|(s, _)| *s == Side::Theirs)
            .map(|(_, h)| h)
            .collect();
        let o = region(&bl, &ol, lo, hi, &ours_h);
        let t = region(&bl, &tl, lo, hi, &theirs_h);
        if ours_h.is_empty() {
            clean.push_str(&t);
        } else if theirs_h.is_empty() || o == t {
            clean.push_str(&o);
        } else {
            if !clean.is_empty() {
                chunks.push(Chunk::Clean(std::mem::take(&mut clean)));
            }
            chunks.push(Chunk::Conflict(Conflict {
                base_line: lo + 1,
                base: bl[lo..hi].concat(),
                ours: o,
                theirs: t,
            }));
        }
    }
    clean.extend(bl[at..].iter().copied());
    if !clean.is_empty() {
        chunks.push(Chunk::Clean(clean));
    }
    Merge { chunks }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "# 할 일\n- [ ] a\n\n# 2026.10.22\n- x\n\n# 2026.10.21\n- y\n";

    #[test]
    fn different_sections_keep_both() {
        let ours = BASE.replace("- x\n", "- x\n- user\n");
        let theirs = BASE.replace("- y\n", "- y\n> [!agent] t\n");
        let m = merge3(BASE, &ours, &theirs);
        assert_eq!(
            m.clean_text().unwrap(),
            "# 할 일\n- [ ] a\n\n# 2026.10.22\n- x\n- user\n\n# 2026.10.21\n- y\n> [!agent] t\n"
        );
    }

    #[test]
    fn same_line_conflicts_and_resolves() {
        let ours = BASE.replace("- x\n", "- mine\n");
        let theirs = BASE.replace("- x\n", "- disk\n");
        let m = merge3(BASE, &ours, &theirs);
        assert_eq!(m.conflict_count(), 1);
        let c = m.conflicts().next().unwrap();
        assert_eq!(c.base_line, 5);
        assert_eq!(
            (c.ours.as_str(), c.theirs.as_str()),
            ("- mine\n", "- disk\n")
        );
        assert_eq!(m.resolve(&[Choice::Theirs]), theirs);
        assert_eq!(m.resolve(&[Choice::Ours]), ours);
        assert_eq!(
            m.resolve(&[Choice::Both]),
            BASE.replace("- x\n", "- mine\n- disk\n")
        );
    }

    #[test]
    fn identical_change_on_both_sides_is_clean() {
        let both = BASE.replace("[ ] a", "[x] a");
        assert_eq!(merge3(BASE, &both, &both).clean_text().unwrap(), both);
    }

    #[test]
    fn adjacent_edits_do_not_conflict() {
        // 사용자가 '- x' 를 고치고 에이전트가 그 바로 뒤에 줄을 붙인다.
        let ours = BASE.replace("- x\n", "- X\n");
        let theirs = BASE.replace("- x\n\n", "- x\n- agent\n\n");
        assert_eq!(
            merge3(BASE, &ours, &theirs).clean_text().unwrap(),
            BASE.replace("- x\n\n", "- X\n- agent\n\n")
        );
    }

    #[test]
    fn same_point_insertions_conflict() {
        let ours = BASE.replace("- x\n", "- x\n- u\n");
        let theirs = BASE.replace("- x\n", "- x\n- a\n");
        assert_eq!(merge3(BASE, &ours, &theirs).conflict_count(), 1);
    }

    #[test]
    fn one_side_only() {
        let ours = BASE.replace("- y\n", "");
        assert_eq!(merge3(BASE, &ours, BASE).clean_text().unwrap(), ours);
        assert_eq!(merge3(BASE, BASE, &ours).clean_text().unwrap(), ours);
    }
}
