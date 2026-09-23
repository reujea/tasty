//! Layout persistence: save/restore workspace layout to `~/.tasty/layouts/NN.json`.
//!
//! 창마다 독립된 레이아웃을 갖도록 디스크 레이어가 **슬롯 단위 파일**로 나뉘어
//! 있다. 슬롯 하나 = [`SavedLayout`] 하나 = engine 하나의 전체 상태. 순서·목록은
//! 파일명(`NN.json`)의 숫자에서 전부 파생되고 별도 인덱스 파일을 두지 않는다 —
//! 인덱스는 실제 파일과 desync 되는 두 번째 진실원이 된다.
//!
//! Captures the structural tree (workspaces → pane nodes → panes → tabs → surface layouts)
//! with minimal per-surface info (cwd, file path, url). No screen/scrollback content.
//!
//! `SavedSurface` is `Terminal` + `Generic { kind, data }`. New surface kinds (including
//! plugins) round-trip via the SurfaceKindRegistry without touching this file.

#[cfg(any(feature = "gui", test))]
mod capture;
mod restore;
mod schema;
#[cfg(any(feature = "gui", test))]
mod scrollback;
#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};
use std::time::Instant;

pub use schema::SavedLayout;

#[cfg(any(feature = "gui", test))]
use crate::core::CoreState;

pub(super) const LAYOUT_VERSION: u32 = 2;

// ── Disk I/O (slot files) ──

/// 레이아웃 슬롯 번호. 파일명(`NN.json`)의 숫자와 같다.
pub(crate) type LayoutSlotId = u32;

const LAYOUTS_SUBDIR: &str = "layouts";
const LEGACY_LAYOUT_FILE: &str = "layout.json";
const SLOT_EXT: &str = "json";

/// 슬롯 디렉터리 (`tasty_home()/layouts`).
///
/// debug/release 격리는 루트(`tasty_home()`)가 담당한다 — debug 빌드는
/// `~/.tasty-debug/layouts/`, release 는 `~/.tasty/layouts/`. 루트가 갈리므로
/// 파일명 접미사(`-debug`)는 두지 않는다.
fn layouts_dir() -> Option<PathBuf> {
    tasty_utils::path::tasty_home().map(|d| d.join(LAYOUTS_SUBDIR))
}

/// `dir/{slot:02}.json`. 2 자리 zero-pad 라 사전순 정렬이 99 까지는 숫자순과
/// 일치하고, 100 이상은 자릿수가 자연 확장된다(정렬 키는 어차피 파싱한 숫자다).
fn slot_path_in(dir: &Path, slot: LayoutSlotId) -> PathBuf {
    dir.join(format!("{slot:02}.{SLOT_EXT}"))
}

// ── 열거 ──

/// 존재하는 슬롯 번호를 오름차순으로. 파일명이 숫자가 아니면 무시한다.
fn list_slots_in(dir: &Path) -> Vec<LayoutSlotId> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        // 슬롯이 한 번도 저장된 적 없으면 디렉터리 자체가 없다 — 정상.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            tracing::warn!("layout slots: read_dir {} failed: {e}", dir.display());
            return Vec::new();
        }
    };
    let mut slots = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some(SLOT_EXT) {
            continue;
        }
        match path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<LayoutSlotId>().ok())
        {
            Some(slot) => slots.push(slot),
            None => tracing::warn!("layout slots: ignoring {}", path.display()),
        }
    }
    slots.sort_unstable();
    // `1.json` 과 `01.json` 이 함께 있으면 같은 슬롯으로 접힌다 — 목록에 같은
    // 번호가 두 번 나오면 호출자가 같은 파일을 두 번 읽는다.
    slots.dedup();
    slots
}

/// 존재하는 슬롯 번호를 오름차순으로.
///
/// 창 생성 시 free 슬롯을 고르는 `App::claim_free_layout_slot` 이 소비한다 —
/// "점유되지 않은 가장 낮은 기존 슬롯" 판정의 후보 집합이다. headless 빌드에는
/// 창 생성 경로가 없어 호출자가 없다.
#[cfg(feature = "gui")]
pub(crate) fn list_slots() -> Vec<LayoutSlotId> {
    match layouts_dir() {
        Some(dir) => list_slots_in(&dir),
        None => Vec::new(),
    }
}

