//! child 목록과 작업 DAG 가 함께 쓰는 부모 ID 표. 형식의 뜻은 docs/dev-guide/cli-structure.md 의
//! "부모 ID 표" 절에 있다. 그래프가 순환해도 패닉하지 않고 모든 행을 낸다.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

/// CLI 에서만 쓰는 형식 이름. 호스트 응답은 표를 모른다.
pub(crate) const TABLE_FORMAT: &str = "table";

const NO_VALUE: &str = "-";

/// id 비교 기준. surface id 는 숫자이므로 문자열로 정렬하면 `10` 이 `9` 앞에 온다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum IdOrder {
    Numeric,
    Text,
}

impl IdOrder {
    fn compare(self, a: &str, b: &str) -> Ordering {
        match self {
            Self::Text => a.cmp(b),
            // 숫자가 아닌 id 는 숫자 뒤에 문자열 순으로 둔다.
            Self::Numeric => match (a.parse::<u64>(), b.parse::<u64>()) {
                (Ok(x), Ok(y)) => x.cmp(&y),
                (Ok(_), Err(_)) => Ordering::Less,
                (Err(_), Ok(_)) => Ordering::Greater,
                (Err(_), Err(_)) => a.cmp(b),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct TableRow {
    pub(super) id: String,
    /// 중복과 순서는 렌더러가 정리한다. 표에 없는 id 도 그대로 적는다.
    pub(super) parents: Vec<String>,
    /// `columns` 와 같은 순서의 셀. `None` 과 빈 문자열은 값 없음이다.
    pub(super) cells: Vec<Option<String>>,
}

/// `id`·`parents` 뒤에 `columns` 를 붙인 표를 만든다. 끝에 줄바꿈이 있다.
pub(super) fn render(columns: &[&str], mut rows: Vec<TableRow>, order: IdOrder) -> String {
    let mut out = String::from("| id | parents |");
    for column in columns {
        out.push(' ');
        out.push_str(column);
        out.push_str(" |");
    }
    out.push('\n');
    out.push_str(&"|---".repeat(columns.len() + 2));
    out.push_str("|\n");

    for row in &mut rows {
        row.parents.sort_by(|a, b| order.compare(a, b));
        row.parents.dedup();
    }
    for index in visit_order(&rows, order) {
        push_row(&mut out, &rows[index]);
    }
    out
}

/// 뿌리부터 깊이 우선 전위 순회하고, 부모가 여럿인 행은 처음 닿는 자리에 둔다.
/// 표 밖의 부모만 가진 행은 뿌리다. 순환이라 뿌리에서 닿지 않는 행은 가장 작은 id 부터 이어서 걷는다.
fn visit_order(rows: &[TableRow], order: IdOrder) -> Vec<usize> {
    let ids: HashSet<&str> = rows.iter().map(|row| row.id.as_str()).collect();
    let mut kids: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, row) in rows.iter().enumerate() {
        for parent in &row.parents {
            kids.entry(parent.as_str()).or_default().push(index);
        }
    }
    // 스택에서 꺼내므로 큰 id 를 먼저 넣는다.
    let descending = |a: &usize, b: &usize| order.compare(&rows[*b].id, &rows[*a].id);
    for list in kids.values_mut() {
        list.sort_by(descending);
    }
    let is_root = |row: &TableRow| !row.parents.iter().any(|p| ids.contains(p.as_str()));
    let mut by_id: Vec<usize> = (0..rows.len()).collect();
    by_id.sort_by(|a, b| descending(b, a));
    let roots = by_id.iter().copied().filter(|i| is_root(&rows[*i]));
    let starts: Vec<usize> = roots.chain(by_id.iter().copied()).collect();

    let mut visited = vec![false; rows.len()];
    let mut result = Vec::with_capacity(rows.len());
    let mut stack = Vec::new();
    for start in starts {
        stack.push(start);
        while let Some(index) = stack.pop() {
            if std::mem::replace(&mut visited[index], true) {
                continue;
            }
            result.push(index);
            if let Some(list) = kids.get(rows[index].id.as_str()) {
                stack.extend(list.iter().copied().filter(|kid| !visited[*kid]));
            }
        }
    }
    result
}

fn push_row(out: &mut String, row: &TableRow) {
    out.push_str("| ");
    out.push_str(&cell(Some(&row.id)));
    out.push_str(" | ");
    let parents: Vec<String> = row
        .parents
        .iter()
        .map(|p| escape(p).replace(',', "\\,"))
        .filter(|p| !p.is_empty())
        .collect();
    if parents.is_empty() {
        out.push_str(NO_VALUE);
    } else {
        out.push_str(&parents.join(","));
    }
    out.push_str(" |");
    for value in &row.cells {
        out.push(' ');
        out.push_str(&cell(value.as_deref()));
        out.push_str(" |");
    }
    out.push('\n');
}

fn cell(value: Option<&str>) -> String {
    let escaped = escape(value.unwrap_or_default());
    if escaped.is_empty() {
        NO_VALUE.to_string()
    } else {
        escaped
    }
}

/// 값이 행과 칸의 경계를 넘지 못하게 한다. 줄바꿈과 캐리지 리턴은 보이게 남기고 다른 제어 문자는 지운다.
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '|' => out.push_str("\\|"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, parents: &[&str]) -> TableRow {
        TableRow {
            id: id.into(),
            parents: parents.iter().map(|p| p.to_string()).collect(),
            cells: vec![Some(format!("v{id}"))],
        }
    }

    fn ids(table: &str) -> Vec<String> {
        table
            .lines()
            .skip(2)
            .map(|line| line.split(" | ").next().unwrap()[2..].to_string())
            .collect()
    }

    #[test]
    fn the_documented_forest_renders_as_the_format_defines() {
        let cell = |v: &str| (v != "-").then(|| v.to_string());
        let forest = [
            [
                "n587",
                "n104",
                "lane-b",
                "exited",
                "/work/repo-b/wt-01",
                "-",
            ],
            [
                "n230",
                "n104",
                "-",
                "idle",
                "/work/repo-a/wt-02",
                "2026-01-05T11:58:12Z",
            ],
            [
                "n104",
                "-",
                "lane-a",
                "active",
                "/work/repo-a/wt-01",
                "2026-01-05T11:40:00Z",
            ],
        ];
        let rows = forest
            .iter()
            .map(|r| TableRow {
                id: r[0].into(),
                parents: cell(r[1]).into_iter().collect(),
                cells: r[2..].iter().map(|v| cell(v)).collect(),
            })
            .collect();
        let table = render(
            &["nickname", "state", "cwd", "last_report_at"],
            rows,
            IdOrder::Text,
        );
        assert_eq!(
            table,
            "| id | parents | nickname | state | cwd | last_report_at |\n\
             |---|---|---|---|---|---|\n\
             | n104 | - | lane-a | active | /work/repo-a/wt-01 | 2026-01-05T11:40:00Z |\n\
             | n230 | n104 | - | idle | /work/repo-a/wt-02 | 2026-01-05T11:58:12Z |\n\
             | n587 | n104 | lane-b | exited | /work/repo-b/wt-01 | - |\n"
        );
    }

    #[test]
    fn a_diamond_row_appears_once_where_it_is_first_reached() {
        let rows = vec![
            row("d", &["c", "b"]),
            row("c", &["a"]),
            row("b", &["a"]),
            row("e", &["c"]),
            row("a", &[]),
        ];
        let table = render(&["v"], rows, IdOrder::Text);
        assert_eq!(ids(&table), ["a", "b", "d", "c", "e"]);
        assert!(table.contains("| d | b,c | vd |\n"), "{table}");
    }

    #[test]
    fn a_cycle_and_what_hangs_below_it_are_all_written() {
        let rows = vec![
            row("b", &["a"]),
            row("a", &["b"]),
            row("c", &["b"]),
            row("z", &[]),
        ];
        let table = render(&["v"], rows, IdOrder::Text);
        assert_eq!(ids(&table), ["z", "a", "b", "c"]);
    }

    #[test]
    fn a_self_parent_is_a_cycle_and_keeps_its_row() {
        let table = render(&["v"], vec![row("a", &["a"])], IdOrder::Text);
        assert_eq!(table.lines().nth(2), Some("| a | a | va |"));
    }

    #[test]
    fn a_row_whose_parents_are_outside_the_table_is_a_root() {
        let table = render(
            &["v"],
            vec![row("b", &["a"]), row("a", &["outside"])],
            IdOrder::Text,
        );
        assert_eq!(ids(&table), ["a", "b"]);
        assert!(table.contains("| a | outside | va |\n"), "{table}");
    }

    #[test]
    fn separators_in_values_cannot_change_rows_or_cells() {
        let rows = vec![TableRow {
            id: "a|b".into(),
            parents: vec!["p,q".into(), "x|y".into()],
            cells: vec![
                Some("n|a\nme\r\t\u{1b}[0m\\".into()),
                Some("\u{7}".into()),
                Some(String::new()),
                None,
            ],
        }];
        let table = render(&["w", "x", "y", "z"], rows, IdOrder::Text);
        let line = table.lines().nth(2).unwrap();
        assert_eq!(table.lines().count(), 3, "{table}");
        assert_eq!(line, r"| a\|b | p\,q,x\|y | n\|a\nme\r[0m\\ | - | - | - |",);
    }

    #[test]
    fn a_repeated_parent_is_written_once() {
        let table = render(&["v"], vec![row("c", &["b", "a", "b"])], IdOrder::Text);
        assert!(table.contains("| c | a,b | vc |\n"), "{table}");
    }

    #[test]
    fn numeric_ids_sort_as_numbers_and_text_ids_as_text() {
        let rows = || vec![row("10", &["100", "20"]), row("9", &[]), row("x", &[])];
        let numeric = render(&["v"], rows(), IdOrder::Numeric);
        assert_eq!(ids(&numeric), ["9", "10", "x"]);
        assert!(numeric.contains("| 10 | 20,100 | v10 |\n"), "{numeric}");
        let text = render(&["v"], rows(), IdOrder::Text);
        assert_eq!(ids(&text), ["10", "9", "x"]);
        assert!(text.contains("| 10 | 100,20 | v10 |\n"), "{text}");
    }

    #[test]
    fn no_line_ends_in_a_space_and_no_line_is_empty() {
        let table = render(&["v"], vec![row("a", &[]), row("b", &["a"])], IdOrder::Text);
        assert!(table.ends_with('\n'));
        for line in table.lines() {
            assert!(!line.is_empty() && !line.ends_with(' '), "{line:?}");
        }
    }

    #[test]
    fn an_empty_table_is_the_two_header_lines() {
        assert_eq!(
            render(&["name", "state"], Vec::new(), IdOrder::Text),
            "| id | parents | name | state |\n|---|---|---|---|\n"
        );
    }
}
