#![forbid(unsafe_code)]

//! Tasty Claude Code plugin — 외부 plugin.
//!
//! `tasty claude launch|spawn|children|parent|tell|broadcast|kill|respawn|install|uninstall|hook|notify-done|notify-error`
//! CLI 세트를 제공한다.
//!
//! 자식 terminal 관리(registry·spawn·wait·kill·reconcile·soft 점유)는 호스트가
//! 내재화한 `terminal.*` IPC(ADR-0040 / occupancy-04)로 위임한다 — 이 plugin 은
//! 자체 child registry 를 보유하지 않는다(호스트 registry 가 단일 SoT). claude
//! 특화(session token 기동, hook fan-out, PTY error scan, install, 텔레메트리)만
//! 여기 남는다.
//!
//! 호스트 코드에는 의존하지 않으며 `tasty-plugin-sdk`만 사용한다.

mod auto_resume;
mod checklist;
mod error_scan;
mod gate;
mod handlers;
mod hook;
mod install;
mod notifications;
mod profile;
mod profile_attach;
mod profile_merge;
mod reboot;
mod state;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use error_scan::{ErrorScanner, scan_target_is_alive};
use handlers::*;
use serde_json::{Value, json};
use state::ClaudeState;
use tasty_plugin_sdk::{
    HostHandle, IpcMethodCtx, IpcMethodError, Plugin, PluginEnv, SurfaceCreateCtx, SurfaceResult,
    i18n::Translator,
};

const PLUGIN_ID: &str = "com.tasty.claude";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");

/// PTY 에러 폴링 간격. 호스트 메모리 스캔(O(1))과의 정확도 차이를 좁히기 위해
/// 짧게. 추적 대상 N개에 대해 주기당 최대 2N IPC(생존 대조 + read_since_scan_mark;
/// 매치될 때만 fire_hook 이 추가된다)지만, N 이 10 이하인 일상 시나리오에서는
/// 무시 가능한 부하 (25 calls/sec @ 10 children).
const ERROR_SCAN_INTERVAL: Duration = Duration::from_millis(800);

struct ClaudePlugin {
    /// claude 특화 상태 — wall-time 텔레메트리 타이밍만(hook 이 소비). child
    /// registry 는 호스트 `terminal.*` 가 소유한다.
    state: ClaudeState,
    scanner: Arc<Mutex<ErrorScanner>>,
    /// API 에러로 끝난 턴의 재개 예약 — hook 핸들러가 쓰고 `claude-auto-resume` 스레드가
    /// 만기를 처리한다(`auto_resume.rs`).
    resume: Arc<Mutex<auto_resume::ResumeTable>>,
    /// 그 스레드가 쓸 문구. `on_start` 에서 스레드로 넘긴다.
    resume_texts: auto_resume::Texts,
    /// reboot 시퀀스 진행 중인 surface 집합 — 같은 surface 중복 reboot 가드.
    rebooting: Arc<Mutex<HashSet<u32>>>,
    /// Claude 세션 프로필 레지스트리·머지 산출물(`profile.rs`) + 게이트 레지스트리
    /// (`gate.rs`) + 게이트별 라운드 상태·마커 파일(`checklist.rs`)의 공용 저장
    /// 루트(`TASTY_PLUGIN_DATA_DIR`).
    /// 호스트가 비정상적으로 주입하지 않았으면 `None` — 두 모듈 모두 등록/부착/발동
    /// 요청을 명시적 에러(또는 checklist 는 안전한 통과)로 처리한다(결정 3: 조용히
    /// 다른 경로에 쓰지 않음).
    plugin_data_dir: Option<PathBuf>,
    /// host 기본 게이트(`continue-checklist`)가 block 결정 시 `reason` 으로 주입하는
    /// 본문. 활성 locale 로 이미 해석된 완성 문자열(`main()` 에서 `Translator` 로 1 회
    /// 계산) — 매 훅 발화마다 lang 파일을 다시 읽지 않는다. 등록 게이트의 본문은
    /// 사용자 파일이라 이 캐시를 쓰지 않고 발화마다 읽는다(`checklist.rs`).
    checklist_body: String,
    /// 사람이 읽는 IPC 에러/응답 문자열 번역용. `main()` 에서 1 회 로드해 재사용한다.
    translator: Translator,
}

