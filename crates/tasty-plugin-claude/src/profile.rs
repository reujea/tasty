//! Claude 세션 프로필 레지스트리 — 이름으로 등록해 둔 `settings.json` 조각을
//! 세션(자기 자신 또는 자식)에 부착한다.
//!
//! `src/hook_handler/registry.rs` 의 형태(patch semantics · priority ↑ → owner
//! tie-break → id 정렬 · `<owner>/<short>` id)를 **미러링**한다 — 타입은
//! 공유하지 않는다 — plugin 이 host 타입에 묶이면 plugin 을 독립적으로 갱신할 수
//! 없어지고, 이 레지스트리의 소비자는 이 plugin 하나뿐이라 공유 이득도 없다.
//!
//! 실질 2 출처: host(내장 훅 전부를 조회 전용 항목으로 나열 — `install::MANAGED_HOOKS`
//! 가 유일한 정의처, 여기서 재정의하지 않는다) + user(사용자가 등록한 프로필의
//! 실체 JSON). plugin manifest 출처는 소비자가 이 plugin 하나뿐이라 비어 있다.
//! host/user 는 id 네임스페이스가 겹치지 않으므로(`host/*` vs `user/*`)
//! patch-fold 는 실제로는 단일 contribution 병합으로만 동작하지만, 형태는
//! 3출처 병합 코드와 동일하게 유지해 나중에 세 번째 출처가 필요해져도 재설계가
//! 없도록 한다.
//!
//! ## 이름으로 부착 가능한 것 — 프로필과 게이트
//!
//! 이름 해석은 등록 프로필만 보는 것이 아니다. Stop-훅 게이트([`crate::gate`])가
//! 같은 이름 평면을 공유하므로, 등록 프로필이 없으면 게이트로 fallback 해 그
//! 게이트를 발동시키는 `Stop` 훅 조각을 만들어 부착한다
//! ([`crate::gate::attach_profile`]). `continue-checklist` 도 이 경로를 타는 host
//! 기본 게이트 하나일 뿐이라, 이 파일에 그 이름이 박히는 자리는 없다.
//!
//! 해석 순서는 등록 프로필 → 게이트(등록 → host 기본) → 내장 훅 토큰(부착 불가
//! 에러) → 미등록 에러. 두 레지스트리가 등록 시점에 동명을 서로 거부하므로 앞의
//! 둘이 실제로 충돌하지는 않지만, 순서는 방어적으로 고정한다.
//!
//! 이 plugin 은 단일 스레드 IPC 디스패치(`ClaudePlugin::handle_ipc_method`)만
//! 레지스트리를 건드리므로 호스트 레지스트리들과 달리 `RwLock`/`OnceLock`
//! 프로세스 전역 싱글턴이 불필요하다 — `ClaudePlugin` 의 plain 필드로 충분하다
//! (`state.rs` 의 `ClaudeState` 와 동일 근거: 별도 스레드가 건드리지 않는다).
//!
//! ## 저장 위치 (결정 3)
//!
//! `TASTY_PLUGIN_DATA_DIR` 하위, 사용자 원본과 tasty 생성 산출물을 분리:
//! - `profiles/registered/<short-name>.json` — 등록 시 호출자가 준 파일의 복사본
//!   (원본이 나중에 옮겨지거나 지워져도 레지스트리는 안전). 실제 attachable 항목의
//!   유일한 소스.
//! - `profiles/generated/<sorted-names>.json` — 프로필 조합을 부착할 때마다
//!   재생성되는 머지 산출물. 캐시가 아니라 항상 최신 등록 내용을 반영하도록
//!   매 attach 시점에 다시 쓴다.
//!
//! `data_dir` 이 `None`(호스트가 주입하지 않은 비정상 기동)이면 등록/부착 모두
//! 명시적 에러로 거부한다 — 조용히 다른 경로에 쓰지 않는다(`~/.claude/` 나
//! 새 경로를 발명하지 않는다는 결정 3 그대로).

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tasty_plugin_sdk::{HostHandle, IpcMethodError, i18n::Translator};
use tracing::warn;

use crate::install::MANAGED_HOOKS;
use crate::profile_merge::{MergeError, merge_contents};

