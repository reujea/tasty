//! 기본 제공 플러그인 (built-in) 인프라.
//!
//! Tasty는 일부 plugin (예: explorer, codex)을 본 바이너리와 함께 배포한다. 이들은:
//!
//! 1. **번들 위치**에서 디스커버됨 — 배포 패키지(release/dist): 실행 파일 옆
//!    `plugins/` 디렉터리, debug workspace 빌드: workspace 를 자동
//!    탐색해 `target/<profile>/builtin-plugins/`를 채움 (`ensure_dev_bundle`).
//! 2. **첫 실행 시** `~/.tasty/plugins/<id>/`에 복사됨 — 사용자가 손댈 수 있는
//!    실제 설치 위치는 사용자 디렉터리 한 곳뿐. `plugins.toml`의
//!    `removed_builtins`에 등록된 id는 자동 복사하지 않는다.
//! 3. **uninstall** 시 `removed_builtins`에 추가되어 다음 실행에서 재등장하지 않음.
//!
//! 외부 플러그인과의 차이는 *발생지*뿐이다. 디스커버리·실행·권한 모델은 동일하다.
//!
//! 위 2 는 **첫 설치**만 설명한다. 첫 복사 이후 플러그인 코드/매니페스트를 고쳤을
//! 때 **호스트 재시작 없이** 실행 중 tasty 에 반영하는 절차(재빌드 → `upgrade-builtins`
//! 재sync → `disable`/`enable` respawn)는 `docs/dev-guide/plugin-development.md` §9.1
//! 참조. 재sync 는 **버전이 갈래를 고르고 그 갈래 안에서 내용이 판정한다**: 같은 버전이면
//! 내용이 다른 파일만 옮기고, 번들이 높으면 전량 덮어쓰고, **설치본이 더 높으면 내용을 안
//! 보고 건너뛴다**(그 경우만 `upgrade-builtins --force` 로 내린다).

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::{PluginManager, PluginsConfig, discovery};
use tasty_plugin_manifest::Manifest;

/// 한 builtin plugin의 패키지 메타 — id, dev workspace crate 경로, plugin 바이너리 이름.
struct BuiltinSpec {
    id: &'static str,
    /// `crates/<crate_dir>/` — workspace 빌드의 매니페스트/lang 원본 위치.
    /// workspace 가 실제로 존재할 때만 dev sync 에 사용된다 (배포 패키지에서는 미사용).
    crate_dir: &'static str,
    /// `target/<profile>/<bin_name>` — workspace 빌드된 plugin 실행 바이너리 이름.
    bin_name: &'static str,
}

// explorer 는 T11 에서 host builtin surface 로 승격됨 — plugin 번들에서 제거.
#[cfg(windows)]
const BUILTINS: &[BuiltinSpec] = &[
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
const BUILTINS: &[BuiltinSpec] = &[
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

/// 자동 builtin upgrade 시 *어떤* 분기로 처리할지 결정한 결과.
///
/// `install_builtins_if_needed` 의 already-present 분기와 신규 `upgrade_builtins`
/// 양쪽에서 공유. mtime 비신뢰성을 피하기 위해 manifest 의 `version` (semver)
/// 을 1차 기준으로 사용한다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BuiltinUpgradeDecision {
    /// 변경 없음 (installed >= bundle, 또는 bundle 매니페스트 corrupt).
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

/// installed / bundle / force 입력으로 upgrade 의사 결정.
///
/// 규칙 (verify §5.5 반영):
/// - force=true → 무조건 `ForceOverwrite`
/// - bundle 매니페스트 파싱 실패 → `Skip` (corrupt bundle 보호)
/// - installed 파싱 실패 + bundle ok → `ResyncSameVersion` (사용자 dir 손상 복구)
/// - 둘 다 ok → bundle vs installed semver 비교
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

/// dest 에 있는데 src 에 없는 항목을 제거한다 — **이 층만** 본다(재귀는 호출자가 한다).
///
/// **복사 정책과 독립이다.** 이 단계의 값은 src 목록과 dest 목록의 *차집합*이라 복사를
/// 얼마나 하든 같다. 한 함수에 묶여 있으면 복사 정책을 바꿀 때 청소가 함께 사라지는데,
/// 그 소실은 조용하다 — 옛 잔존 파일은 어떤 단정도 안 건드리고 사용자 홈에서만 쌓인다.
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
                std::fs::remove_file(entry.path())?;
            }
        }
    }
    Ok(())
}

