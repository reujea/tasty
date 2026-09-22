//! `tasty tool remote-profile ...` 실행 — 원격 접속 프로필 통합 CRUD (ssh + tasty-attach).
//!
//! `~/.tasty/remote-profiles.toml` / `passkeys.toml` 는 client(이 머신) 로컬 파일이라
//! IPC 미경유로 직접 읽고 쓴다. 포커스 비의존(원칙 3): 모든 명령이 `--name` 으로 대상을
//! 지정한다. 비밀 값은 저장하지 않는다 — `--identity <path>` 는 path kind passkey
//! `<name>-key` 로 분리 저장되고, 프로필은 그 passkey 를 이름으로 참조한다.
//!
//! 선언(`RemoteProfileCommands`)은 [`crate::commands::remote_profile`] 에 남는다.

use anyhow::Result;
use tasty_i18n::{t, t_args, t_fmt, t_fmt2};
use tasty_remote_profiles::{
    ImportError, Passkeys, RemoteProfile, RemoteProfiles, SshConfigHost, config_availability,
    enumerate_hosts, imported_as, is_valid_shell, prepare_import, sanitize_passkey_name,
    user_config_path,
};

use crate::commands::remote_profile::RemoteProfileCommands;
use crate::out::outln;

/// `--identity <path>` 를 path kind passkey `<name>-key` 로 분리 저장하고 그 이름을
/// 반환한다(프로필 `passkey_ref` 로 연결). 같은 path 면 그대로 갱신(upsert).
fn link_identity_passkey(passkeys: &mut Passkeys, name: &str, identity: &str) -> Result<String> {
    let pk_name = format!("{}-key", sanitize_passkey_name(name));
    passkeys.upsert_path(&pk_name, identity.to_string())?;
    Ok(pk_name)
}