/// 프로필 short-name 규칙 — `hook_handler` short-name 규칙과 동일한 형태를
/// 미러링(소문자/숫자/하이픈, 최대 32자). 파일명으로도 그대로 쓰이므로 경로
/// traversal 문자(`/`, `..`)를 원천적으로 배제한다.
fn is_valid_short_name(s: &str) -> bool {
    if s.is_empty() || s.len() > 32 {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileError {
    /// 호스트가 `TASTY_PLUGIN_DATA_DIR` 를 주입하지 않은 비정상 기동.
    NoDataDir,
    InvalidShortName(String),
    /// 이름 목록이 비어 있음(`--profile ""` 등).
    EmptyNames,
    UnknownProfile(String),
    /// 조회 전용 항목(내장 훅)은 이름으로 부착할 수 없다.
    NotAttachable(String),
    /// 동명 Stop-훅 게이트가 이미 등록돼 있다. 게이트도 이름으로 부착되므로
    /// (`gate.rs` 모듈 doc) 두 레지스트리는 이름 공간을 공유한다 — 같은 이름을
    /// 양쪽에 두면 조용히 한쪽이 가려지므로 등록 시점에 거부한다.
    GateNameConflict(String),
    SourceNotReadable {
        path: String,
        message: String,
    },
    SourceNotJsonObject {
        path: String,
    },
    Merge(MergeError),
    Io {
        path: String,
        message: String,
    },
}

impl ProfileError {
    pub(crate) fn translate(&self, tr: &Translator) -> String {
        match self {
            Self::NoDataDir => tr.t("claude.profile.no_data_dir").to_string(),
            Self::InvalidShortName(s) => tr.t_fmt("claude.profile.invalid_short_name", s),
            Self::EmptyNames => tr.t("claude.profile.empty_names").to_string(),
            Self::UnknownProfile(id) => tr.t_fmt("claude.profile.unknown_profile", id),
            Self::NotAttachable(id) => tr.t_fmt("claude.profile.not_attachable", id),
            // 두 자리 placeholder — `t_fmt` 는 첫 하나만 채운다(`gate.rs` 의 대칭 항목과 동일).
            Self::GateNameConflict(name) => tr
                .t("claude.profile.gate_name_conflict")
                .replacen("{}", name, 1)
                .replacen("{}", name, 1),
            Self::SourceNotReadable { path, message } => tr
                .t("claude.profile.source_not_readable")
                .replacen("{}", path, 1)
                .replacen("{}", message, 1),
            Self::SourceNotJsonObject { path } => {
                tr.t_fmt("claude.profile.source_not_json_object", path)
            }
            Self::Merge(e) => e.translate(tr),
            Self::Io { path, message } => tr
                .t("claude.profile.io_error")
                .replacen("{}", path, 1)
                .replacen("{}", message, 1),
        }
    }
}

/// `claude profile list` 가 반환하는 항목 요약. IPC 응답으로는 `handlers.rs`
/// 가 `serde_json::json!` 로 직접 변환한다(이 crate 는 `serde` derive 를
/// 직접 의존하지 않는다 — `serde_json` 만으로 충분).
#[derive(Debug, Clone)]
pub struct ProfileSummary {
    /// `<owner>/<short>` 형식.
    pub id: String,
    pub owner: &'static str,
    /// 이름으로 attach 가능한지. 내장 훅 listing 항목은 `false`.
    pub attachable: bool,
    pub description: Option<String>,
}

fn registered_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("profiles").join("registered")
}

fn generated_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("profiles").join("generated")
}

fn registered_file(data_dir: &Path, short_name: &str) -> PathBuf {
    registered_dir(data_dir).join(format!("{short_name}.json"))
}

fn require_data_dir(data_dir: Option<&Path>) -> Result<&Path, ProfileError> {
    data_dir.ok_or(ProfileError::NoDataDir)
}

/// 이 이름으로 등록된 사용자 프로필이 있는가 — `gate::register` 가 반대 방향
/// 충돌을 검사할 때 쓴다(이름 공간이 한 평면이라는 계약의 대칭 절반,
/// `gate.rs` 모듈 doc 참조).
pub(crate) fn is_registered(data_dir: Option<&Path>, short_name: &str) -> bool {
    data_dir.is_some_and(|d| registered_file(d, short_name).is_file())
}

fn parse_names(names_csv: &str) -> Result<Vec<String>, ProfileError> {
    let names: Vec<String> = names_csv
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if names.is_empty() {
        return Err(ProfileError::EmptyNames);
    }
    for n in &names {
        if !is_valid_short_name(n) {
            return Err(ProfileError::InvalidShortName(n.clone()));
        }
    }
    Ok(names)
}