/// 파일 하나를 옮길지 정하는 술어. **훑기와 청소는 셋이 같고 이것만 다르다.**
///
/// 새 갈래를 더하려면 여기 arm 을 더해야 하고, 그러면 [`sync_dir`] 의 `match` 가 exhaustive 라
/// **복사 판정을 반드시 쓰게 된다 — 청소는 공짜로 따라온다.** 이 리팩터의 값 전부가 거기 있다.
/// 전에는 훑기가 셋으로 갈라져 있었고, 셋 중 하나([`sync_dir_if_newer`])만 청소를 빠뜨린 채
/// 오래 있었다. 그 소실은 조용했다 — 옛 잔존 파일은 어떤 단정도 안 건드린다.
///
/// ★ **이것이 닫는 것은 한 부류뿐이다.** "셋 중 하나가 청소를 빠뜨린다" 는 원리적으로 못 나게
/// 되지만, "호출부가 술어를 잘못 고른다" 는 그대로 남는다 — 부팅마다 번들 전량을 다시 쓰던
/// 결함이 정확히 그 부류였다(리터럴 `force` 하나). 다만 그때 조용했던 이유가 **이름이 술어를
/// 숨긴 것**이었고, 여기서는 고른 값이 이름 옆에 적힌다.
#[derive(Clone, Copy)]
enum CopyPolicy {
    /// 판정 없이 덮어쓴다 — 버전이 다를 때의 전량 교체.
    Always,
    /// src 가 더 새것이거나, 같은 눈금이면 내용으로([`copy_if_newer`]). dev 스테이징.
    NewerThenContent,
    /// 내용이 다를 때만([`copy_file_if_content_differs`]). 번들 → 홈 설치.
    ContentDiffers,
}

/// 한 층을 훑어 `src` 를 `dst` 에 맞춘다: `create_dir_all` → **청소** → 항목마다
/// (디렉터리면 재귀 · 파일이면 `policy`).
///
/// 청소([`prune_dest_not_in_src`])는 **층마다** 돌고 복사 정책과 독립이다 — src 목록과 dest
/// 목록의 차집합이라 복사를 얼마나 하든 값이 같다.
///
/// 반환값은 **무엇이든 썼는가**다. 세 호출자 중 하나만 그 값을 쓰지만, 여기서 잃으면 그
/// 사실이 함수 안에서 사라지므로 한 자리에서 돌려준다.
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

/// dest 의 모든 파일을 src 기준으로 강제 교체 (mtime 무시).
/// src 에 없는 dest 의 파일/디렉토리는 *제거* — 옛 버전의 잔존 파일 정리
/// ([`prune_dest_not_in_src`], 층마다 돈다).
///
/// 술어를 [`CopyPolicy::Always`] 로 고정한 [`sync_dir`] 이다 — 호출부는 이 이름으로 고른다.
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

/// exe-relative 경로들에서 bundle root 검색. 추출 사유: FHS fallback 진입 전
/// `current_exe()` 실패 (테스트 환경 등) 가 전체 None 으로 단락되지 않도록.
fn bundle_root_exe_relative() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    bundle_root_from_exe_dir(exe.parent()?)
}

fn bundle_root_from_exe_dir(exe_dir: &Path) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        // .app 번들: `Contents/Resources/plugins/`. exe 옆(`Contents/MacOS/`)이
        // 아닌 이유는 codesign 이 `Contents/MacOS/` 하위의 실행 파일 디렉터리를
        // nested bundle 로 파싱하려다 실패하기 때문(bundle format unrecognized).
        // 아래 exe-relative 보다 먼저 본다 — 구 배치가 남아 있어도 새 것 우선.
        // exe 가 `Contents/MacOS/` 안에 있을 때로 한정한다. 그 확인이 없으면 번들이
        // 아닌 배치(예: `target/<profile>/tasty`)에서 우연히 `../Resources/plugins`
        // 가 존재할 때 exe 옆 `plugins/` 를 가로챈다.
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

/// debug workspace 빌드에서 workspace를 자동 탐색하여
/// `target/<profile>/builtin-plugins/`에 등록된 builtin plugin들의
/// manifest+binary+lang을 동기화. mtime 및 동률 시 내용 비교로 복사하므로 매 부팅 비용은
/// 작다. 한 plugin이라도 동기화에 성공했으면 Some(bundle_root). workspace를 못 찾으면
/// (= 배포 패키지처럼 `crates/`가 없으면) `sync_builtin_dev`가 전부 false 를 반환하여
/// None. 따라서 진짜 배포본에서는 자연히 no-op.
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

/// mkdir + 매니페스트 + 바이너리 복사 — 이 중 하나라도 실패하면 dev bundle
/// 자체가 무효(fatal). sig/lang 은 별도 best-effort 스텝으로 분리되어 있다.
/// 세 스텝을 `&&`로 묶어 첫 실패에서 단락 — 순서·조건은 원본과 동일.
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