impl ClaudePlugin {
    fn new(
        plugin_data_dir: Option<PathBuf>,
        checklist_body: String,
        translator: Translator,
    ) -> Self {
        let resume_texts = auto_resume::Texts {
            message: translator.t("claude.auto_resume.message").to_string(),
            limit_title: translator.t("claude.auto_resume.limit_title").to_string(),
            limit_body: translator.t("claude.auto_resume.limit_body").to_string(),
        };
        Self {
            state: ClaudeState::new(),
            scanner: Arc::new(Mutex::new(ErrorScanner::new())),
            resume: Arc::new(Mutex::new(auto_resume::ResumeTable::default())),
            resume_texts,
            rebooting: Arc::new(Mutex::new(HashSet::new())),
            plugin_data_dir,
            checklist_body,
            translator,
        }
    }
}

impl Plugin for ClaudePlugin {
    fn id(&self) -> &str {
        PLUGIN_ID
    }

    fn version(&self) -> &str {
        PLUGIN_VERSION
    }

    fn create_surface(&mut self, _ctx: SurfaceCreateCtx) -> SurfaceResult {
        // claude plugin은 자체 surface_kind를 등록하지 않는다 — 자식 Claude 프로세스는
        // 호스트의 일반 terminal surface에서 실행되며, surface 자체는 plugin이 직접
        // 만들지 않는다.
        SurfaceResult::default()
    }

