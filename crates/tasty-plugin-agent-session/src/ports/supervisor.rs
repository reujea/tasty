//! 세션을 다루는 **하나의 인터페이스**.
//!
//! 이 plugin 은 경계가 둘이다. 호스트(tasty) 쪽은 [`crate::ports::host::HostCall`] 뒤에,
//! 세션 쪽은 [`Supervisor`] 뒤에 있다 — 둘 다 가짜로 시험할 수 있다.
//!
//! 실물은 [`crate::adapters::driven::local::LocalSupervisor`] 하나다: plugin 프로세스 안에서 claude 를 직접 띄운다
//! ([`crate::adapters::driven::local`]). 핸들러는 trait 만 본다.

use std::time::Duration;

use serde_json::Value;

/// 데몬에 붙을 수 없는 사유 —  **전제 미충족과 제품 결함을 가른다**.
///
/// 이 구분은 그쪽 종료 코드 축(2 vs 1)과 같은 축이다. 뭉뚱그리면 사용자가
/// *"데몬을 띄워라"* 와 *"버그를 고쳐라"* 중 무엇을 해야 할지 모른다.
#[derive(Debug, Clone)]
pub enum ClientError {
    /// **전제 미충족** — 데몬이 없거나 소켓이 죽었다. 결함이 아니다.
    Unavailable(String),
    /// **데몬이 거절했다** — `kind` 를 구조로 들고 있다.
    ///
    /// supervisor 는 전제 미충족(`precondition`)과 제품 결함(`failed`)을
    /// **표면까지 올려** 구분한다(그쪽 spec §4). 문자열로 합치면 그
    /// 구분이 사라져 *"CLI 미설치"* 와 *"세션이 크래시했다"* 가 같은
    /// 빨간불이 된다.
    Daemon {
        /// supervisor 의 `kind` 그대로 — 모르는 값도 **버리지 않는다**.
        kind: String,
        /// 사람이 읽는 사유.  **이것으로 분기하지 마라** — 바뀐다.
        detail: String,
    },
    /// 붙었는데 **상한 안에 답이 안 왔다** — 기다린 시간을 든다.
    ///
    /// `Unavailable` 과 가르는 이유: 종전에는 둘이 같은 갈래였고, 사용자에게는
    /// OS 원문(`Resource temporarily unavailable (os error 35)`)만 보였다. 그
    /// 문면은 소켓 고장처럼 읽히는데 실제로는 데몬이 살아서 다른 일에 묶여 있거나
    /// 턴이 길게 도는 중이다(실측 2026-10-12). 할 일이 다르다 — 기다리거나 그
    /// 턴을 취소한다.
    TimedOut(Duration),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Unavailable(s) => write!(f, "{s}"),
            ClientError::Daemon { kind, detail } => write!(f, "{detail} ({kind})"),
            ClientError::TimedOut(d) => write!(
                f,
                "{}",
                crate::domain::i18n::t1("sup.err.timed_out", &d.as_secs().to_string())
            ),
        }
    }
}

impl ClientError {
    /// 회귀로 집계해야 하는가 — supervisor `is_product_failure` 와 같은 축.
    ///
    /// **전제 미충족은 결함이 아니다.** 데몬이 안 떠 있거나 CLI 가
    /// 없는 것은 우리 잘못이 아니라 **환경**이다.
    pub fn is_product_failure(&self) -> bool {
        matches!(self, ClientError::Daemon { kind, .. } if kind == "failed")
    }
}

