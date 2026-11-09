//! 번들 플러그인을 찾아 사용자 데이터 루트에 설치·동기화한다.
//! 명시적으로 제거한 plugin은 자동 복구하지 않으며 외부 plugin과 같은 실행·권한 모델을 쓴다.
//!
//! 같은 버전은 내용이 다른 파일만 복사하고, 번들 버전이 높으면 전체를 교체한다.
//! 설치본 버전이 더 높으면 건너뛰며 --force로 덮어쓸 수 있다.
//! 설치 절차: docs/dev-guide/plugin-development.md#91-실행-중인-tasty-에-번들-플러그인만-반복-갱신-호스트-재빌드재시작-불필요.

pub mod file_upgrade;

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::{PluginManager, PluginsConfig, discovery};
use tasty_plugin_manifest::Manifest;

/// 한 builtin plugin의 패키지 메타 — id, dev workspace crate 경로, plugin 바이너리 이름.
pub(crate) struct BuiltinSpec {
    pub(crate) id: &'static str,
    /// `crates/<crate_dir>/` — workspace 빌드의 매니페스트/lang 원본 위치.
    /// workspace 가 실제로 존재할 때만 dev sync 에 사용된다 (배포 패키지에서는 미사용).
    crate_dir: &'static str,
    /// `target/<profile>/<bin_name>` — workspace 빌드된 plugin 실행 바이너리 이름.
    bin_name: &'static str,
}

#[cfg(windows)]
pub(crate) const BUILTINS: &[BuiltinSpec] = &[
    BuiltinSpec {
        id: "com.tasty.codex",
        crate_dir: "tasty-plugin-codex",
        bin_name: "tasty-plugin-codex.exe",
    },
    BuiltinSpec {
        id: "com.tasty.claude",
        crate_dir: "tasty-plugin-claude",
        bin_name: "tasty-plugin-claude.exe",
    },
    BuiltinSpec {
        id: "com.tasty.image",
        crate_dir: "tasty-plugin-image",
        bin_name: "tasty-plugin-image.exe",
    },
    BuiltinSpec {
        id: "com.tasty.markdown",
        crate_dir: "tasty-plugin-markdown",
        bin_name: "tasty-plugin-markdown.exe",
    },
    BuiltinSpec {
        id: "com.tasty.mdedit",
        crate_dir: "tasty-plugin-mdedit",
        bin_name: "tasty-plugin-mdedit.exe",
    },
    BuiltinSpec {
        id: "com.tasty.clipboard-viewer",
        crate_dir: "tasty-plugin-clipboard-viewer",
        bin_name: "tasty-plugin-clipboard-viewer.exe",
    },
    BuiltinSpec {
        id: "com.tasty.html",
        crate_dir: "tasty-plugin-html",
        bin_name: "tasty-plugin-html.exe",
    },
    BuiltinSpec {
        id: "com.tasty.git-viewer",
        crate_dir: "tasty-plugin-git-viewer",
        bin_name: "tasty-plugin-git-viewer.exe",
    },
    BuiltinSpec {
        id: "com.tasty.mesh-demo",
        crate_dir: "tasty-plugin-mesh-demo",
        bin_name: "tasty-plugin-mesh-demo.exe",
    },
    BuiltinSpec {
        id: "com.tasty.agent-stream",
        crate_dir: "tasty-plugin-agent-stream",
        bin_name: "tasty-plugin-agent-stream.exe",
    },
];

#[cfg(not(windows))]
pub(crate) const BUILTINS: &[BuiltinSpec] = &[
    BuiltinSpec {
        id: "com.tasty.codex",
        crate_dir: "tasty-plugin-codex",
        bin_name: "tasty-plugin-codex",
    },
    BuiltinSpec {
        id: "com.tasty.claude",
        crate_dir: "tasty-plugin-claude",
        bin_name: "tasty-plugin-claude",
    },
    BuiltinSpec {
        id: "com.tasty.image",
        crate_dir: "tasty-plugin-image",
        bin_name: "tasty-plugin-image",
    },
    BuiltinSpec {
        id: "com.tasty.markdown",
        crate_dir: "tasty-plugin-markdown",
        bin_name: "tasty-plugin-markdown",
    },
    BuiltinSpec {
        id: "com.tasty.mdedit",
        crate_dir: "tasty-plugin-mdedit",
        bin_name: "tasty-plugin-mdedit",
    },
    BuiltinSpec {
        id: "com.tasty.clipboard-viewer",
        crate_dir: "tasty-plugin-clipboard-viewer",
        bin_name: "tasty-plugin-clipboard-viewer",
    },
    BuiltinSpec {
        id: "com.tasty.html",
        crate_dir: "tasty-plugin-html",
        bin_name: "tasty-plugin-html",
    },
    BuiltinSpec {
        id: "com.tasty.git-viewer",
        crate_dir: "tasty-plugin-git-viewer",
        bin_name: "tasty-plugin-git-viewer",
    },
    BuiltinSpec {
        id: "com.tasty.mesh-demo",
        crate_dir: "tasty-plugin-mesh-demo",
        bin_name: "tasty-plugin-mesh-demo",
    },
    BuiltinSpec {
        id: "com.tasty.agent-stream",
        crate_dir: "tasty-plugin-agent-stream",
        bin_name: "tasty-plugin-agent-stream",
    },
];

pub fn is_builtin_plugin(id: &str) -> bool {
    BUILTINS.iter().any(|b| b.id == id)
}

/// 설치본과 번들의 버전을 비교해 선택한 동기화 방식.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BuiltinUpgradeDecision {
    /// 설치본이 더 높거나 번들 버전을 읽지 못해 건너뛴다.
    Skip,
    /// 같은 semver — **내용 기반** idempotent sync (dev workspace hotfix). mtime 은 안 본다:
    /// 시각이 보존된 채 배포된 파일(`cp -p`·아카이브 해제)도 내용이 다르면 반영해야 한다.
    ResyncSameVersion,
    /// bundle > installed — 디렉토리 전체를 mtime 무시하고 덮어씀.
    UpgradeVersion {
        from: semver::Version,
        to: semver::Version,
    },
    /// 사용자가 `--force` 로 명시 요청 — 동일/하위 버전도 강제 재설치.
    ForceOverwrite,
}

/// dest 의 `tasty-plugin.toml` 에서 semver 만 읽음. corrupt/parse-fail → None.
fn read_installed_version(dest: &Path) -> Option<semver::Version> {
    Manifest::load(dest)
        .ok()
        .and_then(|m| semver::Version::parse(&m.version).ok())
}

/// bundle 의 `tasty-plugin.toml` 에서 semver 만 읽음. corrupt/parse-fail → None.
fn read_bundle_version(src: &Path) -> Option<semver::Version> {
    Manifest::load(src)
        .ok()
        .and_then(|m| semver::Version::parse(&m.version).ok())
}

/// force를 우선 적용하고 그 외에는 번들과 설치본 버전을 비교한다.
/// 번들 버전을 읽지 못하면 건너뛰고, 설치본만 읽지 못하면 내용 비교로 복구한다.
pub(crate) fn decide_builtin_upgrade(
    installed: Option<&semver::Version>,
    bundle: Option<&semver::Version>,
    force: bool,
) -> BuiltinUpgradeDecision {
    if force {
        return BuiltinUpgradeDecision::ForceOverwrite;
    }
    let bundle = match bundle {
        Some(b) => b,
        None => return BuiltinUpgradeDecision::Skip,
    };
    let installed = match installed {
        Some(i) => i,
        None => return BuiltinUpgradeDecision::ResyncSameVersion,
    };
    match bundle.cmp(installed) {
        std::cmp::Ordering::Greater => BuiltinUpgradeDecision::UpgradeVersion {
            from: installed.clone(),
            to: bundle.clone(),
        },
        std::cmp::Ordering::Equal => BuiltinUpgradeDecision::ResyncSameVersion,
        std::cmp::Ordering::Less => BuiltinUpgradeDecision::Skip,
    }
}

/// 이 디렉터리에서 원본에 없는 대상 항목을 제거한다. 하위 순회는 호출자가 한다.
fn prune_dest_not_in_src(src: &Path, dest: &Path) -> std::io::Result<()> {
    let src_names: HashSet<OsString> = std::fs::read_dir(src)?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name())
        .collect();
    for entry in std::fs::read_dir(dest)? {
        let entry = entry?;
        if !src_names.contains(&entry.file_name()) {
            if entry.file_type()?.is_dir() {
                std::fs::remove_dir_all(entry.path())?;
            } else {
                tasty_utils::file_replace::remove(&entry.path())?;
            }
        }
    }
    Ok(())
}

/// 파일 복사 조건. 디렉터리 순회와 불필요한 파일 제거는 sync_dir가 공통으로 처리한다.
#[derive(Clone, Copy)]
enum CopyPolicy {
    /// 판정 없이 덮어쓴다 — 버전이 다를 때의 전량 교체.
    Always,
    /// src 가 더 새것이거나, 같은 눈금이면 내용으로([`copy_if_newer`]). dev 스테이징.
    NewerThenContent,
    /// 내용이 다를 때만([`copy_file_if_content_differs`]). 번들 → 홈 설치.
    ContentDiffers,
}

