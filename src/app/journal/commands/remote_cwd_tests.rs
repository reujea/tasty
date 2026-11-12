//! 원격 생성 요청의 cwd 상속 원본. 서버 View는 탭 A를, client는 탭 B를 고른 상태에서 서버가 무엇을 잇는지 본다.
use super::create_spec::{CwdSource, Destination, Spec};
use super::inbound::check_inherit_cwd;
use tasty_ipc::stream::{InheritCwd, SplitAxis, StructuralOp};

/// workspace 1: pane 10(탭 101 = A, 탭 102 = B), pane 20(탭 201). workspace 2: pane 30(탭 301).
/// 모든 surface는 서로 다른 실제 폴더를 가진 explorer다.
struct Fixture {
    session: crate::runtime::engine_session::EngineSession,
    roots: std::collections::HashMap<u32, std::path::PathBuf>,
    _dir: tempfile::TempDir,
}

const A: u32 = 101;
const B: u32 = 102;
const OTHER_PANE: u32 = 201;
const OTHER_WORKSPACE: u32 = 301;

fn fixture() -> Fixture {
    use tasty_core::DomainEvent as E;
    let mut events = vec![
        E::CategoryCreated {
            id: 0,
            name: "normal".into(),
            index: 0,
        },
        E::WorkspaceCreated {
            id: 1,
            name: "one".into(),
            category: 0,
            index: 0,
            pane: 10,
        },
        E::PaneSplit {
            target: 10,
            pane: 20,
            split: tasty_core::SplitSpec {
                direction: crate::model::SplitDirection::Vertical,
                ratio: tasty_core::Ratio::from_f32(0.5),
                placement: tasty_core::Placement::After,
            },
        },
        E::WorkspaceCreated {
            id: 2,
            name: "two".into(),
            category: 0,
            index: 1,
            pane: 30,
        },
    ];
    for (id, pane, index) in [
        (A, 10, 0),
        (B, 10, 1),
        (OTHER_PANE, 20, 0),
        (OTHER_WORKSPACE, 30, 0),
    ] {
        events.push(E::TabCreated {
            id,
            pane,
            index,
            name: "Explorer".into(),
            surface: tasty_core::SurfaceSpec {
                id,
                kind: "explorer".into(),
                data: None,
            },
        });
    }
    let (_, mut session) =
        crate::state::tests::test_state_from_model(crate::state::tests::test_model(events));
    let dir = tempfile::tempdir().expect("temp dir");
    let mut roots = std::collections::HashMap::new();
    for id in [A, B, OTHER_PANE, OTHER_WORKSPACE] {
        let root = dir.path().join(format!("s{id}"));
        std::fs::create_dir(&root).expect("root dir");
        session.runtime.surfaces.insert(
            id,
            Box::new(crate::model::ExplorerPanel::new(id, root.clone())),
        );
        roots.insert(id, root);
    }
    Fixture {
        session,
        roots,
        _dir: dir,
    }
}

/// 서버 View는 pane 10에서 탭 A를 고르고 있다.
fn server_view_on_a() -> crate::runtime::journal_product::CompletionView {
    crate::runtime::journal_product::CompletionView {
        mirror_count: 0,
        focused_panes: [(1, 10)].into_iter().collect(),
        selected_tabs: [(10, A)].into_iter().collect(),
        selected_surfaces: [(A, A), (B, B)].into_iter().collect(),
    }
}

fn public(method: &str, params: serde_json::Value) -> crate::ipc::protocol::JsonRpcRequest {
    serde_json::from_value(serde_json::json!({
        "jsonrpc": "2.0", "method": method, "id": null, "params": params,
    }))
    .expect("request")
}