// ── 등록/조회/해제 (user 출처) ────────────────────────────────────────────

/// `short_name` 으로 프로필을 등록한다. `source_path` 의 내용을 읽어 JSON
/// object 인지 검증한 뒤 `registered/<short_name>.json` 에 복사본을 쓴다 —
/// 원본이 나중에 옮겨지거나 지워져도 레지스트리는 영향받지 않는다.
/// 이미 등록된 이름이면 내용을 덮어쓴다(재등록으로 갱신하는 것이 정상 사용).
pub(crate) fn register(
    data_dir: Option<&Path>,
    short_name: &str,
    source_path: &Path,
) -> Result<(), ProfileError> {
    if !is_valid_short_name(short_name) {
        return Err(ProfileError::InvalidShortName(short_name.to_string()));
    }
    let data_dir = require_data_dir(data_dir)?;
    // 이름 공간이 게이트와 한 평면이다 — 조용히 가리지 않고 여기서 거부한다.
    if crate::gate::is_registered(Some(data_dir), short_name) {
        return Err(ProfileError::GateNameConflict(short_name.to_string()));
    }
    let text =
        std::fs::read_to_string(source_path).map_err(|e| ProfileError::SourceNotReadable {
            path: source_path.display().to_string(),
            message: e.to_string(),
        })?;
    let value: Value =
        serde_json::from_str(&text).map_err(|e| ProfileError::SourceNotReadable {
            path: source_path.display().to_string(),
            message: e.to_string(),
        })?;
    if !value.is_object() {
        return Err(ProfileError::SourceNotJsonObject {
            path: source_path.display().to_string(),
        });
    }
    let dir = registered_dir(data_dir);
    std::fs::create_dir_all(&dir).map_err(|e| ProfileError::Io {
        path: dir.display().to_string(),
        message: e.to_string(),
    })?;
    let dest = registered_file(data_dir, short_name);
    std::fs::write(&dest, text).map_err(|e| ProfileError::Io {
        path: dest.display().to_string(),
        message: e.to_string(),
    })?;
    Ok(())
}

/// `short_name` 등록을 해제한다. 없으면 `UnknownProfile`.
pub(crate) fn unregister(data_dir: Option<&Path>, short_name: &str) -> Result<(), ProfileError> {
    let data_dir = require_data_dir(data_dir)?;
    let path = registered_file(data_dir, short_name);
    if !path.is_file() {
        return Err(ProfileError::UnknownProfile(format!("user/{short_name}")));
    }
    std::fs::remove_file(&path).map_err(|e| ProfileError::Io {
        path: path.display().to_string(),
        message: e.to_string(),
    })?;
    Ok(())
}

/// 등록된 프로필 원본 JSON 을 반환한다(`claude profile show`). 반환하는 owner
/// prefix(`"user"`/`"host"`)는 실제로 어느 출처에서 읽었는지를 그대로 반영한다 —
/// 사용자 등록이 없어 host 기본값으로 fallback 됐는데도 `"user/..."` 라고 답하면
/// 호출자가 실체와 다른 id 로 착각한다.
pub(crate) fn show_registered(
    data_dir: Option<&Path>,
    short_name: &str,
    tr: &Translator,
) -> Result<(&'static str, Value), ProfileError> {
    let data_dir = require_data_dir(data_dir)?;
    let path = registered_file(data_dir, short_name);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        // 등록 프로필이 없으면 게이트로 fallback(등록 프로필 우선). owner 는
        // 게이트 쪽이 실제 출처(user 등록 게이트 / host 기본 게이트)를 돌려준다.
        Err(_) => {
            if let Some((owner, value)) =
                crate::gate::attach_profile(Some(data_dir), short_name, tr)
            {
                return Ok((owner, value));
            }
            return Err(ProfileError::UnknownProfile(format!("user/{short_name}")));
        }
    };
    let value = serde_json::from_str(&text).map_err(|e| ProfileError::Io {
        path: path.display().to_string(),
        message: e.to_string(),
    })?;
    Ok(("user", value))
}

