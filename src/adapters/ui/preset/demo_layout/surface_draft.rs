//! surface 설정 화면의 **draft** — 선택 leaf 의 파라미터(kind · cwd · startup ·
//! params)를 떼어 내 편집하고, 확인 시 한 번에 트리에 되돌려 쓴다.
//!
//! 트리(`DemoLayout`)는 확인 전까지 바뀌지 않는다. 그래서 취소는 draft 를 버리는
//! 것으로 끝나고, draft 안에서 kind 를 여러 번 바꿔도 원본 params 가 중간 kind 때문에
//! 지워지지 않는다 — 정리 규칙([`DemoLayout::set_kind`])은 확인 시점에 **최종 kind 가
//! 원본과 다를 때만** 한 번 돈다. kind 가 같으면 선언 필드만 덮어써 선언되지 않은
//! params 를 보존한다(편집기의 round-trip 계약).

use crate::core::surface_registry::PresetFieldTarget;

use super::{DemoLayout, KindCatalog, Leaf, PaneNode, Root, SurfNode, remove_param, set_param};

/// 한 leaf 의 편집 가능한 파라미터 사본. id·표시명은 담지 않는다 — id 는 트리가
/// 보존하고, 표시명은 적용 시 catalog 에서 다시 해석한다.
#[derive(Clone, Debug, PartialEq)]
pub struct LeafDraft {
    kind: String,
    cwd: Option<String>,
    startup: Option<String>,
    params: serde_json::Value,
}

impl LeafDraft {
    fn of(leaf: &Leaf) -> Self {
        Self {
            kind: leaf.kind.clone(),
            cwd: leaf.cwd.clone(),
            startup: leaf.startup.clone(),
            params: leaf.params.clone(),
        }
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// target(cwd/startup/params[key])의 현재 문자열 값(부재면 빈 문자열).
    pub fn value(&self, target: &PresetFieldTarget) -> String {
        match target {
            PresetFieldTarget::Cwd => self.cwd.clone().unwrap_or_default(),
            PresetFieldTarget::Startup => self.startup.clone().unwrap_or_default(),
            PresetFieldTarget::Params(k) => self
                .params
                .get(k)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
        }
    }

    /// target 에 값을 쓴다. 빈 문자열은 값 부재(전용 컬럼 None · params 키 삭제)다 —
    /// [`DemoLayout::set_field`] 와 같은 규칙.
    pub fn set_value(&mut self, target: &PresetFieldTarget, value: String) {
        let v = (!value.is_empty()).then_some(value);
        match target {
            PresetFieldTarget::Cwd => self.cwd = v,
            PresetFieldTarget::Startup => self.startup = v,
            PresetFieldTarget::Params(k) => match &v {
                Some(s) => set_param(&mut self.params, k, s),
                None => remove_param(&mut self.params, k),
            },
        }
    }

    /// draft 안에서 kind 를 바꾼다. 값은 지우지 않는다 — 두 kind 가 함께 선언한 키
    /// (예: `cwd`)는 그대로 이어지고, 원래 kind 로 돌아오면 원래 값이 다시 보인다.
    /// 새 kind 가 선언한 필드 중 값이 비어 있고 `default` 가 있는 것은 그 값으로
    /// 채운다 — 확인 시 [`DemoLayout::set_kind`] 가 채울 값을 미리 보여 준다.
    pub fn switch_kind(&mut self, kind: &str, catalog: &KindCatalog) {
        self.kind = kind.to_string();
        for f in catalog.fields(kind) {
            if let Some(def) = &f.default
                && self.value(&f.target).is_empty()
            {
                self.set_value(&f.target, def.clone());
            }
        }
    }

    /// 저장본(`orig`)과 다른가. kind 와 **지금 kind 가 선언한 키**만 비교한다 —
    /// 선언되지 않은 params 는 편집할 수 없으니 차이의 근거가 되지 않는다.
    pub fn is_dirty(&self, orig: &LeafDraft, catalog: &KindCatalog) -> bool {
        self.kind != orig.kind
            || catalog
                .fields(&self.kind)
                .iter()
                .any(|f| self.value(&f.target) != orig.value(&f.target))
    }
}

/// 설정 화면 헤더 breadcrumb 의 좌표. `pane` 은 Workspace scope 에서만, `tab` 은
/// Tab scope 가 아닐 때만 있다. 번호는 모두 1 부터 센다(트리 방문 순서).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeafLocation {
    pub pane: Option<usize>,
    pub tab: Option<String>,
    pub surface: usize,
}

