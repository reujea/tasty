//! 외부 변경 검토(A3). 검토 기준(`base` — 외부 쓰기 직전 내용)과 검토 대상(`target` — 외부
//! 쓰기 이후 내용)의 줄 diff 를 hunk 로 보여 주고 hunk 별로 유지·되돌린다.
//!
//! - **유지**는 base 에 그 hunk 를 적용한다 → 그 hunk 가 diff 에서 빠진다.
//! - **되돌리기**는 target 에서 그 hunk 를 base 쪽으로 되돌린다 → 역시 diff 에서 빠진다.
//!
//! 두 연산 모두 결과는 "남은 hunk 가 하나 줄었다" 이고, 0 개가 되면 검토가 끝난다.

use std::collections::HashSet;

use serde::Serialize;

use super::diff::{Hunk, diff_lines, split_lines};

/// hunk 를 누가 만들었나.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    /// `mdedit` 쓰기 연산으로 들어왔거나 `> [!agent]` 블록이다.
    Agent,
    /// 출처를 모른다(파일 도구·다른 편집기).
    External,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReviewHunk {
    pub index: usize,
    /// target 기준 1 기준 시작 줄(삭제만 있는 hunk 는 그 자리).
    pub line: usize,
    pub removed: String,
    pub added: String,
    pub origin: Origin,
}

/// `mdedit` 연산이 넣고 뺀 줄 — hunk 출처 판정에 쓴다.
#[derive(Debug, Clone, Default)]
pub struct AgentLines {
    pub added: HashSet<String>,
    pub removed: HashSet<String>,
}

impl AgentLines {
    /// 연산 한 번(`before` → `after`)의 줄 변화를 기록한다.
    pub fn record(&mut self, before: &str, after: &str) {
        let (bl, al) = (split_lines(before), split_lines(after));
        for h in diff_lines(&bl, &al) {
            self.removed
                .extend(bl[h.a.clone()].iter().map(|s| s.to_string()));
            self.added
                .extend(al[h.b.clone()].iter().map(|s| s.to_string()));
        }
    }

    fn origin(&self, removed: &[&str], added: &[&str]) -> Origin {
        let nonblank = |l: &&&str| !l.trim().is_empty();
        if added.iter().any(|l| l.starts_with("> [!agent]")) {
            return Origin::Agent;
        }
        let added_ok = added
            .iter()
            .filter(nonblank)
            .all(|l| self.added.contains(*l));
        let removed_ok = removed
            .iter()
            .filter(nonblank)
            .all(|l| self.removed.contains(*l));
        let any = added.iter().chain(removed).any(|l| !l.trim().is_empty());
        if any && added_ok && removed_ok {
            Origin::Agent
        } else {
            Origin::External
        }
    }
}

fn hunks_of(base: &str, target: &str) -> Vec<Hunk> {
    diff_lines(&split_lines(base), &split_lines(target))
}

pub fn hunks(base: &str, target: &str, agent: &AgentLines) -> Vec<ReviewHunk> {
    let (bl, tl) = (split_lines(base), split_lines(target));
    diff_lines(&bl, &tl)
        .into_iter()
        .enumerate()
        .map(|(index, h)| ReviewHunk {
            index,
            line: h.b.start + 1,
            removed: bl[h.a.clone()].concat(),
            added: tl[h.b.clone()].concat(),
            origin: agent.origin(&bl[h.a.clone()], &tl[h.b.clone()]),
        })
        .collect()
}

/// hunk 번호 하나를 base 에 적용한 새 base. 번호가 없으면 `None`.
pub fn keep(base: &str, target: &str, index: usize) -> Option<String> {
    let (bl, tl) = (split_lines(base), split_lines(target));
    let h = diff_lines(&bl, &tl).into_iter().nth(index)?;
    let mut out: String = bl[..h.a.start].concat();
    out.push_str(&tl[h.b.clone()].concat());
    out.push_str(&bl[h.a.end..].concat());
    Some(out)
}

/// hunk 번호 하나를 target 에서 되돌린 새 target. 번호가 없으면 `None`.
pub fn revert(base: &str, target: &str, index: usize) -> Option<String> {
    let (bl, tl) = (split_lines(base), split_lines(target));
    let h = hunks_of(base, target).into_iter().nth(index)?;
    let mut out: String = tl[..h.b.start].concat();
    out.push_str(&bl[h.a.clone()].concat());
    out.push_str(&tl[h.b.end..].concat());
    Some(out)
}

/// target 의 변경 줄 번호(1 기준) → 출처. 원문 편집기 거터 표시에 쓴다.
pub fn changed_lines(base: &str, target: &str, agent: &AgentLines) -> Vec<(usize, Origin)> {
    let mut out = Vec::new();
    for h in hunks(base, target, agent) {
        let n = split_lines(&h.added).len().max(1);
        out.extend((h.line..h.line + n).map(|l| (l, h.origin)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "# a\n- x\n\n# b\n- y\n";

    #[test]
    fn keep_and_revert_each_remove_one_hunk() {
        let target = "# a\n- x\n- agent\n\n# b\n- Y\n";
        let none = AgentLines::default();
        assert_eq!(hunks(BASE, target, &none).len(), 2);
        let kept = keep(BASE, target, 0).unwrap();
        assert_eq!(hunks(&kept, target, &none).len(), 1);
        let reverted = revert(BASE, target, 1).unwrap();
        assert_eq!(reverted, "# a\n- x\n- agent\n\n# b\n- y\n");
        assert_eq!(hunks(BASE, &reverted, &none).len(), 1);
        assert!(keep(BASE, target, 5).is_none());
    }

    #[test]
    fn origin_from_agent_ops_and_marker() {
        let mut agent = AgentLines::default();
        agent.record(BASE, "# a\n- x\n- agent\n\n# b\n- y\n");
        let target = "# a\n- x\n- agent\n\n# b\n- Y\n> [!agent] t\n";
        let hs = hunks(BASE, target, &agent);
        let origins: Vec<_> = hs.iter().map(|h| h.origin).collect();
        assert_eq!(origins, vec![Origin::Agent, Origin::Agent]);
        let target = "# a\n- x\n\n# b\n- Z\n";
        assert_eq!(hunks(BASE, target, &agent)[0].origin, Origin::External);
        let lines = changed_lines(BASE, "# a\n- x\n- agent\n\n# b\n- y\n", &agent);
        assert_eq!(lines, vec![(3, Origin::Agent)]);
    }
}