/// `tasty tool remote-profile ...` 로컬 분기 진입점(IPC 미경유).
#[allow(clippy::cognitive_complexity)] // complexity-exempt: CLI 서브커맨드 평면 match 디스패치 — arm 수 많으나 중첩 얕음, 분해 시 라우팅 추적만 어려워짐
pub fn run(command: &RemoteProfileCommands) -> Result<()> {
    match command {
        RemoteProfileCommands::AddSsh {
            name,
            host,
            user,
            port,
            identity,
            options,
            shell,
            label,
        } => {
            if !is_valid_shell(shell) {
                anyhow::bail!("{}", t_fmt("cli.remote_profile.unknown_shell", shell));
            }
            let mut profiles = RemoteProfiles::load();
            let mut passkeys = Passkeys::load();
            let mut p = RemoteProfile::new(name.clone(), "ssh");
            p.set_field("host", host.clone());
            if let Some(u) = user {
                p.set_field("user", u.clone());
            }
            if let Some(pt) = port {
                p.set_field("port", pt.to_string());
            }
            if !options.is_empty() {
                p.set_field("extra_options", options.clone());
            }
            p.set_field("shell", shell.clone());
            p.label = label.clone();
            if let Some(idf) = identity {
                p.passkey_ref = Some(link_identity_passkey(&mut passkeys, name, idf)?);
            }
            let replaced = profiles.get(name).is_some();
            // shell 적용: 명시 셸 → 매핑(즉시), auto → SSH 프로브 1회(수 초 블록 가능).
            let detect = crate::ssh::apply_shell_to_profile(&mut p, &passkeys);
            profiles.upsert(p);
            passkeys.save()?;
            profiles.save()?;
            let key = if replaced {
                "cli.remote_profile.ssh_updated"
            } else {
                "cli.remote_profile.ssh_added"
            };
            outln!("{}", t_fmt2(key, name, host))?;
            report_detect(name, &detect)?;
            Ok(())
        }
        RemoteProfileCommands::AddAttach {
            name,
            ssh_ref,
            host,
            user,
            port,
            identity,
            options,
            remote_tasty,
            port_mode,
            port_file,
            label,
        } => {
            if ssh_ref.is_some() && host.is_some() {
                anyhow::bail!("{}", t("cli.remote_profile.attach_ref_xor_host"));
            }
            if ssh_ref.is_none() && host.is_none() {
                anyhow::bail!("{}", t("cli.remote_profile.attach_needs_ref_or_host"));
            }
            let mut profiles = RemoteProfiles::load();
            let mut passkeys = Passkeys::load();
            let mut p = RemoteProfile::new(name.clone(), "tasty-attach");
            if let Some(r) = ssh_ref {
                if profiles.get(r).is_none() {
                    crate::out::errln!("{}", t_fmt("cli.remote_profile.attach_ref_missing", r));
                }
                p.set_field("ssh_ref", r.clone());
            } else {
                // 인라인 연결 필드.
                if let Some(h) = host {
                    p.set_field("host", h.clone());
                }
                if let Some(u) = user {
                    p.set_field("user", u.clone());
                }
                if let Some(pt) = port {
                    p.set_field("port", pt.to_string());
                }
                if !options.is_empty() {
                    p.set_field("extra_options", options.clone());
                }
                if let Some(idf) = identity {
                    p.passkey_ref = Some(link_identity_passkey(&mut passkeys, name, idf)?);
                }
            }
            p.set_field("remote_tasty", remote_tasty.clone());
            p.set_field("port_mode", port_mode.clone());
            if let Some(pf) = port_file {
                p.set_field("port_file", pf.clone());
            }
            p.label = label.clone();
            let replaced = profiles.get(name).is_some();
            profiles.upsert(p);
            passkeys.save()?;
            profiles.save()?;
            let key = if replaced {
                "cli.remote_profile.attach_updated"
            } else {
                "cli.remote_profile.attach_added"
            };
            outln!("{}", t_fmt(key, name))?;
            Ok(())
        }
        RemoteProfileCommands::List { json, kind } => {
            let profiles = RemoteProfiles::load();
            let matches_kind = |p: &RemoteProfile| kind.as_deref().is_none_or(|k| p.kind == k);
            if *json {
                let arr: Vec<_> = profiles
                    .profiles
                    .iter()
                    .filter(|p| matches_kind(p))
                    .map(profile_json)
                    .collect();
                outln!("{}", serde_json::to_string_pretty(&arr)?)?;
            } else {
                let rows: Vec<&RemoteProfile> = profiles
                    .profiles
                    .iter()
                    .filter(|p| matches_kind(p))
                    .collect();
                if rows.is_empty() {
                    outln!("{}", t("cli.remote_profile.list_empty"))?;
                } else {
                    // 헤더는 컬럼 폭을 언어별로 맞춰야 해서(CJK 는 표시 폭이 2배)
                    // 패딩까지 포함한 한 줄 전체를 번역 값으로 둔다.
                    outln!("{}", t("cli.remote_profile.list_header"))?;
                    for p in rows {
                        let (dest, detail) = summarize(p);
                        outln!(
                            "{:<16} {:<13} {:<24} {:<16} {}",
                            p.name,
                            p.kind,
                            dest,
                            detail,
                            status_of(p),
                        )?;
                    }
                }
            }
            Ok(())
        }
        RemoteProfileCommands::Show { name, json } => {
            let profiles = RemoteProfiles::load();
            let passkeys = Passkeys::load();
            let Some(p) = profiles.get(name) else {
                anyhow::bail!("{}", t_fmt("cli.remote_profile.not_found", name));
            };
            if *json {
                outln!("{}", serde_json::to_string_pretty(p)?)?;
                return Ok(());
            }
            // 왼쪽 라벨 열은 산문이 아니라 **필드 식별자 열**이라 번역하지 않는다.
            // 등록 kind 는 프로필이 실제로 갖는 필드명을 그대로 찍고(`shell` /
            // `port_mode` / `remote_tasty` / `ssh_ref` / `extra_options` 는
            // `[profile.fields]` 의 toml 키 그 자체다), 미등록 kind 는 아래
            // fallback 이 `p.fields` 의 raw 키를 같은 열에 찍는다. 등록 kind 쪽만
            // 번역하면 한 명령의 같은 열이 절반은 번역어, 절반은 raw 필드명이 된다.
            // (`--json` 키와 1:1 대응해서가 아니다 — `type`↔`kind`,
            // `passkey`↔`passkey_ref` 로 이름이 다르고 `destination` / `status` 는
            // 합성값이라 대응 키가 아예 없다. 라벨 옆의 자연어만 번역 대상이다.)
            outln!("name          : {}", p.name)?;
            outln!("type          : {}", p.kind)?;
            if let Some(l) = &p.label {
                outln!("label         : {l}")?;
            }
            if let Some(pk) = &p.passkey_ref {
                let status = if passkeys.get(pk).is_some() {
                    ""
                } else {
                    t("cli.remote_profile.show_passkey_missing")
                };
                outln!("passkey       : {pk}{status}")?;
            }
            if let Some(v) = p.as_ssh() {
                outln!("destination   : {}", v.ssh_destination())?;
                if let Some(port) = v.port() {
                    outln!("port          : {port}")?;
                }
                if !v.extra_options().is_empty() {
                    outln!("extra_options : {}", v.extra_options().join(", "))?;
                }
                outln!("shell         : {}", v.shell())?;
                if v.is_disabled() {
                    outln!(
                        "status        : {}",
                        t_fmt("cli.remote_profile.show_status_disabled", name)
                    )?;
                }
            } else if let Some(a) = p.as_attach() {
                match a.ssh_ref() {
                    Some(r) => outln!(
                        "ssh_ref       : {r} {}",
                        t("cli.remote_profile.show_ref_note")
                    )?,
                    None => {
                        outln!("destination   : {}", a.ssh_destination())?;
                        if let Some(port) = a.port() {
                            outln!("port          : {port}")?;
                        }
                        if !a.extra_options().is_empty() {
                            outln!("extra_options : {}", a.extra_options().join(", "))?;
                        }
                    }
                }
                outln!("remote_tasty  : {}", a.remote_tasty())?;
                outln!("port_mode     : {}", a.port_mode())?;
                if let Some(pf) = a.port_file() {
                    outln!("port_file     : {pf}")?;
                }
            } else {
                for (k, val) in &p.fields {
                    outln!("{k:<14}: {val:?}")?;
                }
            }
            Ok(())
        }
        RemoteProfileCommands::Edit {
            name,
            host,
            user,
            port,
            identity,
            options,
            ssh_ref,
            remote_tasty,
            port_mode,
            port_file,
            shell,
            label,
        } => {
            if let Some(s) = shell
                && !is_valid_shell(s)
            {
                anyhow::bail!("{}", t_fmt("cli.remote_profile.unknown_shell", s));
            }
            let mut profiles = RemoteProfiles::load();
            let mut passkeys = Passkeys::load();
            let Some(mut p) = profiles.get(name).cloned() else {
                anyhow::bail!("{}", t_fmt("cli.remote_profile.not_found", name));
            };
            if let Some(h) = host {
                p.set_field("host", h.clone());
            }
            if let Some(u) = user {
                p.set_field("user", u.clone());
            }
            if let Some(pt) = port {
                p.set_field("port", pt.to_string());
            }
            if let Some(idf) = identity {
                p.passkey_ref = Some(link_identity_passkey(&mut passkeys, name, idf)?);
            }
            if !options.is_empty() {
                p.set_field("extra_options", options.clone());
            }
            if let Some(r) = ssh_ref {
                p.set_field("ssh_ref", r.clone());
            }
            if let Some(rt) = remote_tasty {
                p.set_field("remote_tasty", rt.clone());
            }
            if let Some(pm) = port_mode {
                p.set_field("port_mode", pm.clone());
            }
            if let Some(pf) = port_file {
                p.set_field("port_file", pf.clone());
            }
            if label.is_some() {
                p.label = label.clone();
            }
            // ssh kind 에 --shell 이 주어지면 셸 갱신 + 발견 모드 재도출/재감지.
            let detect = if let Some(s) = shell {
                p.set_field("shell", s.clone());
                if p.kind == "ssh" {
                    crate::ssh::apply_shell_to_profile(&mut p, &passkeys)
                } else {
                    None
                }
            } else {
                None
            };
            profiles.upsert(p);
            passkeys.save()?;
            profiles.save()?;
            outln!("{}", t_fmt("cli.remote_profile.updated", name))?;
            report_detect(name, &detect)?;
            Ok(())
        }
        RemoteProfileCommands::Detect { name } => {
            let profiles = RemoteProfiles::load();
            let Some(p) = profiles.get(name).cloned() else {
                anyhow::bail!("{}", t_fmt("cli.remote_profile.not_found", name));
            };
            if p.as_attach().is_some() {
                return detect_attach(&profiles, &p);
            }
            // ssh kind(또는 기타): 셸 감지 프로브.
            match crate::ssh::detect_and_persist(name) {
                Ok(mode) => outln!(
                    "{}",
                    t_fmt2("cli.remote_profile.detect_ok", name, mode.as_str())
                )?,
                Err(e) => outln!(
                    "{}",
                    t_args(
                        "cli.remote_profile.detect_failed",
                        &[&e.to_string(), name, name]
                    )
                )?,
            }
            Ok(())
        }
        RemoteProfileCommands::Remove { name } => {
            let mut profiles = RemoteProfiles::load();
            if profiles.remove(name) {
                profiles.save()?;
                outln!("{}", t_fmt("cli.remote_profile.removed", name))?;
            } else {
                anyhow::bail!("{}", t_fmt("cli.remote_profile.not_found", name));
            }
            Ok(())
        }
        RemoteProfileCommands::ListLocal { json } => list_local(*json),
        RemoteProfileCommands::Import { from, name, label } => {
            let mut profiles = RemoteProfiles::load();
            let p = prepare_import(&profiles, &enumerate_hosts(), from, name, label.clone())
                .map_err(import_error_message)?;
            profiles.upsert(p);
            profiles.save()?;
            outln!("{}", t_fmt2("cli.remote_profile.import_added", name, from))?;
            // 셸 감지는 SSH 접속을 발생시키므로 가져오기에 묶지 않는다 — 여러 건을
            // 가져올 때 접속이 연쇄로 일어난다. 필요할 때 사용자가 직접 돌린다.
            outln!("{}", t_fmt("cli.remote_profile.import_detect_hint", name))?;
            Ok(())
        }
    }
}