/// 등록된 사용자 프로필 전체 + 내장 훅 listing 항목을 함께 나열한다
/// (`priority` 개념이 필요 없는 단순 목록이라 owner tie-break 없이 owner→id
/// 순으로만 정렬 — host 를 먼저 보여줘 "항상 켜져 있는 것"이 먼저 눈에 띄게 한다).
pub(crate) fn list(data_dir: Option<&Path>, tr: &Translator) -> Vec<ProfileSummary> {
    let mut out: Vec<ProfileSummary> = MANAGED_HOOKS
        .iter()
        .map(|(claude_event, token, matcher)| ProfileSummary {
            id: format!("host/{token}"),
            owner: "host",
            attachable: false,
            description: Some(if matcher.is_empty() {
                tr.t_fmt("claude.profile.builtin_always_installed", claude_event)
            } else {
                tr.t("claude.profile.builtin_always_installed_with_matcher")
                    .replacen("{}", claude_event, 1)
                    .replacen("{}", matcher, 1)
            }),
        })
        .collect();

    for name in crate::gate::host_default_names() {
        out.push(ProfileSummary {
            id: format!("host/{name}"),
            owner: "host",
            attachable: true,
            description: Some(tr.t("claude.profile.gate_attachable").to_string()),
        });
    }

    if let Some(data_dir) = data_dir {
        let profile_names = short_names_in(&registered_dir(data_dir));
        for short in &profile_names {
            out.push(ProfileSummary {
                id: format!("user/{short}"),
                owner: "user",
                attachable: true,
                description: None,
            });
        }
        // 등록 게이트도 이름으로 부착 가능하므로 함께 나열한다. 등록 프로필과
        // 구분되도록 설명을 싣는다(등록 프로필은 `description: None`).
        // `profile-list` 와 `gate-list` 가 둘 다 게이트를 보여주는 것은 의도된
        // 중복이다 — 전자는 "부착 가능한 것들", 후자는 "게이트 정의" 관점.
        for short in crate::gate::registered_names(data_dir) {
            // 두 레지스트리가 동명 등록을 서로 거부하므로 정상적으로는 겹치지
            // 않지만, 겹친 상태에서는 부착이 프로필을 택하므로 목록도 그 실체를
            // 따른다(같은 id 가 두 줄로 보이지 않게).
            if profile_names.contains(&short) {
                continue;
            }
            out.push(ProfileSummary {
                id: format!("user/{short}"),
                owner: "user",
                attachable: true,
                description: Some(tr.t("claude.profile.gate_attachable").to_string()),
            });
        }
    }
    out
}

/// 디렉토리의 `<이름>.json` 파일 이름을 정렬해 돌려준다.
fn short_names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            name.strip_suffix(".json").map(str::to_string)
        })
        .collect();
    names.sort();
    names
}

// ── 부착용 해석(조합 머지 포함) ────────────────────────────────────────────

/// `names_csv`(쉼표 구분 이름 목록)를 등록된 프로필 파일들로 해석하고, 필요하면
/// 머지해 `generated/` 에 실체화한 뒤 그 경로를 반환한다. 이름이 하나뿐이어도
/// 항상 이 경로를 통해 `generated/` 에 다시 쓴다 — attach 경로가 단수/복수로
/// 갈라지지 않게 하기 위함(등록 내용이 바뀐 뒤 재부착 시에도 최신 내용 보장).
pub(crate) fn resolve_names(
    data_dir: Option<&Path>,
    names_csv: &str,
    tr: &Translator,
) -> Result<PathBuf, ProfileError> {
    let data_dir = require_data_dir(data_dir)?;
    let names = parse_names(names_csv)?;

    let mut contents = Vec::with_capacity(names.len());
    for name in &names {
        let path = registered_file(data_dir, name);
        if !path.is_file() {
            // 등록 프로필이 없으면 게이트로 fallback — 등록 게이트든 host 기본
            // 게이트(`continue-checklist`)든 같은 Stop 훅 조각으로 해석된다
            // (모듈 doc "이름으로 부착 가능한 것" 절의 해석 순서).
            if let Some((owner, value)) = crate::gate::attach_profile(Some(data_dir), name, tr) {
                contents.push((format!("{owner}/{name}"), value));
                continue;
            }
            // host/* listing 전용 항목(내장 훅)은 attachable 하지 않다 — 사용자가
            // 이름을 착각해 `--profile stop` 처럼 넘기면 "attach 불가" 를 명확히 알린다.
            if MANAGED_HOOKS.iter().any(|(_, token, _)| token == name) {
                return Err(ProfileError::NotAttachable(format!("host/{name}")));
            }
            return Err(ProfileError::UnknownProfile(format!("user/{name}")));
        }
        let text = std::fs::read_to_string(&path).map_err(|e| ProfileError::Io {
            path: path.display().to_string(),
            message: e.to_string(),
        })?;
        let value: Value = serde_json::from_str(&text).map_err(|e| ProfileError::Io {
            path: path.display().to_string(),
            message: e.to_string(),
        })?;
        contents.push((format!("user/{name}"), value));
    }

    let (merged, warnings) = merge_contents(&contents).map_err(ProfileError::Merge)?;
    for w in &warnings {
        warn!("claude profile resolve({names_csv}): {w}");
    }

    let mut sorted_names = names.clone();
    sorted_names.sort();
    let key = sorted_names.join("+");
    let dir = generated_dir(data_dir);
    std::fs::create_dir_all(&dir).map_err(|e| ProfileError::Io {
        path: dir.display().to_string(),
        message: e.to_string(),
    })?;
    let out_path = dir.join(format!("{key}.json"));
    let text = serde_json::to_string_pretty(&merged).unwrap_or_else(|_| "{}".to_string());
    std::fs::write(&out_path, text).map_err(|e| ProfileError::Io {
        path: out_path.display().to_string(),
        message: e.to_string(),
    })?;
    Ok(out_path)
}

