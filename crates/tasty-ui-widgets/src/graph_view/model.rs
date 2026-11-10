//! 그래프 뷰가 읽는 데이터 계약. 노드·간선의 의미(작업 상태, child 상태)는 모르고
//! 식별자·연결·종류만 읽는다. 카드 내용과 잉크는 [`super::GraphPainter`]가 해석한다.

/// 노드 하나. 식별자는 한 스냅샷 안에서 유일한 문자열 하나다.
///
/// 머신을 넘는 그래프에서는 같은 surface 번호가 다른 머신에 있을 수 있다. 그런 소스는
/// [`machine_scoped_id`]로 머신을 식별자에 넣어 충돌을 막는다. 한 스냅샷에서
/// 머신 범위 식별자와 범위 없는 식별자를 섞지 않는다.
pub trait GraphNodeData {
    /// 선택·레이아웃 캐시·자동 맞춤이 쓰는 식별자.
    fn node_id(&self) -> &str;

    /// 노드 종류. 기본은 일반 노드다.
    fn node_kind(&self) -> GraphNodeKind {
        GraphNodeKind::Regular
    }
}

/// 간선 하나. `from`·`to`는 노드 목록의 인덱스이고 `from`이 앞 레이어다.
pub trait GraphEdgeData {
    fn ends(&self) -> (usize, usize);

    /// 간선이 이어진 상태. 기본은 이어짐이다.
    fn link(&self) -> EdgeLink {
        EdgeLink::Connected
    }
}

/// 그래프 뷰 하나가 그리는 스냅샷.
pub trait GraphData {
    type Node: GraphNodeData;
    type Edge: GraphEdgeData;

    /// 스냅샷의 신원. 자동 맞춤·레이아웃 캐시 키에 들어간다(DAG는 DAG id).
    fn graph_key(&self) -> &str;
    fn nodes(&self) -> &[Self::Node];
    fn edges(&self) -> &[Self::Edge];

    /// 식별자로 노드 인덱스를 찾는다.
    fn index_of(&self, id: &str) -> Option<usize> {
        self.nodes().iter().position(|n| n.node_id() == id)
    }
}

/// 노드 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GraphNodeKind {
    /// 데이터 소스가 실제로 읽은 노드.
    #[default]
    Regular,
    /// surface가 아닌 바깥 부모 같은 항목. 그 너머는 추적하지 않는다.
    External,
    /// 연결이 끊긴 구간을 대신하는 자리 노드.
    Gap,
}

/// 간선이 이어진 상태.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EdgeLink {
    #[default]
    Connected,
    /// 연결이 끊긴 구간.
    Broken,
}

/// 머신을 구분하는 노드 식별자. `machine`은 그 노드를 가진 Tasty 프로세스의 식별자다.
pub fn machine_scoped_id(machine: &str, local_id: &str) -> String {
    format!("{machine}/{local_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_local_id_on_two_machines_yields_two_node_ids() {
        assert_ne!(machine_scoped_id("a1", "7"), machine_scoped_id("b2", "7"));
        assert_eq!(machine_scoped_id("a1", "7"), machine_scoped_id("a1", "7"));
    }
}
