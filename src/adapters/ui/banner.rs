//! 범위별 콘텐츠 위에 표시하는 배너. 마우스 입력은 받지만 키보드 포커스·드래그 이동은 없다.
//! 큐와 TTL은 BannerManager가 관리하고 내용은 호스트 또는 플러그인이 그린다.
//! 사용자 동작에서 시작한 안내에 사용하며 강제 표시 시험은 디버그 경로로 제한한다.
//! 자세한 동작은 docs/design/systems/banner.md와 docs/design/systems/popup.md을 따른다.

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use tasty_remote::refusal::RowNoticeKind;
use tasty_ui_widgets::{
    AttachRefusalBannerClicks, AttachRefusalBannerView, AttachSizeSyncBannerClicks,
    AttachSizeSyncBannerView, ControlSize, IconButton, IconButtonVariant,
    attach_refusal_banner_content, attach_size_sync_banner_content, banner_is_narrow, banner_shell,
};

use crate::adapters::ui::icons;
use crate::theme::Theme;

use super::layout_context::LayoutContext;

pub use tasty_model::banner_kind::{BannerId, BannerScope};

/// 매니저가 그린 카드 안에 배너 내용을 배치한다. 내용별 아이콘·색은 이 함수가 정한다.
pub type BannerContentFn = fn(&mut egui::Ui, &Theme);

/// 스코프당 큐 최대 대기 개수 (표시 중 1 + 대기 5 = 총 6).
const MAX_QUEUED: usize = 5;

/// 호스트 배너 정의. defs::find로 조회한다.
#[derive(Clone, Copy)]
pub struct BannerDef {
    /// 고유 id = kind. 같은 id 는 한 스코프에 하나만 존재한다.
    pub id: BannerId,
    /// TTL(초). `None` 이면 사용자 닫기까지 유지, `Some` 이면 카운트다운 후 자동 소멸.
    /// 사용자 동작(버튼·⋯ 메뉴 행, ×는 제외)이 없는 배너는 `Some`, 동작이 있는 배너는 `None`이다.
    /// 이 규칙의 예외는 해당 정의 위치의 주석에 이유와 함께 개발자의 명확한 지시가 있었다는
    /// 사실과 지시 날짜를 적는다(`docs/principles/feedback-and-recovery.md`의 배너 절).
    /// 정의로 배너를 열 때는 `BannerState::from_def`를 써서 이 값을 그대로 따른다.
    pub ttl_seconds: Option<u32>,
    /// 셸 내부 콘텐츠 draw 함수.
    pub content_fn: BannerContentFn,
}

/// 생명주기는 호스트가 관리하고 내용만 호스트 함수 또는 플러그인 mesh로 나눈다.
#[derive(Clone, Debug, PartialEq)]
pub enum BannerContentSource {
    /// host 프로세스가 `content_fn` 으로 셸 내부를 그린다.
    Host,
    /// plugin 이 egui-mesh 로 content_rect 를 그린다.
    PluginMesh {
        /// 소유 plugin id — set_context forward / mesh frame lookup 에 필요.
        plugin_id: String,
        /// host 발급 banner 인스턴스 식별자.
        instance_id: u64,
        /// 매니페스트 size_hint.height 또는 기본값으로 정한 카드 높이. 너비는 소속 범위를 따른다.
        height: f32,
    },
    /// 연결하지 않은 자동 attach 매핑의 안내. 닫기 버튼을 내용 안에 직접 둔다.
    AttachRefusal {
        anchor: u32,
        kind: RowNoticeKind,
        target: String,
    },
    /// mirror 터미널 크기 동기화 실패. 다시 시도·닫기 버튼을 내용 안에 직접 둔다.
    AttachSizeSync {
        /// mirror 워크스페이스 ID.
        anchor: u32,
        /// 실패한 surface의 탭 제목.
        names: Vec<String>,
        retrying: bool,
    },
}

/// 호스트는 정적 ID, 플러그인은 인스턴스 ID로 구별한다. 큐·중복 검사·카드 좌표가 같은 키를 쓴다.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum BannerKey {
    Host(BannerId),
    Plugin(u64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct BannerState {
    /// 호스트 종류 ID. 플러그인은 빈 문자열이며 content의 인스턴스 ID를 쓴다.
    pub id: BannerId,
    pub scope: BannerScope,
    /// 총 TTL(ms). `None` = TTL 없음(사용자 닫기까지 유지).
    pub ttl_ms: Option<f32>,
    /// 남은 시간(ms). `ttl_ms` 가 `None` 이면 의미 없음.
    pub remaining_ms: f32,
    pub content: BannerContentSource,
    /// 배너를 만든 전경 프로세스의 세대. 현재 mouse-capture에서만 기록한다.
    /// None인 배너는 전경 프로세스가 바뀌어도 자동으로 닫지 않는다.
    pub origin_generation: Option<u64>,
}

impl BannerState {
    /// TTL 없는 배너 — 사용자가 닫을 때까지 유지.
    pub fn persistent(id: BannerId, scope: BannerScope) -> Self {
        Self {
            id,
            scope,
            ttl_ms: None,
            remaining_ms: 0.0,
            content: BannerContentSource::Host,
            origin_generation: None,
        }
    }

    pub fn with_origin_generation(mut self, generation: u64) -> Self {
        self.origin_generation = Some(generation);
        self
    }

    /// 정의의 TTL을 따라 `with_ttl` 또는 `persistent`로 만든다.
    pub fn from_def(def: &BannerDef, scope: BannerScope) -> Self {
        match def.ttl_seconds {
            Some(seconds) => Self::with_ttl(def.id, scope, seconds),
            None => Self::persistent(def.id, scope),
        }
    }

    /// TTL(초) 배너 — 카운트다운 0 에서 자동 소멸.
    pub fn with_ttl(id: BannerId, scope: BannerScope, seconds: u32) -> Self {
        let ms = seconds as f32 * 1000.0;
        Self {
            id,
            scope,
            ttl_ms: Some(ms),
            remaining_ms: ms,
            content: BannerContentSource::Host,
            origin_generation: None,
        }
    }

    /// 연결하지 않은 자동 attach 매핑의 Workspace 배너. 사용자가 닫을 때까지 유지한다.
    pub fn attach_refusal(
        scope: BannerScope,
        anchor: u32,
        kind: RowNoticeKind,
        target: String,
    ) -> Self {
        Self {
            content: BannerContentSource::AttachRefusal {
                anchor,
                kind,
                target,
            },
            ..Self::persistent(defs::BANNER_ATTACH_REFUSAL, scope)
        }
    }

    /// mirror 터미널 크기 동기화 실패의 Workspace 배너. 사용자가 닫거나 실패가 풀릴 때까지 유지한다.
    pub fn attach_size_sync(
        scope: BannerScope,
        anchor: u32,
        names: Vec<String>,
        retrying: bool,
    ) -> Self {
        Self {
            content: BannerContentSource::AttachSizeSync {
                anchor,
                names,
                retrying,
            },
            ..Self::persistent(defs::BANNER_ATTACH_SIZE_SYNC, scope)
        }
    }

    /// 플러그인 mesh 배너. TTL이 없으면 만료되지 않으며 height는 논리 좌표의 카드 높이다.
    pub fn plugin_mesh(
        scope: BannerScope,
        plugin_id: String,
        instance_id: u64,
        ttl_seconds: Option<u32>,
        height: f32,
    ) -> Self {
        let ttl_ms = ttl_seconds.map(|s| s as f32 * 1000.0);
        Self {
            id: "",
            scope,
            ttl_ms,
            remaining_ms: ttl_ms.unwrap_or(0.0),
            content: BannerContentSource::PluginMesh {
                plugin_id,
                instance_id,
                height,
            },
            origin_generation: None,
        }
    }

    pub fn key(&self) -> BannerKey {
        match &self.content {
            BannerContentSource::Host
            | BannerContentSource::AttachRefusal { .. }
            | BannerContentSource::AttachSizeSync { .. } => BannerKey::Host(self.id),
            BannerContentSource::PluginMesh { instance_id, .. } => BannerKey::Plugin(*instance_id),
        }
    }

    /// 우상단에 표시할 남은 초 (올림). TTL 없으면 `None`.
    pub fn remaining_seconds(&self) -> Option<u32> {
        self.ttl_ms
            .map(|_| (self.remaining_ms / 1000.0).ceil().max(0.0) as u32)
    }

    fn reset_countdown(&mut self) {
        if let Some(total) = self.ttl_ms {
            self.remaining_ms = total;
        }
    }
}

/// 배너 추가 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BannerPushOutcome {
    /// 빈 스코프라 즉시 표시됨.
    Shown,
    /// 표시 중인 배너가 있어 큐에 대기됨.
    Queued,
    /// 동일 id 가 표시 중 → 카운트다운만 리셋됨 (새로 안 생김).
    ResetCountdown,
    /// 무시됨 (큐의 동일 id 중복 / 큐 가득 참 / TTL 없는 표시 중 동일 id).
    Ignored,
    /// 동일 id 가 표시 중이지만 `origin_generation` 이 달라(다른 foreground 인스턴스가
    /// 유발) 재사용하지 않고 새 배너로 교체됨.
    Replaced,
}