/// 세션 하나의 **우리가 쓰는 부분** — 모르는 필드는 버리지 않는다.
///
/// ⚠️ **스키마를 우리가 다시 적는 대가**가 여기 있다. 그쪽이 필드를 늘리면
/// 여기서 조용히 안 보인다 — 그래서 [`SessionRow::raw`] 로 원본을 함께 들고
/// 다닌다. 버리면 다음 사람이 무엇이 왔는지 못 본다(`Progress::Other` 가
/// raw 를 남기는 것과 같은 규율).
#[derive(Debug, Clone)]
pub struct SessionRow {
    pub id: String,
    /// `starting`·`running`·`stopped`·`finished`·`failed`·`blocked_precondition`.
    ///
    /// **모르는 값을 기본값으로 접지 않는다** — 문자열 그대로 들고 온다.
    /// enum 으로 고정하면 그쪽이 상태를 늘릴 때 우리가 조용히 틀린다.
    pub state: String,
    /// 어느 agent 로 떴나. `None` 은 **v3 이전 세션**이라 재개가 거부된다.
    pub agent_name: Option<String>,
    /// 되살릴 수 있나 — **상태와 좌표를 둘 다** 본 결과다.
    ///
    /// ⚠️ 직접 계산하지 않는다. `state` 만 보고 판단하면 좌표 없는 세션에
    /// resume 을 걸어 실패한다.
    pub resumable: bool,
    /// **유휴 회수로 멎었나** — *"우리가 재웠다"* 와 *"죽었다"* 를 가른다.
    ///
    /// `state` 는 `stopped` 하나로 **셋**을 덮는다(회수·명시 정지·자식 사망).
    /// 이 값이 그중 회수만 집어낸다. `false` 가 *"죽었다"* 를 뜻하지 않는다 —
    /// 모르면 `false` 다.
    pub reclaimed: bool,
    /// 원본 JSON — 위 칸이 표현 못 하는 것을 잃지 않는다. [`Self::raw`] 로 읽는다.
    raw: serde_json::Value,
}

impl SessionRow {
    /// 이 세션만의 작업 디렉토리 — **오버라이드가 있을 때만** 온다.
    ///
    /// 그쪽이 `skip_serializing_if = "Option::is_none"` 이라 **키 자체가
    /// 없으면** *"agent 설정을 따른다"* 는 뜻이다(실측: 세션 셋 중 하나만
    /// `cwd` 를 들고 있었다).  그것을 *"디렉토리가 없다"* 로 읽으면 안 된다.
    pub fn cwd(&self) -> Option<&str> {
        self.raw.get("cwd").and_then(serde_json::Value::as_str)
    }

    /// 이 세션이 붙든 **에이전트 쪽 세션 id**(claude 면 Claude Code 의 세션 id) — supervisor
    /// id 와 다를 수 있다. 외부 대화를 이어받은 세션은 늘 다르다(`session.spawn` 의 `adopt`).
    pub fn agent_session_id(&self) -> Option<&str> {
        self.raw
            .get("agent_session_id")
            .and_then(serde_json::Value::as_str)
            .filter(|v| !v.is_empty())
    }

    /// 그쪽이 보낸 **원본 그대로** — 우리 칸이 표현 못 하는 것을 꺼내는 길.
    ///
    /// ⚠️ 저장만 하고 아무도 안 읽으면 그것은 **죽은 필드**다. 접근자를
    /// 두어 소비자가 실제로 꺼낼 수 있게 한다 — `Progress::Other` 의 raw 가
    /// 되살릴 수 있어야 값을 하는 것과 같다.
    pub fn raw(&self) -> &serde_json::Value {
        &self.raw
    }

    pub(crate) fn from_value(v: &serde_json::Value) -> Option<Self> {
        Some(Self {
            id: v.get("id")?.as_str()?.to_string(),
            state: v.get("state")?.as_str()?.to_string(),
            agent_name: v
                .get("agent_name")
                .and_then(|x| x.as_str())
                .map(str::to_string),
            resumable: v
                .get("resumable")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            reclaimed: v
                .get("reclaimed")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            raw: v.clone(),
        })
    }
}

/// `session.spawn` 에 실을 것들.
///
/// 인자를 넷씩 나열하면 호출부에서 **순서를 바꿔 넣어도 컴파일된다** — 전부
/// `Option<&str>` 이라 타입이 안 막는다. 이름 있는 칸으로 묶어 그 자리를 없앤다.
#[derive(Default)]
pub struct SpawnOpts<'a> {
    pub agent: &'a str,
    pub cwd: Option<&'a str>,
    pub model: Option<&'a str>,
    pub name: Option<&'a str>,
    pub prompt: Option<&'a str>,
    /// **이어받을 좌표** — 데몬이 모르는, agent 프로필이 쓰던 세션 id.
    ///
    /// 주면 데몬이 `--resume` 경로로 띄워 **그 대화가 이어진다**(그쪽
    /// `spawn --adopt`). `session_id`(아는 세션에 붙기)와 다른 축이다 —
    /// 저쪽은 레코드가 있고 이쪽은 **레코드가 없는 대화**를 끌어온다.
    pub adopt: Option<&'a str>,
    /// 기동 권한 모드 — [`crate::adapters::driven::local::PERMISSION_MODES`] 밖이면 `bad_request` 로 거절한다.
    pub permission_mode: Option<&'a str>,
    /// 권한 확인 · 질문을 **소비자가 답한다**. 답할 수 있는
    /// 자리(대화 탭)만 켠다 — 켜 놓고 아무도 답하지 않으면 그 턴이 영영 멈춘다.
    pub permission_prompts: bool,
    /// 추가 작업 폴더 — 대화 탭 복제가 원본의 목록을 그대로 싣는다.
    pub add_dirs: &'a [String],
    /// 역할 폴더 이름(`roles` 명부) — 여럿을 고를 수 있다. 모르는 이름이면 `bad_request` 로 거절한다.
    pub roles: &'a [String],
}