/// [`ImportError`] 를 사용자 문구로. 크레이트는 문자열을 갖지 않으므로 여기서 만든다.
fn import_error_message(e: ImportError) -> anyhow::Error {
    match e {
        ImportError::UnknownAlias(a) => {
            anyhow::anyhow!("{}", t_fmt("cli.remote_profile.import_unknown_alias", &a))
        }
        ImportError::NameTaken(n) => {
            anyhow::anyhow!("{}", t_fmt("cli.remote_profile.import_name_taken", &n))
        }
    }
}

/// `list-local` — 로컬 ssh config 의 alias 열거. ssh 를 실행하지 않는 순수 파일 읽기다.
fn list_local(json: bool) -> Result<()> {
    let hosts = enumerate_hosts();
    let profiles = RemoteProfiles::load();
    if json {
        // **IPC 와 비대칭인 지점이다.** `remote.profile.list_local` 은 `config_exists`·
        // `config_readable` 을 함께 실어 보내지만 여기 `--json` 은 맨 배열이라 빈 결과의
        // 이유(부재 / 권한 없음 / Host 항목 없음)를 구분하지 못한다. 객체로 감싸면
        // 기존 스크립트를 깨는 breaking change 라 두 갈래를 일부러 어긋난 채 둔다 —
        // 그 구분이 필요한 호출자는 IPC 를 쓰면 된다.
        let arr: Vec<_> = hosts
            .iter()
            .map(|h| {
                serde_json::json!({
                    "name": h.alias,
                    "source": h.source.display().to_string(),
                    "hostname": h.hostname,
                    "user": h.user,
                    "port": h.port,
                    "imported_as": imported_as(&profiles, &h.alias),
                })
            })
            .collect();
        outln!("{}", serde_json::to_string_pretty(&arr)?)?;
        return Ok(());
    }
    if hosts.is_empty() {
        // 빈 목록의 이유는 셋 중 하나고, 사용자가 할 일이 전부 다르다 — 파일을
        // 만든다 / 읽을 수 있게 고친다(권한, 또는 경로가 디렉토리인 경우) /
        // Host 항목을 추가한다. IPC 쪽은 같은 구분을 `config_exists`·
        // `config_readable` 로 실어 보낸다.
        let path = user_config_path();
        let avail = config_availability(path.as_deref());
        let shown = path
            .map(|p| tasty_utils::path::tilde_abbreviate(&p))
            .unwrap_or_else(|| "~/.ssh/config".into());
        let key = if !avail.exists {
            "cli.remote_profile.list_local_no_config"
        } else if !avail.readable {
            "cli.remote_profile.list_local_unreadable"
        } else {
            "cli.remote_profile.list_local_no_alias"
        };
        outln!("{}", t_fmt(key, &shown))?;
        return Ok(());
    }
    // 헤더는 `list` 와 같은 이유로 패딩 포함 한 줄 전체를 번역 값으로 둔다.
    outln!("{}", t("cli.remote_profile.list_local_header"))?;
    for h in &hosts {
        outln!(
            "{:<20} {:<28} {:<20} {}",
            elide(&h.alias, 20),
            elide(&tasty_utils::path::tilde_abbreviate(&h.source), 28),
            elide(&target_hint(h), 20),
            imported_as(&profiles, &h.alias).unwrap_or("-"),
        )?;
    }
    Ok(())
}