/// 호스트에서 닫힌 플러그인 배너의 사유. banner.closed로 전달한다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginBannerCloseKind {
    /// TTL 카운트다운 만료.
    Ttl,
    /// 사용자가 셸 우상단 close X 를 눌렀음.
    UserClose,
}

/// plugin mesh를 합성할 카드 내부의 논리 좌표 영역.
#[derive(Clone)]
pub struct PluginBannerMeshSlot {
    pub plugin_id: String,
    pub instance_id: u64,
    pub content_rect: egui::Rect,
    /// 배너 카드 전체 영역. 잘린 plugin 본문의 툴팁을 카드 아래에 둘 때 쓴다.
    pub card_rect: egui::Rect,
}

#[derive(Default, Clone)]
struct ScopeLane {
    shown: Option<BannerState>,
    queue: VecDeque<BannerState>,
}

impl ScopeLane {
    fn is_empty(&self) -> bool {
        self.shown.is_none() && self.queue.is_empty()
    }
}

#[derive(Default)]
pub struct BannerManager {
    scopes: HashMap<BannerScope, ScopeLane>,
    /// 마지막 `draw` 시각 — TTL 의 실시간 dt 계산용. 테스트는 `advance` 직접 호출.
    last_tick: Option<Instant>,
    /// 직전 프레임에 그린 카드 영역. 범위 전체를 소비하면 터미널 본문 입력까지 막으므로
    /// 카드만 hover 판정에 쓴다. 첫 프레임에는 아직 좌표가 없다.
    card_rects: HashMap<(BannerScope, BannerKey), egui::Rect>,
    /// 이번 프레임의 플러그인 합성 영역. draw 직후 합성기가 가져간다.
    plugin_mesh_slots: Vec<PluginBannerMeshSlot>,
    /// TTL이나 사용자 닫기로 종료된 플러그인 배너. 합성기가 매니저에 전달한다.
    closed_plugin_banners: Vec<(u64, PluginBannerCloseKind)>,
}