#[cfg(test)]
impl SessionRow {
    /// 시험용 최소 행 — 필드가 늘어도 시험이 안 깨진다.
    pub fn for_test(id: &str, state: &str, agent: Option<&str>) -> Self {
        Self {
            id: id.into(),
            state: state.into(),
            agent_name: agent.map(str::to_string),
            resumable: false,
            reclaimed: false,
            raw: serde_json::Value::Null,
        }
    }

    /// 데몬이 낸 원본 행 그대로 — 제품 파서(`from_json`)를 지난다.
    pub fn from_raw_for_test(v: serde_json::Value) -> Self {
        Self::from_value(&v).expect("id·state 가 있어야 행이 된다")
    }
}

/// 세션 쪽이 해 주는 일 전부.
///
/// ⚠️ **여기 없는 것은 일부러 없다** — 재개는 `send_input` 이 자동으로 하므로 따로 내면
/// 죽은 경로가 된다.
pub trait Supervisor {
    /// 세션 기록을 어디에 두는지 — **오류 문면에 싣는다**.
    fn endpoint(&self) -> String;

    /// 세션 전수와 **못 읽은 행 수**. 뒤엣것을 숨기면 목록이 짧아진 것을 모른다.
    fn list(&self) -> Result<(Vec<SessionRow>, usize), ClientError>;

    fn status(&self, id: &str) -> Result<Value, ClientError>;

    /// 프로세스를 놓는다 — **좌표는 남는다**(재개 가능).
    fn stop(&self, id: &str) -> Result<(), ClientError>;

    /// 쉬는 세션을 놓는다(유휴 회수) — [`Self::stop`] 과 같고, 다시 뜰 때까지 행이 `reclaimed` 를
    /// 싣는다. 탭은 그 표지를 보고 현행화를 다시 열지 않는다(`sync_compact::SyncState::reclaimed`).
    fn reclaim(&self, id: &str) -> Result<(), ClientError> {
        self.stop(id)
    }

    /// 세션 레코드를 **지운다 — 되돌릴 수 없다**. 살아 있는 세션은 거절한다.
    ///
    /// GUI 단추는 **안 만든다** — 좌표가 사라져 대화로 돌아갈 길이 없어지므로, 실수로
    /// 누를 수 있는 자리에 두지 않는다. IPC 로만 연다.
    fn delete(&self, id: &str) -> Result<(), ClientError>;

    /// 도는 턴만 취소 — 세션은 산다.
    fn abort(&self, id: &str) -> Result<(), ClientError>;

    /// agent 프로필 명부(이름 순) — [`crate::adapters::driven::store::agents`].
    fn agent_list(
        &self,
    ) -> Result<Vec<crate::adapters::driven::store::agents::AgentProfile>, ClientError>;