/// 원격 op가 번역되는 공개 요청 셋: 새 탭, pane 분할, surface 분할. 대상은 모두 탭 A가 있는 pane이다.
fn translated() -> [(&'static str, crate::ipc::protocol::JsonRpcRequest); 3] {
    [
        (
            "tab.create",
            public(
                "tab.create",
                serde_json::json!({"pane_id": 10, "type": "terminal"}),
            ),
        ),
        (
            "split pane",
            public(
                "split",
                serde_json::json!({"level": "pane", "target_surface": A, "type": "terminal"}),
            ),
        ),
        (
            "split surface",
            public(
                "split",
                serde_json::json!({"level": "surface", "target_surface": A, "type": "terminal"}),
            ),
        ),
    ]
}

fn cwd_of(
    fixture: &mut Fixture,
    inherit: bool,
    source: CwdSource,
) -> Vec<Option<std::path::PathBuf>> {
    fixture.session.runtime.settings.general.inherit_cwd = inherit;
    let engine = fixture.session.borrow_mut();
    translated()
        .into_iter()
        .map(|(label, request)| {
            Spec::from_public(&request, &engine.as_ref(), &server_view_on_a(), source)
                .unwrap_or_else(|response| panic!("{label}: {response:?}"))
                .cwd
        })
        .collect()
}

/// client가 탭 B를 기준으로 실으면 서버 View가 탭 A를 골라도 B의 폴더를 잇는다. 서버 설정이 꺼져 있어도 같다.
#[test]
fn a_named_source_wins_over_the_server_selection_and_setting() {
    let mut fixture = fixture();
    let b = Some(fixture.roots[&B].clone());
    for inherit in [true, false] {
        assert_eq!(
            cwd_of(&mut fixture, inherit, CwdSource::Surface(B)),
            vec![b.clone(), b.clone(), b.clone()],
            "server inherit_cwd = {inherit}"
        );
    }
}

/// 기준이 없으면 서버 View 선택(새 탭·pane 분할은 탭 A, surface 분할은 대상 A)과 서버 설정을 쓴다.
#[test]
fn without_a_source_the_server_selection_and_setting_decide() {
    let mut fixture = fixture();
    let a = Some(fixture.roots[&A].clone());
    assert_eq!(
        cwd_of(&mut fixture, true, CwdSource::View),
        vec![a.clone(), a.clone(), a]
    );
    assert_eq!(
        cwd_of(&mut fixture, false, CwdSource::View),
        vec![None, None, None]
    );
}

/// "상속 안 함"은 서버 선택도 쓰지 않는다.
#[test]
fn no_inherit_uses_neither_the_source_nor_the_server_selection() {
    let mut fixture = fixture();
    assert_eq!(
        cwd_of(&mut fixture, true, CwdSource::NoInherit),
        vec![None, None, None]
    );
}

/// 명시 cwd는 기준보다 우선한다.
#[test]
fn an_explicit_cwd_wins_over_the_named_source() {
    let mut fixture = fixture();
    let other = fixture.roots[&OTHER_PANE].clone();
    let engine = fixture.session.borrow_mut();
    let request = public(
        "tab.create",
        serde_json::json!({"pane_id": 10, "type": "terminal", "cwd": other}),
    );
    let spec = Spec::from_public(
        &request,
        &engine.as_ref(),
        &server_view_on_a(),
        CwdSource::Surface(B),
    )
    .expect("spec");
    assert!(matches!(spec.destination, Destination::Tab { .. }));
    assert_eq!(spec.cwd, Some(other));
}

fn new_tab(inherit_cwd: Option<InheritCwd>) -> StructuralOp {
    StructuralOp::NewTab {
        anchor_surface_id: A,
        surface_kind: "terminal".into(),
        params: serde_json::json!({}),
        inherit_cwd,
    }
}

fn split_surface(inherit_cwd: Option<InheritCwd>) -> StructuralOp {
    StructuralOp::SplitSurface {
        surface_id: A,
        direction: SplitAxis::Vertical,
        surface_kind: "terminal".into(),
        params: serde_json::json!({}),
        inherit_cwd,
    }
}

/// 같은 pane의 기준, 기준 없음, 상속 안 함은 통과한다. 다른 pane·다른 workspace의 기준과 모르는 값은 거절한다.
#[test]
fn the_server_checks_the_named_source() {
    let fixture = fixture();
    let core = &fixture.session.core_state;
    for build in [new_tab, split_surface] {
        for ok in [
            None,
            Some(InheritCwd::NoInherit),
            Some(InheritCwd::Surface(A)),
            Some(InheritCwd::Surface(B)),
        ] {
            assert_eq!(
                check_inherit_cwd(core, &build(ok.clone()), 10, 1),
                Ok(()),
                "{ok:?}"
            );
        }
        let other_pane =
            check_inherit_cwd(core, &build(Some(InheritCwd::Surface(OTHER_PANE))), 10, 1)
                .expect_err("another pane");
        assert!(other_pane.contains("is not in pane 10"), "{other_pane}");
        let other_workspace = check_inherit_cwd(
            core,
            &build(Some(InheritCwd::Surface(OTHER_WORKSPACE))),
            10,
            1,
        )
        .expect_err("another workspace");
        assert!(
            tasty_utils::target::says_no_live_target(
                &other_workspace,
                "surface",
                u64::from(OTHER_WORKSPACE)
            ),
            "{other_workspace}"
        );
        let missing = check_inherit_cwd(core, &build(Some(InheritCwd::Surface(9999))), 10, 1)
            .expect_err("a surface that does not exist");
        assert!(missing.contains("no live surface 9999"), "{missing}");
        let unknown = check_inherit_cwd(
            core,
            &build(Some(InheritCwd::Unknown(serde_json::json!("all")))),
            10,
            1,
        )
        .expect_err("an unknown value");
        assert!(
            unknown.contains("unsupported inherit_cwd value \"all\""),
            "{unknown}"
        );
    }
}

/// client 에이전트 요청(IPC `tab.create`·`split`)도 client View가 그 pane에서 고른 탭을 원본으로 싣는다.
/// 여기서는 이 인스턴스가 client다. View는 pane 10에서 탭 B를 골랐다.
#[cfg(feature = "gui")]
#[test]
fn an_agent_creation_carries_the_client_view_selection() {
    let mut fixture = fixture();
    let client_view = crate::runtime::journal_product::CompletionView {
        mirror_count: 0,
        focused_panes: [(1, 10)].into_iter().collect(),
        selected_tabs: [(10, B)].into_iter().collect(),
        selected_surfaces: [(A, A), (B, B)].into_iter().collect(),
    };
    for (inherit, tab, surface) in [
        (
            true,
            Some(InheritCwd::Surface(B)),
            Some(InheritCwd::Surface(A)),
        ),
        (
            false,
            Some(InheritCwd::NoInherit),
            Some(InheritCwd::NoInherit),
        ),
    ] {
        fixture.session.runtime.settings.general.inherit_cwd = inherit;
        let engine = fixture.session.borrow_mut();
        let sources: Vec<_> = translated()
            .iter()
            .map(|(label, request)| {
                super::forwarding::local_op(request, &engine.as_ref(), &client_view)
                    .unwrap_or_else(|| panic!("{label}: no op"))
                    .inherit_cwd()
                    .cloned()
            })
            .collect();
        assert_eq!(
            sources,
            vec![tab.clone(), tab, surface],
            "inherit_cwd = {inherit}"
        );
    }
}

/// 원격 holder 의 자식 터미널: 서버 View 가 탭 A 를 골라도 client 가 고른 탭 B 의 폴더에서 시작한다.
/// 기준이 없으면 서버 선택과 서버 설정, "상속 안 함"이면 없음, 명시 cwd 가 있으면 그 값이다.
#[test]
fn a_remote_child_starts_in_the_client_selected_folder() {
    use super::child::launch_cwd;
    let mut fixture = fixture();
    let (a, b) = (fixture.roots[&A].clone(), fixture.roots[&B].clone());
    let other = fixture.roots[&OTHER_PANE].to_string_lossy().into_owned();
    for inherit in [true, false] {
        fixture.session.runtime.settings.general.inherit_cwd = inherit;
        let engine = fixture.session.borrow_mut();
        let engine = engine.as_ref();
        let launch = |explicit: Option<&str>, source| {
            launch_cwd(&engine, &server_view_on_a(), 10, explicit, source)
        };
        assert_eq!(launch(None, CwdSource::Surface(B)), Some(b.clone()));
        assert_eq!(launch(None, CwdSource::NoInherit), None);
        assert_eq!(
            launch(None, CwdSource::View),
            inherit.then(|| a.clone()),
            "server inherit_cwd = {inherit}"
        );
        assert_eq!(
            launch(Some(&other), CwdSource::Surface(B)),
            Some(std::path::PathBuf::from(&other))
        );
    }
}

/// host 예약 인자는 자식을 만들 pane 의 surface 만 받는다. 모르는 값도 거절한다.
#[test]
fn a_remote_child_source_must_be_in_its_pane() {
    use super::child::remote_cwd_source;
    let fixture = fixture();
    let pane = fixture
        .session
        .core_state
        .find_pane_by_id(10)
        .expect("pane 10");
    let id = serde_json::Value::Null;
    let source = |value: serde_json::Value| remote_cwd_source(&value, pane, &id);
    assert_eq!(source(serde_json::json!({})).ok(), Some(CwdSource::View));
    assert_eq!(
        source(serde_json::json!({"inherit_cwd": null})).ok(),
        Some(CwdSource::View)
    );
    assert_eq!(
        source(serde_json::json!({"inherit_cwd": "none"})).ok(),
        Some(CwdSource::NoInherit)
    );
    assert_eq!(
        source(serde_json::json!({"inherit_cwd": {"surface": B}})).ok(),
        Some(CwdSource::Surface(B))
    );
    for refused in [
        serde_json::json!({"inherit_cwd": {"surface": OTHER_PANE}}),
        serde_json::json!({"inherit_cwd": "all"}),
    ] {
        assert!(source(refused.clone()).is_err(), "{refused}");
    }
}