// ── IPC handler (main.rs 배선 대상) ────────────────────────────────────────

fn require_name<'a>(params: &'a Value, tr: &Translator) -> Result<&'a str, IpcMethodError> {
    params
        .get("name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| IpcMethodError::invalid_params(tr.t("claude.params.missing_name")))
}

pub(crate) fn to_ipc_err(e: ProfileError, tr: &Translator) -> IpcMethodError {
    IpcMethodError::new(tr.t_fmt("claude.profile.error_prefix", &e.translate(tr)))
}

fn summary_to_json(s: &ProfileSummary) -> Value {
    json!({
        "id": s.id,
        "owner": s.owner,
        "attachable": s.attachable,
        "description": s.description,
    })
}

/// `claude.profile_register` — `--name <short> --file <path>` 로 등록. `file` 은
/// CLI `path_kind = "file"` 정규화를 이미 거친 절대경로.
pub(crate) fn handle_register(
    data_dir: Option<&Path>,
    params: &Value,
    tr: &Translator,
) -> Result<Value, IpcMethodError> {
    let name = require_name(params, tr)?;
    let file = params
        .get("file")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| IpcMethodError::invalid_params(tr.t("claude.params.missing_file")))?;
    register(data_dir, name, Path::new(file)).map_err(|e| to_ipc_err(e, tr))?;
    Ok(json!({ "id": format!("user/{name}") }))
}

/// `claude.profile_unregister` — `--name <short>`.
pub(crate) fn handle_unregister(
    data_dir: Option<&Path>,
    params: &Value,
    tr: &Translator,
) -> Result<Value, IpcMethodError> {
    let name = require_name(params, tr)?;
    unregister(data_dir, name).map_err(|e| to_ipc_err(e, tr))?;
    Ok(json!({ "id": format!("user/{name}") }))
}

/// `claude.profile_list` — 등록된 프로필 + 내장 훅 listing 항목 전체.
pub(crate) fn handle_list(
    data_dir: Option<&Path>,
    _params: &Value,
    tr: &Translator,
) -> Result<Value, IpcMethodError> {
    let entries: Vec<Value> = list(data_dir, tr).iter().map(summary_to_json).collect();
    Ok(json!({ "profiles": entries }))
}

/// `claude.profile_show` — `--name <short>` 로 등록된 원본 JSON 내용을 그대로 반환.
pub(crate) fn handle_show(
    data_dir: Option<&Path>,
    params: &Value,
    tr: &Translator,
) -> Result<Value, IpcMethodError> {
    let name = require_name(params, tr)?;
    let (owner, content) = show_registered(data_dir, name, tr).map_err(|e| to_ipc_err(e, tr))?;
    Ok(json!({ "id": format!("{owner}/{name}"), "content": content }))
}