    fn spawn(&self, opts: &SpawnOpts<'_>) -> Result<Value, ClientError>;

    /// 세션에 추가 작업 폴더를 더한다 — 응답 `add_dirs.{dirs, changed, applied}`.
    fn add_dir(&self, id: &str, path: &str) -> Result<Value, ClientError>;

    /// 세션의 추가 작업 폴더를 뺀다 — 응답은 [`Self::add_dir`] 와 같다.
    fn remove_dir(&self, id: &str, path: &str) -> Result<Value, ClientError>;

    /// 세션이 기다리는 물음에 답한다.
    fn respond(
        &self,
        id: &str,
        answer: &crate::application::prompts::Answer,
    ) -> Result<(), ClientError>;

    /// 턴을 넣고 **답을 다 받을 때까지** 기다린다. 멈춰 있으면 `--resume` 으로 되살린다.
    fn send_input(&self, id: &str, text: &str) -> Result<Value, ClientError>;

    /// agent 이름으로 골라 같은 입력. 일부 실패가 전체를 취소하지 않는다.
    fn broadcast(&self, text: &str, agent: Option<&str>) -> Result<Value, ClientError>;

    /// 지나간 답의 꼬리 — 구독으로는 못 보는 **붙기 전**을 답한다.
    fn output(&self, id: &str, lines: Option<usize>) -> Result<Value, ClientError>;

    /// 이벤트를 **예산 안에서** 모은다. 돌려주는 bool 은 `finished` 를 봤는지다.
    fn watch(
        &self,
        id: &str,
        max_events: usize,
        until_finished: bool,
        budget: Duration,
    ) -> Result<(Vec<Value>, bool), ClientError>;

    /// 세션의 구독 — 줄마다 프레임이 온다. 자식이 바뀌어도(재개) 같은 채널로 계속 온다.
    ///
    /// [`Self::watch`] 는 예산 안에서 모아 돌려주고, 이것은 끊길 때까지 따라가는 워커(`stream`)가 쓴다.
    fn subscribe(&self, id: &str) -> Result<std::sync::mpsc::Receiver<Value>, ClientError>;
}

/// plugin 이 들고 워커 스레드마다 복사해 넘기는 데몬 핸들 — 복사가 공짜이고 스레드를 건넌다.
///
/// 세션 쪽 포트를 받는 plugin 상태가 이것을 요구한다. 실물은 조립 루트가 만든 `LocalSupervisor` 다.
pub trait SupervisorHandle: Supervisor + Copy + Send + 'static {}

impl<T: Supervisor + Copy + Send + 'static> SupervisorHandle for T {}

#[cfg(test)]
pub(crate) mod fake {
    //! 데몬 없이 논리를 재는 가짜.
    //!
    //! 이것이 이 인터페이스를 만든 **이유**다 — 종전에는 `stop`·`spawn`·
    //! `broadcast` 를 지나는 시험이 **한 건도 없었다**(소켓이 필요해서).

    use std::cell::RefCell;

    use super::{ClientError, SessionRow, SpawnOpts, Supervisor, Value};

    /// `spawn` 에 실린 `(model, permission_mode, add_dirs)`.
    pub type SpawnArgs = (Option<String>, Option<String>, Vec<String>);

    /// 부른 것을 적어 두고 정해진 답을 주는 가짜 데몬.
    #[derive(Default)]
    pub struct FakeSupervisor {
        /// 부른 순서대로 `"메서드:인자"`.
        pub calls: RefCell<Vec<String>>,
        /// 세션 상태 — `status` 가 이것을 낸다.
        pub state: RefCell<String>,
        /// 모든 호출을 이 사유로 거절한다.
        pub fail: Option<ClientError>,
        /// 이 id 의 `delete` 만 거절한다 — **부분 실패**를 재려고 둔다.
        /// `fail` 은 전부를 막아 *"하나만 실패했을 때 나머지가 가나"* 를
        /// 못 잰다.
        pub fail_delete: RefCell<Vec<String>>,
        /// `list` 가 낼 세션 행 — 데몬이 낸 원본 JSON 그대로(제품 파서를 지난다).
        pub rows: RefCell<Vec<Value>>,
        /// agent `dev` 프로필의 환경 — 대화 기록을 찾는 설정 폴더(`CLAUDE_CONFIG_DIR`)를 넣는다.
        pub dev_env: RefCell<std::collections::BTreeMap<String, String>>,
        /// agent `dev` 프로필의 작업 폴더 · 추가 폴더.
        pub dev_cwd: RefCell<Option<String>>,
        pub dev_add_dirs: RefCell<Vec<String>>,
        /// `spawn` 응답에 `reused: true` 를 싣는다 — 데몬이 끊긴 기존 기록을 되살린 경우(protocol 9).
        pub spawn_reused: std::cell::Cell<bool>,
        /// `status` 가 실을 대화 id — 있으면 agent `dev` 로 그 대화를 붙든 세션이다.
        pub agent_session: RefCell<Option<String>>,
        /// `status` 의 `turn_active` — 켜면 턴이 도는 세션이다.
        pub turn_active: std::cell::Cell<bool>,
        /// 마지막 `spawn` 이 `permission_prompts` 를 실었나.
        pub prompts_requested: std::cell::Cell<bool>,
        /// `status` 가 실을 `needs_input` — 있으면 물음을 기다리는 세션이다.
        pub needs_input: RefCell<Option<Value>>,
        /// `status` 원문에 덧붙일 칸 — 복제가 읽는 기동 설정(`cwd`·`permission_mode`·`add_dirs` 등).
        pub status_extra: RefCell<Option<Value>>,
        /// 마지막 `spawn` 의 `(model, permission_mode, add_dirs)`.
        pub last_spawn: RefCell<Option<SpawnArgs>>,
    }

