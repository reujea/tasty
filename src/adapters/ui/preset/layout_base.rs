//! 미리보기 캐시가 **어느 저장소 판에서 지어졌는가** — 저장 직전의 경합 판정과, 보기 모드
//! 미리보기의 새로고침 판정(`refresh_view_cache`)이 함께 쓴다.
//!
//! 편집 화면은 preset 을 저장소에서 한 번 읽어 `DemoLayout` 으로 캐시하고, 구조 편집
//! 자동 저장과 설정 화면 확인은 그 캐시로 레이아웃을 통째로 갈아 쓴다. 그 사이 에이전트가
//! `preset.save` 로 같은 이름을 덮어썼으면 캐시는 낡았고, 그대로 쓰면 에이전트의 쓰기가
//! 말없이 사라진다(원칙 1). 그래서 캐시를 지을 때와 저장할 때 저장소의 **레이아웃 부분**을
//! 이 값으로 떠 두고, 저장 직전에 대조한다. 근거는
//! `docs/adr/0531-the-preset-editor-does-not-overwrite-a-preset-changed-behind-its-cache.md`.
//!
//! 레이아웃 부분만 보는 이유: 저장은 레이아웃만 갈아 쓰고 이름·subtitle 같은 메타는 저장
//! 시점의 저장소 값을 그대로 둔다(`persist_layout`). 메타만 바뀐 쓰기는 덮이지 않으므로
//! 경합이 아니다.

use tasty_presets::{PresetKind, PresetPane, PresetPaneNode, PresetStore, PresetSurfaceLayout};

/// 저장이 갈아 쓰는 자리의 저장소 값. kind 마다 `persist_layout` 이 바꾸는 필드와 같다.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum LayoutBase {
    /// `WorkspacePreset::layout`.
    Workspace(PresetPaneNode),
    /// `TabPreset::tab.layout`.
    Tab(PresetSurfaceLayout),
    /// `PanePreset::pane`.
    Pane(PresetPane),
}

impl LayoutBase {
    /// 저장소의 지금 판. preset 이 없으면 `None`.
    pub(super) fn current(store: &PresetStore, kind: PresetKind, name: &str) -> Option<Self> {
        match kind {
            PresetKind::Workspace => store
                .get_workspace(name)
                .map(|p| Self::Workspace(p.layout.clone())),
            PresetKind::Tab => store.get_tab(name).map(|p| Self::Tab(p.tab.layout.clone())),
            PresetKind::Pane => store.get_pane(name).map(|p| Self::Pane(p.pane.clone())),
        }
    }
}