// ── 로드 ──

/// 슬롯 하나를 읽은 결과.
///
/// "없음" 과 "못 읽음" 을 **구분한다.** 둘을 같은 `None` 으로 뭉개면 권한 오류나 손상
/// JSON 이 "이 슬롯을 쓴 적 없음" 과 같아지고, 그 뒤 [`save_slot`] 이 같은 슬롯에 현재
/// 상태를 써서 사용자의 창 구성을 대체한다. 읽지 못한 슬롯은 덮어쓰지 않는 것이 유일한
/// 보호 수단이다.
pub(crate) enum SlotLoad {
    /// 정상적으로 읽고 파싱했다.
    Loaded(SavedLayout),
    /// 슬롯 파일이 없다 — 첫 실행이거나 그 슬롯을 쓴 적이 없다. 새로 써도 잃을 것이 없다.
    Absent,
    /// 파일이 있는데 **읽지** 못했다(권한 · IO), 또는 이 빌드가 모르는 미래 version 이다.
    /// 사용자 레이아웃이 디스크에 남아 있고 옮길 수도 없으므로 **저장하면 안 된다.**
    Unreadable,
    /// 파일을 읽었지만 해석하지 못했다. 원본은 그 자리에 그대로 있다 — 저장 직전에
    /// `NN.json.bak` 으로 옮긴 뒤 쓴다.
    Unparsable,
}

/// 슬롯 하나를 읽는다.
///
/// **읽기는 파일을 건드리지 않는다.** 해석 실패는 [`SlotLoad::Unparsable`] 로만 알리고,
/// 보존(백업으로 이동)은 실제로 덮어쓰려는 순간([`save_slot`])에 한다 — 부팅 중 이 슬롯을
/// 읽는 곳이 하나가 아니라서(GC 와 engine), 읽는 쪽이 옮기면 나중에 읽는 쪽은 사건 자체를
/// 못 본다.
///
/// 읽기 자체가 실패한 경우(권한·IO)도 파일을 건드리지 않는다. 내용을 확인하지 못한
/// 파일은 옮길 수조차 없으므로 저장을 막는 것이 유일한 보호 수단이다.
///
/// 지원 범위 밖 version 도 [`SlotLoad::Unreadable`] 이다. 파일 자체는 멀쩡하고 새 버전의
/// tasty 가 읽을 수 있으므로 백업하지 않고 그대로 둔다 — 구버전으로 한 번 켰다고 신버전이
/// 저장한 레이아웃이 사라져서는 안 된다.
fn load_slot_in(dir: &Path, slot: LayoutSlotId) -> SlotLoad {
    let path = slot_path_in(dir, slot);
    match std::fs::read_to_string(&path) {
        Ok(json) => parse_slot_json(&path, &json),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => SlotLoad::Absent,
        Err(e) => {
            tracing::error!(
                "failed to read layout slot {}: {e} — starting without it and refusing to \
                 overwrite it (fix permissions or move the file to start fresh)",
                path.display()
            );
            SlotLoad::Unreadable
        }
    }
}

/// 슬롯 JSON 의 판정만 한다 — **로그를 남기지 않는다.**
///
/// 로드 시점(`parse_slot_json`)과 저장 직전 재확인(`preserve_unparsable_slot`)이 같은
/// 기준을 쓴다. 두 곳이 어긋나면 "로드는 손상이라 했는데 저장은 정상이라 한다" 같은
/// 모순이 생기므로, 판정은 **이 함수 하나뿐이고** 나머지는 그 결과에 로그만 얹는다.
fn classify_slot_json(json: &str) -> SlotLoad {
    match serde_json::from_str::<SavedLayout>(json) {
        Ok(layout) if layout.version > LAYOUT_VERSION => SlotLoad::Unreadable,
        Ok(layout) => SlotLoad::Loaded(layout),
        Err(_) => SlotLoad::Unparsable,
    }
}