impl BannerManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// 같은 범위의 중복은 무시하되 표시 중 TTL 배너는 시간을 초기화한다.
    /// 빈 범위는 바로 표시하고 그 외에는 최대 5개까지 대기한다.
    pub fn push(&mut self, banner: BannerState) -> BannerPushOutcome {
        let lane = self.scopes.entry(banner.scope.clone()).or_default();

        if let Some(shown) = lane.shown.as_mut()
            && shown.key() == banner.key()
        {
            // 전경 프로세스가 바뀌었으면 같은 종류도 새 배너로 교체한다. None끼리는 재사용한다.
            if shown.origin_generation != banner.origin_generation {
                *shown = banner;
                return BannerPushOutcome::Replaced;
            }
            return if shown.ttl_ms.is_some() {
                shown.reset_countdown();
                BannerPushOutcome::ResetCountdown
            } else {
                BannerPushOutcome::Ignored
            };
        }

        if lane.queue.iter().any(|b| b.key() == banner.key()) {
            return BannerPushOutcome::Ignored;
        }

        if lane.shown.is_none() {
            lane.shown = Some(banner);
            BannerPushOutcome::Shown
        } else if lane.queue.len() < MAX_QUEUED {
            lane.queue.push_back(banner);
            BannerPushOutcome::Queued
        } else {
            BannerPushOutcome::Ignored
        }
    }

    /// 표시 중인 배너를 닫고 다음 항목을 표시한다. 반환된 플러그인 배너의 종료 통지는 호출자가 맡는다.
    pub fn close_shown(&mut self, scope: &BannerScope) -> Option<BannerState> {
        let lane = self.scopes.get_mut(scope)?;
        let removed = lane.shown.take()?;
        lane.shown = lane.queue.pop_front();
        if lane.is_empty() {
            self.scopes.remove(scope);
        }
        Some(removed)
    }

    /// 자동 닫기가 다른 배너를 닫지 않도록 종류가 일치할 때만 제거한다.
    pub fn close_shown_if_id(&mut self, scope: &BannerScope, id: BannerId) -> Option<BannerState> {
        let lane = self.scopes.get(scope)?;
        if lane.shown.as_ref()?.id != id {
            return None;
        }
        self.close_shown(scope)
    }

    /// 플러그인 인스턴스를 표시 또는 대기열에서 제거한다. 매니저 통지는 호출자가 처리한다.
    pub fn close_by_instance(&mut self, instance_id: u64) -> bool {
        let target = BannerKey::Plugin(instance_id);
        let mut target_scope: Option<BannerScope> = None;
        for (scope, lane) in self.scopes.iter_mut() {
            if lane.shown.as_ref().is_some_and(|b| b.key() == target) {
                target_scope = Some(scope.clone());
                break;
            }
            if let Some(pos) = lane.queue.iter().position(|b| b.key() == target) {
                lane.queue.remove(pos);
                return true;
            }
        }
        if let Some(scope) = target_scope {
            return self.close_shown(&scope).is_some();
        }
        false
    }

    pub fn plugin_instances(&self) -> impl Iterator<Item = u64> + '_ {
        self.scopes.values().flat_map(|lane| {
            lane.shown
                .iter()
                .chain(lane.queue.iter())
                .filter_map(|b| match &b.content {
                    BannerContentSource::PluginMesh { instance_id, .. } => Some(*instance_id),
                    BannerContentSource::Host
                    | BannerContentSource::AttachRefusal { .. }
                    | BannerContentSource::AttachSizeSync { .. } => None,
                })
        })
    }

    pub fn take_plugin_mesh_slots(&mut self) -> Vec<PluginBannerMeshSlot> {
        std::mem::take(&mut self.plugin_mesh_slots)
    }

    pub fn drain_closed_plugin_banners(&mut self) -> Vec<(u64, PluginBannerCloseKind)> {
        std::mem::take(&mut self.closed_plugin_banners)
    }

    pub fn close_by_id(&mut self, id: BannerId) -> bool {
        let mut target_scope: Option<BannerScope> = None;
        for (scope, lane) in self.scopes.iter_mut() {
            if lane.shown.as_ref().is_some_and(|b| b.id == id) {
                target_scope = Some(scope.clone());
                break;
            }
            if let Some(pos) = lane.queue.iter().position(|b| b.id == id) {
                lane.queue.remove(pos);
                return true;
            }
        }
        if let Some(scope) = target_scope {
            return self.close_shown(&scope).is_some();
        }
        false
    }

    /// 연결하지 않은 매핑 배너를 `desired` 하나로 맞춘다. 같으면 그대로 두고, 다르면 모두 닫고 새로 넣는다.
    pub fn sync_attach_refusal(&mut self, desired: Option<BannerState>) {
        self.sync_single(defs::BANNER_ATTACH_REFUSAL, desired);
    }

    /// 크기 동기화 실패 배너를 `desired` 하나로 맞춘다. 규칙은 매핑 배너와 같다.
    pub fn sync_attach_size_sync(&mut self, desired: Option<BannerState>) {
        self.sync_single(defs::BANNER_ATTACH_SIZE_SYNC, desired);
    }

    /// 종류 `id`의 배너를 `desired` 하나로 맞춘다. 같으면 그대로 두고, 다르면 모두 닫고 새로 넣는다.
    fn sync_single(&mut self, id: BannerId, desired: Option<BannerState>) {
        let current: Vec<&BannerState> = self
            .scopes
            .values()
            .flat_map(|lane| lane.shown.iter().chain(lane.queue.iter()))
            .filter(|b| b.id == id)
            .collect();
        if current.len() == usize::from(desired.is_some())
            && current.first().copied() == desired.as_ref()
        {
            return;
        }
        while self.close_by_id(id) {}
        if let Some(banner) = desired {
            self.push(banner);
        }
    }

    /// hover·비가시 상태에서는 TTL을 멈춘다. 만료 시 다음 배너를 표시하고 플러그인 종료를 기록한다.
    /// 시험에서는 경과 시간을 직접 전달할 수 있다.
    pub fn advance(&mut self, dt_ms: f32, is_paused: impl Fn(&BannerScope, &BannerKey) -> bool) {
        let mut expired: Vec<BannerScope> = Vec::new();
        for (scope, lane) in self.scopes.iter_mut() {
            if let Some(shown) = lane.shown.as_mut()
                && shown.ttl_ms.is_some()
                && !is_paused(scope, &shown.key())
            {
                shown.remaining_ms -= dt_ms;
                if shown.remaining_ms <= 0.0 {
                    expired.push(scope.clone());
                }
            }
        }
        for scope in expired {
            if let Some(removed) = self.close_shown(&scope)
                && let BannerContentSource::PluginMesh { instance_id, .. } = removed.content
            {
                self.closed_plugin_banners
                    .push((instance_id, PluginBannerCloseKind::Ttl));
            }
        }
    }

    /// 표시 중 배너의 카운트다운을 강제 설정한다 (debug). TTL 배너에만 적용.
    // 이유: 호출부가 debug.rs(`#![cfg(debug_assertions)]`)와 `#[cfg(test)]` 뿐이라
    // release+non-test 빌드에서 미사용으로 잡힌다.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub fn set_countdown(&mut self, scope: &BannerScope, seconds: u32) -> bool {
        if let Some(lane) = self.scopes.get_mut(scope)
            && let Some(shown) = lane.shown.as_mut()
            && shown.ttl_ms.is_some()
        {
            let ms = seconds as f32 * 1000.0;
            shown.ttl_ms = Some(ms);
            shown.remaining_ms = ms;
            return true;
        }
        false
    }

    /// 현재 표시 중인 배너들 (스코프당 0~1). draw·debug 가 순회.
    pub fn shown_banners(&self) -> impl Iterator<Item = &BannerState> {
        self.scopes.values().filter_map(|lane| lane.shown.as_ref())
    }

    /// 스코프의 대기 큐 (표시 중 제외).
    // 이유: 호출부가 debug.rs(`#![cfg(debug_assertions)]`)뿐이라 release 빌드에서
    // 미사용으로 잡힌다.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub fn queued_banners(&self, scope: &BannerScope) -> impl Iterator<Item = &BannerState> {
        self.scopes
            .get(scope)
            .into_iter()
            .flat_map(|lane| lane.queue.iter())
    }

    /// 전체 스코프 대기 큐 합 (debug 요약).
    // 이유: 호출부가 debug.rs(`#![cfg(debug_assertions)]`)와 `#[cfg(test)]` 뿐이라
    // release+non-test 빌드에서 미사용으로 잡힌다.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub fn total_queued(&self) -> usize {
        self.scopes.values().map(|lane| lane.queue.len()).sum()
    }

    /// 직전 프레임의 egui 논리 좌표. 아직 그리지 않은 배너는 None이다.
    // 이유: 호출부가 debug.rs(`#[cfg(debug_assertions)]`)뿐이라 release 빌드에서
    // 미사용으로 잡힌다. `total_queued` 와 같은 사유.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub fn card_rect(&self, scope: &BannerScope, key: &BannerKey) -> Option<egui::Rect> {
        self.card_rects.get(&(scope.clone(), key.clone())).copied()
    }

    /// 직전에 그린 창 범위(View·Workspace) 배너 카드의 사각형. pane·tab·surface 범위는 뺀다.
    pub fn window_scope_rects(&self) -> Vec<egui::Rect> {
        self.card_rects
            .iter()
            .filter(|((scope, _), _)| {
                matches!(scope, BannerScope::View | BannerScope::Workspace(_))
            })
            .map(|(_, rect)| *rect)
            .collect()
    }

    pub fn has_any(&self) -> bool {
        !self.scopes.is_empty()
    }

    /// 더 높은 범위의 배너가 있으면 이 배너를 어둡게 표시한다.
    fn is_recessed(&self, scope: &BannerScope, visible_max_priority: u8) -> bool {
        scope.priority() < visible_max_priority
    }
}

/// hovered이면 아래 터미널·구분선에 마우스 입력을 전달하지 않는다.
#[derive(Clone)]
pub struct BannerDrawResult {
    pub hovered: bool,
    /// 더보기 클릭의 범위와 버튼 좌표. 호출자가 context menu를 여는 데 쓴다.
    pub more_clicked: Option<(BannerScope, egui::Rect)>,
    /// 빈 경우에도 생성하는 배너 Area. 중앙 레이어 정렬에서 탭바·상태바의 기준으로 쓴다.
    pub layer: egui::LayerId,
    /// 연결하지 않은 매핑 배너의 anchor와 이번 프레임의 클릭.
    pub attach_refusal: Option<(u32, AttachRefusalBannerClicks)>,
    /// 크기 동기화 실패 배너의 mirror 워크스페이스와 이번 프레임의 클릭.
    pub attach_size_sync: Option<(u32, AttachSizeSyncBannerClicks)>,
}

