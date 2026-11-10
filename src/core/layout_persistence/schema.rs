//! 슬롯 JSON의 저장 형식. Terminal 외 surface는 Generic의 kind와 data로 저장한다.

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::model::{SplitDirection, WorkspaceCategoryId};

#[derive(Serialize, Deserialize)]
pub struct SavedLayout {
    pub version: u32,
    pub workspaces: Vec<SavedWorkspace>,
    pub active_workspace: usize,
    /// 표시 순서의 카테고리 목록. 필드가 없던 파일은 복원 때 기본 normal 분류를 만든다.
    #[serde(default)]
    pub categories: Vec<SavedCategory>,
}

#[derive(Serialize, Deserialize)]
pub struct SavedCategory {
    pub id: WorkspaceCategoryId,
    pub name: String,
    #[serde(default)]
    pub collapsed: bool,
}

#[derive(Serialize, Deserialize)]
pub struct SavedWorkspace {
    pub name: String,
    pub subtitle: String,
    pub description: String,
    pub pane_layout: SavedPaneNode,
    /// 왼쪽부터 깊이 우선으로 센 leaf pane 중 선택된 pane의 인덱스.
    pub focused_pane_index: usize,
    /// 필드가 없는 이전 파일은 기본 normal ID인 0으로 읽는다.
    #[serde(default)]
    pub category: WorkspaceCategoryId,
}

#[derive(Serialize, Deserialize)]
pub enum SavedPaneNode {
    Leaf(SavedPane),
    Split {
        direction: SavedSplitDirection,
        ratio: f32,
        first: Box<SavedPaneNode>,
        second: Box<SavedPaneNode>,
    },
}

#[derive(Serialize, Deserialize)]
pub struct SavedPane {
    pub tabs: Vec<SavedTab>,
    pub active_tab: usize,
}

#[derive(Serialize, Deserialize)]
pub struct SavedTab {
    pub name: String,
    pub explicit_name: Option<String>,
    pub surface: SavedSurfaceLayout,
}

#[derive(Serialize, Deserialize)]
pub enum SavedSurfaceLayout {
    Leaf(SavedSurface),
    Split {
        direction: SavedSplitDirection,
        ratio: f32,
        first: Box<SavedSurfaceLayout>,
        second: Box<SavedSurfaceLayout>,
    },
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub enum SavedSplitDirection {
    Horizontal,
    Vertical,
}

/// Terminal은 host의 PTY·셸·waker가 필요해 별도로 복원한다.
/// Generic 데이터의 내용은 각 등록 종류의 snapshot·restore가 정한다.
#[derive(Serialize)]
pub enum SavedSurface {
    Terminal {
        cwd: Option<String>,
        /// 실행할 복원 명령. 터미널 surface 메타데이터에서 가져온다.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        restore_command: Option<String>,
        /// 별도 scrollback 파일 ID. 저장 설정이 꺼져 있으면 capture에 넣지 않으며 None이면 읽기를 생략한다.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scrollback_ref: Option<String>,
    },
    Generic {
        kind: String,
        data: Value,
    },
}

impl<'de> Deserialize<'de> for SavedSurface {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(d)?;
        let obj = v
            .as_object()
            .ok_or_else(|| de::Error::custom("SavedSurface must be an object"))?;
        if obj.len() != 1 {
            return Err(de::Error::custom(
                "SavedSurface object must have exactly one variant key",
            ));
        }
        let (key, inner) = obj.iter().next().unwrap();
        match key.as_str() {
            "Terminal" => {
                let cwd = inner
                    .get("cwd")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let restore_command = inner
                    .get("restore_command")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let scrollback_ref = inner
                    .get("scrollback_ref")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                Ok(SavedSurface::Terminal {
                    cwd,
                    restore_command,
                    scrollback_ref,
                })
            }
            "Generic" => {
                let kind = inner
                    .get("kind")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| de::Error::custom("Generic missing 'kind'"))?
                    .to_string();
                let data = inner.get("data").cloned().unwrap_or_else(|| json!({}));
                Ok(SavedSurface::Generic { kind, data })
            }
            other => Err(de::Error::unknown_variant(other, &["Terminal", "Generic"])),
        }
    }
}

impl From<SplitDirection> for SavedSplitDirection {
    fn from(d: SplitDirection) -> Self {
        match d {
            SplitDirection::Horizontal => SavedSplitDirection::Horizontal,
            SplitDirection::Vertical => SavedSplitDirection::Vertical,
        }
    }
}

impl From<SavedSplitDirection> for SplitDirection {
    fn from(d: SavedSplitDirection) -> Self {
        match d {
            SavedSplitDirection::Horizontal => SplitDirection::Horizontal,
            SavedSplitDirection::Vertical => SplitDirection::Vertical,
        }
    }
}