/// 읽어온 슬롯 JSON 을 해석한다. 지원 범위 밖 version 은 파일을 그대로 두고 잠근다.
/// 판정은 [`classify_slot_json`] 에 맡기고 여기서는 사용자에게 필요한 로그만 남긴다.
fn parse_slot_json(path: &Path, json: &str) -> SlotLoad {
    let load = classify_slot_json(json);
    match &load {
        SlotLoad::Unreadable => tracing::error!(
            "layout slot {} holds a version this build does not support ({} or lower) — \
             starting without it and leaving it untouched so a newer build can still read it",
            path.display(),
            LAYOUT_VERSION
        ),
        SlotLoad::Unparsable => tracing::error!(
            "failed to parse layout slot {} — starting fresh; the file is left as it \
             is and will be moved aside to a .bak before anything overwrites it",
            path.display()
        ),
        SlotLoad::Loaded(_) | SlotLoad::Absent => {}
    }
    load
}

/// 슬롯 하나를 읽는다. 홈을 못 찾으면 [`SlotLoad::Unreadable`] — 경로를 모르는 채
/// 저장하면 엉뚱한 자리에 쓰게 되므로 "없음" 으로 낙관하지 않는다.
pub(crate) fn load_slot(slot: LayoutSlotId) -> SlotLoad {
    match layouts_dir() {
        Some(dir) => load_slot_in(&dir, slot),
        None => SlotLoad::Unreadable,
    }
}

/// 이 슬롯을 옆으로 옮길 백업 자리가 **이미** 소진됐는가 — 파일을 건드리지 않는다.
///
/// 보존은 첫 저장(= `finish_boot` 이후)에 일어나는데, 부팅 알림은 그보다 먼저 뜬다.
/// 그래서 알림이 "옆에 `.bak` 으로 보관합니다" 와 "옮기지 못해 저장이 막힙니다" 중
/// 무엇을 말할지는 여기서 미리 갈라야 한다. 예산 계산은 복제하지 않고
/// `tasty_utils::path::backup_budget_is_exhausted` 하나를 쓴다.
pub(crate) fn slot_preservation_is_blocked_in(dir: &Path, slot: LayoutSlotId) -> bool {
    tasty_utils::path::backup_budget_is_exhausted(&slot_path_in(dir, slot))
}

/// [`slot_preservation_is_blocked_in`] 의 실제 홈 판. layouts 디렉터리를 못 찾으면
/// 저장 자체가 일어나지 않으므로 "막혔다" 고 하지 않는다.
pub(crate) fn slot_preservation_is_blocked(slot: LayoutSlotId) -> bool {
    layouts_dir().is_some_and(|dir| slot_preservation_is_blocked_in(&dir, slot))
}

// ── 저장 ──

/// Save layout to disk. Non-blocking best-effort.
///
/// `&mut CoreState` 인 이유: `SavedLayout::capture` 가 새 persist_id 를 발급하면
/// 해당 surface 인스턴스의 `scrollback_persist_id` 필드에 기록해 다음 capture 가
/// 같은 ID 를 재사용한다.
///
/// 호출자는 항상 `DomainIntent::SaveLayoutNow` (Core::apply 내부) 를 경유한다 —
/// module 외부에서 본 fn 을 직접 부르지 않도록 `pub(crate)` 로 제한.
#[cfg(any(feature = "gui", test))]
pub(crate) fn save_slot(engine: &mut CoreState, active_workspace: usize, slot: LayoutSlotId) {
    // 테스트는 engine 에 심어 둔 임시 경로를 쓴다 — 저장 경로 전체
    // (`apply_save_layout_now` → `save_slot` → 보존 → 쓰기)를 사용자의 실제 홈을
    // 건드리지 않고 지나가려면 이 지점 하나만 갈아끼우면 된다.
    #[cfg(test)]
    let resolved = engine.layouts_dir_override.clone().or_else(layouts_dir);
    #[cfg(not(test))]
    let resolved = layouts_dir();
    let dir = match resolved {
        Some(d) => d,
        None => {
            tracing::error!(
                "cannot determine the tasty home directory for layout save — this session's \
                 window layout will not be restored next time"
            );
            return;
        }
    };
    save_slot_in_dir(engine, active_workspace, slot, &dir);
}

