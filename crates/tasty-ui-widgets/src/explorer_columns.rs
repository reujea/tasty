//! Explorer 상세 보기의 열 정의. 본체와 갤러리가 같은 열 폭을 쓴다.
//! 하위 폴더 검색 결과에서는 Type 대신 시작 폴더 기준 상대 경로를 보이는 Folder 열을 Name 뒤에 둔다.

use tasty_type_appearance::theme::Theme;
use tasty_type_geometry::length::LogicalPx;

use crate::table::{TableAlign, TableColumn, TableColumnWidth};

/// 디자인 `DetailRow` 의 `gridTemplateColumns: 1fr 80px 132px 92px` 와 각 열의 최소 폭.
/// 이름 열 하한과 크기·날짜·종류 열 폭은 같은 역할의 토큰(`explorer_detail_*` 접근자)이 있다.
/// 접근자는 ui_zoom을 따르므로 바꾸면 열 폭이 배율에 따라 달라진다. 그 전환은 별도 작업이라 리터럴로 둔다.
const NAME_MIN: LogicalPx = LogicalPx(140.0);
const SIZE_W: LogicalPx = LogicalPx(80.0);
const SIZE_MIN: LogicalPx = LogicalPx(64.0);
const DATE_W: LogicalPx = LogicalPx(132.0);
const DATE_MIN: LogicalPx = LogicalPx(108.0);
const TYPE_W: LogicalPx = LogicalPx(92.0);
const TYPE_MIN: LogicalPx = LogicalPx(72.0);

/// 상세 보기 열 제목과 정렬 키.
pub struct ExplorerDetailHeads<'a, K> {
    pub name: (&'a str, K),
    pub size: (&'a str, K),
    pub modified: (&'a str, K),
    pub kind: (&'a str, K),
    /// `Some` 이면 검색 결과 열 구성이다. Type 을 빼고 이 제목의 Folder 열을 Name 뒤에 둔다.
    pub folder: Option<&'a str>,
}

/// 상세 보기 열. 기본은 Name · Size · Date modified · Type, 검색 결과는 Name · Folder · Size · Date modified.
pub fn explorer_detail_columns<'a, K>(
    theme: &Theme,
    heads: ExplorerDetailHeads<'a, K>,
) -> Vec<TableColumn<'a, K>> {
    let mut columns = vec![TableColumn {
        title: heads.name.0,
        width: TableColumnWidth::Remainder {
            at_least: NAME_MIN,
            clip: true,
        },
        align: TableAlign::Left,
        sort_id: Some(heads.name.1),
    }];
    if let Some(folder) = heads.folder {
        columns.push(TableColumn {
            title: folder,
            width: TableColumnWidth::Exact(theme.explorer_search_folder_col_width()),
            align: TableAlign::Left,
            sort_id: None,
        });
    }
    columns.push(TableColumn {
        title: heads.size.0,
        width: TableColumnWidth::Initial {
            initial: SIZE_W,
            at_least: SIZE_MIN,
        },
        align: TableAlign::Right,
        sort_id: Some(heads.size.1),
    });
    columns.push(TableColumn {
        title: heads.modified.0,
        width: TableColumnWidth::Initial {
            initial: DATE_W,
            at_least: DATE_MIN,
        },
        align: TableAlign::Left,
        sort_id: Some(heads.modified.1),
    });
    if heads.folder.is_none() {
        columns.push(TableColumn {
            title: heads.kind.0,
            width: TableColumnWidth::Initial {
                initial: TYPE_W,
                at_least: TYPE_MIN,
            },
            align: TableAlign::Left,
            sort_id: Some(heads.kind.1),
        });
    }
    columns
}

/// 이름 열을 뺀 상세 열 폭의 합. 이름 열 끝을 알아야 하는 편집 줄이 쓴다.
pub fn explorer_detail_tail_width(search: bool, theme: &Theme) -> f32 {
    let tail = SIZE_W.value() + DATE_W.value();
    if search {
        tail + theme.explorer_search_folder_col_width().value()
    } else {
        tail + TYPE_W.value()
    }
}
