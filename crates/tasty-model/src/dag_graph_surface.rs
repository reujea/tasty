//! `DagGraphSurface` — task DAG 를 노드/엣지 그래프로 관찰하는 host builtin surface.
//!
//! explorer/image/markdown panel 과 같은 패턴: surface 는 **식별 + 관찰 대상 지정**
//! 만 보유하고, 무거운 view state(레이아웃 캐시, 줌/팬, 선택, 폴링 타이머)는 host 의
//! `DagGraphView`(`src/adapters/ui/surface/dag_graph/view.rs`)에 둔다.
//!
//! 여기 남는 것은 **재시작 후에도 같은 화면이어야 하는 값**뿐이다 — 어떤 DAG 를
//! 보고 있었는지(`dag_id`/`workspace_id`)와 어느 방향으로 그리고 있었는지
//! (`direction`). snapshot/restore 의 JSON 변환은 host 의 `register_dag_graph`
//! (`src/core/surface_registry/builtins.rs`)가 담당해 본 crate 는 GUI/serde 무관을
//! 유지한다.
//!
//! 줌/팬/선택은 담지 않는다 — 재시작 후 그래프 모양이 달라져 있을 수 있어 예전
//! 뷰포트를 복원하면 엉뚱한 빈 곳을 보게 된다. 복원 직후에는 auto-fit 이 돈다.

use std::path::PathBuf;

use super::SurfaceId;
use super::surface_trait::Surface;

/// 레이어가 뻗어나가는 방향. 캔버스 크롬의 방향 토글이 바꾼다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DagDirection {
    /// 레이어가 왼쪽 → 오른쪽. 형제는 세로로 쌓인다.
    ///
    /// 기본값이다. `agent.task_graph --format dot` 이 `rankdir=LR` 을 내보내 CLI
    /// 출력과 멘탈 모델이 일치하고, 노드 카드가 가로로 긴 형태(168×48)라 LR 이
    /// 화면 폭을 아낀다.
    #[default]
    LeftRight,
    /// 레이어가 위 → 아래. 형제는 가로로 늘어선다.
    TopDown,
}

impl DagDirection {
    /// snapshot 직렬화용 안정 식별자.
    pub fn as_str(self) -> &'static str {
        match self {
            DagDirection::LeftRight => "lr",
            DagDirection::TopDown => "td",
        }
    }

    /// 식별자 → 방향. 알 수 없으면 기본값(`LeftRight`).
    // 무한 실패(default fallback) 파서라 `FromStr`(fallible)과 시그니처가 맞지 않고
    // `as_str` 과 대칭을 이루는 의도된 API 이므로 trait 구현 권고를 끈다.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        match s {
            "td" => DagDirection::TopDown,
            _ => DagDirection::LeftRight,
        }
    }

    /// 토글 결과.
    pub fn toggled(self) -> Self {
        match self {
            DagDirection::LeftRight => DagDirection::TopDown,
            DagDirection::TopDown => DagDirection::LeftRight,
        }
    }
}

/// Task DAG 관찰 surface.
pub struct DagGraphSurface {
    pub id: SurfaceId,
    /// 관찰 대상 DAG 의 식별자(`agent.dag_list` 의 `DagSummary::id` — `d:<키>` 또는
    /// `c:<root task id>`).
    ///
    /// `None` 이면 host view 가 workspace 안에서 자동 선택한다(진행 중인 DAG 우선,
    /// 없으면 가장 최근 갱신). 사용자가 헤더 드롭다운으로 고르면 `Some` 이 되어
    /// 그때부터 고정된다 — 폴링이 대상을 바꿔치기하지 않는다.
    pub dag_id: Option<String>,
    /// 관찰 대상 workspace. `None` 이면 이 surface 가 **속한** workspace 를 본다.
    ///
    /// 활성 workspace 가 아니라 소속 workspace 다(원칙 3 — 포커스 독립성). 다른
    /// workspace 를 보려면 `--meta '{"workspace_id":N}'` 로 명시 지정한다.
    pub workspace_id: Option<u32>,
    /// 레이어 진행 방향. 토글 결과가 레이아웃 영속에 실린다.
    pub direction: DagDirection,
}