    impl FakeSupervisor {
        pub fn new() -> Self {
            Self {
                state: RefCell::new("running".into()),
                ..Default::default()
            }
        }

        pub fn failing(msg: &str) -> Self {
            Self {
                fail: Some(ClientError::Unavailable(msg.to_string())),
                ..Self::new()
            }
        }

        /// 이 id 의 `delete` 만 실패하게 한다.
        pub fn fail_delete_for(&self, id: &str) {
            self.fail_delete.borrow_mut().push(id.to_string());
        }

        /// 지우려고 **시도한** 순서 — 성공·실패를 가리지 않는다.
        pub fn deleted(&self) -> Vec<String> {
            self.calls
                .borrow()
                .iter()
                .filter_map(|c| c.strip_prefix("delete:").map(str::to_string))
                .collect()
        }

        fn note(&self, what: &str) -> Result<(), ClientError> {
            self.calls.borrow_mut().push(what.to_string());
            match &self.fail {
                Some(ClientError::Unavailable(m)) => Err(ClientError::Unavailable(m.clone())),
                Some(ClientError::Daemon { kind, detail }) => Err(ClientError::Daemon {
                    kind: kind.clone(),
                    detail: detail.clone(),
                }),
                Some(ClientError::TimedOut(d)) => Err(ClientError::TimedOut(*d)),
                None => Ok(()),
            }
        }

        /// 부른 메서드 이름만 — 순서 대조용.
        pub fn methods(&self) -> Vec<String> {
            self.calls
                .borrow()
                .iter()
                .map(|c| c.split(':').next().unwrap_or("").to_string())
                .collect()
        }
    }

    impl Supervisor for FakeSupervisor {
        fn endpoint(&self) -> String {
            "/fake/port_file".into()
        }

        fn agent_list(
            &self,
        ) -> Result<Vec<crate::adapters::driven::store::agents::AgentProfile>, ClientError>
        {
            self.note("agent_list")?;
            Ok(vec![
                crate::adapters::driven::store::agents::AgentProfile {
                    name: "dev".into(),
                    kind: "claude".into(),
                    cwd: self.dev_cwd.borrow().clone(),
                    add_dirs: self.dev_add_dirs.borrow().clone(),
                    env: self.dev_env.borrow().clone(),
                    ..Default::default()
                },
                crate::adapters::driven::store::agents::AgentProfile {
                    name: "sdk".into(),
                    kind: "pi".into(),
                    cwd: Some("/tmp/w".into()),
                    env: Default::default(),
                    ..Default::default()
                },
            ])
        }

        fn list(&self) -> Result<(Vec<SessionRow>, usize), ClientError> {
            self.note("list")?;
            let rows: Vec<SessionRow> = self
                .rows
                .borrow()
                .iter()
                .cloned()
                .map(SessionRow::from_raw_for_test)
                .collect();
            Ok((rows, 0))
        }

        fn status(&self, id: &str) -> Result<Value, ClientError> {
            self.note(&format!("status:{id}"))?;
            let mut v = serde_json::json!({
                "id": id,
                "state": *self.state.borrow(),
                "reclaimed": *self.state.borrow() == "stopped",
                "turn_active": self.turn_active.get(),
            });
            if let Some(n) = self.needs_input.borrow().as_ref() {
                v["needs_input"] = n.clone();
            }
            if let Some(c) = self.agent_session.borrow().as_ref() {
                v["agent_name"] = "dev".into();
                v["agent_session_id"] = c.as_str().into();
            }
            if let Some(Value::Object(extra)) = self.status_extra.borrow().as_ref() {
                for (k, x) in extra {
                    v[k] = x.clone();
                }
            }
            Ok(v)
        }