/// 개발 빌드의 crate `lang/` → staging `lang/` 동기화. src 가 더 새것일 때만 복사하고,
/// src 에 없는 dest 항목은 층마다 지운다([`prune_dest_not_in_src`]).
///
/// ## 청소가 왜 이 함수에서 빠져 있었나
///
/// 이 자리는 오래 **복사 전용**이었다. 형제 둘(`overwrite_builtin_dir` · `sync_dir_by_content`)은
/// 청소를 하고 doc 에 그렇게 적었는데, 이 함수만 청소도 doc 도 없었다 — 의도된 예외가 아니라
/// 빠뜨린 쪽이다.
///
/// 그 소실이 조용했던 이유는 **같은 "스테이징" 단계를 셸도 하기 때문**이다. `Justfile` 의
/// `build-plugins` · `build-plugin` 과 `scripts/build-linux.sh` · `scripts/build-macos-dmg.sh` 는
/// 전부 `rm -rf <dest>/lang` 뒤 전량 복사라 삭제가 이미 들어 있다. 그래서 잔존물은
/// **호스트가 부팅 때 스스로 스테이징하는 갈래**([`ensure_dev_bundle`])에서만 생겼고, 다음 셸
/// 스테이징까지만 살았다. 좁지만 죽은 경로는 아니다 — `cargo build --workspace` 후 그냥 실행하는
/// 것이 안내된 개발 흐름이다.
///
/// 잔존의 대가는 i18n 이다. crate 에서 지운 lang 파일이 staging 에 남으면, 홈 설치는 **그 staging
/// 을 src 로** 삼아 prune 하므로([`sync_dir_by_content`]) 홈에서도 안 지워진다. 즉 지울 수 있는
/// 지점이 여기 하나뿐이었는데 여기에만 청소가 없었다.
///
/// ## dest 는 번들 전용이라는 전제
///
/// prune 은 **지우는** 동작이라 dest 에 사용자 파일이 섞이면 안 된다. 호출자는 하나뿐이고
/// ([`sync_builtin_dev_lang`]) 그 dest 는 `<exe_dir>/builtin-plugins/<id>/lang` 이다.
/// [`bundle_root`] 의 환경변수 override 와 exe 옆 `plugins/` 는 이 갈래에 **도달하기 전에**
/// 반환되므로, 여기 오는 dest 는 이 코드가 만든 빌드 산출물 디렉터리뿐이다. 위 셸 경로들이 같은
/// 디렉터리를 통째로 `rm -rf` 하고 있으니, 층별 prune 은 이미 통용되는 것보다 약하다.
/// 다른 호출자를 붙일 때 이 전제를 같이 확인해라 — 안 맞으면 붙일 곳이 여기가 아니다.
///
/// ## 훑는 함수가 왜 셋인가
///
/// 셋의 차이는 **파일 단위 복사 술어 하나**뿐이다 — 무조건([`copy_atomic`]) · mtime 과 동률내용
/// ([`copy_if_newer`]) · 내용([`copy_file_if_content_differs`]). 훑기와 prune 은 같고, 지금은
/// 실제로 하나다: 셋 다 술어를 [`CopyPolicy`] 로 넘겨 [`sync_dir`] 을 부르는 이름일 뿐이다.
/// 그래서 위 "청소가 빠져 있었다" 는 **다시 날 수 없다** — 청소는 훑기 안에 한 번 있고, 술어를
/// 무엇으로 고르든 따라온다. 이 함수는 그 셋 중 하나를 이름으로 고르는 자리다.
fn sync_dir_if_newer(src: &Path, dst: &Path) -> std::io::Result<()> {
    sync_dir(src, dst, CopyPolicy::NewerThenContent).map(|_| ())
}

/// src가 dest보다 더 최신이거나 dest가 없으면 복사. dest가 더 새것이면 no-op이고,
/// **mtime이 같으면 내용으로 판정한다.**
///
/// ## 동률에서 왜 내용을 보나
///
/// 전에는 `sm <= dm`이 곧 no-op이었다. 그 한 줄이 두 가지를 한꺼번에 흡수했다.
///
/// - **같은 눈금**에 떨어진 두 파일은 순서를 알 수 없다. 판정 불가를 "안 바뀜"으로 읽는다.
/// - 더 나쁜 쪽: `cp -p`·아카이브 해제·rsync는 mtime을 **원본 것으로 되돌린다.** 그래서
///   시각이 같고 **내용이 다른** 파일이 정확히 이 갈래로 들어온다. 우리 배포 경로의
///   실제 형태다.
///
/// 그 흡수의 대가는 조용하다 — 낡은 plugin 바이너리가 최신 매니페스트를 달고 설치되고,
/// 그것은 **정상 부팅해 정상 응답한다**(`manager::pump`의 version drift 경고가 그
/// 회귀를 적어 두고 있다). 그래서 동률은 skip이 아니라 **비교의 방아쇠**로 쓴다.
///
/// ## 왜 전량 비교가 아닌가 — 실측
///
/// 형제 경로(홈 설치)는 [`sync_dir_by_content`]로 **항상** 내용을 본다. 여기서 같은 것을
/// 하면 debug 번들 기준 양쪽 2.31 GB를 매 부팅 읽는다: 더운 캐시 **1.09 s**, 찬 캐시
/// **1.56 s**(페이지를 `posix_fadvise(DONTNEED)`로 비우고 잰 값. 같은 계기로 262 MB
/// 파일 하나가 찬 0.228 s / 더운 0.020 s라 비움이 실제로 들었다). 부팅은 대개 찬 캐시라
/// 그 값을 그대로 문다.
///
/// 동률에서만 비교하면 그 비용이 **정상 흐름에 안 붙는다** — 재빌드한 파일은 mtime이
/// 앞서므로 읽지 않고 복사하고, 안 바뀐 파일은 dest가 더 새것이라 읽지 않고 건너뛴다.
/// 읽는 것은 시각이 정확히 겹친 파일뿐이고, 그것이 위 두 갈래가 사는 자리다.
/// 실측: 지금 트리에서 동률은 **0 건**이라 이 판정이 읽는 바이트가 **0**이다(부팅 시각
/// 전후 비교에서도 지연이 안 나온다).
///
/// ★ 그래서 비용과 값이 **같은 조건에 걸려 있다**: 비용이 생기는 때는 동률이 생길 때뿐이고,
/// 동률이 생기는 그때가 바로 이 판정이 값을 내는 때다. 동률이 없으면 공짜고, 동률이 있으면
/// 그것을 읽는 것이 이 함수의 존재 이유다. 한쪽만 보고 "읽기가 붙었다"고 되돌리지 마라.
///
/// ## 남는 구멍 (선언한다)
///
/// **dest가 더 새것인데 내용이 다른 경우는 여전히 못 본다.** dest는 이 함수만 쓰므로
/// 그 mtime은 복사 시각이고, src가 나중에 **더 옛 시각을 달고** 바뀌어야 성립한다
/// (`-p`를 단 복원 등). 그 갈래를 닫으려면 전량 비교로 돌아가야 하고, 위 값이 그 대가다.
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