/// 표시용 hint 한 칸 — `HostName[:Port]`(둘 다 없으면 빈 칸). 프로필에는 저장되지
/// 않는 값이라 "이 alias 가 어디를 가리키나" 를 눈으로 확인하는 용도다.
fn target_hint(h: &SshConfigHost) -> String {
    match (&h.hostname, h.port) {
        (Some(host), Some(port)) => format!("{host}:{port}"),
        (Some(host), None) => host.clone(),
        (None, Some(port)) => format!(":{port}"),
        (None, None) => "-".into(),
    }
}

/// 표 한 칸을 넘기는 값은 말줄임한다. alias 는 사용자가 쓴 로컬 파일에서 오므로 길이
/// 상한이 없다 — 열이 밀려 표가 통째로 어긋나는 것만 막는다(`--json` 은 원본 그대로).
fn elide(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        return s.to_string();
    }
    let head: String = s.chars().take(width.saturating_sub(1)).collect();
    format!("{head}…")
}

/// tasty-attach 프로필의 원격 포트를 실제로 발견해 검증한다(SSH 접속 1회).
fn detect_attach(profiles: &RemoteProfiles, p: &RemoteProfile) -> Result<()> {
    let passkeys = Passkeys::load();
    let (target, remote_tasty, port_mode, port_file) =
        crate::ssh::resolve_attach_target(p, profiles, &passkeys)?;
    let ssh = crate::ssh::resolve_ssh_path();
    let mode = crate::ssh::PortMode::parse(&port_mode)?;
    let verify = std::env::var("TASTY_SSH_VERIFY").is_ok();
    let debug = cfg!(debug_assertions);
    match crate::ssh::discover_remote_port(
        &ssh,
        &target,
        &remote_tasty,
        mode,
        verify,
        debug,
        port_file.as_deref(),
    ) {
        Ok(port) => {
            outln!(
                "{}",
                t_fmt2(
                    "cli.remote_profile.attach_detect_ok",
                    &p.name,
                    &port.to_string()
                )
            )?;
            Ok(())
        }
        Err(e) => {
            outln!(
                "{}",
                t_fmt("cli.remote_profile.attach_detect_failed", &e.to_string())
            )?;
            Ok(())
        }
    }
}