impl BannerManager {
    /// TTL과 표시·닫기를 처리한다. view_placeholder가 없으면 View 배너는 표시하지 않는다.
    /// more_menu_open_for의 버튼은 hover와 관계없이 강조한다.
    /// 범위 우선순위대로 그려 높은 배너가 앞에 오게 하고 나머지는 어둡게 한다.
    pub fn draw(
        &mut self,
        ctx: &egui::Context,
        draw_ctx: &LayoutContext,
        theme: &Theme,
        view_placeholder: Option<egui::Rect>,
        _reduced_motion: bool,
        more_menu_open_for: Option<&BannerScope>,
    ) -> BannerDrawResult {
        // 경과 시간만 먼저 구하고 TTL 갱신은 hover 판정 뒤에 한다.
        let now = Instant::now();
        let dt_ms = self
            .last_tick
            .map(|prev| now.duration_since(prev).as_secs_f32() * 1000.0)
            .unwrap_or(0.0)
            .min(1000.0); // 프레임 폭주/디버그 정지 후 점프 방지(최대 1s)
        self.last_tick = Some(now);

        self.plugin_mesh_slots.clear();

        struct Slot {
            scope: BannerScope,
            key: BannerKey,
            /// host 배너 content_fn 조회용 id (plugin 배너면 `""`).
            id: BannerId,
            /// plugin egui-mesh 배너면 (plugin_id, instance_id, 셸 높이). host 면 None.
            mesh: Option<(String, u64, f32)>,
            /// 연결하지 않은 매핑 배너면 (anchor, 이유, 대상).
            refusal: Option<(u32, RowNoticeKind, String)>,
            /// 크기 동기화 실패 배너면 (mirror 워크스페이스, 이름, 다시 시도 중).
            size_sync: Option<(u32, Vec<String>, bool)>,
            zone: egui::Rect,
            /// 스코프 폭이 `banner_narrow_below`보다 좁아 버튼을 글 아래 줄로 내리는지.
            narrow: bool,
            remaining_seconds: Option<u32>,
        }
        let mut slots: Vec<Slot> = Vec::new();
        for banner in self.shown_banners() {
            let Some((zone, scope_width)) =
                Self::banner_zone(&banner.scope, draw_ctx, view_placeholder, theme)
            else {
                continue; // 백그라운드(스코프 비가시) — draw 안 함, TTL 정지(아래).
            };
            let mesh = match &banner.content {
                BannerContentSource::PluginMesh {
                    plugin_id,
                    instance_id,
                    height,
                } => Some((plugin_id.clone(), *instance_id, *height)),
                BannerContentSource::Host
                | BannerContentSource::AttachRefusal { .. }
                | BannerContentSource::AttachSizeSync { .. } => None,
            };
            let refusal = match &banner.content {
                BannerContentSource::AttachRefusal {
                    anchor,
                    kind,
                    target,
                } => Some((*anchor, *kind, target.clone())),
                _ => None,
            };
            let size_sync = match &banner.content {
                BannerContentSource::AttachSizeSync {
                    anchor,
                    names,
                    retrying,
                } => Some((*anchor, names.clone(), *retrying)),
                _ => None,
            };
            slots.push(Slot {
                scope: banner.scope.clone(),
                key: banner.key(),
                id: banner.id,
                mesh,
                refusal,
                size_sync,
                zone,
                narrow: banner_is_narrow(scope_width, theme),
                remaining_seconds: banner.remaining_seconds(),
            });
        }

        let visible_max_priority = slots.iter().map(|s| s.scope.priority()).max().unwrap_or(0);

        slots.sort_by_key(|s| s.scope.priority());

        let pointer = ctx.pointer_hover_pos();
        let mut hovered_any = false;
        let mut hovered_ids: Vec<(BannerScope, BannerKey)> = Vec::new();
        let mut close_requests: Vec<BannerScope> = Vec::new();
        let mut more_clicked: Option<(BannerScope, egui::Rect)> = None;
        let mut attach_refusal: Option<(u32, AttachRefusalBannerClicks)> = None;
        let mut attach_size_sync: Option<(u32, AttachSizeSyncBannerClicks)> = None;
        let mut mesh_drawn: Vec<PluginBannerMeshSlot> = Vec::new();
        // 이번 좌표로 교체해 사라진 배너를 제외한다. hover에는 직전 프레임 값을 쓴다.
        let mut next_card_rects: HashMap<(BannerScope, BannerKey), egui::Rect> = HashMap::new();

        // 같은 Foreground의 팝업과 순서를 맞추려면 Area에 등록해야 한다.
        // 여러 범위의 배너를 한 Area에 넣고 내부에서 낮은 범위부터 그린다.
        // 위치는 각 child Ui로 지정하고 드래그·클릭에 의한 순서 변경은 막는다.
        egui::Area::new(egui::Id::new("banner_layer"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::Pos2::ZERO)
            .movable(false)
            .interactable(true)
            .sense(egui::Sense::hover())
            .constrain(false)
            .show(ctx, |area_ui| {
                for slot in &slots {
                    let recessed = self.is_recessed(&slot.scope, visible_max_priority);
                    let opacity = if recessed {
                        theme.opacity_recessed()
                    } else {
                        1.0
                    };
                    // 범위 전체가 아닌 실제 카드만 입력을 소비한다. 첫 프레임에는 hover=false다.
                    let card_key = (slot.scope.clone(), slot.key.clone());
                    let banner_hovered = pointer.is_some_and(|p| {
                        self.card_rects
                            .get(&card_key)
                            .is_some_and(|r| r.contains(p))
                    });
                    if banner_hovered {
                        hovered_any = true;
                        hovered_ids.push(card_key.clone());
                    }

                    let mut mesh_content_rect: Option<egui::Rect> = None;
                    let mut child = ui_at(area_ui, slot.zone);
                    let card_rect = banner_shell(&mut child, theme, opacity, |ui| {
                        if let Some((_, _, height)) = &slot.mesh {
                            // 우측 제어 버튼 영역을 빼고 플러그인 mesh의 합성 영역을 확보한다.
                            let aff = theme.item_height_interactive.value();
                            let w = (ui.available_width() - aff).max(1.0);
                            let (rect, _) = ui
                                .allocate_exact_size(egui::vec2(w, *height), egui::Sense::hover());
                            mesh_content_rect = Some(rect);
                        } else if let Some((anchor, kind, target)) = &slot.refusal {
                            // 닫기 버튼이 내용 안에 있어 모서리 닫기 슬롯을 쓰지 않는다.
                            let (before, after) = super::attach_notice::title_parts();
                            let body = super::attach_notice::banner_body(*kind);
                            let clicks = attach_refusal_banner_content(
                                ui,
                                theme,
                                &AttachRefusalBannerView {
                                    title_before: before,
                                    target,
                                    title_after: after,
                                    body: &body,
                                    remove: crate::i18n::t("remote.refusal.remove"),
                                    dismiss: crate::i18n::t("remote.refusal.dismiss"),
                                    narrow: slot.narrow,
                                },
                            );
                            if clicks.dismiss {
                                close_requests.push(slot.scope.clone());
                            }
                            if clicks.remove || clicks.dismiss {
                                attach_refusal = Some((*anchor, clicks));
                            }
                            return;
                        } else if let Some((anchor, names, retrying)) = &slot.size_sync {
                            let names: Vec<&str> = names.iter().map(String::as_str).collect();
                            let clicks = attach_size_sync_banner_content(
                                ui,
                                theme,
                                &AttachSizeSyncBannerView {
                                    title: crate::i18n::t("remote.size_sync.title"),
                                    names_line: crate::i18n::t("remote.size_sync.names"),
                                    hint: crate::i18n::t("remote.size_sync.hint"),
                                    many: crate::i18n::t("remote.size_sync.many"),
                                    names: &names,
                                    retry: crate::i18n::t("remote.size_sync.retry"),
                                    retry_all: crate::i18n::t("remote.size_sync.retry_all"),
                                    dismiss: crate::i18n::t("remote.size_sync.dismiss"),
                                    retrying: *retrying,
                                    narrow: slot.narrow,
                                },
                            );
                            if clicks.dismiss {
                                close_requests.push(slot.scope.clone());
                            }
                            if clicks.retry || clicks.dismiss {
                                attach_size_sync = Some((*anchor, clicks));
                            }
                            return;
                        } else if let Some(def) = defs::find(slot.id) {
                            (def.content_fn)(ui, theme);
                        } else {
                            ui.label(slot.id);
                        }
                        // mouse-capture는 더보기·닫기 두 버튼의 폭을 항상 비워 hover 때 본문이 흔들리지 않게 한다.
                        let show_more = slot.id == defs::BANNER_MOUSE_CAPTURE;
                        let more_active = more_menu_open_for == Some(&slot.scope);
                        let avail = ui.max_rect();
                        let corner = if show_more {
                            // 시안의 ⋯/× 열은 본문 흐름 밖에 고정된다. 셸 안쪽 위 `banner-padding-y`,
                            // 오른쪽 `space-sm` 자리이며, 본문은 `banner-more-reserve`만큼 비워 둔다.
                            let right = avail.right() + theme.banner_padding_x().value()
                                - theme.spacing_sm.value();
                            egui::Rect::from_min_max(
                                egui::pos2(
                                    right - theme.banner_more_reserve().value(),
                                    avail.top(),
                                ),
                                egui::pos2(right, avail.top() + ControlSize::Sm.height(theme)),
                            )
                        } else {
                            let reserve = theme.item_height_interactive.value();
                            egui::Rect::from_min_max(
                                egui::pos2(avail.right() - reserve, avail.top()),
                                egui::pos2(
                                    avail.right(),
                                    avail.top() + theme.item_height_interactive.value(),
                                ),
                            )
                        };
                        let mut corner_ui = ui.new_child(egui::UiBuilder::new().max_rect(corner));
                        corner_ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.spacing_mut().item_spacing.x =
                                    theme.banner_more_column_gap().value();
                                if banner_hovered {
                                    // 폰트에 없는 문자 대신 공용 SVG 닫기 버튼을 사용한다.
                                    if IconButton::new()
                                        .variant(IconButtonVariant::Ghost)
                                        .size(ControlSize::Sm)
                                        .show(ui, theme, &|ui, rect, c| {
                                            icons::CLOSE.image(rect.height(), c).paint_at(ui, rect)
                                        })
                                        .clicked()
                                    {
                                        close_requests.push(slot.scope.clone());
                                    }
                                } else if let Some(secs) = slot.remaining_seconds {
                                    ui.label(
                                        egui::RichText::new(secs.to_string())
                                            .monospace()
                                            .size(theme.font_size_micro.value())
                                            .color(theme.banner_countdown_fg().to_egui()),
                                    );
                                }
                                // hover 중이거나 메뉴가 열렸을 때 더보기를 표시한다.
                                if show_more && (banner_hovered || more_active) {
                                    let resp = IconButton::new()
                                        .variant(IconButtonVariant::Ghost)
                                        .size(ControlSize::Sm)
                                        .active(more_active)
                                        .show(ui, theme, &|ui, rect, c| {
                                            icons::MORE.image(rect.height(), c).paint_at(ui, rect)
                                        })
                                        .on_hover_text(crate::i18n::t(
                                            "banner.mouse_capture.more_button",
                                        ));
                                    if resp.clicked() {
                                        more_clicked = Some((slot.scope.clone(), resp.rect));
                                    }
                                }
                            },
                        );
                    });
                    if let (Some((plugin_id, instance_id, _)), Some(content_rect)) =
                        (&slot.mesh, mesh_content_rect)
                    {
                        mesh_drawn.push(PluginBannerMeshSlot {
                            plugin_id: plugin_id.clone(),
                            instance_id: *instance_id,
                            content_rect,
                            card_rect,
                        });
                    }
                    next_card_rects.insert(card_key, card_rect);
                }
            });