/// `std::fs::copy`의 안전 대체 — temp 파일에 쓰고 atomic rename으로 dest에 swap.
///
/// macOS에서 같은 경로에 binary를 in-place 덮어쓰면 kernel이 캐시한 code signature가
/// invalid로 판정되어 다음 exec 시 `SIGKILL (Code Signature Invalid)`로 죽는다
/// (Taskgated). rename은 inode를 교체하므로 kernel이 새 시그니처를 다시 읽는다.
///
/// Linux/Windows에서도 동일하게 동작 — partial-write race 방지 효과까지 덤으로 얻는다.
///
/// macOS: rename 성공 후 `dest`의 `com.apple.quarantine` xattr을 best-effort로
/// 제거한다(`strip_quarantine`) — dist(.app) 번들은 Finder가 quarantine된 DMG에서
/// 드래그/추출될 때 내부 파일에도 재귀적으로 quarantine xattr을 남기고, 이 상태로
/// 남은 plugin 바이너리는 ad-hoc 서명(비공증)이라 Gatekeeper가 exec 자체를 막는다.
/// plugin 서브프로세스가 조용히 spawn 실패하면 host는 frame을 영원히 못 받아
/// surface가 완전히 빈 채로 멈춘다 — 이미 앱 자체의 Gatekeeper 승인을 통과해
/// 실행 중인 이 프로세스가 자신이 번들에서 방금 풀어낸 리소스의 quarantine을
/// 스스로 제거하는 것은 안전하다.
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
    #[cfg(target_os = "macos")]
    strip_quarantine(dest);
    Ok(())
}

/// `dest`의 `com.apple.quarantine` xattr을 제거한다(있으면). 속성이 애초에 없는
/// 경우(`ENOATTR`)는 정상 상태(quarantine되지 않은 파일)이므로 조용히 넘어가고,
/// 그 외 실패는 warn만 남긴다 — 이 정리가 실패해도 파일 자체는 정상 복사됐으니
/// 설치 흐름을 막을 이유가 없다(best-effort).
#[cfg(target_os = "macos")]
fn strip_quarantine(dest: &Path) {
    use std::os::unix::ffi::OsStrExt;

    let Ok(c_path) = std::ffi::CString::new(dest.as_os_str().as_bytes()) else {
        return;
    };
    // SAFETY: c_path/c_attr 모두 유효한 NUL-terminated 버퍼이고, removexattr은
    // 이를 읽기만 한다(dest 파일 자체는 변경하지 않고 xattr만 제거).
    let ret = unsafe { libc::removexattr(c_path.as_ptr(), c"com.apple.quarantine".as_ptr(), 0) };
    if ret != 0 {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() != Some(libc::ENOATTR) {
            tracing::warn!(
                "strip quarantine xattr for {} failed: {err}",
                dest.display()
            );
        }
    }
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
    if config_dirty && let Err(e) = mgr.config.save() {
        tracing::warn!("install_builtins: save plugins.toml failed: {e}");
    }
}

/// `install_builtins_if_needed` Step 1 — 번들에서 복사/갱신.
///
/// builtin 은 호스트 소유 리소스다(사용자가 직접 편집하는 용도가 아니다). 그래서 부팅마다
/// 번들 쪽으로 맞추되, **무엇을 쓸지는 판정한다** — dest 가 없으면 통째로 복사하고, 있으면
/// [`install_builtin_overwrite_present`] 가 버전으로 갈래를 고른 뒤 같은 버전 갈래에서는
/// **내용**으로 파일 단위 판정을 한다([`sync_dir_by_content`]). mtime 은 어느 갈래에도 없다.
/// dev(`just run`)·배포(dmg) 모두 같은 경로다.
///
/// **버전 bump 없이 내용만 고친 plugin 도 반영된다** — 같은 버전 갈래가 내용을 보기 때문이다.
/// 반영 안 되는 경우가 하나 남는다: **설치본이 번들보다 높은 버전일 때는 건너뛴다.** 예전에는
/// 이 자리에 force 가 박혀 있어 그 경우까지 덮어썼는데, force 를 걷어내며 원래의 판정
/// (`installed >= bundle` → Skip)이 살아났다. 낡은 브랜치의 빌드를 띄우면 홈에 남은 더 높은
/// 버전이 유지된다는 뜻이고, 그때 되돌리는 수단은 `tasty plugin upgrade-builtins --force` 다.
///
/// 단 사용자가 명시 제거한 builtin(`is_builtin_removed`)은 복원하지 않는다.
///
/// 반환값: `true` 면 항목별 실패(warn 후 계속) — caller 는 Step 2 를 건너뛰고 다음 spec.
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
    let src = bundle.join(spec.id);
    if !src.is_dir() {
        tracing::debug!(
            "builtin plugin '{}' not in bundle ({}), skipping",
            spec.id,
            src.display()
        );
        return false;
    }
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