        fn stop(&self, id: &str) -> Result<(), ClientError> {
            self.note(&format!("stop:{id}"))?;
            *self.state.borrow_mut() = "stopped".into();
            Ok(())
        }

        fn delete(&self, id: &str) -> Result<(), ClientError> {
            self.note(&format!("delete:{id}"))?;
            // 시도는 기록한 **뒤에** 거절한다 — 안 그러면 "무엇을
            // 시도했나" 가 안 남아 부분 실패를 못 잰다.
            if self.fail_delete.borrow().iter().any(|x| x == id) {
                return Err(ClientError::Daemon {
                    kind: "invalid_state".into(),
                    detail: format!("{id} 는 지울 수 없다(가짜)"),
                });
            }
            Ok(())
        }

        fn abort(&self, id: &str) -> Result<(), ClientError> {
            self.note(&format!("abort:{id}"))
        }

        fn add_dir(&self, id: &str, path: &str) -> Result<Value, ClientError> {
            self.note(&format!("add_dir:{id}:{path}"))?;
            Ok(
                serde_json::json!({ "add_dirs": { "dirs": [path], "changed": true, "applied": true } }),
            )
        }

        fn remove_dir(&self, id: &str, path: &str) -> Result<Value, ClientError> {
            self.note(&format!("remove_dir:{id}:{path}"))?;
            Ok(serde_json::json!({ "add_dirs": { "dirs": [], "changed": true, "applied": false } }))
        }

        fn respond(
            &self,
            id: &str,
            answer: &crate::application::prompts::Answer,
        ) -> Result<(), ClientError> {
            self.note(&format!(
                "respond:{id}:{}:{}:{}",
                answer.request_id,
                answer.decision,
                answer
                    .updated_input
                    .as_ref()
                    .map(Value::to_string)
                    .unwrap_or_default()
            ))
        }

        fn spawn(&self, opts: &SpawnOpts<'_>) -> Result<Value, ClientError> {
            self.prompts_requested.set(opts.permission_prompts);
            *self.last_spawn.borrow_mut() = Some((
                opts.model.map(str::to_string),
                opts.permission_mode.map(str::to_string),
                opts.add_dirs.to_vec(),
            ));
            self.note(&format!("spawn:{}:{}", opts.agent, opts.cwd.unwrap_or("-")))?;
            if let Some(a) = opts.adopt {
                self.calls.borrow_mut().push(format!("adopt:{a}"));
            }
            let mut v = serde_json::json!({ "id": "새세션", "agent_name": opts.agent });
            if self.spawn_reused.get() {
                v["reused"] = serde_json::Value::Bool(true);
            }
            Ok(v)
        }

        fn send_input(&self, id: &str, text: &str) -> Result<Value, ClientError> {
            self.note(&format!("send_input:{id}:{text}"))?;
            // 회수돼 있었으면 **되살아난다** — 그쪽 자동 재개를 흉내낸다.
            *self.state.borrow_mut() = "running".into();
            Ok(serde_json::json!({ "output": { "text": "답" } }))
        }

        fn broadcast(&self, text: &str, agent: Option<&str>) -> Result<Value, ClientError> {
            self.note(&format!("broadcast:{}:{text}", agent.unwrap_or("*")))?;
            Ok(serde_json::json!({ "sent": ["a"], "failed": [] }))
        }

        fn output(&self, id: &str, lines: Option<usize>) -> Result<Value, ClientError> {
            self.note(&format!("output:{id}:{}", lines.unwrap_or(0)))?;
            Ok(serde_json::json!({ "lines": ["가짜 출력"], "total": 1 }))
        }

        fn watch(
            &self,
            id: &str,
            _max_events: usize,
            _until_finished: bool,
            _budget: std::time::Duration,
        ) -> Result<(Vec<Value>, bool), ClientError> {
            self.note(&format!("watch:{id}"))?;
            Ok((vec![serde_json::json!({ "phase": "finished" })], true))
        }

        /// 프레임을 낼 자식이 없다 — 보낸 쪽을 바로 놓아 받는 쪽이 끊김을 본다.
        fn subscribe(&self, id: &str) -> Result<std::sync::mpsc::Receiver<Value>, ClientError> {
            self.note(&format!("subscribe:{id}"))?;
            let (_tx, rx) = std::sync::mpsc::channel();
            Ok(rx)
        }
    }
}
