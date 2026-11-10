#![forbid(unsafe_code)]

//! Tasty 마크다운 공동 편집 plugin — 사용자 1 명과 에이전트가 같은 로컬 `.md` 를 함께 고친다.
//!
//! - 사용자: webview surface(`mdedit` kind)에서 미리보기 ↔ 원문 편집, 저장, 체크박스 토글,
//!   외부 변경 검토(hunk 별 유지·되돌리기), conflict 해결, 스냅샷 복원.
//! - 에이전트: `mdedit.*` IPC + `tasty mdedit …` CLI 로 헤딩 경로를 지정해 섹션을 읽고 쓴다
//!   (`--if-match` 낙관적 잠금). 대상은 파일 경로이고 편집기가 열려 있지 않아도 된다.
//!
//! 결정과 이유: docs/surfaces/mdedit/index.md#설계-결정 (웹뷰 입력 채널 · namespace · 사이드카 가드 · 방어선 · 렌더러 공유).
//! 사용 문서: `docs/surfaces/mdedit/index.md`.

// 이유: 테스트 본문의 `let _ =` 는 정책이 사유를 요구하지 않는 자리라
// `clippy::let_underscore_must_use` 명부에 섞이면 안 된다(docs/dev-guide/error-handling.md).
#![cfg_attr(test, allow(clippy::let_underscore_must_use))]

mod bridge;
mod core;
mod ipc;
mod page;
#[cfg(test)]
mod pin;
mod preview;
mod session;
mod view;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use serde_json::{Value, json};
use tasty_plugin_sdk::file_watch::{self, ContentDigest, WatchCmd};
use tasty_plugin_sdk::{
    BusHandle, CommandInvokeCtx, EventDispatchCtx, HostHandle, IpcMethodCtx, IpcMethodError,
    Plugin, PluginEnv, SurfaceCreateCtx, SurfaceRestoreCtx, SurfaceResult, SurfaceSnapshotCtx,
    Translator, WebviewNavigationAttemptCtx,
};

use crate::core::snapshot::{Retention, SnapshotStore};
// 미리보기는 markdown 렌더러를 그 소스 자리에서 컴파일한다(preview.rs 의 #[path]). 그 소스의
// 테스트는 자기 크레이트 기준 `crate::render` 를 쓰므로, 시험 빌드에서만 같은 이름을 루트에 둔다.
#[cfg(test)]
pub(crate) use crate::preview::render;
use crate::session::{Mode, Session};

const PLUGIN_ID: &str = "com.tasty.mdedit";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");
const THEME_CHANGED_EVENT: &str = "theme.changed";
pub(crate) const RELOAD_METHOD: &str = "mdedit.reload";

pub(crate) struct MdEditPlugin {
    pub sessions: HashMap<u32, Session>,
    pub host: Option<HostHandle>,
    /// 이 plugin 의 UI 문자열(`mdedit.*`).
    pub tr: Translator,
    /// 공유 렌더러가 쓰는 markdown plugin 의 카탈로그(`markdown.*`, docs/surfaces/mdedit/index.md#렌더러-제자리-컴파일).
    pub md_tr: Translator,
    pub store: SnapshotStore,
    watch_tx: Option<mpsc::Sender<WatchCmd>>,
}