/// 하위 디렉터리를 순회해 원본에 없는 항목을 제거하고 정책에 맞는 파일을 복사한다.
/// 반환값은 파일 복사 여부다. 삭제만 수행한 경우는 포함하지 않는다.
fn sync_dir(src: &Path, dst: &Path, policy: CopyPolicy) -> std::io::Result<bool> {
    std::fs::create_dir_all(dst)?;
    prune_dest_not_in_src(src, dst)?;
    let mut wrote = false;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dest_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            wrote |= sync_dir(&src_path, &dest_path, policy)?;
        } else {
            wrote |= match policy {
                CopyPolicy::Always => {
                    copy_atomic(&src_path, &dest_path)?;
                    true
                }
                CopyPolicy::NewerThenContent => copy_if_newer(&src_path, &dest_path)?,
                CopyPolicy::ContentDiffers => copy_file_if_content_differs(&src_path, &dest_path)?,
            };
        }
    }
    Ok(wrote)
}

/// 원본의 모든 파일을 복사하고 원본에 없는 대상 항목을 제거한다.
fn overwrite_builtin_dir(src: &Path, dest: &Path) -> std::io::Result<()> {
    sync_dir(src, dest, CopyPolicy::Always).map(|_| ())
}

/// 번들 plugin 디렉터리들이 있는 루트 경로.
///
/// - 첫째: `TASTY_BUILTIN_PLUGINS_DIR` 환경 변수 강제 override.
/// - 둘째 (macOS 한정): `.app` 번들의 `Contents/Resources/plugins/`.
/// - 셋째: 실행 파일 옆 `plugins/` (release/dist에서 packaging 시 함께 복사,
///   portable .tar.gz / .zip 설치).
/// - 넷째: 실행 파일 옆 `builtin-plugins/`. debug 에서만 workspace 원본을
///   mtime 및 동률 시 내용 비교로 동기화한다. release/dist 는 빌드 단계가 스테이징한 번들을
///   그대로 사용하며, 시작 시 workspace 원본을 복사하지 않는다.
/// - 다섯째 (linux 한정): FHS 표준 경로 `/usr/lib/tasty/plugins/`,
///   `/usr/share/tasty/plugins/` — `.deb` / `.rpm` 패키지가 `/usr/bin/tasty` 옆이
///   아닌 FHS 친화 위치에 plugin 을 설치한다. exe-relative 보다 우선순위가
///   낮음 — 같은 머신에 portable archive 가 풀려있으면 그쪽이 항상 우선이라
///   두 설치 형태가 충돌하지 않는다.
pub fn bundle_root() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("TASTY_BUILTIN_PLUGINS_DIR") {
        let path = PathBuf::from(p);
        if path.is_dir() {
            return Some(path);
        }
    }
    if let Some(found) = bundle_root_exe_relative() {
        return Some(found);
    }
    #[cfg(target_os = "linux")]
    {
        for sys_path in ["/usr/lib/tasty/plugins", "/usr/share/tasty/plugins"] {
            let path = PathBuf::from(sys_path);
            if path.is_dir() {
                return Some(path);
            }
        }
    }
    None
}

/// 실행 파일 기준 번들 탐색. 실행 파일 경로 조회에 실패해도 FHS 탐색은 계속할 수 있게 분리한다.
fn bundle_root_exe_relative() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    bundle_root_from_exe_dir(exe.parent()?)
}

/// 실행 파일 기준으로 번들을 찾는다. debug에서는 개발 번들도 준비한다.
/// 통합 테스트도 이 함수를 사용해 자식 인스턴스와 같은 번들을 선택한다.
pub fn bundle_root_from_exe_dir(exe_dir: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        // macOS codesign이 실행 파일 디렉터리를 nested bundle로 오해하지 않도록
        // Contents/Resources/plugins를 사용한다. 실제 Contents/MacOS 배치에서만 찾는다.
        let bundle_contents = exe_dir
            .parent()
            .filter(|_| exe_dir.file_name().is_some_and(|n| n == "MacOS"));
        if let Some(contents) = bundle_contents {
            let in_resources = contents.join("Resources").join("plugins");
            if in_resources.is_dir() {
                return Some(in_resources);
            }
        }
    }
    let next_to_exe = exe_dir.join("plugins");
    if next_to_exe.is_dir() {
        return Some(next_to_exe);
    }
    // Release bundles pair signed manifests with build-time artifacts.
    // Never replace individual files from a subsequently edited workspace.
    if cfg!(debug_assertions)
        && let Some(dev) = ensure_dev_bundle(exe_dir)
    {
        return Some(dev);
    }
    let dev_bundle = exe_dir.join("builtin-plugins");
    if dev_bundle.is_dir() {
        return Some(dev_bundle);
    }
    None
}

/// debug 실행 파일 위의 워크스페이스에서 매니페스트·바이너리·번역을 동기화한다.
/// 하나라도 준비되면 번들 경로를 반환하며 워크스페이스가 없으면 동기화하지 않는다.
fn ensure_dev_bundle(exe_dir: &Path) -> Option<PathBuf> {
    // exe_dir = .../target/<profile>
    let target_dir = exe_dir.parent()?; // .../target
    let workspace = target_dir.parent()?; // workspace root

    let bundle_root = exe_dir.join("builtin-plugins");
    let mut any_synced = false;
    for spec in BUILTINS {
        if sync_builtin_dev(workspace, exe_dir, &bundle_root, spec) {
            any_synced = true;
        }
    }
    if any_synced {
        Some(bundle_root)
    } else {
        // bundle_root가 이미 존재할 수도 있다 (이전 부팅에서 동기화됨).
        // 그 경우엔 bundle_root() 호출자가 별도 분기에서 fallback으로 발견.
        if bundle_root.is_dir() {
            Some(bundle_root)
        } else {
            None
        }
    }
}

/// 한 builtin plugin을 dev bundle로 동기화. 바이너리 또는 매니페스트가
/// workspace에 없으면 (예: codex만 빌드 안 됨, 또는 배포 패키지라 `crates/`가 아예
/// 없음) false 반환. release/dist 는 이 경로를 호출하지 않는다.
fn sync_builtin_dev(
    workspace: &Path,
    exe_dir: &Path,
    bundle_root: &Path,
    spec: &BuiltinSpec,
) -> bool {
    let plugin_bin = exe_dir.join(spec.bin_name);
    let crate_dir = workspace.join("crates").join(spec.crate_dir);
    let src_manifest = crate_dir.join("tasty-plugin.toml");
    if !plugin_bin.exists() || !src_manifest.exists() {
        return false;
    }

    let dest_dir = bundle_root.join(spec.id);
    if !sync_builtin_dev_required(&plugin_bin, &src_manifest, &dest_dir, spec) {
        return false;
    }
    sync_builtin_dev_sig(&crate_dir, &dest_dir, spec);
    sync_builtin_dev_lang(&crate_dir, &dest_dir, spec);
    true
}

/// 디렉터리 생성, 매니페스트·바이너리 복사는 하나라도 실패하면 중단한다.
/// 서명과 번역 복사는 별도로 시도한다.
fn sync_builtin_dev_required(
    plugin_bin: &Path,
    src_manifest: &Path,
    dest_dir: &Path,
    spec: &BuiltinSpec,
) -> bool {
    dev_sync_step(
        || std::fs::create_dir_all(dest_dir),
        || format!("dev bundle: mkdir {} failed", dest_dir.display()),
    ) && dev_sync_step(
        || copy_if_newer(src_manifest, &dest_dir.join("tasty-plugin.toml")),
        || format!("dev bundle: copy manifest for {} failed", spec.id),
    ) && dev_sync_step(
        || copy_if_newer(plugin_bin, &dest_dir.join(spec.bin_name)),
        || format!("dev bundle: copy binary for {} failed", spec.id),
    )
}

/// 한 fallible 스텝 실행 — 실패 시 `<msg>: <e>` 형식으로 warn 후 `false`.
fn dev_sync_step<T>(op: impl FnOnce() -> std::io::Result<T>, msg: impl FnOnce() -> String) -> bool {
    if let Err(e) = op() {
        tracing::warn!("{}: {e}", msg());
        return false;
    }
    true
}

/// 매니페스트 서명 sidecar(.sig)가 있으면 함께 동기화한다. 없으면 (미서명 dev
/// workspace) skip — debug 빌드는 trust gate 를 우회하므로 무방하다.
/// release/dist 의 서명 스테이징은 빌드 단계가 담당한다.
/// 복사 실패는 비치명 (debug 는 어차피 우회) 이므로 warn 만 남긴다.
fn sync_builtin_dev_sig(crate_dir: &Path, dest_dir: &Path, spec: &BuiltinSpec) {
    let src_sig = crate_dir.join("tasty-plugin.toml.sig");
    if src_sig.exists()
        && let Err(e) = copy_if_newer(&src_sig, &dest_dir.join("tasty-plugin.toml.sig"))
    {
        tracing::warn!("dev bundle: copy sig for {} failed: {e}", spec.id);
    }
}

/// plugin lang/ 디렉토리도 함께 동기화 (i18n 키 호스트 머지에 필요) — best-effort.
fn sync_builtin_dev_lang(crate_dir: &Path, dest_dir: &Path, spec: &BuiltinSpec) {
    let src_lang = crate_dir.join("lang");
    if src_lang.is_dir() {
        let dest_lang = dest_dir.join("lang");
        if let Err(e) = sync_dir_if_newer(&src_lang, &dest_lang) {
            tracing::warn!("dev bundle: copy lang for {} failed: {e}", spec.id);
        }
    }
}

/// 개발 번들의 번역 파일을 동기화한다. 수정 시각이 같으면 내용을 비교한다.
/// 원본에서 사라진 파일도 제거하므로 대상에는 사용자 파일을 섞으면 안 된다.
/// 현재 호출 대상은 이 코드가 준비한 builtin-plugins/<id>/lang 디렉터리다.
fn sync_dir_if_newer(src: &Path, dst: &Path) -> std::io::Result<()> {
    sync_dir(src, dst, CopyPolicy::NewerThenContent).map(|_| ())
}