/// `save_slot` 의 본체 — layouts 디렉터리를 인자로 받는다.
///
/// 보존 판정과 쓰기가 **한 함수 안에서 이어져** 있어야 "백업하지 않고 덮어썼다" 를
/// 테스트가 잡을 수 있다. 분리하면 각각은 통과하는데 배선만 빠진 상태를 놓친다.
///
/// `save_slot` 과 마찬가지로 프로덕션 호출자는 `DomainIntent::SaveLayoutNow`(`Core::apply`
/// 내부) 하나뿐이다 — 여기를 직접 부르면 `layout_slot_protected` 가드(`apply_save_layout_now`)
/// 를 건너뛰어 **읽지 못한 슬롯에 써 버린다.** 디렉터리를 지정해야 하는 테스트 외에는
/// `save_slot` 을 쓴다.
#[cfg(any(feature = "gui", test))]
pub(crate) fn save_slot_in_dir(
    engine: &mut CoreState,
    active_workspace: usize,
    slot: LayoutSlotId,
    dir: &Path,
) {
    let Some(json) = serialize_layout(engine, active_workspace) else {
        return;
    };
    // 부팅 때 해석하지 못한 슬롯이면, 덮어쓰기 전에 원본을 옆으로 옮긴다. 옮기지 못하면
    // 쓰지 않는다 — 사용자의 창 구성이 남아 있는 자리를 지우게 된다.
    if engine.layout_slot_unparsable {
        if !preserve_unparsable_slot(dir, slot) {
            engine.layout_slot_preserve_failed = true;
            return;
        }
        engine.layout_slot_unparsable = false;
        engine.layout_slot_preserve_failed = false;
    }
    save_slot_in(dir, slot, &json);
}

/// 저장 직전 재확인의 결론.
#[cfg(any(feature = "gui", test))]
enum SlotReplace {
    /// 원본을 옆으로 옮긴 뒤 써야 한다 — 여전히 해석되지 않는다.
    MoveAside,
    /// 그대로 써도 된다 — 지금은 해석되거나, 파일이 이미 없다.
    WriteOver,
    /// 쓰면 안 된다 — 내용을 모르거나, 신버전이 써 놓은 것이다.
    Refuse,
}

/// 부팅 때 세운 플래그와 이 호출 사이에 파일이 바뀌었을 수 있으므로 다시 읽어 본다.
/// 같은 `TASTY_HOME` 을 쓰는 다른 인스턴스가 같은 슬롯에 써 넣는 경우다(슬롯 점유는
/// 프로세스 안에서만 본다). 설정 경로(`Settings::protect_existing_file`)가 하는
/// 재확인과 같은 것이고, 판정 기준도 `classify_slot_json` 으로 공유한다.
///
/// **경합을 없애지는 못한다 — 좁힐 뿐이다.** 여기의 read 와 호출자의 rename 은 별개
/// syscall 이고 사이에 잠금이 없어서, 그 틈에 끼어든 write 는 여전히 정상 파일을
/// `.bak` 으로 흘린다. 닫히는 것은 "부팅 판정 → 첫 저장"(수 분) 창이고 남는 것은
/// 두 syscall 사이다. 잠금을 도입하지 않은 근거는 `docs/design/systems/storage.md`.
#[cfg(any(feature = "gui", test))]
fn recheck_slot_before_replacing(path: &Path) -> SlotReplace {
    let json = match std::fs::read_to_string(path) {
        Ok(json) => json,
        // 이미 사라졌다면 지킬 것이 없다.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return SlotReplace::WriteOver,
        // 내용을 확인하지 못한 파일은 옮길 수도, 덮어쓸 수도 없다.
        Err(e) => {
            tracing::error!(
                "refusing to overwrite the layout slot {}: it could not be re-read before being \
                 replaced ({e}) — this session's window layout will not be saved",
                path.display()
            );
            return SlotReplace::Refuse;
        }
    };
    match classify_slot_json(&json) {
        // 지금은 해석된다 — 옮길 이유가 없다. 이 슬롯은 이 engine 것이므로 현재
        // 상태로 덮어쓰는 것이 정상 동작이다. 멀쩡한 파일을 `.bak` 으로 흘리면
        // 9 개뿐인 백업 예산을 정상 파일이 깎는다.
        SlotLoad::Loaded(_) => SlotReplace::WriteOver,
        // 그 사이 이 빌드가 모르는 version 이 됐다(신버전이 써 놓았다). 백업하지도
        // 덮어쓰지도 않는다 — 구버전으로 한 번 켰다고 신버전 레이아웃이 사라지면 안 된다.
        SlotLoad::Unreadable => {
            tracing::error!(
                "refusing to overwrite the layout slot {}: it now holds a layout from a newer \
                 build — this session's window layout will not be saved",
                path.display()
            );
            SlotReplace::Refuse
        }
        SlotLoad::Unparsable | SlotLoad::Absent => SlotReplace::MoveAside,
    }
}