    fn handle_ipc_method(&mut self, ctx: IpcMethodCtx) -> Result<Value, IpcMethodError> {
        match ctx.method.as_str() {
            "claude.hook" => hook::handle_claude_hook(
                &mut self.state,
                &self.scanner,
                &self.resume,
                &ctx.host,
                &ctx.params,
                self.plugin_data_dir.as_deref(),
                &self.translator,
            ),
            "claude.checklist_hook" => checklist::handle_checklist_hook(
                &ctx.host,
                self.plugin_data_dir.as_deref(),
                &self.checklist_body,
                &ctx.params,
                &self.translator,
            ),
            "claude.checklist_enable" => checklist::handle_enable(
                self.plugin_data_dir.as_deref(),
                &ctx.params,
                &self.translator,
            ),
            "claude.checklist_disable" => checklist::handle_disable(
                self.plugin_data_dir.as_deref(),
                &ctx.params,
                &self.translator,
            ),
            "claude.checklist_status" => checklist::handle_status(
                self.plugin_data_dir.as_deref(),
                &ctx.params,
                &self.translator,
            ),
            "claude.install" => match install::run_install(&self.translator) {
                Ok(added) => Ok(json!({ "installed": added })),
                Err(e) => Err(IpcMethodError::new(
                    self.translator
                        .t_fmt("claude.install.install_failed", &e.to_string()),
                )),
            },
            "claude.uninstall" => match install::run_uninstall(&self.translator) {
                Ok(removed) => Ok(json!({ "uninstalled": removed })),
                Err(e) => Err(IpcMethodError::new(
                    self.translator
                        .t_fmt("claude.install.uninstall_failed", &e.to_string()),
                )),
            },
            // 자식 관리 명령은 모두 호스트 `terminal.*` 로 위임(handlers.rs).
            "claude.parent" => handle_parent(&ctx.host, &ctx.params, &self.translator),
            "claude.state" => handle_state(&ctx.host, &ctx.params, &self.translator),
            "claude.children" => handle_children(&ctx.host, &ctx.params, &self.translator),
            "claude.kill" => handle_kill(&self.scanner, &ctx.host, &ctx.params, &self.translator),
            "claude.broadcast" => handle_broadcast(&ctx.host, &ctx.params, &self.translator),
            "claude.tell" => handle_tell(&ctx.host, &ctx.params, &self.translator),
            "claude.notify_done" => handle_notify_done(&ctx.host, &ctx.params, &self.translator),
            "claude.notify_error" => handle_notify_error(&ctx.host, &ctx.params, &self.translator),
            // launch/respawn/spawn — claude 특화 기동 명령을 host registry 위에 얹는다.
            "claude.launch" => handle_launch(
                &self.scanner,
                &ctx.host,
                &ctx.params,
                self.plugin_data_dir.as_deref(),
                &self.translator,
            ),
            "claude.respawn" => handle_respawn(
                &self.scanner,
                &ctx.host,
                &ctx.params,
                self.plugin_data_dir.as_deref(),
                &self.translator,
            ),
            "claude.spawn" => handle_spawn(
                &self.scanner,
                &ctx.host,
                &ctx.params,
                self.plugin_data_dir.as_deref(),
                &self.translator,
            ),
            "claude.reboot" => reboot::handle_reboot(
                &self.rebooting,
                &ctx.host,
                &ctx.params,
                self.plugin_data_dir.as_deref(),
                &self.translator,
            ),
            // 부모 → 자식 지속 프로필 부착. 대상 해석(`--child` → surface id)만
            // 다르고 그 뒤는 reboot 과 같은 진입점을 타므로 `rebooting` set 도
            // 그대로 넘긴다 — 같은 자식에 두 명령이 겹치면 뒤엣것이 거부된다.
            "claude.child_profile" => handle_child_profile(
                &self.rebooting,
                &ctx.host,
                &ctx.params,
                self.plugin_data_dir.as_deref(),
                &self.translator,
            ),
            // Claude 세션 프로필 레지스트리 — 등록/조회/해제/조합-해석. 전부
            // `TASTY_PLUGIN_DATA_DIR` 하위 저장(profile.rs, 결정 3).
            "claude.profile_register" => profile::handle_register(
                self.plugin_data_dir.as_deref(),
                &ctx.params,
                &self.translator,
            ),
            "claude.profile_unregister" => profile::handle_unregister(
                self.plugin_data_dir.as_deref(),
                &ctx.params,
                &self.translator,
            ),
            "claude.profile_list" => profile::handle_list(
                self.plugin_data_dir.as_deref(),
                &ctx.params,
                &self.translator,
            ),
            "claude.profile_show" => profile::handle_show(
                self.plugin_data_dir.as_deref(),
                &ctx.params,
                &self.translator,
            ),
            "claude.profile_current" => profile::handle_current(
                self.plugin_data_dir.as_deref(),
                &ctx.host,
                &ctx.params,
                &self.translator,
            ),
            // Stop-훅 게이트 레지스트리 — (본문, 센티넬, 라운드 상한) 3요소를
            // 이름으로 등록/조회/해제한다. 프로필과 이름 공간을 공유하므로
            // 양방향 동명 충돌은 등록 시점에 거부된다(gate.rs 모듈 doc).
            "claude.gate_register" => gate::handle_register(
                self.plugin_data_dir.as_deref(),
                &ctx.params,
                &self.translator,
            ),
            "claude.gate_unregister" => gate::handle_unregister(
                self.plugin_data_dir.as_deref(),
                &ctx.params,
                &self.translator,
            ),
            "claude.gate_list" => gate::handle_list(
                self.plugin_data_dir.as_deref(),
                &ctx.params,
                &self.translator,
            ),
            "claude.gate_show" => gate::handle_show(
                self.plugin_data_dir.as_deref(),
                &ctx.params,
                &self.translator,
            ),
            other => Err(IpcMethodError::not_found(other)),
        }
    }