        self.card_rects = next_card_rects;
        self.plugin_mesh_slots = mesh_drawn;

        let visible_ids: std::collections::HashSet<(BannerScope, BannerKey)> = slots
            .iter()
            .map(|s| (s.scope.clone(), s.key.clone()))
            .collect();
        let hovered_set: std::collections::HashSet<(BannerScope, BannerKey)> =
            hovered_ids.into_iter().collect();
        // hover·비가시 상태는 TTL을 멈춘다. reduced_motion은 카운트다운에는 영향을 주지 않는다.
        self.advance(dt_ms, |scope, key| {
            let k = (scope.clone(), key.clone());
            hovered_set.contains(&k) || !visible_ids.contains(&k)
        });

        for scope in close_requests {
            if let Some(removed) = self.close_shown(&scope)
                && let BannerContentSource::PluginMesh { instance_id, .. } = removed.content
            {
                self.closed_plugin_banners
                    .push((instance_id, PluginBannerCloseKind::UserClose));
            }
        }

        if self.has_any() {
            ctx.request_repaint();
        }

        BannerDrawResult {
            hovered: hovered_any,
            more_clicked,
            layer: egui::LayerId::new(egui::Order::Foreground, egui::Id::new("banner_layer")),
            attach_refusal,
            attach_size_sync,
        }
    }

    /// 보이는 범위의 위·좌·우에 Theme.spacing_sm 여백을 적용한다. 아래 여백은 없다.
    /// 좁은 배치 판정에 쓰도록 여백을 빼기 전 범위의 폭을 함께 돌려준다.
    fn banner_zone(
        scope: &BannerScope,
        draw_ctx: &LayoutContext,
        view_placeholder: Option<egui::Rect>,
        theme: &Theme,
    ) -> Option<(egui::Rect, f32)> {
        let margin = theme.spacing_sm.value();
        let base = match scope {
            BannerScope::View => view_placeholder?,
            BannerScope::Workspace(ws_idx) => {
                if *ws_idx != draw_ctx.active_workspace {
                    return None;
                }
                // 워크스페이스 콘텐츠 = 보이는 pane 전체. 사이드바·제목 표시줄을 덮지 않고 탭 바 아래에 둔다.
                let rect = draw_ctx
                    .pane_rects
                    .iter()
                    .map(|(_, r)| *r)
                    .reduce(|a, b| a.union(b))?;
                egui::Rect::from_min_max(
                    egui::pos2(rect.left(), rect.top() + theme.tab_bar_height.value()),
                    rect.max,
                )
            }
            BannerScope::Pane(pane_id) => {
                let rect = draw_ctx
                    .pane_rects
                    .iter()
                    .find(|(id, _)| id == pane_id)
                    .map(|(_, r)| *r)?;
                egui::Rect::from_min_max(
                    egui::pos2(rect.left(), rect.top() + theme.tab_bar_height.value()),
                    rect.max,
                )
            }
            BannerScope::Tab(pane_id, tab_idx) => {
                let active = draw_ctx
                    .active_tabs
                    .iter()
                    .any(|(pid, idx)| pid == pane_id && idx == tab_idx);
                if !active {
                    return None;
                }
                let rect = draw_ctx
                    .pane_rects
                    .iter()
                    .find(|(id, _)| id == pane_id)
                    .map(|(_, r)| *r)?;
                egui::Rect::from_min_max(
                    egui::pos2(rect.left(), rect.top() + theme.tab_bar_height.value()),
                    rect.max,
                )
            }
            BannerScope::Surface(surface_id) => draw_ctx
                .surface_rects
                .iter()
                .find(|(id, _)| id == surface_id)
                .map(|(_, r)| *r)?,
        };
        let zone = egui::Rect::from_min_max(
            egui::pos2(base.left() + margin, base.top() + margin),
            egui::pos2(base.right() - margin, base.bottom()),
        );
        if zone.width() <= 0.0 {
            return None;
        }
        Some((zone, base.width()))
    }
}

/// 같은 Area 안에서 범위별 child Ui를 만들고 그 범위로 클리핑한다.
fn ui_at(parent: &mut egui::Ui, rect: egui::Rect) -> egui::Ui {
    let mut child = parent.new_child(
        egui::UiBuilder::new()
            .id_salt(("banner_zone", rect.left() as i32, rect.top() as i32))
            .max_rect(rect),
    );
    child.set_clip_rect(rect);
    child
}

pub mod defs {
    use super::{BannerDef, BannerId};
    use crate::adapters::ui::icons;
    use crate::i18n::t;
    use crate::theme::Theme;

    /// 앞 글리프 자리. 시안처럼 `banner-glyph-offset`만큼 내려 제목의 대문자 선에 맞춘다.
    fn leading_glyph_rect(ui: &mut egui::Ui, theme: &Theme) -> egui::Rect {
        let glyph = theme.icon_glyph_size_md.value();
        let offset = theme.banner_glyph_offset().value();
        let (slot, _) =
            ui.allocate_exact_size(egui::vec2(glyph, glyph + offset), egui::Sense::hover());
        egui::Rect::from_min_size(
            egui::pos2(slot.left(), slot.top() + offset),
            egui::vec2(glyph, glyph),
        )
    }