/// 원본이 더 새로우면 복사하고, 시각이 같으면 내용이 다를 때만 복사한다.
/// cp -p나 압축 해제로 시각이 같아져도 내용 변경을 반영하기 위한 비교다.
/// 매번 번들 전체를 읽는 비용을 피하려 대상이 더 새로우면 비교하지 않는다.
/// 따라서 과거 시각을 유지한 채 원본 내용만 바꾼 경우는 놓칠 수 있다.
fn copy_if_newer(src: &Path, dest: &Path) -> std::io::Result<bool> {
    if let (Ok(src_meta), Ok(dest_meta)) = (std::fs::metadata(src), std::fs::metadata(dest))
        && let (Ok(sm), Ok(dm)) = (src_meta.modified(), dest_meta.modified())
    {
        if sm < dm {
            return Ok(false);
        }
        // 판정 불가(읽기 실패)는 복사 쪽으로 보낸다 — "같다"고 **확인된** 때만 건너뛴다.
        if sm == dm && matches!(file_content_differs(src, dest), Ok(false)) {
            return Ok(false);
        }
    }
    copy_atomic(src, dest)?;
    Ok(true)
}

/// 같은 디렉터리의 임시 파일에 복사한 뒤 rename으로 대상을 교체한다.
/// macOS에서 실행 파일을 제자리 덮어쓰면 캐시된 코드 서명과 달라져 실행이 거부될 수 있다.
/// macOS의 quarantine 같은 보안 속성은 바꾸지 않는다. 복사본은 원본의 속성을 그대로 가질 수 있고,
/// 속성 처리는 사용자가 정한다(docs/principles/control-and-data.md#본체와-기반-기술).
fn copy_atomic(src: &Path, dest: &Path) -> std::io::Result<()> {
    let parent = dest.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "destination path has no parent",
        )
    })?;
    // 같은 디렉터리 안에 temp를 만들어야 rename이 cross-filesystem 에러를 안 낸다.
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let file_name = dest.file_name().and_then(|s| s.to_str()).unwrap_or("file");
    let tmp = parent.join(format!(".{file_name}.tmp.{pid}.{nanos:x}"));

    if let Err(e) = std::fs::copy(src, &tmp) {
        // tmp가 부분 생성됐을 수 있으니 best-effort 정리. NotFound는 정상.
        if let Err(re) = std::fs::remove_file(&tmp)
            && re.kind() != std::io::ErrorKind::NotFound
        {
            tracing::trace!("builtin install tmp {} cleanup failed: {re}", tmp.display());
        }
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&tmp, dest) {
        if let Err(re) = std::fs::remove_file(&tmp)
            && re.kind() != std::io::ErrorKind::NotFound
        {
            tracing::trace!("builtin install tmp {} cleanup failed: {re}", tmp.display());
        }
        return Err(e);
    }
    Ok(())
}

/// 모든 기본 제공 플러그인을 점검:
/// 1. 사용자 디렉터리에 없고 `removed_builtins` 목록에도 없으면 번들에서 복사 +
///    매니페스트 권한을 자동 grant.
/// 2. 이미 사용자 디렉터리에 있지만 `plugins.toml`에 grant 엔트리가 한 번도
///    기록된 적 없는 builtin은 매니페스트 권한을 자동 grant (이전 버전에서
///    builtin으로 인식되지 않은 채 설치된 plugin 복구). 사용자가 명시적으로
///    빈 리스트로 둔 경우(`granted = []`)는 entry는 있으니 건드리지 않는다.
///
/// 실패한 항목은 warn 로그만 남기고 계속 진행 (다른 builtin은 영향 없음).
pub fn install_builtins_if_needed(mgr: &mut PluginManager) {
    let dest_root = match discovery::plugin_root() {
        Some(p) => p,
        None => {
            tracing::warn!("install_builtins: cannot resolve plugin root");
            return;
        }
    };
    let bundle = bundle_root();

    let before = mgr.config.clone();
    let mut config_dirty = false;
    for spec in BUILTINS {
        let dest = dest_root.join(spec.id);
        let already_present = dest.exists();

        // Step 1: 번들에서 복사. 항목별 실패는 warn 후 다음 spec 으로 (Step 2 skip).
        if install_builtin_bundle_step(
            mgr,
            spec,
            &dest_root,
            &dest,
            already_present,
            bundle.as_deref(),
        ) {
            continue;
        }

        // Step 2: dest가 존재하면 매니페스트 권한을 grant entry에 반영.
        if install_builtin_grant_step(mgr, spec, &dest) {
            config_dirty = true;
        }
    }
    if config_dirty && let Err(e) = file_upgrade::persist_boot_config(mgr, before) {
        tracing::warn!("install_builtins: save plugins.toml failed: {e}");
    }
}

/// 번들에서 신규 설치하거나 버전·내용 비교로 갱신한다. 명시적으로 제거한 plugin은 제외한다.
/// 설치본이 더 높은 버전이면 유지하며 되돌리려면 upgrade-builtins --force를 사용한다.
/// true는 실패를 뜻하며 호출자는 권한 갱신을 건너뛰고 다음 plugin을 처리한다.
fn install_builtin_bundle_step(
    mgr: &PluginManager,
    spec: &BuiltinSpec,
    dest_root: &Path,
    dest: &Path,
    already_present: bool,
    bundle: Option<&Path>,
) -> bool {
    if mgr.config.is_builtin_removed(spec.id) {
        return false;
    }
    let Some(bundle) = bundle else {
        return false;
    };
    let src = match crate::official::bundle_source(bundle, spec.id) {
        Ok(Some(src)) => src,
        Ok(None) => return false,
        Err(_) => return true,
    };
    if let Err(e) = std::fs::create_dir_all(dest_root) {
        tracing::warn!(
            "install_builtins: mkdir {} failed: {e}",
            dest_root.display()
        );
        return true;
    }
    if already_present {
        install_builtin_overwrite_present(spec, &src, dest)
    } else {
        install_builtin_fresh_copy(spec, &src, dest)
    }
}

/// 사용자 디렉터리에 아직 없는 builtin 을 번들에서 신규 복사.
/// 반환값: `true` 면 실패(warn 후 계속) — caller 는 Step 2 skip.
fn install_builtin_fresh_copy(spec: &BuiltinSpec, src: &Path, dest: &Path) -> bool {
    if let Err(e) = copy_dir_recursive(src, dest) {
        tracing::warn!("install_builtins: copy '{}' failed: {e}", spec.id);
        return true;
    }
    tracing::info!("installed builtin plugin '{}' from bundle", spec.id);
    false
}

/// 이미 설치된 plugin을 강제 덮어쓰기 없이 갱신한다. true면 실패다.
fn install_builtin_overwrite_present(spec: &BuiltinSpec, src: &Path, dest: &Path) -> bool {
    let installed_v = read_installed_version(dest);
    let bundle_v = read_bundle_version(src);
    match decide_builtin_upgrade(installed_v.as_ref(), bundle_v.as_ref(), false) {
        BuiltinUpgradeDecision::Skip => {
            log_builtin_skip(spec.id, installed_v.as_ref(), bundle_v.as_ref());
            false
        }
        BuiltinUpgradeDecision::ResyncSameVersion => run_dir_sync_step(
            || sync_dir_by_content(src, dest).map(|_| ()),
            spec.id,
            "resync",
        ),
        BuiltinUpgradeDecision::UpgradeVersion { from, to } => {
            tracing::info!("upgrading builtin '{}' v{} → v{}", spec.id, from, to);
            run_dir_sync_step(|| overwrite_builtin_dir(src, dest), spec.id, "upgrade")
        }
        BuiltinUpgradeDecision::ForceOverwrite => {
            // `tasty plugin upgrade-builtins --force` 만 여기 온다 — 버전·내용 무시하고
            // 통째로 덮어쓴다. 부팅 경로는 force 를 안 넘기므로 이 팔에 오지 않는다.
            log_builtin_force_overwrite(spec.id, installed_v.as_ref(), bundle_v.as_ref());
            run_dir_sync_step(
                || overwrite_builtin_dir(src, dest),
                spec.id,
                "force-overwrite",
            )
        }
    }
}

/// 건너뛴 설치본이 번들과 같은 버전인지 더 높은 버전인지 구분한다.
#[derive(Debug, PartialEq, Eq)]
enum SkipCase {
    /// 설치본과 번들이 같은 버전이다.
    UpToDate,
    /// 설치본이 번들보다 **높다.** 최신이 아니라 번들보다 **앞선** 것이고, 이번 부팅은
    /// 번들을 반영하지 않는다.
    InstalledAheadOfBundle,
}

/// 버전 두 개로 `Skip` 의 경우를 가른다. 판정을 로그에서 떼어내 시험이 직접 부를 수 있게 한다.
fn classify_skip(
    installed_v: Option<&semver::Version>,
    bundle_v: Option<&semver::Version>,
) -> SkipCase {
    match (installed_v, bundle_v) {
        (Some(i), Some(b)) if i > b => SkipCase::InstalledAheadOfBundle,
        _ => SkipCase::UpToDate,
    }
}