fn find_leaf(node: &SurfNode, id: usize) -> Option<&Leaf> {
    match node {
        SurfNode::Leaf(l) => (l.id == id).then_some(l),
        SurfNode::Split { first, second, .. } => {
            find_leaf(first, id).or_else(|| find_leaf(second, id))
        }
    }
}

/// `node` 안에서 `id` leaf 의 방문 순서(1 부터). 없으면 None.
fn leaf_ordinal(node: &SurfNode, id: usize) -> Option<usize> {
    fn walk(node: &SurfNode, id: usize, k: &mut usize) -> bool {
        match node {
            SurfNode::Leaf(l) => {
                *k += 1;
                l.id == id
            }
            SurfNode::Split { first, second, .. } => walk(first, id, k) || walk(second, id, k),
        }
    }
    let mut k = 0;
    walk(node, id, &mut k).then_some(k)
}

impl DemoLayout {
    /// 각 탭의 surface 루트를 (pane 번호, 탭 이름, 루트) 로 방문 순서대로 모은다.
    /// pane 번호는 Workspace scope 에서만 채운다.
    fn surf_roots(&self) -> Vec<(Option<usize>, Option<&str>, &SurfNode)> {
        fn walk<'a>(
            node: &'a PaneNode,
            workspace: bool,
            pane_no: &mut usize,
            out: &mut Vec<(Option<usize>, Option<&'a str>, &'a SurfNode)>,
        ) {
            match node {
                PaneNode::Leaf(pane) => {
                    *pane_no += 1;
                    for t in &pane.tabs {
                        let no = workspace.then_some(*pane_no);
                        out.push((no, Some(t.name.as_str()), &t.layout));
                    }
                }
                PaneNode::Split { first, second, .. } => {
                    walk(first, workspace, pane_no, out);
                    walk(second, workspace, pane_no, out);
                }
            }
        }
        let mut out = Vec::new();
        match &self.root {
            Root::Panes(n) => {
                let workspace = self.scope == super::Scope::Workspace;
                walk(n, workspace, &mut 0, &mut out);
            }
            Root::TabFrame(s) => out.push((None, None, s)),
        }
        out
    }

    /// `id` leaf 의 파라미터 사본. leaf 가 없으면 None.
    pub fn leaf_draft(&self, id: usize) -> Option<LeafDraft> {
        self.surf_roots()
            .into_iter()
            .find_map(|(_, _, root)| find_leaf(root, id))
            .map(LeafDraft::of)
    }

    /// `id` leaf 의 위치(breadcrumb). leaf 가 없으면 None.
    pub fn leaf_location(&self, id: usize) -> Option<LeafLocation> {
        self.surf_roots().into_iter().find_map(|(pane, tab, root)| {
            leaf_ordinal(root, id).map(|surface| LeafLocation {
                pane,
                tab: tab.map(str::to_string),
                surface,
            })
        })
    }

    /// draft 를 `id` leaf 에 한 번에 적용한다. leaf id · pane id 는 그대로 둔다.
    /// leaf 가 없으면 아무것도 안 하고 `false`.
    ///
    /// - kind 가 같으면: 그 kind 가 선언한 필드만 덮어쓴다(선언되지 않은 params 보존).
    /// - kind 가 다르면: draft 의 값 전체를 옮긴 뒤 [`DemoLayout::set_kind`] 의 정리
    ///   규칙(새 kind 가 안 쓰는 컬럼·params 제거 + default 채움)을 한 번 적용한다.
    pub fn apply_leaf_draft(
        &mut self,
        id: usize,
        draft: &LeafDraft,
        catalog: &KindCatalog,
    ) -> bool {
        let Some(orig) = self.leaf_draft(id) else {
            return false;
        };
        if orig.kind == draft.kind {
            for f in catalog.fields(&draft.kind) {
                self.set_field(id, &f.target, draft.value(&f.target));
            }
        } else {
            super::for_each_surf_root_mut(&mut self.root, &mut |node| {
                if let Some(l) = super::find_leaf_mut(node, id) {
                    l.cwd = draft.cwd.clone();
                    l.startup = draft.startup.clone();
                    l.params = draft.params.clone();
                }
            });
            self.set_kind(id, &draft.kind, catalog);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::ui::preset::demo_layout::EDIT_KINDS;
    use tasty_presets::{
        PanePreset, PresetPane, PresetPaneNode, PresetSplitDirection, PresetSurface,
        PresetSurfaceLayout, PresetTab, TabPreset, WorkspacePreset,
    };

    fn tc() -> KindCatalog {
        KindCatalog::from_pairs(
            EDIT_KINDS
                .iter()
                .map(|k| (k.to_string(), k.to_string()))
                .collect(),
        )
    }

    fn leaf(kind: &str, cwd: Option<&str>, params: serde_json::Value) -> PresetSurfaceLayout {
        PresetSurfaceLayout::Leaf {
            surface: PresetSurface {
                id: None,
                kind: kind.into(),
                cwd: cwd.map(Into::into),
                startup_command: None,
                params,
            },
        }
    }

    fn pane_of(layout: PresetSurfaceLayout) -> PresetPane {
        PresetPane {
            tabs: vec![PresetTab {
                explicit_name: Some("dev".into()),
                layout,
            }],
            active_tab: 0,
        }
    }

    fn pane_preset(layout: PresetSurfaceLayout) -> PanePreset {
        PanePreset {
            name: "p".into(),
            pane: pane_of(layout),
        }
    }

    fn first_leaf_id(dl: &DemoLayout) -> usize {
        match &dl.surf_roots()[0].2 {
            SurfNode::Leaf(l) => l.id,
            _ => panic!("expected a single leaf"),
        }
    }

    fn only_surface(dl: &DemoLayout) -> PresetSurface {
        match dl.rebuild_single_pane().unwrap().tabs[0].layout.clone() {
            PresetSurfaceLayout::Leaf { surface } => surface,
            _ => panic!("expected a single leaf"),
        }
    }

    #[test]
    fn draft_apply_commits_all_fields_and_keeps_ids() {
        let cat = tc();
        let mut dl = DemoLayout::from_pane(
            &pane_preset(leaf("terminal", None, serde_json::Value::Null)),
            &cat,
        );
        let id = first_leaf_id(&dl);
        let mut d = dl.leaf_draft(id).expect("leaf exists");
        d.set_value(&PresetFieldTarget::Cwd, "/tmp/x".into());
        d.set_value(&PresetFieldTarget::Startup, "cargo watch".into());
        assert!(dl.apply_leaf_draft(id, &d, &cat));

        let s = only_surface(&dl);
        assert_eq!(s.cwd.as_deref(), Some("/tmp/x"));
        assert_eq!(s.startup_command.as_deref(), Some("cargo watch"));
        assert_eq!(first_leaf_id(&dl), id, "leaf id survives the apply");
        assert_eq!(
            s.id,
            Some(id as u32),
            "persisted surface id survives the apply"
        );
    }

    #[test]
    fn draft_discard_leaves_tree_untouched() {
        let cat = tc();
        let dl = DemoLayout::from_pane(
            &pane_preset(leaf("terminal", Some("/a"), serde_json::Value::Null)),
            &cat,
        );
        let before = dl.clone();
        let id = first_leaf_id(&dl);
        let mut d = dl.leaf_draft(id).unwrap();
        d.set_value(&PresetFieldTarget::Cwd, "/elsewhere".into());
        d.switch_kind("markdown", &cat);
        // apply 하지 않는다 = 취소.
        drop(d);
        assert_eq!(dl, before);
        assert_eq!(only_surface(&dl).cwd.as_deref(), Some("/a"));
    }

    #[test]
    fn draft_kind_roundtrip_preserves_original_params() {
        let cat = tc();
        let mut dl = DemoLayout::from_pane(
            &pane_preset(leaf(
                "markdown",
                None,
                serde_json::json!({ "file": "/docs/a.md", "legacy_x": 7 }),
            )),
            &cat,
        );
        let id = first_leaf_id(&dl);
        let orig = dl.leaf_draft(id).unwrap();
        let mut d = orig.clone();
        d.switch_kind("terminal", &cat);
        assert!(d.is_dirty(&orig, &cat));
        d.switch_kind("markdown", &cat);
        assert!(!d.is_dirty(&orig, &cat), "back to the saved kind = clean");
        assert_eq!(
            d.value(&PresetFieldTarget::Params("file".into())),
            "/docs/a.md"
        );
        assert!(dl.apply_leaf_draft(id, &d, &cat));

        let s = only_surface(&dl);
        assert_eq!(s.kind, "markdown");
        assert_eq!(
            s.params.get("file").and_then(|v| v.as_str()),
            Some("/docs/a.md")
        );
        assert_eq!(s.params.get("legacy_x").and_then(|v| v.as_i64()), Some(7));
    }

    #[test]
    fn draft_apply_same_kind_keeps_unknown_params() {
        let cat = tc();
        let mut dl = DemoLayout::from_pane(
            &pane_preset(leaf(
                "terminal",
                Some("/a"),
                serde_json::json!({ "x_custom": "keep" }),
            )),
            &cat,
        );
        let id = first_leaf_id(&dl);
        let mut d = dl.leaf_draft(id).unwrap();
        d.set_value(&PresetFieldTarget::Cwd, "/b".into());
        assert!(dl.apply_leaf_draft(id, &d, &cat));

        let s = only_surface(&dl);
        assert_eq!(s.cwd.as_deref(), Some("/b"));
        assert_eq!(
            s.params.get("x_custom").and_then(|v| v.as_str()),
            Some("keep"),
            "same-kind apply must not run the kind-change cleanup"
        );
    }

    #[test]
    fn draft_apply_kind_change_runs_cleanup_once() {
        let cat = tc();
        let mut dl = DemoLayout::from_pane(
            &pane_preset(leaf(
                "markdown",
                Some("/keep"),
                serde_json::json!({ "file": "/a.md", "legacy": 1 }),
            )),
            &cat,
        );
        let id = first_leaf_id(&dl);
        let mut d = dl.leaf_draft(id).unwrap();
        d.switch_kind("terminal", &cat);
        d.set_value(&PresetFieldTarget::Startup, "make".into());
        assert!(dl.apply_leaf_draft(id, &d, &cat));

        let s = only_surface(&dl);
        assert_eq!(s.kind, "terminal");
        assert_eq!(
            s.cwd.as_deref(),
            Some("/keep"),
            "cwd is declared by both kinds"
        );
        assert_eq!(s.startup_command.as_deref(), Some("make"));
        assert!(
            s.params.as_object().is_none_or(|o| o.is_empty()),
            "params terminal does not declare are cleared: {:?}",
            s.params
        );
    }

    #[test]
    fn dirty_ignores_undeclared_params_and_sees_declared_ones() {
        let cat = tc();
        let dl = DemoLayout::from_pane(
            &pane_preset(leaf(
                "terminal",
                Some("/a"),
                serde_json::json!({ "x": "1" }),
            )),
            &cat,
        );
        let orig = dl.leaf_draft(first_leaf_id(&dl)).unwrap();
        let mut d = orig.clone();
        assert!(!d.is_dirty(&orig, &cat));
        set_param(&mut d.params, "x", "2");
        assert!(
            !d.is_dirty(&orig, &cat),
            "undeclared key is not part of the diff"
        );
        d.set_value(&PresetFieldTarget::Cwd, "/b".into());
        assert!(d.is_dirty(&orig, &cat));
        d.set_value(&PresetFieldTarget::Cwd, "/a".into());
        assert!(
            !d.is_dirty(&orig, &cat),
            "typing the saved value back is clean"
        );
    }

    #[test]
    fn apply_to_missing_leaf_is_refused() {
        let cat = tc();
        let mut dl = DemoLayout::from_pane(
            &pane_preset(leaf("terminal", None, serde_json::Value::Null)),
            &cat,
        );
        let id = first_leaf_id(&dl);
        let d = dl.leaf_draft(id).unwrap();
        let before = dl.clone();
        assert!(!dl.apply_leaf_draft(id + 1000, &d, &cat));
        assert_eq!(dl, before);
        assert!(dl.leaf_draft(id + 1000).is_none());
    }

    #[test]
    fn location_counts_panes_only_in_workspace_scope() {
        let cat = tc();
        let two = PresetSurfaceLayout::Split {
            direction: PresetSplitDirection::Vertical,
            ratio: 0.5,
            first: Box::new(leaf("terminal", None, serde_json::Value::Null)),
            second: Box::new(leaf("markdown", None, serde_json::Value::Null)),
        };
        let ws = WorkspacePreset {
            name: "w".into(),
            subtitle: String::new(),
            description: String::new(),
            layout: PresetPaneNode::Split {
                direction: PresetSplitDirection::Vertical,
                ratio: 0.5,
                first: Box::new(PresetPaneNode::Leaf {
                    pane: pane_of(leaf("terminal", None, serde_json::Value::Null)),
                }),
                second: Box::new(PresetPaneNode::Leaf {
                    pane: pane_of(two.clone()),
                }),
            },
        };
        let dl = DemoLayout::from_workspace(&ws, &cat);
        let roots = dl.surf_roots();
        let SurfNode::Split { second, .. } = roots[1].2 else {
            panic!("second pane holds a split");
        };
        let SurfNode::Leaf(l) = second.as_ref() else {
            panic!()
        };
        assert_eq!(
            dl.leaf_location(l.id),
            Some(LeafLocation {
                pane: Some(2),
                tab: Some("dev".into()),
                surface: 2,
            })
        );

        // Pane scope: pane 번호 없이 탭 · surface.
        let dl = DemoLayout::from_pane(&pane_preset(two.clone()), &cat);
        let id = first_leaf_id_of_split(&dl);
        assert_eq!(
            dl.leaf_location(id),
            Some(LeafLocation {
                pane: None,
                tab: Some("dev".into()),
                surface: 1,
            })
        );

        // Tab scope: surface 번호만.
        let dl = DemoLayout::from_tab(
            &TabPreset {
                name: "t".into(),
                tab: PresetTab {
                    explicit_name: None,
                    layout: two,
                },
            },
            &cat,
        );
        let id = first_leaf_id_of_split(&dl);
        assert_eq!(
            dl.leaf_location(id),
            Some(LeafLocation {
                pane: None,
                tab: None,
                surface: 1,
            })
        );
    }

    fn first_leaf_id_of_split(dl: &DemoLayout) -> usize {
        match dl.surf_roots()[0].2 {
            SurfNode::Split { first, .. } => match first.as_ref() {
                SurfNode::Leaf(l) => l.id,
                _ => panic!(),
            },
            _ => panic!("expected a split"),
        }
    }
}
