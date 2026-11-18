//! 요청 주체를 구분해 사용자 선택·복원 기록 정책에 사용한다.
//! 도메인이 GUI 큐 타입에 의존하지 않도록 여기서 정의하고 UI 쪽이 재사용한다.

/// User·Agent·System을 구별한다. OSC 등 자동 후속 처리는 System이며 사용자 조작으로 보지 않는다.
/// System은 도메인 요청에 사용하고 UI 조작을 자동으로 만드는 용도로 쓰지 않는다.
#[derive(Debug, Clone)]
pub enum IntentOrigin {
    #[cfg_attr(
        all(not(feature = "gui"), not(test)),
        expect(dead_code, reason = "only the gui raises a user intent")
    )]
    User {
        #[allow(dead_code)]
        source: UserSource,
    },
    Agent {
        #[allow(dead_code)]
        source: AgentSource,
    },
    System,
}

/// 사용자 요청 출처를 Debug 출력에 남기기 위한 값.
#[cfg_attr(
    not(feature = "gui"),
    expect(dead_code, reason = "only the gui raises a user intent")
)]
#[derive(Debug, Clone)]
pub enum UserSource {
    Shortcut(#[allow(dead_code)] &'static str),
    Menu(#[allow(dead_code)] &'static str),
    ContextMenu,
}

/// 에이전트 요청 채널과 plugin 식별자.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum AgentSource {
    Ipc,
    Plugin(String),
    Cli,
    /// 원격 client가 forward한 구조 요청. 원격 쪽 사용자 조작이어도 이 호스트에서는 에이전트 요청이다.
    Remote,
}

impl IntentOrigin {
    pub fn is_user(&self) -> bool {
        matches!(self, IntentOrigin::User { .. })
    }

    #[allow(dead_code)]
    pub fn is_agent(&self) -> bool {
        matches!(self, IntentOrigin::Agent { .. })
    }

    #[allow(dead_code)]
    pub fn is_system(&self) -> bool {
        matches!(self, IntentOrigin::System)
    }
}

/// 파일 식별 worker를 거쳐 돌아올 때도 사용자 요청인지 구별하기 위한 값.
/// origin_surface_id는 대상 위치이며 요청 주체와는 별개다.
/// plugin이 확인된 클릭을 전달할 수도 있으므로 IPC로 왔다는 이유만으로 Agent로 정하지 않는다.
#[cfg_attr(
    not(feature = "gui"),
    expect(
        dead_code,
        reason = "every producer of a file dispatch origin is gui-only"
    )
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileDispatchOrigin {
    /// 직접 사용자 조작 또는 host가 확인한 사용자 입력을 plugin이 중계한 요청.
    User,
    /// 외부 IPC(CLI·에이전트 세션)의 파일 열기 요청. 사용자 화면에 picker를 띄우지 않는다.
    Agent,
    /// 사용자 입력을 증명하지 못한 plugin 중계 요청. 선택은 Agent처럼 유지하고
    /// 무매칭 fallback picker는 연다(사용자 입력일 수 있어 아무 반응이 없으면 안 된다).
    PluginUnverified,
}

impl FileDispatchOrigin {
    /// 사용자가 연 결과만 선택해 에이전트 요청이 사용자 포커스를 옮기지 않게 한다.
    #[cfg(feature = "gui")]
    pub(crate) fn selects_result(self) -> bool {
        matches!(self, Self::User)
    }
}

/// 사용자 입력이 정한 파일 열기 결과 탭의 자리. 파일 위치와 실행 출처를 정하는 `origin_surface_id`와
/// 따로 싣는다. 로컬 파일을 mirror surface에 놓아도 원격 파일로 판정하지 않게 하려는 것이다.
#[cfg_attr(
    not(feature = "gui"),
    expect(
        dead_code,
        reason = "every producer of a file dispatch origin is gui-only"
    )
)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FileResultPlace {
    /// 입력이 자리를 정하지 않은 요청(에이전트 IPC 등). 결과 탭은 처리 때의 선택 pane에 연다.
    #[default]
    Unbound,
    /// 입력 때 정한 pane. 그 사이 사라졌으면 다른 pane에 열지 않고 버린다.
    Pane(u32),
    /// 새 로컬 workspace. mirror workspace에서 요청해 받은 파일이다.
    NewLocalWorkspace,
}

/// picker 표시와 핸들러 실행이 함께 옮기는 요청 출처와 크기 제한 정책.
#[cfg_attr(
    not(feature = "gui"),
    expect(
        dead_code,
        reason = "every producer of a file dispatch origin is gui-only"
    )
)]
#[derive(Debug, Clone, Copy)]
pub struct FileDispatchPolicy {
    pub dispatch_origin: FileDispatchOrigin,
    pub ignore_size_limit: bool,
    /// 최종 결과를 받을 요청의 run([`crate::request_runs`]). 결과를 돌려받지 않는 요청은 None이다.
    pub run: Option<u64>,
    /// 결과 탭을 열 자리. 사용자 입력이 정한 값이다.
    pub result_place: FileResultPlace,
}

/// 명시된 surface의 pane을 찾는다. 대상이 사라졌으면 현재 선택 pane으로 대체하지 않고 거절한다.
#[cfg(feature = "gui")]
pub(crate) fn require_origin_pane(
    engine: &crate::core::CoreState,
    surface_id: u32,
) -> Result<u32, String> {
    engine.find_pane_for_surface(surface_id).ok_or_else(|| {
        crate::core::request_target::unowned_target_message(
            crate::core::request_target::ResourceId {
                kind: crate::core::request_target::Kind::Surface,
                id: u64::from(surface_id),
            },
            "file_handler.dispatch",
        )
    })
}