/// 설치본이 더 높으면 이번 번들을 적용하지 않았다는 경고와 강제 갱신 명령을 남긴다.
/// 같은 버전의 확인 메시지는 debug 레벨로 남긴다.
fn log_builtin_skip(
    id: &str,
    installed_v: Option<&semver::Version>,
    bundle_v: Option<&semver::Version>,
) {
    match classify_skip(installed_v, bundle_v) {
        SkipCase::UpToDate => tracing::debug!(
            "builtin '{}' up-to-date (installed v{:?}, bundle v{:?})",
            id,
            installed_v.map(|v| v.to_string()),
            bundle_v.map(|v| v.to_string()),
        ),
        SkipCase::InstalledAheadOfBundle => tracing::warn!(
            "builtin '{}' installed v{:?} is ahead of bundle v{:?} — this boot keeps the \
             installed copy and does NOT apply the bundle; run \
             `tasty plugin upgrade-builtins --force` to overwrite it",
            id,
            installed_v.map(|v| v.to_string()),
            bundle_v.map(|v| v.to_string()),
        ),
    }
}

fn log_builtin_force_overwrite(
    id: &str,
    installed_v: Option<&semver::Version>,
    bundle_v: Option<&semver::Version>,
) {
    tracing::info!(
        "force-overwriting builtin '{}' (installed v{:?}, bundle v{:?})",
        id,
        installed_v.map(|v| v.to_string()),
        bundle_v.map(|v| v.to_string()),
    );
}

/// 디렉터리 동기화 스텝 실행 공통 헬퍼 — 실패 시 `install_builtins: <verb> '<id>' failed`
/// 형식으로 warn 후 `true`(실패) 반환, 성공 시 `false`.
fn run_dir_sync_step(op: impl FnOnce() -> std::io::Result<()>, id: &str, verb: &str) -> bool {
    if let Err(e) = op() {
        tracing::warn!("install_builtins: {verb} '{id}' failed: {e}");
        return true;
    }
    false
}

/// `install_builtins_if_needed` Step 2 — dest 존재 시 매니페스트 권한을 grant entry에 반영.
///   - grant entry 없음 → 매니페스트 권한 전체를 set (최초 install).
///   - grant entry 있음 → 매니페스트 신규 추가분만 증분 grant (기존 사용자 대상으로 새
///     버전 builtin이 추가한 permission을 자동 수용).
///
/// 기존에 grant된 토큰은 *제거하지 않는다*. 사용자가 명시적 deny한 경우는 본 helper 책임 밖.
/// 매니페스트에서 사라진 token은 다음 install 시점에 set_granted로 덮어쓰일 때만 정리된다.
///
/// 반환값: config 를 변경했으면 `true` (caller 가 dirty 마킹).
fn install_builtin_grant_step(mgr: &mut PluginManager, spec: &BuiltinSpec, dest: &Path) -> bool {
    if !dest.exists() {
        return false;
    }
    let Ok(manifest) = Manifest::load(dest) else {
        return false;
    };
    // 다른 id를 선언한 폴더의 권한을 이 공식 id로 기록하지 않는다.
    if manifest.permissions.is_empty() || manifest.id != spec.id {
        return false;
    }
    if !mgr.config.grants.contains_key(spec.id) {
        mgr.config
            .set_granted(spec.id, manifest.permissions.clone());
        tracing::info!(
            "auto-granted manifest permissions for builtin '{}'",
            spec.id
        );
        true
    } else if apply_builtin_permission_diff(&mut mgr.config, spec.id, &manifest.permissions) {
        tracing::info!(
            "auto-granted new manifest permissions for builtin '{}'",
            spec.id
        );
        true
    } else {
        false
    }
}