    fn on_start(&mut self, host: HostHandle, _bus: tasty_plugin_sdk::BusHandle) {
        // PTY error scan 을 위한 background polling thread 만 띄운다. child registry
        // lifecycle(spawn/kill/reconcile)은 호스트가 소유하므로 여기서 하지 않는다 —
        // error_scan 은 launch surface(top-level)와 spawn/respawn 자식 모두에 대해
        // enable 되며, 추적 대상이 사라지면 이 스레드 자신이 매 tick 생존을 확인해
        // disable 한다(`error_scan_loop` / `error_scan::scan_target_is_alive` 참조).
        let scanner = self.scanner.clone();
        let resume = self.resume.clone();
        let resume_host = host.clone();
        let resume_texts = self.resume_texts.clone();
        // spawn 실패 시 패닉을 유지한다 — 호스트(tasty)가 아니라 **이 plugin
        // 프로세스만** 죽고, 호스트는 plugin 사망을 이미 감지·복구한다. 호스트 쪽
        // 스레드 spawn 이 에러 반환으로 바뀐 것과 대칭이 아닌 이유가 이것이다:
        // 실패 폭발 반경이 다르다(`docs/dev-guide/error-handling.md`).
        std::thread::Builder::new()
            .name("claude-error-scan".into())
            .spawn(move || error_scan_loop(scanner, host))
            .expect("spawn claude-error-scan thread");
        // 같은 이유로 패닉을 유지한다. 스캐너와 스레드를 나누는 이유: 만기 처리는
        // `terminal.tell` 을 부르고 그 호출은 제출 확인까지 수백 ms 를 쓴다 — 스캔 주기에
        // 얹으면 그동안 모든 surface 의 에러 스캔이 선다.
        std::thread::Builder::new()
            .name("claude-auto-resume".into())
            .spawn(move || auto_resume::run_loop(resume, resume_host, resume_texts))
            .expect("spawn claude-auto-resume thread");
    }
}

fn error_scan_loop(scanner: Arc<Mutex<ErrorScanner>>, host: HostHandle) {
    loop {
        std::thread::sleep(ERROR_SCAN_INTERVAL);
        // lock을 짧게 잡고 snapshot만 떠서 IPC 호출 동안 다른 메서드(enable/disable)가
        // 끼어들 수 있게 한다.
        // poison 이어도 루프를 접지 않는다 — 여기서 `return` 하면 스캐너가 sticky poison
        // 하나로 프로세스가 끝날 때까지 죽고, 그 사실이 로그 한 줄로만 남는다. 지키는
        // 자료구조가 복구 가능하므로 계속 돈다(보고는 첫 1 회, `lock_scanner` 안에서).
        let surfaces = crate::error_scan::lock_scanner(&scanner).enabled_snapshot();
        for (sid, target) in surfaces {
            // surface.closed 구독(ef57061d 로 제거됨) 대신, 이미 도는 폴링 주기에
            // 편승해 생존을 확인한다 — 사라진 대상은 disable 해 enabled/dedupe
            // 상태를 정리한다(최대 800ms 지연, 추가 구독 배선 없음). 판정 기준은
            // 등록 경로에 따라 다르다(`ScanTarget` 문서 참조).
            if !scan_target_is_alive(&host, sid, target) {
                crate::error_scan::lock_scanner(&scanner).disable(sid);
                continue;
            }
            // 반환값(매치된 snippet)은 단위 테스트용. polling 루프에서는 무시.
            crate::error_scan::lock_scanner(&scanner).scan_one(&host, sid);
        }
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    // `tasty_plugin_sdk::run()` 도 내부적으로 `PluginEnv::load()` 를 호출한다(연결/
    // 인증용) — 여기서 한 번 더 읽는 것은 단순 env var read 라 부작용 없이 중복
    // 가능하다. `data_dir` 이 없으면(비정상 기동) `None` 을 그대로 넘겨 profile.rs
    // 가 등록/부착을 명시적으로 거부하게 한다(결정 3).
    let env = PluginEnv::load().ok();
    let plugin_data_dir = env.as_ref().and_then(|e| e.data_dir.clone());
    // `env` 부재(비정상 기동)에도 `Translator::default()` 로 안전 폴백(키 그대로 반환).
    let translator = env
        .as_ref()
        .map(Translator::from_plugin_env)
        .unwrap_or_default();
    // 활성 locale 로 1 회 해석해 캐시한다 — 매 Stop 훅 발화마다 lang 파일을 다시
    // 읽지 않는다.
    let checklist_body = translator.t("claude.checklist.body").to_string();
    tasty_plugin_sdk::run(ClaudePlugin::new(
        plugin_data_dir,
        checklist_body,
        translator,
    ))
}