/// already-present builtin 을 번들 대비 갱신.
///
/// **force 를 넘기지 않는다.** 예전에는 여기서 `true` 를 박아 판정이 항상 `ForceOverwrite` 로
/// 떨어졌고, 그래서 같은 번들로 재부팅해도 번들 전량(debug 45 파일 ≈ 1.1 GB)을 다시 썼다. 그때
/// 쓰기를 정당화한 것은 "mtime 이 거짓일 수 있다" 였는데, 지금은 같은 버전 갈래가 mtime 이 아니라
/// **내용**으로 판정하므로([`sync_dir_by_content`]) 그 보험이 필요 없다.
///
/// 반환값: `true` 면 실패(warn 후 계속) — caller 는 Step 2 skip.
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

/// `Skip` 갈래에 오는 두 경우. 이름이 하나면 뒤쪽이 앞쪽 뒤에 숨는다.
#[derive(Debug, PartialEq, Eq)]
enum SkipCase {
    /// 설치본과 번들이 같은 버전 — 진짜로 최신이다.
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

/// `Skip` 갈래의 로그.
///
/// ## 왜 한 문장이면 안 되나
///
/// 이 자리는 예전에 두 경우를 `up-to-date` 한 문장에 뭉쳐 `debug!` 로 흘렸다. release 파일
/// 로그는 warn 이상만 남으므로, **설치본이 번들보다 높아 이번 부팅이 번들을 안 쓴 사실이
/// 어디에도 안 남았다.** 빌드 쪽에는 이미 같은 함정의 경고가 있다("낡은 채로 조용히
/// 실행된다") — 이건 그 설치 경로 판이고, worktree 를 여러 개 두고 서로 다른 tip 에서
/// 부팅하면 실제로 다른 worktree 가 설치한 바이너리를 조용히 쓰게 된다.
///
/// 그래서 앞선 경우는 **warn** 으로 올리고, 문장에서 "up-to-date" 를 뺀다(최신이 아니라
/// 앞선 것이다). 되돌리는 수단을 문장 안에 이름으로 넣어, 로그만 보고 조치할 수 있게 한다.
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
    // F.B.11-4: bridge::validate_bin_extras 는 본 바이너리 chain — discover 와 동일하게
    // install/add 경로의 caller 가 chain 한다.
    let Ok(manifest) = Manifest::load(dest) else {
        return false;
    };
    if manifest.permissions.is_empty() {
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
    /// 변경 없이 통과 — installed >= bundle, 동일 버전 mtime resync, 사용자 제거 등.
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

/// 사용자/AI 가 명시 호출하는 builtin 재설치 진입점.
///
/// `install_builtins_if_needed` 와의 차이:
/// - `force` 입력으로 동일/하위 버전도 덮어쓸 수 있음 (recovery).
/// - 각 plugin 당 한 줄 report 항목 반환 — CLI/IPC 응답으로 노출.
/// - 변경된 항목이 있으면 PluginManager 의 packages/extensions 를 재계산.
///
/// 실행 중 plugin process 의 binary 가 교체될 수 있다 (POSIX 는 inode 교체로 안전,
/// Windows 는 sharing violation 가능). 본 함수는 *process 재시작을 수행하지 않는다*
/// — 새 binary 는 다음 plugin restart (또는 부팅) 후 효과 발생.
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

/// §2.E.1 의사결정: `restore_all` 이 true 이면 전체 clear (restore_removed 무시).
/// 그렇지 않으면 명시된 id 만 unmark. 변경이 있었으면 plugins.toml 저장.
fn restore_removed_builtins(
    mgr: &mut PluginManager,
    restore_removed: &[String],
    restore_all: bool,
) {
    let removed_cleared = apply_restore_removed(mgr, restore_removed, restore_all);
    if removed_cleared && let Err(e) = mgr.config.save() {
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

/// UpgradeVersion / ForceOverwrite 공통 스텝: (선택적 swap-shutdown) → overwrite →
/// (선택적 respawn). 순서·조건을 원본 그대로 보존한다.
/// Ok(SwapResult) 성공, Err(item) 은 Failed report 항목 (caller 는 그대로 push + skip).
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
    #[cfg(not(debug_assertions))]
    {
        use crate::bundle_sig::{TrustDecision, verify_bundle_signature};
        match verify_bundle_signature(src) {
            Ok(TrustDecision::Trusted) => Ok(()),
            Ok(TrustDecision::Untrusted {
                plugin_id,
                fingerprint,
                reason,
                ..
            }) => {
                // 임베드 + known_plugins.toml 모두 거부. release 빌드의 builtin 은
                // *반드시* 임베드 키로 통과되어야 하므로, Untrusted = 사실상 차단.
                // trust 모달은 외부 plugin 경로용(0.7+ marketplace) 이며 builtin
                // 흐름엔 노출하지 않는다.
                tracing::warn!(
                    "builtin '{}' is untrusted (reason: {reason:?}, fp: {fingerprint})",
                    plugin_id
                );
                Err(BuiltinUpgradeItem {
                    id: spec.id.into(),
                    action: BuiltinUpgradeAction::Skipped {
                        installed_version: read_installed_version(dest).map(|v| v.to_string()),
                        bundle_version: None,
                        reason: format!("untrusted: {reason:?}"),
                    },
                })
            }
            Err(e) => {
                tracing::warn!("builtin '{}' bundle signature check failed: {e}", spec.id);
                Err(BuiltinUpgradeItem {
                    id: spec.id.into(),
                    action: BuiltinUpgradeAction::Skipped {
                        installed_version: read_installed_version(dest).map(|v| v.to_string()),
                        bundle_version: None,
                        reason: format!("signature-invalid: {e}"),
                    },
                })
            }
        }
    }
    #[cfg(debug_assertions)]
    {
        // debug 빌드는 서명 검증 결과를 로그로만 남기고 설치를 건너뛰지 않으므로
        // 설치 대상 경로를 읽을 일이 없다 — release 갈래에서만 쓰인다.
        let _ = dest;
        match crate::bundle_sig::verify_bundle_signature(src) {
            Ok(_) => {}
            Err(e) => {
                tracing::debug!(
                    "dev bundle '{}' signature check: {e} (debug build = ignored)",
                    spec.id
                );
            }
        }
        Ok(())
    }
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
    // 방금 disable 되었거나 무응답 재시작 중인 plugin 은 옛 프로세스가 아직 빠지는 중일 수
    // 있다 — 둘 다 회수를 기다리지 않는다. 그 디렉토리에 **실제로 쓰는 갈래만** 그 회수를
    // 끝까지 기다린다(실행 중인 파일은 Windows 에서 덮어쓰거나 지울 수 없고 Linux 에서는
    // `ETXTBSY` 가 난다). 쓰지 않는 갈래가 기다리면 메인 스레드가 이유 없이 최대 2 s 선다 —
    // 그때 회수는 뒤에서 이어지고 재기동 예약도 회수 기록과 함께 남는다(ADR-0457).
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
    // 회수 중이던 것이 무응답 재시작이나 회수 중에 온 enable 이었으면 그 재기동 예약을
    // 여기서 잇는다 — 기다리며 예약을 가져왔으므로 안 이으면 enabled 인 plugin 이 꺼진 채
    // 남는다(새 파일로 뜬다).
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
            // 버전은 그대로여도 **파일이 바뀌었으면 바뀐 것**이다. 이 값이 false 로 고정돼
            // 있으면 같은 버전으로 내용만 고친 plugin 이 재기동 대상에서 조용히 빠진다.
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
    if config_dirty && let Err(e) = mgr.config.save() {
        tracing::warn!("upgrade_builtins: save plugins.toml failed: {e}");
    }
}

/// 기존 builtin grant entry에 매니페스트 신규 permission 만 증분 추가.
///
/// 기존 사용자의 `plugins.toml`에 이미 plugin entry가 있는 경우 (`grants` map에
/// 키가 있는 상태), `install_builtins_if_needed`의 step 2 첫 번째 분기 (entry
/// 없을 때만 set_granted)는 동작하지 않는다. 새 버전 builtin이 매니페스트에
/// permission을 추가했을 때 이 helper로 신규 token만 증분 grant 한다.
///
/// 기존 grant token은 제거하지 않는다 (사용자가 명시적으로 deny 했을 가능성).
/// 반환값: 신규 token이 하나라도 추가되면 true.
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
    if mgr.config.mark_builtin_removed(id)
        && let Err(e) = mgr.config.save()
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

/// 번들 → 설치 디렉터리 동기화. **내용이 다른 파일만** 쓰고, src 에 없는 dest 항목은 층마다
/// 지운다([`prune_dest_not_in_src`]).
///
/// **mtime 을 판정에 쓰지 않는다.** 예전에는 이 자리가 mtime 비교였고, 그러면 번들 쪽 mtime 이
/// 거짓일 때(`cp -p`·아카이브 해제처럼 mtime 을 보존하는 경로) **새 파일을 건너뛴다.** 그
/// 위험 때문에 부팅 경로는 아예 무조건 덮어쓰기를 골랐었고, 그 대가가 같은 번들로 재부팅해도
/// 번들 전량을 다시 쓰는 것이었다. 내용으로 판정하면 둘 다 필요 없다.
///
/// 반환값은 **무엇이든 썼는가**다. 호출자가 그 값으로 "같은 버전인데 내용이 바뀌었다" 를
/// 보고·재기동 판정에 쓴다 — 안 돌려주면 그 사실이 이 함수 안에서 사라진다.
fn sync_dir_by_content(src: &Path, dst: &Path) -> std::io::Result<bool> {
    sync_dir(src, dst, CopyPolicy::ContentDiffers)
}

/// 내용이 다를 때만 복사. 판정 순서가 **비용 순서**이고, 어느 단계에도 mtime 이 없다.
///
/// 1. dest 가 없다 → 복사
/// 2. 크기가 다르다 → 복사 (해시 불필요 — 재빌드는 대개 여기서 걸린다)
/// 3. 크기가 같다 → 양쪽을 **바이트로** 비교 (해시가 아니다 — 아래 함수 주석)
///
/// "크기와 mtime 이 둘 다 같으면 해시를 생략" 하는 지름길은 **넣지 않는다.** 그 한 줄이
/// mtime 을 판정에 도로 들여서, 번들 mtime 이 거짓이고 크기까지 같은 경우에 이 함수가 사려던
/// 내성이 사라진다 — 그리고 그 사라짐은 조용하다(거짓 mtime 이 실제로 올 때까지 증상이 없다).
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

/// 두 파일을 **바이트로** 비교한다. 통째로 안 올리고 64 KiB 씩 흘려 읽어 첫 불일치에서 끊는다.
///
/// ## 왜 해시가 아닌가 — 잰 값이다
///
/// 처음엔 양쪽 sha256 을 비교했다. 그것은 같은 물음에 **더 비싼 답**이다: 읽는 바이트는
/// 똑같이 양쪽 전부인데, 그 위에 암호 연산이 얹히고 **불일치를 만나도 끝까지 읽는다.**
/// 번들 45 파일 ≈ 1.1 GB(debug), 더운 캐시, 세 회 평균:
///
/// | 판정 | 시간 |
/// |------|------|
/// | 양쪽 sha256 | 4.31 s |
/// | 바이트 비교 | **0.20 s** |
///
/// 해시가 필요한 것은 원격 대조나 서명처럼 **한쪽만 있을 때**다. 두 파일이 모두 로컬에
/// 있으면 요약본을 만들 이유가 없다 — 같은지만 물으면 되고, 그 답에는 충돌 논증도 필요 없다.
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
mod upgrade_retire_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explorer_is_not_builtin_plugin() {
        // T11: explorer 는 host builtin surface 로 승격되어 plugin 번들에서 제거됨.
        assert!(!is_builtin_plugin("com.tasty.explorer"));
    }

    #[test]
    fn claude_design_is_not_builtin_plugin() {
        // docs/adr/0510-design-work-flows-from-claude-design-through-gallery-app-and-site.md: 별도 프로젝트로 분리하며
        // tasty 본체 번들에서 완전히 제거됨.
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

    /// 판정 기준으로 쓰는 **알려진 옛 시각**. 1970 + 1e6 초 = 1970-01-12.
    ///
    /// 시험이 "다시 쓰였나" 를 물을 때 두 스냅샷의 mtime 을 서로 비교하면 파일시스템 틱에
    /// 의존한다 — 두 쓰기가 같은 틱에 들어가면 나노초까지 같아서 **다시 썼는데도 같다**로
    /// 읽힌다(CI 러너에서 실제로 났다. 우리 디스크는 틱이 잘게 나뉘어 초록이었다).
    /// 대신 쓰기 전에 dest 를 이 값으로 찍어 두면, 다시 쓰인 파일은 "지금" 이 되어 이 값과
    /// **반드시** 다르고 안 쓰인 파일은 이 값 **그대로**다. 해상도와 무관해진다.
    fn stale_stamp() -> std::time::SystemTime {
        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000)
    }

    /// dest 의 모든 파일 mtime 을 [`stale_stamp`] 로 찍는다.
    ///
    /// ☆ 곁효과가 하나 있고 그게 이롭다 — 이러면 dest 가 src 보다 **오래된** 상태가 된다.
    /// mtime 을 보는 정책이라면 그것만으로 전량 재복사를 하므로, 아래 단정들은 "내용으로
    /// 판정한다" 까지 함께 묻게 된다.
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

    /// 스테이징 경로(`copy_if_newer`)의 동률 갈래 — **세 방향을 한 자리에서 묻는다.**
    ///
    /// 시계에 안 기댄다: 두 파일을 **같은 알려진 값**으로 찍는다. `cp -p`·아카이브 해제·
    /// rsync 가 mtime 을 원본 것으로 되돌린 상태가 정확히 이 모양이다.
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
            "시각이 같고 내용이 다르면 복사해야 한다 — 이 갈래가 낡은 바이너리를 들여보냈다"
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

        // ③ **선언한 구멍**: dest 가 더 새것이면 내용을 안 본다. 이 단정은 결함을 막는
        // 것이 아니라 **범위를 고정한다** — 넓히려면 전량 비교로 돌아가야 하고 그 대가는
        // `copy_if_newer` 의 doc 에 값으로 적혀 있다.
        std::fs::write(&src, "newer content").unwrap();
        set_mtime(&src, stale_stamp());
        set_mtime(&dest, stale_stamp() + std::time::Duration::from_secs(1));
        copy_if_newer(&src, &dest).unwrap();
        assert_eq!(
            std::fs::read_to_string(&dest).unwrap(),
            "new",
            "dest 가 더 새것이면 내용을 안 본다 — 선언된 구멍이다"
        );
    }

    #[test]
    fn second_sync_of_the_same_bundle_writes_nothing() {
        // 이 시험이 이 변경의 값 자체다 — 같은 번들로 두 번 돌면 두 번째는 아무것도 안 쓴다.
        // 판정은 mtime 스냅샷 차분이다(쓰였으면 mtime 이 움직인다).
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        let dest = tmp.path().join("dest");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("bin"), "content").unwrap();
        std::fs::write(src.join("tasty-plugin.toml"), "version = \"0.1.0\"").unwrap();

        sync_dir_by_content(&src, &dest).unwrap();
        // ★ 옛 시각으로 찍고 나서 묻는다. 이 단정은 방향이 반대라 **거짓 빨강**은 못 만들지만
        // **거짓 초록**은 만든다 — 다시 썼는데 두 쓰기가 같은 틱이면 값이 같아 통과한다.
        // 그쪽이 더 위험하다: 안 보이기 때문이고, 이 시험은 위 주석대로 "이 변경의 값 자체"
        // 라서 그 러너에서는 아무것도 안 재게 된다.
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
        // ★ 이 변경의 존재 이유. src 가 dest 보다 **오래된** mtime 을 갖고 크기까지 같은데
        // 내용이 다르다 — mtime 판정이면 건너뛰고, 내용 판정이면 복사한다.
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

    /// 이 스코프 동안 나가는 tracing 이벤트를 문자열로 모은다. 레벨을 **이름이 아니라
    /// 실제 이벤트**로 확인하려는 것이라, 판정 함수를 다시 부르는 대조가 되지 않는다.
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
        // release 파일 로그는 warn 이상만 남는다. 이 경우가 debug 로 나가면 "이번 부팅이
        // 번들을 안 썼다" 가 어디에도 안 남는다 — 그 침묵이 이 시험이 겨누는 것이다.
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
        // 반대 팔 — 진짜 up-to-date 까지 warn 으로 올리면 경고가 배경 소음이 된다.
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
        // 바이트 비교는 64 KiB 씩 흘려 읽는다. 첫 청크만 보고 답하면 번들 바이너리처럼 큰
        // 파일에서 **뒷부분만 바뀐 변경**을 통째로 놓친다 — 크기가 같으면 그 앞 단계도 안 걸러준다.
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
        // 청소 **단독** 단정. 이 시험이 없으면 "복사 정책을 바꿨더니 청소가 조용히
        // 사라졌다" 를 아무것도 못 잡는다 — 잔존 파일은 다른 단정을 안 건드린다.
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

        // ⓪ 대조군 — src 에 있는 것은 남아야 한다. 이 두 줄이 없으면 "지울 것만 지웠다" 와
        // "다 지웠다" 가 같은 모양으로 통과한다.
        assert!(dest.join("en.toml").exists(), "src 에 있는 파일을 지웠다");
        assert!(dest.join("ko.toml").exists(), "src 에 있는 파일을 지웠다");
        assert!(
            !dest.join("ja.toml").exists(),
            "crate 에서 빠진 lang 파일이 staging 에 남았다"
        );
    }