/// 해석하지 못한 슬롯 파일을 `NN.json.bak` 으로 옮긴다. 덮어써도 되면 `true`.
///
/// **옮기기 전에 `recheck_slot_before_replacing` 으로 다시 확인한다** — 부팅 때의 판정과
/// 이 호출 사이에 파일이 바뀌어 있을 수 있다.
#[cfg(any(feature = "gui", test))]
fn preserve_unparsable_slot(dir: &Path, slot: LayoutSlotId) -> bool {
    let path = slot_path_in(dir, slot);
    match recheck_slot_before_replacing(&path) {
        SlotReplace::WriteOver => return true,
        SlotReplace::Refuse => return false,
        SlotReplace::MoveAside => {}
    }
    match tasty_utils::path::preserve_corrupt_file(&path) {
        Ok(Some(backup)) => {
            tracing::error!(
                "the layout slot {} could not be parsed at startup; it was moved to {} before \
                 being replaced",
                path.display(),
                backup.display()
            );
            true
        }
        Ok(None) => true,
        Err(e) => {
            tracing::error!(
                "refusing to overwrite the layout slot {}: the unparsable original could not be \
                 preserved ({e}) — this session's window layout will not be saved; move the \
                 file aside first",
                path.display()
            );
            false
        }
    }
}

#[cfg(any(feature = "gui", test))]
fn serialize_layout(engine: &mut CoreState, active_workspace: usize) -> Option<String> {
    let saved = SavedLayout::capture(engine, active_workspace);
    match serde_json::to_string_pretty(&saved) {
        Ok(j) => Some(j),
        Err(e) => {
            tracing::error!(
                "failed to serialize layout: {e} — this session's window layout will not be \
                 restored next time"
            );
            None
        }
    }
}

/// 슬롯 파일 write. **원자적**이어야 한다 — 슬롯이 여러 개이므로 잘린 JSON 하나가
/// union GC 를 통해 *다른* 슬롯의 scrollback 까지 잃게 만든다
/// ([`gc_scrollback_orphans_all_slots`] 의 "모르면 지우지 않는다"). tmp write →
/// rename 은 `store::scrollback::write_in` 과 같은 패턴이다.
#[cfg(any(feature = "gui", test))]
fn save_slot_in(dir: &Path, slot: LayoutSlotId, json: &str) {
    let path = slot_path_in(dir, slot);
    if let Err(e) = write_slot_atomic(dir, &path, json) {
        tracing::error!(
            "failed to write layout slot {}: {e} — this session's window layout will not be \
             restored next time",
            path.display()
        );
    }
}

