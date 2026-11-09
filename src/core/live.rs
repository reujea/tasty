//! Non-journal observations used by admission. No sockets, channels or execution callbacks.
use crate::core::state::SurfaceMessage;
use std::collections::HashMap;
pub(crate) struct LiveDomainState {
    pub(crate) notifications: crate::notification::NotificationStore,
    pub(crate) occupancy: crate::core::attach::OccupancyRegistry,
    pub(crate) surface_titles: HashMap<u32, tasty_model::SurfaceTitle>,
    pub(crate) last_key_input: HashMap<u32, std::time::Instant>,
    pub(crate) busy_surfaces: std::collections::HashSet<u32>,
    /// `surface.set_busy`로 알린 busy. 폴링이 `busy_surfaces`를 통째로 바꾸므로 따로 두고, 판정은 둘의
    /// 합집합이다. 값은 터미널에서 알린 뒤 처음 관측한 전경 pid다(plugin surface와 아직 관측 전이면 None).
    /// 전경이 그 pid에서 바뀌면 알린 프로그램이 끝난 것으로 보고 지운다.
    pub(crate) reported_busy_surfaces: HashMap<u32, Option<u32>>,
    // attention은 여러 알림 원인이 공유하며 알림 패널 항목과는 별도 상태다.
    pub(crate) attention: crate::core::state::attention::AttentionStore,
    // busy 폴링에서 얻은 전경 이름으로 마우스 캡처 제한도 계산한다.
    pub(crate) mouse_capture_disabled_surfaces: std::collections::HashSet<u32>,
    /// 첫 출력 뒤 OSC 133 경계가 없는 surface에 셸 통합 안내를 고려할 기준 시각.
    pub(crate) shell_integration_first_output_at:
        std::collections::HashMap<u32, std::time::Instant>,
    /// OSC 133 경계를 한 번이라도 받아 안내 대상에서 제외한 surface.
    pub(crate) shell_integration_boundary_seen: std::collections::HashSet<u32>,
    /// 안내 요청 이벤트를 이미 보낸 surface. 배너를 보여줬는지는 창 상태가 따로 기록한다.
    pub(crate) shell_integration_hint_requested: std::collections::HashSet<u32>,
    /// 프롬프트(OSC 133 A)를 그린 PTY 연결의 generation. 그 뒤의 D만 명령 완료로 센다.
    pub(crate) prompt_shown: HashMap<u32, u64>,
    // 매 프레임 OS 프로세스를 조회하지 않도록 busy 폴링의 전경 이름을 재사용한다.
    pub(crate) foreground_names: std::collections::HashMap<u32, String>,
    /// 폴링에서 전경 이름이 바뀔 때 올리는 번호. PID나 실제 프로세스 동일성을 판별하는 값은 아니다.
    pub(crate) foreground_generation: std::collections::HashMap<u32, u64>,
    pub(crate) command_index: crate::core::command_index::CommandIndex,
    pub(crate) surface_messages: HashMap<u32, Vec<SurfaceMessage>>,
    pub(crate) surface_next_message_id: u32,
}

impl Default for LiveDomainState {
    fn default() -> Self {
        Self {
            notifications: crate::notification::NotificationStore::with_counter(
                500,
                std::sync::Arc::new(std::sync::atomic::AtomicU64::new(1)),
            ),
            occupancy: Default::default(),
            surface_titles: HashMap::new(),
            last_key_input: HashMap::new(),
            busy_surfaces: std::collections::HashSet::new(),
            reported_busy_surfaces: HashMap::new(),
            attention: crate::core::state::attention::AttentionStore::default(),
            mouse_capture_disabled_surfaces: std::collections::HashSet::new(),
            shell_integration_first_output_at: std::collections::HashMap::new(),
            shell_integration_boundary_seen: std::collections::HashSet::new(),
            shell_integration_hint_requested: std::collections::HashSet::new(),
            prompt_shown: HashMap::new(),
            foreground_names: std::collections::HashMap::new(),
            foreground_generation: std::collections::HashMap::new(),
            command_index: crate::core::command_index::CommandIndex::new(),
            surface_messages: HashMap::new(),
            surface_next_message_id: 0,
        }
    }
}

impl LiveDomainState {
    pub(crate) fn with_notifications(
        counter: std::sync::Arc<std::sync::atomic::AtomicU64>,
        coalesce: u64,
    ) -> Self {
        Self {
            notifications: crate::notification::NotificationStore::with_counter(coalesce, counter),
            ..Default::default()
        }
    }
}