/// 한 builtin plugin 이 본 실행에서 처리된 결과. CLI/IPC 응답에 포함된다.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BuiltinUpgradeItem {
    pub id: String,
    pub action: BuiltinUpgradeAction,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BuiltinUpgradeAction {
    /// 설치 건너뜀 또는 같은 버전의 내용 동기화 결과. 상세는 reason에 담는다.
    Skipped {
        installed_version: Option<String>,
        bundle_version: Option<String>,
        reason: String,
    },
    /// bundle > installed — 디렉토리 교체 성공.
    Upgraded {
        from: String,
        to: String,
        /// `--restart-running` 경로에서 swap restart 가 성공했는지. 기존 client
        /// 호환을 위해 `#[serde(default)]`.
        #[serde(default)]
        was_restarted: bool,
        /// swap restart 가 실패한 경우 에러 메시지. 성공 / 미시도면 `None`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        restart_error: Option<String>,
    },
    /// force=true 또는 신규 install — 강제 (재)설치 성공. version 은 bundle 의 값.
    Reinstalled {
        version: String,
        #[serde(default)]
        was_restarted: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        restart_error: Option<String>,
    },
    /// bundle 디렉토리 자체에 해당 plugin 이 없음 (dev partial build 등).
    NotInBundle,
    /// 파일 IO 실패 — Windows sharing violation 등.
    Failed { reason: String },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BuiltinUpgradeReport {
    pub items: Vec<BuiltinUpgradeItem>,
}

/// 명시적인 번들 업데이트. force와 제거 목록 복구, 실행 중 plugin의 재시작 옵션을 받는다.
/// plugin별 결과를 반환하고 파일이 변경된 항목의 매니저 상태를 갱신한다.
/// 회수 중이던 프로세스의 재시작 예약도 보존한다.
pub fn upgrade_builtins(
    mgr: &mut PluginManager,
    force: bool,
    restore_removed: &[String],
    restore_all: bool,
    restart_running: bool,
) -> BuiltinUpgradeReport {
    let dest_root = match discovery::plugin_root() {
        Some(p) => p,
        None => {
            tracing::warn!("upgrade_builtins: cannot resolve plugin root");
            return BuiltinUpgradeReport {
                items: vec![BuiltinUpgradeItem {
                    id: String::new(),
                    action: BuiltinUpgradeAction::Failed {
                        reason: "cannot resolve plugin root".into(),
                    },
                }],
            };
        }
    };
    let bundle = bundle_root();
    let mut items = Vec::with_capacity(BUILTINS.len());
    let mut changed_ids: Vec<String> = Vec::new();

    restore_removed_builtins(mgr, restore_removed, restore_all);

    for spec in BUILTINS {
        let outcome = process_builtin_upgrade(
            mgr,
            spec,
            &dest_root,
            bundle.as_deref(),
            force,
            restart_running,
        );
        if outcome.changed {
            changed_ids.push(spec.id.into());
        }
        items.push(outcome.item);
    }

    if !changed_ids.is_empty() {
        finalize_builtin_upgrades(mgr, &dest_root, &changed_ids);
    }

    BuiltinUpgradeReport { items }
}

/// restore_all이면 제거 목록 전체를, 아니면 지정된 ID만 복구하고 변경을 저장한다.
fn restore_removed_builtins(
    mgr: &mut PluginManager,
    restore_removed: &[String],
    restore_all: bool,
) {
    let before = mgr.config.clone();
    let removed_cleared = apply_restore_removed(mgr, restore_removed, restore_all);
    if removed_cleared && let Err(e) = file_upgrade::persist_boot_config(mgr, before) {
        tracing::warn!("upgrade_builtins: save plugins.toml after unmark failed: {e}");
    }
}

/// `restore_all` 이면 전체 clear(명시된 id 무시), 아니면 명시된 id 만 unmark.
/// 반환값: 하나라도 실제로 바뀌었으면 `true`(caller 가 save 여부 판단).
fn apply_restore_removed(
    mgr: &mut PluginManager,
    restore_removed: &[String],
    restore_all: bool,
) -> bool {
    if restore_all {
        let cleared = mgr.config.clear_removed_builtins();
        if cleared {
            tracing::info!("upgrade_builtins: cleared all removed_builtins (restore_all)");
        }
        return cleared;
    }
    let mut removed_cleared = false;
    for id in restore_removed {
        if mgr.config.unmark_builtin_removed(id) {
            removed_cleared = true;
            tracing::info!("upgrade_builtins: restored '{id}' from removed_builtins");
        }
    }
    removed_cleared
}

/// 한 builtin spec 의 upgrade 처리 결과. `changed` 면 caller 가 changed_ids 에 push.
struct SpecUpgrade {
    item: BuiltinUpgradeItem,
    changed: bool,
}

/// 파일 교체 뒤 선택적으로 수행한 재시작 결과.
struct SwapResult {
    was_restarted: bool,
    restart_error: Option<String>,
}

fn swap_overwrite_respawn(
    mgr: &mut PluginManager,
    spec: &BuiltinSpec,
    src: &Path,
    dest: &Path,
    restart_running: bool,
) -> Result<SwapResult, BuiltinUpgradeItem> {
    let should_swap = restart_running && mgr.is_running(spec.id);
    if should_swap && let Err(e) = mgr.swap_shutdown_internal(spec.id) {
        return Err(BuiltinUpgradeItem {
            id: spec.id.into(),
            action: BuiltinUpgradeAction::Failed {
                reason: format!("swap-shutdown-failed: {e}"),
            },
        });
    }
    if let Err(e) = overwrite_builtin_dir(src, dest) {
        if should_swap && let Err(re) = mgr.swap_respawn_internal(spec.id) {
            tracing::warn!(
                "respawn after failed overwrite of '{}' failed: {re}",
                spec.id
            );
        }
        return Err(BuiltinUpgradeItem {
            id: spec.id.into(),
            action: BuiltinUpgradeAction::Failed {
                reason: e.to_string(),
            },
        });
    }
    let restart_error = if should_swap {
        mgr.swap_respawn_internal(spec.id)
            .err()
            .map(|e| e.to_string())
    } else {
        None
    };
    Ok(SwapResult {
        was_restarted: should_swap && restart_error.is_none(),
        restart_error,
    })
}

fn not_in_bundle_item(spec: &BuiltinSpec) -> BuiltinUpgradeItem {
    BuiltinUpgradeItem {
        id: spec.id.into(),
        action: BuiltinUpgradeAction::NotInBundle,
    }
}

/// release 빌드 한정: bundle manifest 의 ed25519 detached signature 검증.
/// sidecar 누락 / 변조 / 잘못된 서명 → Err(Skipped) 로 차단.
/// debug 빌드는 dev workspace bundle 이 unsigned 라 warn 로깅 후 Ok 통과.
fn verify_builtin_bundle_trust(
    spec: &BuiltinSpec,
    src: &Path,
    dest: &Path,
) -> Result<(), BuiltinUpgradeItem> {
    let known = crate::known_plugins::KnownPlugins::load().unwrap_or_default();
    file_upgrade::verify_trust(spec, src, dest, &known)
}

/// dest 미존재 시 번들에서 신규 설치. 성공 → Reinstalled(changed), 실패 → Failed.
fn install_new_builtin(
    spec: &BuiltinSpec,
    src: &Path,
    dest: &Path,
    dest_root: &Path,
    bundle_v: Option<&semver::Version>,
) -> SpecUpgrade {
    if let Err(e) = std::fs::create_dir_all(dest_root).and_then(|_| copy_dir_recursive(src, dest)) {
        return SpecUpgrade {
            item: BuiltinUpgradeItem {
                id: spec.id.into(),
                action: BuiltinUpgradeAction::Failed {
                    reason: e.to_string(),
                },
            },
            changed: false,
        };
    }
    tracing::info!("installed builtin plugin '{}' from bundle", spec.id);
    SpecUpgrade {
        item: BuiltinUpgradeItem {
            id: spec.id.into(),
            action: BuiltinUpgradeAction::Reinstalled {
                version: bundle_v.map(|v| v.to_string()).unwrap_or_default(),
                was_restarted: false,
                restart_error: None,
            },
        },
        changed: true,
    }
}

/// dest 존재 시 decide_builtin_upgrade 판정 → 해당 분기 적용.
#[allow(clippy::too_many_arguments)] // reason: 호출부가 따로 구한 판정 입력을 그대로 넘긴다 — 묶어도 조립 코드만 늘고 재사용할 호출부가 없다
fn apply_builtin_upgrade_decision(
    mgr: &mut PluginManager,
    spec: &BuiltinSpec,
    src: &Path,
    dest: &Path,
    installed_v: Option<semver::Version>,
    bundle_v: Option<semver::Version>,
    force: bool,
    restart_running: bool,
) -> SpecUpgrade {
    // 회수 중인 프로세스의 파일을 실제로 변경할 때만 회수를 기다린다.
    // 변경이 없으면 회수와 재시작 예약을 그대로 둔다.
    let mut respawn = false;
    let upgrade = match decide_builtin_upgrade(installed_v.as_ref(), bundle_v.as_ref(), force) {
        BuiltinUpgradeDecision::Skip => SpecUpgrade {
            item: BuiltinUpgradeItem {
                id: spec.id.into(),
                action: BuiltinUpgradeAction::Skipped {
                    installed_version: installed_v.map(|v| v.to_string()),
                    bundle_version: bundle_v.map(|v| v.to_string()),
                    reason: "installed >= bundle".into(),
                },
            },
            changed: false,
        },
        BuiltinUpgradeDecision::ResyncSameVersion => {
            let (upgrade, waited_respawn) =
                resync_same_version(mgr, spec, src, dest, installed_v, bundle_v);
            respawn = waited_respawn;
            upgrade
        }
        BuiltinUpgradeDecision::UpgradeVersion { from, to } => {
            tracing::info!("upgrading builtin '{}' v{} → v{}", spec.id, from, to);
            respawn = mgr.wait_retired(spec.id);
            match swap_overwrite_respawn(mgr, spec, src, dest, restart_running) {
                Ok(swap) => SpecUpgrade {
                    item: BuiltinUpgradeItem {
                        id: spec.id.into(),
                        action: BuiltinUpgradeAction::Upgraded {
                            from: from.to_string(),
                            to: to.to_string(),
                            was_restarted: swap.was_restarted,
                            restart_error: swap.restart_error,
                        },
                    },
                    changed: true,
                },
                Err(item) => SpecUpgrade {
                    item,
                    changed: false,
                },
            }
        }
        BuiltinUpgradeDecision::ForceOverwrite => {
            tracing::info!(
                "force-reinstalling builtin '{}' (installed v{:?}, bundle v{:?})",
                spec.id,
                installed_v.as_ref().map(|v| v.to_string()),
                bundle_v.as_ref().map(|v| v.to_string()),
            );
            respawn = mgr.wait_retired(spec.id);
            match swap_overwrite_respawn(mgr, spec, src, dest, restart_running) {
                Ok(swap) => SpecUpgrade {
                    item: BuiltinUpgradeItem {
                        id: spec.id.into(),
                        action: BuiltinUpgradeAction::Reinstalled {
                            version: bundle_v.as_ref().map(|v| v.to_string()).unwrap_or_default(),
                            was_restarted: swap.was_restarted,
                            restart_error: swap.restart_error,
                        },
                    },
                    changed: true,
                },
                Err(item) => SpecUpgrade {
                    item,
                    changed: false,
                },
            }
        }
    };
    // 회수를 기다리며 가져온 재시작 예약을 새 파일로 이어간다.
    if respawn {
        mgr.start_if_still_wanted(spec.id);
    }
    upgrade
}

/// 같은 버전 갈래 — 내용이 다른 파일만 옮긴다. 두 번째 값은 회수를 기다리며 가져온 재기동
/// 예약이다([`PluginManager::wait_retired`]).
fn resync_same_version(
    mgr: &mut PluginManager,
    spec: &BuiltinSpec,
    src: &Path,
    dest: &Path,
    installed_v: Option<semver::Version>,
    bundle_v: Option<semver::Version>,
) -> (SpecUpgrade, bool) {
    // 회수 중이면 쓰기 전에 기다려야 하는데, 이 갈래는 대개 아무것도 안 쓴다 — 쓸 것이 있을
    // 때만 기다린다.
    let mut respawn = false;
    if mgr.is_retiring(spec.id) && sync_probe::sync_would_touch(src, dest) {
        respawn = mgr.wait_retired(spec.id);
    }
    let upgrade = match sync_dir_by_content(src, dest) {
        Err(e) => SpecUpgrade {
            item: BuiltinUpgradeItem {
                id: spec.id.into(),
                action: BuiltinUpgradeAction::Failed {
                    reason: e.to_string(),
                },
            },
            changed: false,
        },
        Ok(wrote) => SpecUpgrade {
            item: BuiltinUpgradeItem {
                id: spec.id.into(),
                action: BuiltinUpgradeAction::Skipped {
                    installed_version: installed_v.map(|v| v.to_string()),
                    bundle_version: bundle_v.map(|v| v.to_string()),
                    reason: if wrote {
                        "same-version (content resync: files rewritten)".into()
                    } else {
                        "same-version (content resync: nothing to write)".into()
                    },
                },
            },
            // 같은 버전이라도 복사한 파일이 있으면 후속 갱신 대상으로 알린다.
            changed: wrote,
        },
    };
    (upgrade, respawn)
}

/// 한 builtin spec 의 upgrade 전체 처리 (removed → not-in-bundle → 서명 → install/decide).
fn process_builtin_upgrade(
    mgr: &mut PluginManager,
    spec: &BuiltinSpec,
    dest_root: &Path,
    bundle: Option<&Path>,
    force: bool,
    restart_running: bool,
) -> SpecUpgrade {
    let dest = dest_root.join(spec.id);

    if mgr.config.is_builtin_removed(spec.id) {
        return SpecUpgrade {
            item: BuiltinUpgradeItem {
                id: spec.id.into(),
                action: BuiltinUpgradeAction::Skipped {
                    installed_version: read_installed_version(&dest).map(|v| v.to_string()),
                    bundle_version: None,
                    reason: "user-removed".into(),
                },
            },
            changed: false,
        };
    }

    let Some(bundle_root) = bundle else {
        return SpecUpgrade {
            item: not_in_bundle_item(spec),
            changed: false,
        };
    };
    let src = bundle_root.join(spec.id);
    if !src.is_dir() {
        return SpecUpgrade {
            item: not_in_bundle_item(spec),
            changed: false,
        };
    }

    if let Err(item) = verify_builtin_bundle_trust(spec, &src, &dest) {
        return SpecUpgrade {
            item,
            changed: false,
        };
    }

    let installed_v = read_installed_version(&dest);
    let bundle_v = read_bundle_version(&src);

    if !dest.exists() {
        return install_new_builtin(spec, &src, &dest, dest_root, bundle_v.as_ref());
    }

    apply_builtin_upgrade_decision(
        mgr,
        spec,
        &src,
        &dest,
        installed_v,
        bundle_v,
        force,
        restart_running,
    )
}

/// changed builtin 들에 대해 PluginManager state 재계산 + 매니페스트 신규 permission
/// 자동 grant (install_builtins_if_needed step 2 와 동일 규칙).
fn finalize_builtin_upgrades(mgr: &mut PluginManager, dest_root: &Path, changed_ids: &[String]) {
    // 디스크가 바뀌었으니 PluginManager state 재계산.
    mgr.refresh_packages();
    mgr.recompute_extensions();
    let before = mgr.config.clone();
    let mut config_dirty = false;
    for id in changed_ids {
        let dest = dest_root.join(id);
        if !dest.exists() {
            continue;
        }
        if let Ok(manifest) = Manifest::load(&dest) {
            if manifest.permissions.is_empty() {
                continue;
            }
            if !mgr.config.grants.contains_key(id) {
                mgr.config.set_granted(id, manifest.permissions.clone());
                config_dirty = true;
            } else if apply_builtin_permission_diff(&mut mgr.config, id, &manifest.permissions) {
                config_dirty = true;
            }
        }
    }
    if config_dirty && let Err(e) = file_upgrade::persist_boot_config(mgr, before) {
        tracing::warn!("upgrade_builtins: save plugins.toml failed: {e}");
    }
}

/// 기존 권한에 새 매니페스트의 권한을 추가한다. 기존 토큰은 제거하지 않는다.
/// 하나라도 추가했으면 true를 반환한다.
fn apply_builtin_permission_diff(
    config: &mut PluginsConfig,
    id: &str,
    manifest_permissions: &[String],
) -> bool {
    let mut changed = false;
    for token in manifest_permissions {
        if config.grant(id, token) {
            changed = true;
        }
    }
    changed
}

/// uninstall 흐름에서 호출 — built-in인 경우 `removed_builtins`에 등록하여
/// 다음 부팅의 `install_builtins_if_needed`가 다시 복사하지 않게 한다.
/// 외부 플러그인이면 no-op.
pub fn mark_builtin_removed(mgr: &mut PluginManager, id: &str) {
    if !is_builtin_plugin(id) {
        return;
    }
    let before = mgr.config.clone();
    if mgr.config.mark_builtin_removed(id)
        && let Err(e) = file_upgrade::persist_boot_config(mgr, before)
    {
        tracing::warn!("mark_builtin_removed: save plugins.toml failed: {e}");
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            copy_atomic(&entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

/// 내용이 다른 파일을 복사하고 원본에 없는 대상 항목을 제거한다.
/// mtime은 비교하지 않는다. 반환값은 파일 복사 여부이며 삭제만 한 경우는 false다.
fn sync_dir_by_content(src: &Path, dst: &Path) -> std::io::Result<bool> {
    sync_dir(src, dst, CopyPolicy::ContentDiffers)
}

/// 대상이 없거나 크기·내용이 다르면 복사한다. mtime은 비교에 사용하지 않는다.
fn copy_file_if_content_differs(src: &Path, dst: &Path) -> std::io::Result<bool> {
    if !file_content_differs(src, dst)? {
        return Ok(false);
    }
    copy_atomic(src, dst)?;
    Ok(true)
}

/// [`copy_file_if_content_differs`] 의 판정부. dest 를 못 읽으면 "다르다" 로 답한다 —
/// 그래야 읽기 실패가 **안 쓰는 쪽**이 아니라 쓰는 쪽으로 물러난다.
fn file_content_differs(src: &Path, dst: &Path) -> std::io::Result<bool> {
    let (Ok(src_meta), Ok(dst_meta)) = (std::fs::metadata(src), std::fs::metadata(dst)) else {
        return Ok(true);
    };
    if src_meta.len() != dst_meta.len() {
        return Ok(true);
    }
    files_differ_bytewise(src, dst)
}

/// 두 파일을 64KiB씩 바이트로 비교하고 첫 차이에서 멈춘다.
/// 해시 계산 없이 동일 여부를 확인하며 전체 파일을 메모리에 올리지 않는다.
fn files_differ_bytewise(a: &Path, b: &Path) -> std::io::Result<bool> {
    const CHUNK: usize = 64 * 1024;
    let (mut fa, mut fb) = (std::fs::File::open(a)?, std::fs::File::open(b)?);
    let mut buf_a = vec![0u8; CHUNK];
    let mut buf_b = vec![0u8; CHUNK];
    loop {
        // `read` 는 요청보다 적게 줄 수 있다. 두 스트림의 경계가 어긋나도 답이 틀리지 않게
        // 각 청크를 **가득 채운 뒤** 비교한다 — `read_exact` 는 파일 끝에서 에러가 되므로
        // 직접 채운다.
        let na = fill(&mut fa, &mut buf_a)?;
        let nb = fill(&mut fb, &mut buf_b)?;
        if na != nb {
            return Ok(true);
        }
        if na == 0 {
            return Ok(false);
        }
        if buf_a[..na] != buf_b[..nb] {
            return Ok(true);
        }
    }
}

/// 버퍼가 가득 차거나 EOF 일 때까지 읽는다. 반환값은 채운 바이트 수.
fn fill(f: &mut std::fs::File, buf: &mut [u8]) -> std::io::Result<usize> {
    use std::io::Read;
    let mut filled = 0;
    while filled < buf.len() {
        match f.read(&mut buf[filled..])? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(filled)
}

mod sync_probe;

#[cfg(test)]
mod bundle_selection_tests;

#[cfg(test)]
mod bundle_identity_tests;

#[cfg(test)]
mod upgrade_retire_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explorer_is_not_builtin_plugin() {
        assert!(!is_builtin_plugin("com.tasty.explorer"));
    }

    #[test]
    fn claude_design_is_not_builtin_plugin() {
        assert!(!is_builtin_plugin("com.tasty.claude-design"));
    }

    #[test]
    fn codex_is_builtin() {
        assert!(is_builtin_plugin("com.tasty.codex"));
    }

    #[test]
    fn claude_is_builtin() {
        assert!(is_builtin_plugin("com.tasty.claude"));
    }

    #[test]
    fn image_is_builtin() {
        assert!(is_builtin_plugin("com.tasty.image"));
    }

    #[test]
    fn markdown_is_builtin() {
        assert!(is_builtin_plugin("com.tasty.markdown"));
    }

    #[test]
    fn clipboard_viewer_is_builtin() {
        assert!(is_builtin_plugin("com.tasty.clipboard-viewer"));
    }

    #[test]
    fn unknown_is_not_builtin() {
        assert!(!is_builtin_plugin("com.example.foo"));
    }

    #[test]
    fn permission_diff_appends_new_tokens_only() {
        let mut cfg = PluginsConfig::default();
        cfg.set_granted(
            "com.tasty.image",
            vec!["surface.read".into(), "surface.write".into()],
        );

        let manifest = vec![
            "surface.read".into(),
            "surface.write".into(),
            "file_handler.define".into(),
            "file_handler.handle:image".into(),
        ];

        let changed = apply_builtin_permission_diff(&mut cfg, "com.tasty.image", &manifest);
        assert!(changed);

        let granted = cfg.granted_permissions("com.tasty.image");
        assert!(granted.contains("surface.read"));
        assert!(granted.contains("surface.write"));
        assert!(granted.contains("file_handler.define"));
        assert!(granted.contains("file_handler.handle:image"));
    }

    #[test]
    fn permission_diff_is_noop_when_manifest_already_covered() {
        let mut cfg = PluginsConfig::default();
        cfg.set_granted(
            "com.tasty.image",
            vec!["surface.read".into(), "surface.write".into()],
        );

        let manifest = vec!["surface.read".into(), "surface.write".into()];

        let changed = apply_builtin_permission_diff(&mut cfg, "com.tasty.image", &manifest);
        assert!(!changed);
        assert_eq!(cfg.granted_permissions("com.tasty.image").len(), 2);
    }

    fn v(s: &str) -> semver::Version {
        semver::Version::parse(s).unwrap()
    }

    #[test]
    fn decide_returns_skip_when_bundle_is_lower() {
        let installed = v("1.2.0");
        let bundle = v("1.1.0");
        assert_eq!(
            decide_builtin_upgrade(Some(&installed), Some(&bundle), false),
            BuiltinUpgradeDecision::Skip
        );
    }

    #[test]
    fn decide_returns_upgrade_when_bundle_is_higher() {
        let installed = v("1.0.0");
        let bundle = v("1.1.0");
        assert_eq!(
            decide_builtin_upgrade(Some(&installed), Some(&bundle), false),
            BuiltinUpgradeDecision::UpgradeVersion {
                from: installed,
                to: bundle,
            }
        );
    }

    #[test]
    fn decide_returns_resync_when_versions_equal() {
        let installed = v("1.0.0");
        let bundle = v("1.0.0");
        assert_eq!(
            decide_builtin_upgrade(Some(&installed), Some(&bundle), false),
            BuiltinUpgradeDecision::ResyncSameVersion
        );
    }

    #[test]
    fn decide_returns_force_overwrite_when_force_true() {
        let installed = v("9.9.9");
        let bundle = v("1.0.0");
        assert_eq!(
            decide_builtin_upgrade(Some(&installed), Some(&bundle), true),
            BuiltinUpgradeDecision::ForceOverwrite
        );
        // 입력이 None 이어도 force 우선
        assert_eq!(
            decide_builtin_upgrade(None, None, true),
            BuiltinUpgradeDecision::ForceOverwrite
        );
    }

    #[test]
    fn decide_returns_skip_when_bundle_version_unparsable() {
        let installed = v("1.0.0");
        assert_eq!(
            decide_builtin_upgrade(Some(&installed), None, false),
            BuiltinUpgradeDecision::Skip
        );
    }

    #[test]
    fn decide_returns_resync_when_installed_version_unparsable_but_bundle_ok() {
        let bundle = v("1.0.0");
        assert_eq!(
            decide_builtin_upgrade(None, Some(&bundle), false),
            BuiltinUpgradeDecision::ResyncSameVersion
        );
    }

    /// 파일의 mtime 을 강제로 세운다 — `cp -p`·아카이브 해제가 하는 일과 같은 결과.
    fn set_mtime(path: &std::path::Path, t: std::time::SystemTime) {
        let f = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        f.set_modified(t).unwrap();
    }

    /// 재복사를 확인할 기준 시각. 대상 파일을 과거로 설정해 파일시스템의 시간 해상도에
    /// 따라 연속된 두 쓰기가 같은 시각으로 기록되는 문제를 피한다.
    fn stale_stamp() -> std::time::SystemTime {
        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000)
    }

    /// 대상 파일들을 기준 과거 시각으로 설정한다. 내용이 같으면 원본이 더 새로워도 복사하지 않아야 한다.
    fn stamp_all_stale(dir: &std::path::Path) {
        for e in std::fs::read_dir(dir).unwrap() {
            set_mtime(&e.unwrap().path(), stale_stamp());
        }
    }

    fn mtimes(dir: &std::path::Path) -> Vec<(std::ffi::OsString, std::time::SystemTime)> {
        let mut v: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| {
                let e = e.unwrap();
                (e.file_name(), e.metadata().unwrap().modified().unwrap())
            })
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }

    /// 같은 시각의 파일에서 내용이 같거나 다른 경우와 대상이 더 새로운 경우를 구분한다.
    #[test]
    fn a_staged_copy_with_the_same_mtime_is_judged_by_content() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dest = tmp.path().join("dest");

        // ① 시각이 같고 내용이 다르다 → 복사해야 한다.
        std::fs::write(&src, "new").unwrap();
        std::fs::write(&dest, "old").unwrap();
        set_mtime(&src, stale_stamp());
        set_mtime(&dest, stale_stamp());
        copy_if_newer(&src, &dest).unwrap();
        assert_eq!(
            std::fs::read_to_string(&dest).unwrap(),
            "new",
            "시각이 같아도 내용이 다르면 복사해야 한다"
        );

        // ② 시각도 내용도 같다 → 안 써야 한다. 판정은 옛 스탬프가 남아 있는가로 한다.
        set_mtime(&src, stale_stamp());
        set_mtime(&dest, stale_stamp());
        copy_if_newer(&src, &dest).unwrap();
        assert_eq!(
            std::fs::metadata(&dest).unwrap().modified().unwrap(),
            stale_stamp(),
            "같은 내용을 다시 쓰면 안 된다 — 옛 시각이 그대로여야 한다"
        );

        // 대상이 더 새로우면 내용을 비교하지 않는 현재 한계를 확인한다.
        std::fs::write(&src, "newer content").unwrap();
        set_mtime(&src, stale_stamp());
        set_mtime(&dest, stale_stamp() + std::time::Duration::from_secs(1));
        copy_if_newer(&src, &dest).unwrap();
        assert_eq!(
            std::fs::read_to_string(&dest).unwrap(),
            "new",
            "대상이 더 새로우면 현재 정책은 내용을 비교하지 않는다"
        );
    }

    #[test]
    fn second_sync_of_the_same_bundle_writes_nothing() {
        // 같은 번들을 두 번 동기화하면 두 번째에는 복사하지 않아야 한다.
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dest = tmp.path().join("dest");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("bin"), "content").unwrap();
        std::fs::write(src.join("tasty-plugin.toml"), "version = \"0.1.0\"").unwrap();

        sync_dir_by_content(&src, &dest).unwrap();
        // 재복사를 놓치지 않도록 대상 시각을 과거로 설정한 뒤 비교한다.
        stamp_all_stale(&dest);
        let first = mtimes(&dest);
        sync_dir_by_content(&src, &dest).unwrap();

        assert_eq!(first, mtimes(&dest), "두 번째 동기화가 파일을 다시 썼다");
        assert!(
            mtimes(&dest).iter().all(|(_, t)| *t == stale_stamp()),
            "옛 시각이 그대로여야 한다 — 하나라도 '지금' 이면 다시 쓴 것이다"
        );
    }

    #[test]
    fn the_sync_reports_whether_it_wrote() {
        // 이 반환값이 `upgrade-builtins` 의 보고문과 재기동 판정을 정한다. 늘 false 면
        // 같은 버전으로 내용만 고친 plugin 이 재기동 대상에서 조용히 빠진다.
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dest = tmp.path().join("dest");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("bin"), "one").unwrap();

        assert!(
            sync_dir_by_content(&src, &dest).unwrap(),
            "첫 설치가 false 를 냈다"
        );
        assert!(
            !sync_dir_by_content(&src, &dest).unwrap(),
            "안 쓴 회차가 true 를 냈다"
        );

        std::fs::write(src.join("bin"), "two").unwrap();
        assert!(
            sync_dir_by_content(&src, &dest).unwrap(),
            "내용이 바뀐 회차가 false 를 냈다"
        );
    }

    #[test]
    fn only_the_changed_file_is_rewritten() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dest = tmp.path().join("dest");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("changed"), "before").unwrap();
        std::fs::write(src.join("untouched"), "same").unwrap();
        sync_dir_by_content(&src, &dest).unwrap();
        // 두 스냅샷을 서로 비교하지 않는다 — 같은 틱에 들어가면 다시 써도 같은 값이 나온다.
        // 대신 **알려진 옛 시각**을 기준으로 삼는다(`stale_stamp` 주석).
        stamp_all_stale(&dest);

        std::fs::write(src.join("changed"), "after!").unwrap(); // 같은 크기, 다른 내용
        sync_dir_by_content(&src, &dest).unwrap();

        let after = mtimes(&dest);
        assert_eq!(
            std::fs::read_to_string(dest.join("changed")).unwrap(),
            "after!"
        );
        let m = |v: &[(std::ffi::OsString, std::time::SystemTime)], n: &str| {
            v.iter().find(|(k, _)| k == n).unwrap().1
        };
        assert_ne!(
            m(&after, "changed"),
            stale_stamp(),
            "바뀐 파일이 옛 시각 그대로다 — 다시 안 썼다"
        );
        assert_eq!(
            m(&after, "untouched"),
            stale_stamp(),
            "안 바뀐 파일이 다시 쓰였다"
        );
    }

    #[test]
    fn a_lying_mtime_does_not_hide_a_different_file() {
        // 크기가 같고 원본 시각이 더 오래돼도 내용이 다르면 복사한다.
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dest = tmp.path().join("dest");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("bin"), "aaaa").unwrap();
        sync_dir_by_content(&src, &dest).unwrap();

        std::fs::write(src.join("bin"), "bbbb").unwrap(); // 같은 크기
        let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        set_mtime(&src.join("bin"), old);

        sync_dir_by_content(&src, &dest).unwrap();

        assert_eq!(
            std::fs::read_to_string(dest.join("bin")).unwrap(),
            "bbbb",
            "src 가 더 오래된 mtime 이라고 새 내용을 건너뛰었다"
        );
    }

    /// tracing 이벤트를 받아 실제 로그 레벨과 메시지를 검사한다.
    fn capture_logs(f: impl FnOnce()) -> String {
        use std::sync::{Arc, Mutex};
        #[derive(Clone)]
        struct Buf(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for Buf {
            fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(b);
                Ok(b.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let buf = Buf(Arc::new(Mutex::new(Vec::new())));
        let sink = buf.clone();
        let sub = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(move || sink.clone())
            .finish();
        tracing::subscriber::with_default(sub, f);
        let out = buf.0.lock().unwrap().clone();
        String::from_utf8_lossy(&out).into_owned()
    }

    #[test]
    fn a_home_copy_ahead_of_the_bundle_is_warned_not_hidden() {
        // 설치본이 더 높아 번들을 적용하지 않으면 warn으로 알려야 한다.
        let (i, b) = (v("0.1.60"), v("0.1.59"));
        let logs = capture_logs(|| log_builtin_skip("com.tasty.x", Some(&i), Some(&b)));

        assert!(logs.contains("WARN"), "warn 으로 안 나갔다: {logs}");
        assert!(
            logs.contains("upgrade-builtins --force"),
            "되돌리는 수단이 문장에 없다: {logs}"
        );
        assert!(
            !logs.contains("up-to-date"),
            "최신이 아니라 앞선 것이다: {logs}"
        );
    }

    #[test]
    fn an_equal_version_stays_quiet() {
        // 같은 버전은 경고하지 않는다.
        let (i, b) = (v("0.1.60"), v("0.1.60"));
        let logs = capture_logs(|| log_builtin_skip("com.tasty.x", Some(&i), Some(&b)));

        assert!(
            !logs.contains("WARN"),
            "같은 버전인데 warn 이 나갔다: {logs}"
        );
        assert!(logs.contains("up-to-date"), "아무것도 안 남았다: {logs}");
    }

    #[test]
    fn a_missing_version_is_not_read_as_ahead() {
        // 매니페스트를 못 읽으면 `None` 이 온다. 그것을 "앞섰다" 로 읽으면 손상된 설치가
        // 경고를 쏟는다 — 그 경우는 조용한 쪽으로 물러난다.
        let b = v("0.1.59");
        assert_eq!(classify_skip(None, Some(&b)), SkipCase::UpToDate);
        assert_eq!(classify_skip(Some(&b), None), SkipCase::UpToDate);
    }

    #[test]
    fn a_difference_past_the_first_chunk_is_still_seen() {
        // 첫 64KiB 뒤에서 바뀐 내용도 확인해야 한다.
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dest = tmp.path().join("dest");
        std::fs::create_dir_all(&src).unwrap();

        let mut a = vec![b'x'; 200 * 1024];
        std::fs::write(src.join("bin"), &a).unwrap();
        sync_dir_by_content(&src, &dest).unwrap();

        *a.last_mut().unwrap() = b'y'; // 마지막 바이트 하나, 크기는 그대로
        std::fs::write(src.join("bin"), &a).unwrap();
        let wrote = sync_dir_by_content(&src, &dest).unwrap();

        assert!(wrote, "뒤쪽 청크의 차이를 못 봤다");
        assert_eq!(
            std::fs::read(dest.join("bin")).unwrap().last().copied(),
            Some(b'y')
        );
    }

    #[test]
    fn content_sync_still_removes_what_the_bundle_dropped() {
        // 청소가 복사 정책과 함께 사라지지 않았는지 — 정책이 바뀐 자리에서 다시 묻는다.
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dest = tmp.path().join("dest");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("keep"), "keep").unwrap();
        std::fs::write(src.join("dropped"), "gone soon").unwrap();
        sync_dir_by_content(&src, &dest).unwrap();
        assert!(dest.join("dropped").exists());

        std::fs::remove_file(src.join("dropped")).unwrap();
        sync_dir_by_content(&src, &dest).unwrap();

        assert!(
            !dest.join("dropped").exists(),
            "번들에서 빠진 파일이 남았다"
        );
        assert!(dest.join("keep").exists());
    }

    #[test]
    fn prune_removes_only_what_src_lacks_and_copies_nothing() {
        // 복사와 별도로 원본에 없는 항목의 삭제를 확인한다.
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dest = tmp.path().join("dest");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(dest.join("gone-dir")).unwrap();
        std::fs::write(src.join("keep"), "src-side").unwrap();
        std::fs::write(dest.join("keep"), "dest-side").unwrap();
        std::fs::write(dest.join("stale"), "remove me").unwrap();
        std::fs::write(dest.join("gone-dir/inner"), "remove me too").unwrap();

        prune_dest_not_in_src(&src, &dest).unwrap();

        assert!(!dest.join("stale").exists(), "src 에 없는 파일이 남았다");
        assert!(
            !dest.join("gone-dir").exists(),
            "src 에 없는 디렉터리가 남았다"
        );
        // 청소는 **복사를 하지 않는다** — 이름이 겹치는 파일의 내용은 그대로여야 한다.
        assert_eq!(
            std::fs::read_to_string(dest.join("keep")).unwrap(),
            "dest-side"
        );
    }

    #[test]
    fn dev_lang_sync_removes_what_the_crate_dropped() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("lang");
        let dest = tmp.path().join("staged-lang");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(src.join("en.toml"), "en").unwrap();
        std::fs::write(src.join("ko.toml"), "ko").unwrap();
        std::fs::write(dest.join("en.toml"), "en").unwrap();
        std::fs::write(dest.join("ko.toml"), "ko").unwrap();
        // crate 에서 빠진 파일 — staging 에만 남아 있다.
        std::fs::write(dest.join("ja.toml"), "ja-dropped").unwrap();

        sync_dir_if_newer(&src, &dest).unwrap();

        // 원본에 남은 파일은 삭제하지 않아야 한다.
        assert!(dest.join("en.toml").exists(), "src 에 있는 파일을 지웠다");
        assert!(dest.join("ko.toml").exists(), "src 에 있는 파일을 지웠다");
        assert!(
            !dest.join("ja.toml").exists(),
            "crate 에서 빠진 lang 파일이 staging 에 남았다"
        );
    }

    /// Always는 같은 내용도 다시 쓰고 ContentDiffers는 건너뛰는지 확인한다.
    /// 파일 내용을 비교해서는 구분할 수 없어 대상의 과거 mtime이 바뀌었는지 검사한다.
    #[test]
    fn the_forced_overwrite_rewrites_even_when_the_content_is_identical() {
        let tmp = tempfile::tempdir().unwrap();
        let build = |label: &str| {
            let src = tmp.path().join(label).join("src");
            let dest = tmp.path().join(label).join("dest");
            std::fs::create_dir_all(&src).unwrap();
            std::fs::create_dir_all(&dest).unwrap();
            std::fs::write(src.join("f"), "same").unwrap();
            std::fs::write(dest.join("f"), "same").unwrap();
            set_mtime(&dest.join("f"), stale_stamp());
            (src, dest)
        };
        let mtime_of =
            |d: &std::path::Path| std::fs::metadata(d.join("f")).unwrap().modified().unwrap();

        let (src, dest) = build("forced");
        overwrite_builtin_dir(&src, &dest).unwrap();
        assert_ne!(
            mtime_of(&dest),
            stale_stamp(),
            "Always는 같은 내용도 다시 복사해야 한다"
        );

        // 같은 입력을 내용 비교 정책으로 처리하면 다시 쓰지 않는다.
        let (src, dest) = build("by-content");
        assert!(
            !sync_dir_by_content(&src, &dest).unwrap(),
            "ContentDiffers 가 같은 내용인데 썼다고 보고했다"
        );
        assert_eq!(
            mtime_of(&dest),
            stale_stamp(),
            "ContentDiffers 가 같은 내용을 다시 썼다"
        );
    }

    /// 대상이 더 새롭고 내용이 다를 때 세 복사 정책의 결과를 비교한다.
    #[test]
    fn the_three_wrappers_differ_only_in_the_copy_predicate() {
        let tmp = tempfile::tempdir().unwrap();
        let old = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        let newer = old + std::time::Duration::from_secs(60);
        let build = |label: &str| {
            let src = tmp.path().join(label).join("src");
            let dest = tmp.path().join(label).join("dest");
            std::fs::create_dir_all(&src).unwrap();
            std::fs::create_dir_all(&dest).unwrap();
            std::fs::write(src.join("f"), "new").unwrap();
            std::fs::write(dest.join("f"), "old").unwrap();
            set_mtime(&src.join("f"), old);
            set_mtime(&dest.join("f"), newer);
            (src, dest)
        };
        let read = |d: &std::path::Path| std::fs::read_to_string(d.join("f")).unwrap();

        let (src, dest) = build("always");
        overwrite_builtin_dir(&src, &dest).unwrap();
        assert_eq!(
            read(&dest),
            "new",
            "Always는 대상의 시각과 관계없이 덮어써야 한다"
        );

        let (src, dest) = build("newer");
        sync_dir_if_newer(&src, &dest).unwrap();
        assert_eq!(
            read(&dest),
            "old",
            "NewerThenContent 가 더 새 dest 를 덮었다"
        );

        let (src, dest) = build("content");
        assert!(
            sync_dir_by_content(&src, &dest).unwrap(),
            "ContentDiffers 가 내용이 다른데 안 썼다고 보고했다"
        );
        assert_eq!(
            read(&dest),
            "new",
            "ContentDiffers 가 내용이 다른데 안 썼다"
        );

        // 대상이 없으면 세 정책 모두 파일을 만들어야 한다.
        for label in ["z-always", "z-newer", "z-content"] {
            let src = tmp.path().join(label).join("src");
            let dest = tmp.path().join(label).join("dest");
            std::fs::create_dir_all(&src).unwrap();
            std::fs::write(src.join("f"), "new").unwrap();
            match label {
                "z-always" => overwrite_builtin_dir(&src, &dest).unwrap(),
                "z-newer" => sync_dir_if_newer(&src, &dest).unwrap(),
                _ => {
                    assert!(sync_dir_by_content(&src, &dest).unwrap());
                }
            }
            assert_eq!(read(&dest), "new", "{label}: 새 dest 를 안 만들었다");
        }
    }

    #[test]
    fn overwrite_builtin_dir_removes_stale_files() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dest = tmp.path().join("dest");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(src.join("a"), "a-new").unwrap();
        std::fs::write(src.join("b"), "b-new").unwrap();
        std::fs::write(dest.join("a"), "a-old").unwrap();
        std::fs::write(dest.join("b"), "b-old").unwrap();
        std::fs::write(dest.join("stale.txt"), "remove me").unwrap();

        overwrite_builtin_dir(&src, &dest).unwrap();

        assert_eq!(std::fs::read_to_string(dest.join("a")).unwrap(), "a-new");
        assert_eq!(std::fs::read_to_string(dest.join("b")).unwrap(), "b-new");
        assert!(!dest.join("stale.txt").exists());
    }

    #[test]
    fn overwrite_builtin_dir_recreates_nested_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dest = tmp.path().join("dest");
        std::fs::create_dir_all(src.join("lang")).unwrap();
        std::fs::create_dir_all(dest.join("lang")).unwrap();
        std::fs::write(src.join("lang/en.toml"), "en=new").unwrap();
        std::fs::write(dest.join("lang/en.toml"), "en=old").unwrap();
        std::fs::write(dest.join("lang/old.toml"), "stale").unwrap();

        overwrite_builtin_dir(&src, &dest).unwrap();

        assert_eq!(
            std::fs::read_to_string(dest.join("lang/en.toml")).unwrap(),
            "en=new"
        );
        assert!(!dest.join("lang/old.toml").exists());
    }

    #[test]
    fn permission_diff_preserves_existing_extra_tokens() {
        // 사용자가 명시적으로 추가했을 수도 있는 매니페스트 외 토큰은 제거하지 않는다.
        let mut cfg = PluginsConfig::default();
        cfg.set_granted(
            "com.tasty.image",
            vec!["surface.read".into(), "user.extra.token".into()],
        );

        let manifest = vec!["surface.read".into(), "file_handler.define".into()];

        let changed = apply_builtin_permission_diff(&mut cfg, "com.tasty.image", &manifest);
        assert!(changed);

        let granted = cfg.granted_permissions("com.tasty.image");
        assert!(granted.contains("surface.read"));
        assert!(granted.contains("user.extra.token")); // preserved
        assert!(granted.contains("file_handler.define")); // newly added
    }
}