/// tmp write → rename. rename 이 실패하면 tmp 를 치운다 — 확장자가 `.tmp` 라
/// [`list_slots_in`] 이 슬롯으로 오인하지는 않지만 그대로 두면 계속 쌓인다.
#[cfg(any(feature = "gui", test))]
fn write_slot_atomic(dir: &Path, path: &Path, json: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = path.with_extension(format!("{SLOT_EXT}.tmp"));
    std::fs::write(&tmp, json)?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            if let Err(cleanup) = std::fs::remove_file(&tmp) {
                tracing::debug!("layout slots: cleanup {} failed: {cleanup}", tmp.display());
            }
            Err(e)
        }
    }
}

// ── 삭제 ──

#[cfg(any(feature = "gui", test))]
fn delete_slot_in(dir: &Path, slot: LayoutSlotId) {
    let path = slot_path_in(dir, slot);
    if let Err(e) = std::fs::remove_file(&path)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!("Failed to delete {}: {e}", path.display());
    }
}

/// 슬롯 파일을 지운다. 없으면 no-op — `restore_layout` 이 꺼진 채로 창이 닫힐 때
/// (`App::retire_main_engine`) 쓴다.
#[cfg(feature = "gui")]
pub(crate) fn delete_slot(slot: LayoutSlotId) {
    let Some(dir) = layouts_dir() else { return };
    delete_slot_in(&dir, slot);
}

// ── 레거시 마이그레이션 ──

/// 단일 파일 시절의 `tasty_home()/layout.json` 을 슬롯 1 로 옮긴다.
///
/// `layouts/` 가 **없을 때만** 동작한다 — 이미 슬롯을 쓰고 있는 인스턴스에
/// 옛 파일이 남아 있어도 덮어쓰지 않는다. 복사가 아니라 rename 이라 중간
/// 상태(양쪽에 반쪽 파일)가 생기지 않는다.
///
/// 부팅 1 회 호출을 전제로 한다([`migrate_and_gc_on_boot`]).
fn migrate_legacy_in(home: &Path) {
    let legacy = home.join(LEGACY_LAYOUT_FILE);
    if !legacy.exists() {
        return;
    }
    let dir = home.join(LAYOUTS_SUBDIR);
    if dir.exists() {
        // 조용히 방치하면 사용자는 이 파일이 왜 무시되는지 알 수 없다.
        tracing::warn!(
            "{} is ignored — layout slots live in {} now (safe to delete the legacy file)",
            legacy.display(),
            dir.display()
        );
        return;
    }
    if let Err(e) = move_legacy_into_slot_one(&legacy, &dir) {
        // 레이아웃이 유실된 것이 아니라 "이번 부팅에 복원되지 않는" 강등이다.
        tracing::warn!(
            "layout migration: {} -> {} failed: {e}",
            legacy.display(),
            dir.display()
        );
    }
}

fn move_legacy_into_slot_one(legacy: &Path, dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::rename(legacy, slot_path_in(dir, 1))
}

// ── scrollback union GC ──

/// 전 슬롯이 참조하는 `scrollback_ref` 의 **합집합**으로 GC 를 1 회 돌린다.
///
/// 슬롯별로 GC 하면 슬롯 1 을 읽은 창이 슬롯 2·3 이 참조하는 `.bin` 을 전부
/// orphan 으로 판정해 지운다. 그래서 GC 는 engine 생성 시점이 아니라 **부팅 1 회**,
/// 전 슬롯 union 으로만 일어난다.
///
/// **존재하는데 파싱이 안 되는 슬롯이 하나라도 있으면 아무것도 지우지 않는다** —
/// 그 슬롯이 무엇을 참조하는지 모르는 채 GC 하면 손상은 JSON 하나인데 손실은
/// 그 슬롯의 scrollback 전체가 된다. "모르면 지우지 않는다" 가 안전한 방향이다.
/// (파일이 아예 없는 슬롯 번호는 애초에 [`list_slots_in`] 에 안 잡히므로 해당 없음.)
fn gc_scrollback_orphans_all_slots_in(layouts: &Path, scrollback: &Path) {
    let mut union = std::collections::HashSet::new();
    for slot in list_slots_in(layouts) {
        match load_slot_in(layouts, slot) {
            SlotLoad::Loaded(layout) => union.extend(layout.collect_scrollback_refs()),
            // `list_slots_in` 이 잡은 번호라 파일은 있었다. `Absent` 는 방금 손상 파일을
            // 백업으로 옮긴 경우 — 그 슬롯이 무엇을 참조했는지 여전히 모르므로 아래
            // `Unreadable` 과 똑같이 전면 스킵한다("모르면 지우지 않는다").
            SlotLoad::Absent | SlotLoad::Unreadable | SlotLoad::Unparsable => {
                tracing::warn!(
                    "scrollback GC skipped: layout slot {slot} in {} could not be read — \
                     keeping every scrollback file",
                    layouts.display()
                );
                return;
            }
        }
    }
    crate::scrollback_store::gc_orphans_in(scrollback, &union);
}