    /// [`CopyPolicy::Always`] 를 다른 둘과 가르는 성질은 **내용이 같아도 쓴다** 하나뿐이다.
    ///
    /// 그리고 그 차이는 **내용으로 관측되지 않는다** — 결과 파일이 어느 쪽이든 같아서, mtime 으로만
    /// 보인다. 이 시험이 없으면 누가 `Always` 를 `ContentDiffers` 로 접어도 스위트가 전부 초록이다
    /// (변이로 확인했다). arm 이 셋인 이유가 셋이 갈린다는 것인데, 한 갈림이 안 잡히면 enum 이
    /// 조용히 둘로 접힌다.
    ///
    /// 시계에 안 기댄다 — dest 를 [`stale_stamp`] 로 찍어 두면 다시 쓰인 파일은 "지금" 이 되어 그
    /// 값과 반드시 다르고, 안 쓰인 파일은 그 값 그대로다. 두 스냅샷을 서로 비교하지 않으므로
    /// 파일시스템 눈금과 무관하다.
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
            "Always 가 내용이 같다고 건너뛰었다 — 그러면 ContentDiffers 와 구별되지 않는다"
        );

        // ⓪ 대조군 — 같은 입력에 ContentDiffers 를 걸면 **안 써야** 한다. 이 칸이 없으면 위
        // 단정이 "이 계기는 아무 mtime 이나 움직인다" 와 구별되지 않는다.
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

    /// 세 이름이 **같은 훑기**를 쓰되 술어만 다르다는 것을 한 입력에서 가른다.
    ///
    /// 판별 입력은 하나뿐이다 — **dest 가 더 새것이고 내용이 다르다.** 그때만 셋의 답이
    /// 갈린다(내용이 같으면 뒤 둘이 같아지고, dest 가 더 옛것이면 셋이 다 쓴다). 합치기
    /// 전에는 이 칸을 재는 시험이 없었다: 세 함수가 각자 자기 시험만 갖고 있어서
    /// **셋을 같은 입력에 대 본 적이 없었다.**
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
        assert_eq!(read(&dest), "new", "Always 가 판정을 했다");

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

        // ⓪ 대조군 — dest 가 아예 없으면 셋 다 쓴다. 이 칸이 없으면 위 "old" 단정이
        // "이 술어는 아무것도 안 쓴다" 와 구별되지 않는다.
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