impl DagGraphSurface {
    pub fn new(id: SurfaceId) -> Self {
        Self {
            id,
            dag_id: None,
            workspace_id: None,
            direction: DagDirection::default(),
        }
    }

    /// 관찰 대상을 지정한 생성자 (IPC/CLI params · snapshot 복원 공용).
    pub fn with_target(
        id: SurfaceId,
        dag_id: Option<String>,
        workspace_id: Option<u32>,
        direction: DagDirection,
    ) -> Self {
        Self {
            id,
            dag_id,
            workspace_id,
            direction,
        }
    }
}

impl Surface for DagGraphSurface {
    crate::impl_surface_any!();

    fn kind(&self) -> &'static str {
        "dag_graph"
    }
    fn type_name(&self) -> &'static str {
        "DAG"
    }
    fn surface_id(&self) -> Option<SurfaceId> {
        Some(self.id)
    }

    fn display_name(&self) -> String {
        // explicit DAG(`d:<사용자 키>`)면 그 키가 읽을 만한 이름이다. derived
        // (`c:<task id>`)는 기계 id 라 탭 제목으로 쓸 값이 아니고, 미지정이면 아직
        // 대상이 없다 — 둘 다 kind 표시명으로 떨어뜨린다. 실제 DAG 이름은 헤더가
        // 보여준다(모델은 task 데이터를 모른다).
        match self.dag_id.as_deref().and_then(|id| id.strip_prefix("d:")) {
            Some(key) if !key.is_empty() => key.to_string(),
            _ => self.type_name().to_string(),
        }
    }

    fn source_cwd(&self) -> Option<PathBuf> {
        // `None` — 이 surface 는 파일이나 디렉토리에 매여 있지 않다. 관찰 대상은
        // workspace 의 task 레코드(memory store)이지 파일시스템 경로가 아니므로,
        // 여기서 새 터미널을 열 때 상속시킬 "그럴듯한 cwd" 가 존재하지 않는다.
        // 없는 경로를 지어내면 그 값이 주소창·경로 복사·attach wire 로 새어나간다
        // (Surface cwd 불변식 — `docs/design/policies/cwd.md#surface-cwd-invariant`).
        None
    }

    fn to_tree_json(&self) -> serde_json::Value {
        let mut obj = serde_json::json!({
            "kind": self.kind(),
            "type": self.type_name(),
            "id": self.id,
            "direction": self.direction.as_str(),
        });
        if let Some(dag) = &self.dag_id {
            obj["dag_id"] = serde_json::json!(dag);
        }
        if let Some(ws) = self.workspace_id {
            obj["workspace_id"] = serde_json::json!(ws);
        }
        obj
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_round_trips_through_str() {
        for d in [DagDirection::LeftRight, DagDirection::TopDown] {
            assert_eq!(DagDirection::from_str(d.as_str()), d);
        }
        // 알 수 없는 값은 기본값으로.
        assert_eq!(DagDirection::from_str("diagonal"), DagDirection::LeftRight);
    }

    #[test]
    fn direction_toggle_is_an_involution() {
        for d in [DagDirection::LeftRight, DagDirection::TopDown] {
            assert_eq!(d.toggled().toggled(), d);
        }
    }

    #[test]
    fn display_name_uses_explicit_dag_key_only() {
        let mut s = DagGraphSurface::new(7);
        assert_eq!(s.display_name(), "DAG");
        s.dag_id = Some("d:build-and-deploy".to_string());
        assert_eq!(s.display_name(), "build-and-deploy");
        // derived id 는 기계 id — 탭 제목으로 노출하지 않는다.
        s.dag_id = Some("c:t-1700000000-000001".to_string());
        assert_eq!(s.display_name(), "DAG");
    }

    #[test]
    fn source_cwd_is_none() {
        assert_eq!(DagGraphSurface::new(1).source_cwd(), None);
    }
}