/// 경로를 비교 가능한 한 형태로 — 같은 파일을 여는 편집기와 IPC 대상을 맞춘다.
pub(crate) fn canonical(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

impl MdEditPlugin {
    fn new(env: &PluginEnv) -> Self {
        let data = env
            .data_dir
            .clone()
            // 이유: data dir 은 host 가 spawn 할 때 늘 준다 — 없는 것은 host 밖에서 손으로 띄운
            // 경우뿐이고, 그때 스냅샷은 그 실행의 임시 산출물이라 인스턴스끼리 겹쳐도 잃을 것이 없다.
            .unwrap_or_else(|| std::env::temp_dir().join(PLUGIN_ID));
        Self {
            sessions: HashMap::new(),
            host: None,
            tr: Translator::from_plugin_env(env),
            md_tr: preview::markdown_translator(env.plugin_dir.as_deref(), &env.locale),
            store: SnapshotStore::new(data.join("snapshots")),
            watch_tx: None,
        }
    }

    /// 설정 창의 스냅샷 보존값(개수·일수). 못 읽으면 기본값.
    pub fn retention(&self) -> Retention {
        let d = Retention::default();
        let Some(host) = &self.host else {
            return d;
        };
        let num = |key: &str| -> Option<f64> {
            let v = host
                .call("settings.get_plugin_setting", json!({ "storage_key": key }))
                .map_err(|e| tracing::debug!("mdedit: setting {key} unavailable: {e}"))
                .ok()?;
            let inner = v.get("value")?;
            inner
                .get("Number")
                .and_then(Value::as_f64)
                .or_else(|| inner.as_f64())
        };
        Retention {
            keep_count: num("snapshot_keep_count").map_or(d.keep_count, |n| n.max(1.0) as usize),
            keep_days: num("snapshot_keep_days").map_or(d.keep_days, |n| n.max(1.0) as u64),
        }
    }

    fn open(&mut self, surface_id: u32, file: Option<String>, mode: Option<&str>) -> SurfaceResult {
        let Some(file) = file else {
            tracing::warn!("mdedit surface {surface_id}: no 'file' param — nothing to edit");
            return SurfaceResult::default();
        };
        let path = canonical(Path::new(&file));
        let mut s = Session::open(path.clone());
        if mode == Some("source") {
            s.mode = Mode::Source;
        }
        self.sessions.insert(surface_id, s);
        if let Some(tx) = &self.watch_tx
            && tx
                .send(WatchCmd::Register {
                    surface_id,
                    path: Some(path.to_string_lossy().into_owned()),
                })
                .is_err()
        {
            tracing::warn!("mdedit watch: register failed for surface {surface_id}");
        }
        self.reload_page(surface_id);
        SurfaceResult {
            display_name: None,
            snapshot: Some(json!({ "file": path.to_string_lossy() })),
        }
    }
}

fn file_param(v: &Value) -> Option<String> {
    v.get("params")
        .and_then(|p| p.get("file"))
        .or_else(|| v.get("file"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

impl Plugin for MdEditPlugin {
    fn id(&self) -> &str {
        PLUGIN_ID
    }

    fn version(&self) -> &str {
        PLUGIN_VERSION
    }

    fn on_start(&mut self, host: HostHandle, bus: BusHandle) {
        if let Err(e) = bus.subscribe(THEME_CHANGED_EVENT) {
            tracing::warn!("mdedit: theme.changed subscribe failed: {e}");
        }
        self.host = Some(host.clone());
        // 외부 쓰기 감지 — markdown 과 같은 SDK 감시기(내용 지문)를 쓰고, 변경이면 자기
        // `mdedit.reload` 를 self_invoke 한다. 실제 읽기·병합은 그 메서드 하나로 모인다.
        let (tx, rx) = mpsc::channel();
        self.watch_tx = Some(tx);
        if let Err(e) = std::thread::Builder::new()
            .name("mdedit-watch".to_string())
            .spawn(move || file_watch::run::<ContentDigest>(host, rx, RELOAD_METHOD))
        {
            tracing::warn!("mdedit watch worker spawn failed — external changes go unseen: {e}");
        }
    }

    fn create_surface(&mut self, ctx: SurfaceCreateCtx) -> SurfaceResult {
        let mode = ctx
            .params
            .get("params")
            .and_then(|p| p.get("mode"))
            .and_then(Value::as_str)
            .map(str::to_string);
        self.open(ctx.surface_id, file_param(&ctx.params), mode.as_deref())
    }

    fn restore_surface(&mut self, ctx: SurfaceRestoreCtx) -> SurfaceResult {
        let file = ctx
            .data
            .get("file")
            .and_then(Value::as_str)
            .map(str::to_string);
        self.open(ctx.surface_id, file, None)
    }

    fn snapshot_surface(&mut self, ctx: SurfaceSnapshotCtx) -> Value {
        self.sessions
            .get(&ctx.surface_id)
            .map_or(Value::Null, |s| json!({ "file": s.path.to_string_lossy() }))
    }

    fn destroy_surface(&mut self, surface_id: u32) {
        self.sessions.remove(&surface_id);
        if let Some(tx) = &self.watch_tx
            && tx.send(WatchCmd::Unregister { surface_id }).is_err()
        {
            tracing::warn!("mdedit watch: unregister failed for surface {surface_id}");
        }
    }

    fn handle_ipc_method(&mut self, ctx: IpcMethodCtx) -> Result<Value, IpcMethodError> {
        self.dispatch_ipc(&ctx.method, &ctx.params)
    }

    fn handle_command(&mut self, ctx: CommandInvokeCtx) -> SurfaceResult {
        if ctx.command_id == "save" && self.sessions.contains_key(&ctx.surface_id) {
            self.post(ctx.surface_id, &page::save_msg());
        }
        SurfaceResult::default()
    }

    fn on_event(&mut self, ctx: EventDispatchCtx) {
        if ctx.envelope.key == THEME_CHANGED_EVENT {
            let ids: Vec<u32> = self.sessions.keys().copied().collect();
            for id in ids {
                self.reload_page(id);
            }
        }
    }

    fn on_webview_navigation_attempt(&mut self, ctx: WebviewNavigationAttemptCtx) {
        self.on_navigation(ctx.surface_id, &ctx.url);
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let env = PluginEnv::load()?;
    tasty_plugin_sdk::run(MdEditPlugin::new(&env))
}

#[cfg(test)]
mod manifest_tests {
    /// 매니페스트가 host 검증기를 통과한다 — 통과 못 하면 plugin 이 아예 뜨지 않는다.
    #[test]
    fn manifest_validates_and_versions_match() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let m = tasty_plugin_manifest::Manifest::load(dir).expect("manifest loads");
        m.validate().expect("manifest validates");
        assert_eq!(m.id, super::PLUGIN_ID);
        assert_eq!(m.version, super::PLUGIN_VERSION);
    }
}

#[cfg(test)]
mod lang_tests {
    use std::collections::BTreeSet;

    fn keys(v: &toml::Value, prefix: &str, out: &mut BTreeSet<String>) {
        if let Some(t) = v.as_table() {
            for (k, v) in t {
                let p = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                if v.is_table() {
                    keys(v, &p, out);
                } else {
                    out.insert(p);
                }
            }
        }
    }

    fn load(lang: &str) -> BTreeSet<String> {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("lang/{lang}.toml"));
        let v: toml::Value = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let mut out = BTreeSet::new();
        keys(&v, "", &mut out);
        out
    }

    /// 세 언어가 같은 키 집합을 갖고, 소스·매니페스트가 쓰는 키가 전부 en 에 있다.
    #[test]
    fn three_languages_share_keys_and_cover_every_use() {
        let en = load("en");
        assert_eq!(en, load("ko"), "ko differs from en");
        assert_eq!(en, load("ja"), "ja differs from en");
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        // 소스: UI 문자열 키(IPC 메서드 이름·권한은 키가 아니다). 매니페스트: `*_key = "…"` 값.
        let ui = regex::Regex::new(
            r#""(mdedit\.(?:agent_badge|toolbar|status|banner|conflict|review|origin|snapshots)[a-z_.]*)""#,
        )
        .unwrap();
        let manifest_key = regex::Regex::new(
            r#"(?:i18n_key|label_key|title_key|suffix_key|placeholder_key) = "([^"]+)""#,
        )
        .unwrap();
        let mut used = BTreeSet::new();
        for f in ["src/page.rs", "src/session.rs", "src/view.rs"] {
            let text = std::fs::read_to_string(dir.join(f)).unwrap();
            used.extend(ui.captures_iter(&text).map(|c| c[1].to_string()));
        }
        let text = std::fs::read_to_string(dir.join("tasty-plugin.toml")).unwrap();
        used.extend(manifest_key.captures_iter(&text).map(|c| c[1].to_string()));
        for k in crate::page::INLINE_KEYS {
            used.insert(format!("mdedit.inline.{k}"));
        }
        for r in ["external", "agent", "save", "restore"] {
            used.insert(format!("mdedit.snapshots.reason.{r}"));
        }
        // `format!` 로 잇는 접두(`mdedit.snapshots.reason.`)는 키가 아니다.
        used.retain(|k| !k.ends_with('.'));
        let missing: Vec<_> = used.difference(&en).collect();
        assert!(
            missing.is_empty(),
            "keys missing from lang/en.toml: {missing:?}"
        );
    }
}