/// 전 슬롯 union scrollback GC. 슬롯이 하나도 없으면 빈 집합으로 호출한다
/// (알려진 ref 가 없으므로 전부 orphan — 슬롯 도입 전과 같은 의미).
fn gc_scrollback_orphans_all_slots() {
    let (Some(layouts), Some(scrollback)) =
        (layouts_dir(), crate::scrollback_store::scrollback_dir())
    else {
        return;
    };
    gc_scrollback_orphans_all_slots_in(&layouts, &scrollback);
}

/// 부팅 1 회 훅. **마이그레이션 → GC 순서가 고정**이다 — GC 가 마이그레이션
/// 결과(슬롯 1 로 옮겨진 레거시 레이아웃)를 봐야 그 참조를 union 에 넣는다.
/// engine(슬롯 로드)이 만들어지기 전에 불러야 한다.
///
/// `restore_layout` 이 꺼져 있으면 GC 를 건너뛴다 — 그 설정에서는 애초에 슬롯을
/// 저장하지 않으므로(`impl_workspace::apply_save_layout_now` 의 게이트), union 이 비어
/// 옛 `.bin` 을 전부 지우게 된다. 슬롯 도입 전에도 GC 는 같은 조건 안에 있었다.
/// 마이그레이션 자체는 설정과 무관하게 한다(나중에 켰을 때 복원되도록).
pub(crate) fn migrate_and_gc_on_boot(restore_layout: bool) {
    if let Some(home) = tasty_utils::path::tasty_home() {
        migrate_legacy_in(&home);
    }
    if restore_layout {
        gc_scrollback_orphans_all_slots();
    }
}

// ── Dirty flag / debounce state ──

/// Tracks whether the layout has been modified and needs saving.
#[derive(Default)]
pub struct LayoutDirtyTracker {
    dirty: bool,
    dirty_since: Option<Instant>,
}

impl LayoutDirtyTracker {
    pub fn new() -> Self {
        Self::default()
    }
}

impl LayoutDirtyTracker {
    /// Mark layout as dirty (called on structural changes).
    ///
    /// 시각은 **처음 dirty 가 된 순간**만 기록한다(뒤이은 변경으로 리셋하지 않는다)
    /// — 연속 변경 중에도 첫 변경으로부터 debounce 안에 반드시 한 번 저장된다.
    pub fn mark_dirty(&mut self) {
        if !self.dirty {
            self.dirty = true;
            self.dirty_since = Some(Instant::now());
        }
    }

    /// 처음 dirty 가 된 시각. 호스트가 여기에 debounce 를 더해
    /// `Tick::LayoutFlush` 데드라인을 잡는다(`docs/dev-guide/timer-hub.md`) —
    /// 주기 판정은 이 타입이 아니라 타이머 허브가 한다.
    #[cfg(feature = "gui")]
    pub fn dirty_since(&self) -> Option<Instant> {
        self.dirty_since
    }

    /// Reset after a successful save.
    #[cfg(any(feature = "gui", test))]
    pub fn clear(&mut self) {
        self.dirty = false;
        self.dirty_since = None;
    }

    /// Force check if dirty (for shutdown flush).
    #[cfg(any(feature = "gui", test))]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
}