    /// 마우스를 캡처한 TUI에서 사용자가 드래그 선택을 시도했을 때 표시한다.
    pub const BANNER_MOUSE_CAPTURE: BannerId = "mouse-capture";

    fn content_mouse_capture(ui: &mut egui::Ui, theme: &Theme) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_md.value();
            let rect = leading_glyph_rect(ui, theme);
            icons::MOUSE
                .image(rect.height(), theme.banner_icon_fg().to_egui())
                .paint_at(ui, rect);
            ui.vertical(|ui| {
                // draw의 ⋯/× 열 자리(`banner-more-reserve`)를 비워 본문 크기를 유지한다.
                ui.set_max_width(
                    (ui.available_width() - theme.banner_more_reserve().value()).max(0.0),
                );
                ui.spacing_mut().item_spacing.y = theme.banner_text_gap().value();
                ui.label(
                    egui::RichText::new(t("banner.mouse_capture.title"))
                        .size(theme.font_size_body.value())
                        .strong()
                        .color(theme.banner_fg().to_egui()),
                );
                ui.label(
                    egui::RichText::new(t("banner.mouse_capture.body"))
                        .size(theme.font_size_caption.value())
                        .color(theme.banner_body_fg().to_egui()),
                );
            });
        });
    }

    /// 자동 attach 매핑을 연결하지 않았을 때 그 워크스페이스에 표시한다. 내용은
    /// `BannerContentSource::AttachRefusal`이 그리므로 정적 정의 목록에는 없다.
    pub const BANNER_ATTACH_REFUSAL: BannerId = "attach-refusal";

    /// mirror 터미널 크기 동기화가 자동 재시도 뒤에도 실패했을 때 그 워크스페이스에 표시한다.
    /// 내용은 `BannerContentSource::AttachSizeSync`가 그리므로 정적 정의 목록에는 없다.
    pub const BANNER_ATTACH_SIZE_SYNC: BannerId = "attach-size-sync";

    /// 출력이 있는데도 PromptBoundary를 받지 못하면 안내한다. 자동으로 설치하거나 수정하지 않는다.
    pub const BANNER_SHELL_INTEGRATION_MISSING: BannerId = "shell-integration-missing";

    /// 셸 통합 안내의 TTL(초). 동작이 없는 배너라 자동으로 닫힌다. 본문을 읽고 알아차릴
    /// 시간을 둔 디자인 고정값이며 설정으로 노출하지 않는다.
    pub const SHELL_INTEGRATION_MISSING_TTL_SECONDS: u32 = 10;

    /// 셸 통합 안내 정의. 제품 경로는 `BannerState::from_def`로 이 정의를 연다.
    pub const SHELL_INTEGRATION_MISSING: BannerDef = BannerDef {
        id: BANNER_SHELL_INTEGRATION_MISSING,
        ttl_seconds: Some(SHELL_INTEGRATION_MISSING_TTL_SECONDS),
        content_fn: content_shell_integration_missing,
    };

    fn content_shell_integration_missing(ui: &mut egui::Ui, theme: &Theme) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = theme.spacing_md.value();
            let rect = leading_glyph_rect(ui, theme);
            icons::TERM
                .image(rect.height(), theme.banner_icon_fg().to_egui())
                .paint_at(ui, rect);
            ui.vertical(|ui| {
                ui.set_max_width(
                    (ui.available_width() - theme.item_height_interactive.value()).max(0.0),
                );
                ui.spacing_mut().item_spacing.y = theme.banner_text_gap().value();
                ui.label(
                    egui::RichText::new(t("banner.shell_integration_missing.title"))
                        .size(theme.font_size_body.value())
                        .strong()
                        .color(theme.banner_fg().to_egui()),
                );
                ui.label(
                    egui::RichText::new(t("banner.shell_integration_missing.body"))
                        .size(theme.font_size_caption.value())
                        .color(theme.banner_body_fg().to_egui()),
                );
            });
        });
    }

    // 이유: 호출부가 debug.rs(`#![cfg(debug_assertions)]`)뿐이라 release 빌드에서
    // 미사용으로 잡힌다.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub fn all_defs() -> &'static [BannerDef] {
        &DEFS
    }

    static DEFS: [BannerDef; 2] = [
        BannerDef {
            id: BANNER_MOUSE_CAPTURE,
            ttl_seconds: None,
            content_fn: content_mouse_capture,
        },
        SHELL_INTEGRATION_MISSING,
    ];

    pub fn find(id: &str) -> Option<&'static BannerDef> {
        DEFS.iter().find(|d| d.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn persistent(id: BannerId, scope: BannerScope) -> BannerState {
        BannerState::persistent(id, scope)
    }

    #[test]
    fn card_rect_answers_only_for_what_was_drawn() {
        let mut mgr = BannerManager::new();
        let scope = BannerScope::View;
        let key = BannerKey::Host("mouse-capture");
        assert_eq!(
            mgr.card_rect(&scope, &key),
            None,
            "렌더링하지 않은 배너에는 측정 좌표가 없어야 한다"
        );

        let drawn = egui::Rect::from_min_size(egui::pos2(8.0, 32.0), egui::vec2(1264.0, 50.0));
        mgr.card_rects.insert((scope.clone(), key.clone()), drawn);
        assert_eq!(mgr.card_rect(&scope, &key), Some(drawn));

        assert_eq!(mgr.card_rect(&BannerScope::Workspace(0), &key), None);
        assert_eq!(mgr.card_rect(&scope, &BannerKey::Plugin(1)), None);
    }

    #[test]
    fn push_into_empty_scope_shows_immediately() {
        let mut mgr = BannerManager::new();
        let out = mgr.push(persistent("a", BannerScope::Surface(1)));
        assert_eq!(out, BannerPushOutcome::Shown);
        assert_eq!(mgr.shown_banners().count(), 1);
        assert_eq!(mgr.total_queued(), 0);
    }

    #[test]
    fn queue_caps_at_one_shown_plus_five_queued_and_drops_seventh() {
        let mut mgr = BannerManager::new();
        let scope = BannerScope::Surface(1);
        let ids = ["a", "b", "c", "d", "e", "f", "g"];
        let outcomes: Vec<_> = ids
            .iter()
            .map(|id| mgr.push(persistent(id, scope.clone())))
            .collect();
        assert_eq!(outcomes[0], BannerPushOutcome::Shown);
        for o in &outcomes[1..6] {
            assert_eq!(*o, BannerPushOutcome::Queued);
        }
        assert_eq!(outcomes[6], BannerPushOutcome::Ignored);
        assert_eq!(mgr.shown_banners().count(), 1);
        assert_eq!(mgr.total_queued(), 5);
    }

    #[test]
    fn same_id_shown_with_ttl_resets_countdown() {
        let mut mgr = BannerManager::new();
        let scope = BannerScope::Pane(7);
        mgr.push(BannerState::with_ttl("ttl", scope.clone(), 6));
        mgr.advance(3000.0, |_, _| false);
        let before = mgr.shown_banners().next().unwrap().remaining_ms;
        assert!(before < 6000.0);
        let out = mgr.push(BannerState::with_ttl("ttl", scope.clone(), 6));
        assert_eq!(out, BannerPushOutcome::ResetCountdown);
        assert_eq!(mgr.shown_banners().count(), 1);
        assert_eq!(mgr.total_queued(), 0);
        assert_eq!(mgr.shown_banners().next().unwrap().remaining_ms, 6000.0);
    }

    #[test]
    fn same_id_shown_without_ttl_is_ignored() {
        let mut mgr = BannerManager::new();
        let scope = BannerScope::View;
        mgr.push(persistent("x", scope.clone()));
        let out = mgr.push(persistent("x", scope));
        assert_eq!(out, BannerPushOutcome::Ignored);
        assert_eq!(mgr.shown_banners().count(), 1);
    }

    #[test]
    fn same_id_in_queue_is_ignored() {
        let mut mgr = BannerManager::new();
        let scope = BannerScope::Surface(1);
        mgr.push(persistent("a", scope.clone())); // shown
        mgr.push(persistent("b", scope.clone())); // queued
        let out = mgr.push(persistent("b", scope.clone())); // dup queued → ignore
        assert_eq!(out, BannerPushOutcome::Ignored);
        assert_eq!(mgr.total_queued(), 1);
    }

    #[test]
    fn close_promotes_queue_head() {
        let mut mgr = BannerManager::new();
        let scope = BannerScope::Surface(1);
        mgr.push(persistent("a", scope.clone()));
        mgr.push(persistent("b", scope.clone()));
        assert_eq!(mgr.shown_banners().next().unwrap().id, "a");
        assert!(mgr.close_shown(&scope).is_some());
        assert_eq!(mgr.shown_banners().next().unwrap().id, "b");
        assert_eq!(mgr.total_queued(), 0);
    }

    #[test]
    fn close_last_removes_scope_lane() {
        let mut mgr = BannerManager::new();
        let scope = BannerScope::Surface(1);
        mgr.push(persistent("a", scope.clone()));
        assert!(mgr.close_shown(&scope).is_some());
        assert!(!mgr.has_any());
        assert!(mgr.close_shown(&scope).is_none()); // 더 닫을 것 없음
    }

    #[test]
    fn ttl_counts_down_and_expires_to_promote() {
        let mut mgr = BannerManager::new();
        let scope = BannerScope::Pane(1);
        mgr.push(BannerState::with_ttl("ttl", scope.clone(), 2)); // 2000ms
        mgr.push(persistent("next", scope.clone())); // queued
        mgr.advance(1500.0, |_, _| false);
        assert_eq!(mgr.shown_banners().next().unwrap().id, "ttl");
        mgr.advance(600.0, |_, _| false); // 2100ms 누적 → 만료
        assert_eq!(mgr.shown_banners().next().unwrap().id, "next");
    }

    #[test]
    fn ttl_pauses_when_flagged() {
        let mut mgr = BannerManager::new();
        let scope = BannerScope::Pane(1);
        mgr.push(BannerState::with_ttl("ttl", scope.clone(), 5));
        mgr.advance(3000.0, |_, _| true);
        assert_eq!(mgr.shown_banners().next().unwrap().remaining_ms, 5000.0);
        mgr.advance(1000.0, |_, _| false);
        assert_eq!(mgr.shown_banners().next().unwrap().remaining_ms, 4000.0);
    }

    #[test]
    fn shell_integration_banner_expires_after_its_ttl() {
        let mut mgr = BannerManager::new();
        let scope = BannerScope::Surface(1);
        mgr.push(BannerState::with_ttl(
            defs::BANNER_SHELL_INTEGRATION_MISSING,
            scope.clone(),
            defs::SHELL_INTEGRATION_MISSING_TTL_SECONDS,
        ));
        assert_eq!(
            defs::find(defs::BANNER_SHELL_INTEGRATION_MISSING).and_then(|d| d.ttl_seconds),
            Some(10)
        );
        mgr.advance(9_900.0, |_, _| false);
        assert_eq!(
            mgr.shown_banners().next().map(|b| b.id),
            Some(defs::BANNER_SHELL_INTEGRATION_MISSING)
        );
        mgr.advance(200.0, |_, _| false);
        assert_eq!(mgr.shown_banners().count(), 0);
    }

    #[test]
    fn shell_integration_banner_stays_while_paused() {
        let mut mgr = BannerManager::new();
        let scope = BannerScope::Surface(1);
        mgr.push(BannerState::with_ttl(
            defs::BANNER_SHELL_INTEGRATION_MISSING,
            scope.clone(),
            defs::SHELL_INTEGRATION_MISSING_TTL_SECONDS,
        ));
        mgr.advance(60_000.0, |_, _| true);
        assert_eq!(
            mgr.shown_banners().next().map(|b| b.id),
            Some(defs::BANNER_SHELL_INTEGRATION_MISSING)
        );
    }

    #[test]
    fn ttl_does_not_affect_persistent_banner() {
        let mut mgr = BannerManager::new();
        let scope = BannerScope::View;
        mgr.push(persistent("p", scope.clone()));
        mgr.advance(100_000.0, |_, _| false);
        assert_eq!(mgr.shown_banners().count(), 1);
    }

    #[test]
    fn cross_scope_recessed_lower_priority() {
        let mut mgr = BannerManager::new();
        mgr.push(persistent("v", BannerScope::View));
        mgr.push(persistent("s", BannerScope::Surface(1)));
        let view_max = BannerScope::View.priority();
        assert!(!mgr.is_recessed(&BannerScope::View, view_max));
        assert!(mgr.is_recessed(&BannerScope::Surface(1), view_max));
    }

    #[test]
    fn remaining_seconds_ceils() {
        let b = BannerState::with_ttl("t", BannerScope::View, 6);
        assert_eq!(b.remaining_seconds(), Some(6));
        let mut b2 = b.clone();
        b2.remaining_ms = 5400.0;
        assert_eq!(b2.remaining_seconds(), Some(6)); // ceil
        b2.remaining_ms = 4001.0;
        assert_eq!(b2.remaining_seconds(), Some(5));
        let p = BannerState::persistent("p", BannerScope::View);
        assert_eq!(p.remaining_seconds(), None);
    }

    #[test]
    fn push_replaces_shown_when_origin_generation_differs() {
        let mut mgr = BannerManager::new();
        let scope = BannerScope::Surface(1);
        mgr.push(persistent("mouse-capture", scope.clone()).with_origin_generation(1));
        let out = mgr.push(persistent("mouse-capture", scope.clone()).with_origin_generation(2));
        assert_eq!(out, BannerPushOutcome::Replaced);
        assert_eq!(mgr.shown_banners().count(), 1);
        assert_eq!(
            mgr.shown_banners().next().unwrap().origin_generation,
            Some(2)
        );
    }

    #[test]
    fn push_ignores_shown_when_origin_generation_matches() {
        let mut mgr = BannerManager::new();
        let scope = BannerScope::Surface(1);
        mgr.push(persistent("mouse-capture", scope.clone()).with_origin_generation(1));
        let out = mgr.push(persistent("mouse-capture", scope.clone()).with_origin_generation(1));
        assert_eq!(out, BannerPushOutcome::Ignored);
        assert_eq!(mgr.shown_banners().count(), 1);
    }

    #[test]
    fn close_shown_if_id_closes_matching_id_only() {
        let mut mgr = BannerManager::new();
        let scope = BannerScope::Surface(1);
        mgr.push(persistent("mouse-capture", scope.clone()));
        assert!(
            mgr.close_shown_if_id(&scope, "shell-integration-missing")
                .is_none()
        );
        assert_eq!(mgr.shown_banners().count(), 1);
        assert!(mgr.close_shown_if_id(&scope, "mouse-capture").is_some());
        assert!(!mgr.has_any());
    }

    #[test]
    fn close_shown_if_id_on_empty_scope_is_none() {
        let mut mgr = BannerManager::new();
        assert!(
            mgr.close_shown_if_id(&BannerScope::Surface(99), "mouse-capture")
                .is_none()
        );
    }

    #[test]
    fn close_by_id_removes_from_queue_or_shown() {
        let mut mgr = BannerManager::new();
        let scope = BannerScope::Surface(1);
        mgr.push(persistent("a", scope.clone()));
        mgr.push(persistent("b", scope.clone()));
        assert!(mgr.close_by_id("b"));
        assert_eq!(mgr.total_queued(), 0);
        assert_eq!(mgr.shown_banners().next().unwrap().id, "a");
        assert!(mgr.close_by_id("a"));
        assert!(!mgr.has_any());
    }

    #[test]
    fn set_countdown_overrides_remaining() {
        let mut mgr = BannerManager::new();
        let scope = BannerScope::Pane(1);
        mgr.push(BannerState::with_ttl("t", scope.clone(), 10));
        assert!(mgr.set_countdown(&scope, 2));
        assert_eq!(mgr.shown_banners().next().unwrap().remaining_ms, 2000.0);
    }

    #[test]
    fn independent_scopes_each_show_one() {
        let mut mgr = BannerManager::new();
        mgr.push(persistent("a", BannerScope::Surface(1)));
        mgr.push(persistent("b", BannerScope::Surface(2)));
        assert_eq!(mgr.shown_banners().count(), 2);
    }

    fn plugin_mesh(instance_id: u64, scope: BannerScope, ttl: Option<u32>) -> BannerState {
        BannerState::plugin_mesh(scope, "com.example.p".to_string(), instance_id, ttl, 64.0)
    }

    #[test]
    fn plugin_mesh_key_uses_instance_id() {
        let a = plugin_mesh(1, BannerScope::Surface(9), None);
        let b = plugin_mesh(2, BannerScope::Surface(9), None);
        assert_eq!(a.key(), BannerKey::Plugin(1));
        assert_ne!(a.key(), b.key());
        assert_ne!(
            a.key(),
            BannerState::persistent("x", BannerScope::Surface(9)).key()
        );
    }

    #[test]
    fn distinct_plugin_instances_queue_not_dedup() {
        let mut mgr = BannerManager::new();
        let scope = BannerScope::Surface(9);
        assert_eq!(
            mgr.push(plugin_mesh(1, scope.clone(), None)),
            BannerPushOutcome::Shown
        );
        assert_eq!(
            mgr.push(plugin_mesh(2, scope.clone(), None)),
            BannerPushOutcome::Queued
        );
        assert_eq!(mgr.shown_banners().count(), 1);
        assert_eq!(mgr.total_queued(), 1);
        assert_eq!(mgr.plugin_instances().count(), 2);
    }

    #[test]
    fn plugin_ttl_expiry_records_close_reason() {
        let mut mgr = BannerManager::new();
        let scope = BannerScope::Surface(9);
        mgr.push(plugin_mesh(7, scope.clone(), Some(2))); // 2000ms
        mgr.advance(2100.0, |_, _| false); // 만료
        let closed = mgr.drain_closed_plugin_banners();
        assert_eq!(closed, vec![(7, PluginBannerCloseKind::Ttl)]);
        assert!(!mgr.has_any());
        assert!(mgr.drain_closed_plugin_banners().is_empty());
    }

    #[test]
    fn close_by_instance_removes_plugin_banner() {
        let mut mgr = BannerManager::new();
        let scope = BannerScope::Surface(9);
        mgr.push(plugin_mesh(1, scope.clone(), None)); // shown
        mgr.push(plugin_mesh(2, scope.clone(), None)); // queued
        assert!(mgr.close_by_instance(2));
        assert_eq!(mgr.total_queued(), 0);
        assert!(mgr.close_by_instance(1));
        assert!(!mgr.has_any());
        assert!(!mgr.close_by_instance(1)); // 더 없음
    }

    /// mouse-capture 배너를 그리고 hover 한 채 × 왼쪽의 ⋯ 자리(`banner-more-reserve` 열)를 차례로 누른다.
    /// 더보기 버튼이 눌린 범위를 돌려준다. × 자리(오른쪽 끝 한 칸)는 누르지 않는다.
    /// 배너는 프로그램 이름을 받지 않으므로 이름을 모르는 surface(원격 mirror 포함)도 같은 결과다.
    fn click_across_the_more_column() -> Option<BannerScope> {
        let theme = crate::theme::theme();
        let scope = BannerScope::Surface(42);
        let mut mgr = BannerManager::new();
        mgr.push(BannerState::persistent(
            defs::BANNER_MOUSE_CAPTURE,
            scope.clone(),
        ));
        let surface = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 400.0));
        let layout = crate::adapters::ui::LayoutContext {
            active_workspace: 0,
            active_workspace_id: 0,
            pane_rects: Vec::new(),
            surface_rects: vec![(42, surface)],
            active_tabs: Vec::new(),
            active_tab_ids: Vec::new(),
            workspace_rect: None,
        };
        let ctx = egui::Context::default();
        let run = |events: Vec<egui::Event>, mgr: &mut BannerManager| {
            let mut clicked = None;
            let _frame = ctx.run(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ctx| {
                    let r = mgr.draw(ctx, &layout, &theme, None, false, None);
                    clicked = r.more_clicked.map(|(s, _)| s);
                },
            );
            clicked
        };
        run(vec![], &mut mgr);
        let card = mgr
            .card_rect(&scope, &BannerKey::Host(defs::BANNER_MOUSE_CAPTURE))
            .expect("card drawn");
        let y =
            card.top() + theme.banner_padding_y().value() + ControlSize::Sm.height(&theme) * 0.5;
        let close = theme.item_height_interactive;
        let column_left = card.right() - (close + theme.banner_more_reserve()).value();
        let column_right = card.right() - close.value();
        let mut x = column_left;
        while x < column_right {
            let at = egui::pos2(x, y);
            for events in [
                vec![egui::Event::PointerMoved(at)],
                vec![egui::Event::PointerMoved(at)],
                vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                }],
                vec![egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                }],
            ] {
                if let Some(s) = run(events, &mut mgr) {
                    return Some(s);
                }
            }
            x += 2.0;
        }
        None
    }

    /// 프로그램 이름과 관계없이 mouse-capture 배너는 hover 때 ⋯ 를 그리고, 누르면 그 범위로 메뉴를 연다.
    #[test]
    fn the_mouse_capture_more_button_is_always_drawn_and_clickable() {
        assert_eq!(
            click_across_the_more_column(),
            Some(BannerScope::Surface(42))
        );
    }

    /// 본문은 ⋯·× 두 버튼 열(56)을 항상 비운다. 이름 미상 상태에서도 줄지 않는다.
    /// 단어 단위 줄바꿈은 줄 끝에 여유를 남기므로 폭을 1px 씩 바꿔 가며 가장 작은 오른쪽 여유를 잰다.
    #[test]
    fn the_mouse_capture_body_always_reserves_the_two_button_column() {
        let theme = crate::theme::theme();
        let reserve = theme.banner_more_reserve();
        assert_eq!(reserve, tasty_type_geometry::length::LogicalPx(56.0));
        let def = defs::find(defs::BANNER_MOUSE_CAPTURE).expect("def");
        let ctx = egui::Context::default();
        let mut least_slack = f32::MAX;
        for width in 240..480 {
            let width = width as f32;
            let _frame = ctx.run(egui::RawInput::default(), |ctx| {
                egui::Area::new(egui::Id::new(("mouse_capture_body", width.to_bits())))
                    .fixed_pos(egui::Pos2::ZERO)
                    .show(ctx, |ui| {
                        // 최소 폭은 두지 않아 그린 내용의 오른쪽 끝이 min_rect 에 드러난다.
                        ui.set_max_width(width);
                        (def.content_fn)(ui, &theme);
                        least_slack = least_slack.min(width - ui.min_rect().right());
                    });
            });
        }
        assert!(
            least_slack >= reserve.value() - 0.5,
            "least right slack {least_slack}"
        );
        assert!(
            least_slack < reserve.value() + 8.0,
            "the sweep reaches the reserve: {least_slack}"
        );
    }
}