/// `claude.profile_current` — 이 세션(surface)에 지금 부착된 프로필 + 항상
/// 살아있는 내장 훅을 함께 보여준다("지금 이 세션에 무슨 프로필이 걸려 있나").
pub(crate) fn handle_current(
    data_dir: Option<&Path>,
    host: &HostHandle,
    params: &Value,
    tr: &Translator,
) -> Result<Value, IpcMethodError> {
    let surface_id = crate::handlers::require_target_surface(params, tr)?;
    let attached = crate::reboot::attached_profile_summary(host, surface_id);
    let builtins: Vec<Value> = list(data_dir, tr)
        .iter()
        .filter(|s| !s.attachable)
        .map(summary_to_json)
        .collect();
    Ok(json!({
        "surface_id": surface_id,
        "attached_names": attached.names,
        "attached_path": attached.path,
        "builtin": builtins,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_profile(dir: &Path, name: &str, content: &str) {
        std::fs::create_dir_all(registered_dir(dir)).unwrap();
        std::fs::write(registered_file(dir, name), content).unwrap();
    }

    /// 게이트를 실제 등록 경로로 만든다 — 본문이 센티넬을 포함해야 통과한다.
    fn register_gate(dir: &Path, name: &str) {
        let body = dir.join(format!("{name}-body.md"));
        std::fs::write(&body, format!("{name} 본문\n[[{name}-DONE]]\n")).unwrap();
        crate::gate::register(
            Some(dir),
            name,
            &body,
            Some(&format!("[[{name}-DONE]]")),
            Some(2),
        )
        .unwrap();
    }

    /// 해석 결과 파일에서 Stop 훅 command 문자열들을 뽑는다.
    fn stop_commands(path: &Path) -> Vec<String> {
        let content: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        content["hooks"]["Stop"]
            .as_array()
            .expect("Stop 훅 배열")
            .iter()
            .flat_map(|entry| entry["hooks"].as_array().cloned().unwrap_or_default())
            .filter_map(|h| h["command"].as_str().map(str::to_string))
            .collect()
    }

    fn test_translator() -> Translator {
        let lang_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("lang");
        Translator::load(&lang_dir, "en")
    }

    #[test]
    fn register_copies_source_and_show_returns_it() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.json");
        std::fs::write(&src, r#"{"env":{"A":"1"}}"#).unwrap();
        register(Some(tmp.path()), "myprofile", &src).unwrap();
        let (owner, shown) =
            show_registered(Some(tmp.path()), "myprofile", &test_translator()).unwrap();
        assert_eq!(owner, "user");
        assert_eq!(shown["env"]["A"], "1");
    }

    #[test]
    fn register_rejects_non_object_json() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.json");
        std::fs::write(&src, r#"["not","object"]"#).unwrap();
        let err = register(Some(tmp.path()), "bad", &src).unwrap_err();
        assert!(matches!(err, ProfileError::SourceNotJsonObject { .. }));
    }

    #[test]
    fn register_rejects_invalid_short_name() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.json");
        std::fs::write(&src, r#"{}"#).unwrap();
        let err = register(Some(tmp.path()), "Bad Name!", &src).unwrap_err();
        assert!(matches!(err, ProfileError::InvalidShortName(_)));
    }

    #[test]
    fn register_without_data_dir_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.json");
        std::fs::write(&src, r#"{}"#).unwrap();
        let err = register(None, "x", &src).unwrap_err();
        assert_eq!(err, ProfileError::NoDataDir);
    }

    #[test]
    fn unregister_removes_and_second_call_errors() {
        let tmp = tempfile::tempdir().unwrap();
        write_profile(tmp.path(), "gone", "{}");
        unregister(Some(tmp.path()), "gone").unwrap();
        let err = unregister(Some(tmp.path()), "gone").unwrap_err();
        assert!(matches!(err, ProfileError::UnknownProfile(_)));
    }

    #[test]
    fn list_includes_builtin_hooks_and_user_profiles() {
        let tmp = tempfile::tempdir().unwrap();
        write_profile(tmp.path(), "myprofile", "{}");
        let entries = list(Some(tmp.path()), &test_translator());
        assert!(entries.iter().any(|e| e.id == "host/stop" && !e.attachable));
        assert!(
            entries
                .iter()
                .any(|e| e.id == "user/myprofile" && e.attachable)
        );
    }

    #[test]
    fn list_without_data_dir_still_shows_builtins() {
        let entries = list(None, &test_translator());
        assert!(!entries.is_empty());
        assert!(entries.iter().all(|e| e.owner == "host"));
    }

    #[test]
    fn resolve_single_name_materializes_generated_file() {
        let tmp = tempfile::tempdir().unwrap();
        write_profile(tmp.path(), "solo", r#"{"env":{"A":"1"}}"#);
        let path = resolve_names(Some(tmp.path()), "solo", &test_translator()).unwrap();
        assert!(path.starts_with(generated_dir(tmp.path())));
        let content: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(content["env"]["A"], "1");
    }

    #[test]
    fn resolve_two_names_merges_both() {
        let tmp = tempfile::tempdir().unwrap();
        write_profile(tmp.path(), "one", r#"{"env":{"A":"1"}}"#);
        write_profile(tmp.path(), "two", r#"{"env":{"B":"2"}}"#);
        let path = resolve_names(Some(tmp.path()), "one,two", &test_translator()).unwrap();
        let content: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(content["env"]["A"], "1");
        assert_eq!(content["env"]["B"], "2");
    }

    #[test]
    fn resolve_is_order_independent_for_generated_filename() {
        let tmp = tempfile::tempdir().unwrap();
        write_profile(tmp.path(), "one", "{}");
        write_profile(tmp.path(), "two", "{}");
        let a = resolve_names(Some(tmp.path()), "one,two", &test_translator()).unwrap();
        let b = resolve_names(Some(tmp.path()), "two,one", &test_translator()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn resolve_unknown_name_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let err = resolve_names(Some(tmp.path()), "nope", &test_translator()).unwrap_err();
        assert!(matches!(err, ProfileError::UnknownProfile(_)));
    }

    #[test]
    fn resolve_builtin_hook_name_is_not_attachable() {
        let tmp = tempfile::tempdir().unwrap();
        let err = resolve_names(Some(tmp.path()), "stop", &test_translator()).unwrap_err();
        assert!(matches!(err, ProfileError::NotAttachable(_)));
    }

    #[test]
    fn resolve_reflects_latest_registered_content_on_reattach() {
        let tmp = tempfile::tempdir().unwrap();
        write_profile(tmp.path(), "changing", r#"{"env":{"A":"1"}}"#);
        let first = resolve_names(Some(tmp.path()), "changing", &test_translator()).unwrap();
        let first_content: Value =
            serde_json::from_str(&std::fs::read_to_string(&first).unwrap()).unwrap();
        assert_eq!(first_content["env"]["A"], "1");

        write_profile(tmp.path(), "changing", r#"{"env":{"A":"2"}}"#);
        let second = resolve_names(Some(tmp.path()), "changing", &test_translator()).unwrap();
        let second_content: Value =
            serde_json::from_str(&std::fs::read_to_string(&second).unwrap()).unwrap();
        assert_eq!(second_content["env"]["A"], "2");
    }

    #[test]
    fn resolve_host_default_profile_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        let path =
            resolve_names(Some(tmp.path()), "continue-checklist", &test_translator()).unwrap();
        let content: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(content["hooks"]["Stop"].is_array());
        // host 기본 게이트도 다른 게이트와 같은 경로로 부착된다.
        assert_eq!(
            stop_commands(&path),
            vec![crate::install::tasty_guarded_command(
                "tasty claude checklist-hook --gate continue-checklist"
            )]
        );
    }

    #[test]
    fn resolve_registered_gate_by_name() {
        let tmp = tempfile::tempdir().unwrap();
        register_gate(tmp.path(), "mygate");
        let path = resolve_names(Some(tmp.path()), "mygate", &test_translator()).unwrap();
        let content: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(
            content["hooks"]["Stop"].is_array(),
            "등록 게이트가 Stop 훅으로 해석되지 않았다: {content}"
        );
    }

    #[test]
    fn generated_hook_command_contains_gate_flag() {
        let tmp = tempfile::tempdir().unwrap();
        register_gate(tmp.path(), "mygate");
        let path = resolve_names(Some(tmp.path()), "mygate", &test_translator()).unwrap();
        let commands = stop_commands(&path);
        assert_eq!(commands.len(), 1);
        assert!(
            commands[0].contains("checklist-hook --gate mygate"),
            "훅 명령에 게이트 이름이 실리지 않으면 host 기본 게이트로 판정된다: {}",
            commands[0]
        );
    }

    /// 명령 형태를 이 파일에 따로 박아 두면 `install.rs` 만 고쳤을 때 두 경로가
    /// 갈린다 — 생성 결과가 `tasty_guarded_command` 산출물과 문자 단위로 같은지 본다.
    #[test]
    fn generated_hook_command_uses_guarded_form() {
        let tmp = tempfile::tempdir().unwrap();
        register_gate(tmp.path(), "mygate");
        let path = resolve_names(Some(tmp.path()), "mygate", &test_translator()).unwrap();
        assert_eq!(
            stop_commands(&path),
            vec![crate::install::tasty_guarded_command(
                "tasty claude checklist-hook --gate mygate"
            )]
        );
    }

    #[test]
    fn registered_profile_shadows_gate_of_same_name() {
        let tmp = tempfile::tempdir().unwrap();
        register_gate(tmp.path(), "mygate");
        // 41 의 상호 배제를 우회해 같은 이름의 등록 프로필을 손으로 만든 상태.
        write_profile(tmp.path(), "mygate", r#"{"env":{"FROM_PROFILE":"1"}}"#);

        let path = resolve_names(Some(tmp.path()), "mygate", &test_translator()).unwrap();
        let content: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(content["env"]["FROM_PROFILE"], "1");
        assert!(content["hooks"].is_null(), "게이트가 프로필을 덮었다");

        let (owner, shown) =
            show_registered(Some(tmp.path()), "mygate", &test_translator()).unwrap();
        assert_eq!(owner, "user");
        assert_eq!(shown["env"]["FROM_PROFILE"], "1");

        // 목록에도 한 줄로만 나온다.
        let entries = list(Some(tmp.path()), &test_translator());
        assert_eq!(entries.iter().filter(|e| e.id == "user/mygate").count(), 1);
    }

    #[test]
    fn show_registered_reports_user_owner_for_registered_gate() {
        let tmp = tempfile::tempdir().unwrap();
        register_gate(tmp.path(), "mygate");
        let (owner, content) =
            show_registered(Some(tmp.path()), "mygate", &test_translator()).unwrap();
        assert_eq!(owner, "user", "등록 게이트인데 host 로 보고했다");
        assert!(content["hooks"]["Stop"].is_array());
    }

    #[test]
    fn list_shows_registered_gates_as_attachable() {
        let tmp = tempfile::tempdir().unwrap();
        register_gate(tmp.path(), "mygate");
        write_profile(tmp.path(), "myprofile", "{}");
        let entries = list(Some(tmp.path()), &test_translator());

        let gate = entries
            .iter()
            .find(|e| e.id == "user/mygate")
            .expect("등록 게이트가 목록에 없다");
        assert!(gate.attachable);
        assert!(
            gate.description.is_some(),
            "게이트는 등록 프로필과 구분되는 설명을 가져야 한다"
        );
        let profile = entries.iter().find(|e| e.id == "user/myprofile").unwrap();
        assert!(profile.attachable && profile.description.is_none());
    }

    /// 게이트 둘을 함께 부착하면 Stop 훅이 두 개 등록된다(`profile_merge` 의
    /// hooks concat) — 42 가 라운드 상태를 게이트별로 쪼갠 전제.
    #[test]
    fn resolve_two_gates_registers_two_stop_hooks() {
        let tmp = tempfile::tempdir().unwrap();
        register_gate(tmp.path(), "gate-a");
        let path = resolve_names(
            Some(tmp.path()),
            "gate-a,continue-checklist",
            &test_translator(),
        )
        .unwrap();
        let commands = stop_commands(&path);
        assert_eq!(commands.len(), 2, "Stop 훅이 하나로 합쳐졌다: {commands:?}");
        assert!(commands.iter().any(|c| c.contains("--gate gate-a")));
        assert!(
            commands
                .iter()
                .any(|c| c.contains("--gate continue-checklist"))
        );
    }

    #[test]
    fn resolve_prefers_user_registration_over_host_default() {
        let tmp = tempfile::tempdir().unwrap();
        write_profile(
            tmp.path(),
            "continue-checklist",
            r#"{"env":{"OVERRIDDEN":"1"}}"#,
        );
        let path =
            resolve_names(Some(tmp.path()), "continue-checklist", &test_translator()).unwrap();
        let content: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(content["env"]["OVERRIDDEN"], "1");
        assert!(content["hooks"].is_null());
    }

    #[test]
    fn show_registered_falls_back_to_host_default() {
        let tmp = tempfile::tempdir().unwrap();
        let (owner, content) =
            show_registered(Some(tmp.path()), "continue-checklist", &test_translator()).unwrap();
        assert_eq!(owner, "host");
        assert!(content["hooks"]["Stop"].is_array());
    }

    #[test]
    fn list_includes_attachable_host_default_profile() {
        let entries = list(None, &test_translator());
        assert!(
            entries
                .iter()
                .any(|e| e.id == "host/continue-checklist" && e.attachable)
        );
    }
}