/// list --json 한 항목. kind 별 대표 필드를 노출한다.
fn profile_json(p: &RemoteProfile) -> serde_json::Value {
    if let Some(v) = p.as_ssh() {
        serde_json::json!({
            "name": p.name,
            "kind": p.kind,
            "host": v.host(),
            "user": v.user(),
            "port": v.port(),
            "passkey_ref": p.passkey_ref,
            "shell": v.shell(),
            "detect_failed": v.detect_failed(),
        })
    } else if let Some(a) = p.as_attach() {
        serde_json::json!({
            "name": p.name,
            "kind": p.kind,
            "ssh_ref": a.ssh_ref(),
            "host": a.host(),
            "port": a.port(),
            "passkey_ref": p.passkey_ref,
            "remote_tasty": a.remote_tasty(),
            "port_mode": a.port_mode(),
            "port_file": a.port_file(),
        })
    } else {
        serde_json::json!({ "name": p.name, "kind": p.kind })
    }
}

/// list 표의 (HOST/REF, DETAIL) 두 컬럼을 kind 별로 채운다.
fn summarize(p: &RemoteProfile) -> (String, String) {
    if let Some(v) = p.as_ssh() {
        (v.ssh_destination(), format!("shell={}", v.shell()))
    } else if let Some(a) = p.as_attach() {
        let host_ref = match a.ssh_ref() {
            Some(r) => format!("ref:{r}"),
            None => a.ssh_destination(),
        };
        (host_ref, format!("tasty={}", a.remote_tasty()))
    } else {
        (String::new(), String::new())
    }
}

/// list STATUS 컬럼. 비활성(ssh 감지 실패)·미등록 타입을 표시.
fn status_of(p: &RemoteProfile) -> &'static str {
    if p.as_ssh().map(|v| v.is_disabled()).unwrap_or(false) {
        t("cli.remote_profile.status_detect_failed")
    } else if !p.is_builtin_kind() {
        t("cli.remote_profile.status_unknown_kind")
    } else {
        ""
    }
}

/// `apply_shell_to_profile` 결과를 사용자에게 출력한다. 명시 셸(None)은 조용히 넘어간다.
fn report_detect(name: &str, detect: &Option<Result<crate::ssh::PortMode>>) -> Result<()> {
    match detect {
        Some(Ok(mode)) => outln!(
            "{}",
            t_fmt("cli.remote_profile.auto_detect_ok", mode.as_str())
        )?,
        Some(Err(e)) => outln!(
            "{}",
            t_args(
                "cli.remote_profile.auto_detect_failed",
                &[&e.to_string(), name, name]
            )
        )?,
        None => {}
    }
    Ok(())
}
