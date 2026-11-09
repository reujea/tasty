//! explorer 드래그 앤 드롭. 항목을 폴더 행·셀, 트리 노드, 즐겨찾기, 목록 본문(보고 있는 폴더)에
//! 놓으면 붙여넣기와 같은 작업을 요청한다. 다른 explorer 칸에 놓을 수도 있다.
//!
//! 칸들은 egui `DragAndDrop` payload 를 함께 읽는다. 각 칸은 그리는 동안 놓을 수 있는 자리를
//! [`note`] 류로 남기고, 상태줄을 그린 뒤 [`frame`] 이 그 자리로 드래그 시작·대상·놓기를 처리한다.

use std::path::{Path, PathBuf};
use std::time::Instant;

use tasty_settings::keybindings::parse::Combo;
use tasty_type_appearance::theme::Theme;
use tasty_ui_widgets::{DragChipProps, DragOp, DropTintSlot, drag_chip, paint_drop_ring};

use super::ExplorerView;
use super::ops::{OpsAction, glyph};
use crate::adapters::ui::icons;
use crate::app::explorer_files::job::OpKind;
use crate::core::fs_list::DirEntryInfo;
use crate::i18n::{t, t_fmt};

/// 끌고 있는 항목. 시작한 칸만 놓을 곳이 없을 때의 칩을 그린다.
pub(crate) struct Payload {
    paths: Vec<PathBuf>,
    single_dir: bool,
    source: egui::Id,
    /// 경로가 있는 컴퓨터. 로컬이면 None, 원격이면 그 mirror workspace 다. 같은 곳에만 놓는다.
    origin: Option<u32>,
    /// 원격 경로가 있는 디스크의 서버 쪽 식별값. 모르면 None 이고 놓을 때 복사가 기본이다.
    volume: Option<u64>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    /// 목록·격자·자세히 보기의 항목. `wide` 면 자세히 보기 행이라 본문 폭 전체가 행이다.
    Entry {
        wide: bool,
    },
    Tree {
        open: bool,
    },
    Favorite,
    /// 보고 있는 폴더(목록 본문 전체).
    Body,
}

#[derive(Clone, Debug)]
struct Spot {
    path: PathBuf,
    is_dir: bool,
    /// 보이는 부분(잘린 자리).
    rect: egui::Rect,
    /// 잘리기 전 칸 전체.
    full: egui::Rect,
    kind: Kind,
}

#[derive(Clone, Default)]
struct Spots(Vec<Spot>);

/// 놓을 자리를 차지한 마지막 pass. 아무 칸도 차지하지 않으면 시작한 칸이 거절 칩을 그린다.
#[derive(Clone, Copy, Default)]
struct Claim(u64);

/// 칸 하나의 드래그 상태. 대상 폴더가 바뀔 때만 디스크를 다시 읽는다.
#[derive(Default)]
pub(crate) struct DragState {
    /// 드래그 동작을 뒤집는 modifier. 호스트가 매 프레임 설정에서 채운다.
    pub(crate) flip: Option<Combo>,
    /// popup·modal 이 떠 있다. 그동안은 드래그를 시작하지도 받지도 않는다.
    pub(crate) blocked: bool,
    /// 이번 누름에서 이미 드래그를 시작했다. Esc 로 취소된 뒤 같은 누름으로 다시 시작하지 않는다.
    started: bool,
    source_widget: Option<egui::Id>,
    hover: Option<(PathBuf, Instant)>,
    probe: Option<Probe>,
    /// OS 에서 끌어 오는 파일. 호스트가 매 프레임 창의 hover 상태에서 채운다.
    pub(crate) os: Option<OsHover>,
    /// OS 파일을 놓으면 갈 곳. 놓기 처리(`take_os_drop`)가 소비한다.
    os_target: Option<(PathBuf, Verdict)>,
    /// 이번 프레임의 틴트 자리. 틴트는 행 아래에 깔리는데 대상은 행을 그린 뒤에 정해진다.
    slots: TintSlots,
    /// 앞 프레임에 놓기 틴트가 덮은 파일 영역. 행은 틴트보다 먼저 그려져 앞 프레임 값으로 글자를 고른다.
    drop_tint: Option<DropTint>,
}

/// 칠할 면마다 잡아 둔 틴트 자리. 이름 입력 대상과 끌기 대상은 함께 보일 수 있어 따로 둔다.
#[derive(Clone, Copy, Default)]
struct TintSlots {
    files: Option<DropTintSlot>,
    create: Option<DropTintSlot>,
    sidebar: Option<DropTintSlot>,
}

#[derive(Clone)]
enum DropTint {
    Body,
    Row(PathBuf),
}

impl DragState {
    /// 파일 영역의 바탕을 칠한 직후, 행을 그리기 전에 부른다.
    pub(crate) fn reserve_files(&mut self, ui: &egui::Ui) {
        self.slots.create = Some(DropTintSlot::reserve(ui.painter()));
        self.slots.files = Some(DropTintSlot::reserve(ui.painter()));
    }

    /// 사이드바의 바탕을 칠한 직후, 행을 그리기 전에 부른다.
    pub(crate) fn reserve_sidebar(&mut self, ui: &egui::Ui) {
        self.slots.sidebar = Some(DropTintSlot::reserve(ui.painter()));
    }

    #[cfg(test)]
    pub(crate) fn tint_body_for_test(&mut self) {
        self.drop_tint = Some(DropTint::Body);
    }

    /// 이 경로의 행이 놓기 틴트 아래에 있는지.
    pub(crate) fn drops_on(&self, path: &Path) -> bool {
        match &self.drop_tint {
            Some(DropTint::Body) => true,
            Some(DropTint::Row(row)) => row == path,
            None => false,
        }
    }

    fn note_drop_tint(&mut self, kind: Kind, dest: &Path) {
        self.drop_tint = match kind {
            Kind::Body => Some(DropTint::Body),
            Kind::Entry { .. } => Some(DropTint::Row(dest.to_path_buf())),
            Kind::Tree { .. } | Kind::Favorite => None,
        };
    }
}

/// OS 에서 창 위로 끌어 온 파일과 포인터 위치(논리 포인트).
#[derive(Clone, Debug)]
pub(crate) struct OsHover {
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) pos: egui::Pos2,
}

struct Probe {
    dest: PathBuf,
    paths: Vec<PathBuf>,
    same_volume: bool,
    writable: bool,
    /// 판정에 쓰는 원본·대상 경로([`resolve`]).
    resolved: (Vec<PathBuf>, PathBuf),
}

/// 놓기 판정.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Verdict {
    Go(OpKind),
    /// 거절 이유의 번역 키.
    No(&'static str),
}

fn spots_id() -> egui::Id {
    egui::Id::new("explorer_drag_spots")
}

fn claim_id() -> egui::Id {
    egui::Id::new("explorer_drag_claim")
}

fn push(ui: &egui::Ui, spot: Spot) {
    let rect = spot.rect.intersect(ui.clip_rect());
    if !rect.is_positive() {
        return;
    }
    ui.ctx().data_mut(|d| {
        d.get_temp_mut_or_default::<Spots>(spots_id()).0.push(Spot {
            rect,
            full: spot.rect,
            ..spot
        })
    });
}

/// 목록·격자 항목 자리.
pub(crate) fn note(ui: &egui::Ui, entry: &DirEntryInfo, rect: egui::Rect) {
    if entry.name == ".." {
        return;
    }
    push(
        ui,
        Spot {
            path: entry.path.clone(),
            is_dir: entry.is_dir,
            rect,
            full: egui::Rect::NOTHING,
            kind: Kind::Entry { wide: false },
        },
    );
}

/// 자세히 보기의 이름 셀. 행 전체를 대상으로 삼는다.
pub(crate) fn note_row(ui: &egui::Ui, entry: &DirEntryInfo) {
    if entry.name == ".." {
        return;
    }
    push(
        ui,
        Spot {
            path: entry.path.clone(),
            is_dir: entry.is_dir,
            rect: ui.max_rect(),
            full: egui::Rect::NOTHING,
            kind: Kind::Entry { wide: true },
        },
    );
}

pub(crate) fn note_tree(ui: &egui::Ui, dir: &Path, rect: egui::Rect, open: bool) {
    push(
        ui,
        Spot {
            path: dir.to_path_buf(),
            is_dir: true,
            rect,
            full: egui::Rect::NOTHING,
            kind: Kind::Tree { open },
        },
    );
}

pub(crate) fn note_favorite(ui: &egui::Ui, path: &Path, rect: egui::Rect) {
    push(
        ui,
        Spot {
            path: path.to_path_buf(),
            is_dir: true,
            rect,
            full: egui::Rect::NOTHING,
            kind: Kind::Favorite,
        },
    );
}

/// 자리의 실제 영역. 자세히 보기 행은 본문 폭으로 넓힌다.
fn area(spot: &Spot, body: egui::Rect) -> egui::Rect {
    match spot.kind {
        Kind::Entry { wide: true } => {
            egui::Rect::from_x_y_ranges(body.x_range(), spot.rect.y_range())
        }
        _ => spot.rect,
    }
}

/// 이번 프레임에 그린 목록 항목. 자세히 보기 행은 본문 폭으로 넓힌다.
fn drawn_entries(spots: &[Spot], body: egui::Rect) -> Vec<super::cursor::Drawn> {
    spots
        .iter()
        .filter(|s| matches!(s.kind, Kind::Entry { .. }))
        .map(|s| {
            let full = Spot {
                rect: s.full,
                ..s.clone()
            };
            super::cursor::Drawn {
                path: s.path.clone(),
                rect: area(&full, body),
                shown: area(s, body),
            }
        })
        .collect()
}

/// 포인터 아래의 대상: 폴더면 그 폴더, 파일·빈 곳이면 보고 있는 폴더(본문 전체).
fn target(
    spots: &[Spot],
    pos: egui::Pos2,
    body: egui::Rect,
    shown: Option<&Path>,
) -> Option<(PathBuf, egui::Rect, Kind)> {
    let hit = spots.iter().rev().find(|s| area(s, body).contains(pos));
    match hit {
        Some(s) if s.is_dir => Some((s.path.clone(), area(s, body), s.kind)),
        Some(s) if !matches!(s.kind, Kind::Entry { .. }) => None,
        _ if body.contains(pos) => shown.map(|d| (d.to_path_buf(), body, Kind::Body)),
        _ => None,
    }
}

fn first_parent(paths: &[PathBuf]) -> Option<&Path> {
    paths.first().and_then(|p| p.parent())
}

/// 이 칸에 `origin` 의 경로를 놓을 수 없는 이유. 다른 컴퓨터의 경로는 놓지 않고, 원격 쓰기를 받지 않는
/// 서버의 칸에도 놓지 않는다.
fn blocked(view: &ExplorerView, origin: Option<u32>) -> Option<&'static str> {
    if origin != view.mirror_ws_id {
        Some("explorer.drag.other_computer")
    } else if !view.writes_allowed {
        Some("explorer.drag.remote")
    } else {
        None
    }
}

/// 같은 디스크면 이동, 다른 디스크면 복사. `flip` 이면 뒤집는다. `blocked` 는 경로를 보지 않고 거절하는
/// 이유(다른 컴퓨터, 쓸 수 없는 원격)의 번역 키다.
pub(crate) fn verdict(
    paths: &[PathBuf],
    dest: &Path,
    same_volume: bool,
    writable: bool,
    flip: bool,
    blocked: Option<&'static str>,
) -> Verdict {
    if let Some(reason) = blocked {
        return Verdict::No(reason);
    }
    if paths.iter().any(|p| dest.starts_with(p)) {
        return Verdict::No("explorer.drag.into_itself");
    }
    let op = match (same_volume, flip) {
        (true, false) | (false, true) => OpKind::Move,
        _ => OpKind::Copy,
    };
    if op == OpKind::Move && paths.iter().all(|p| p.parent() == Some(dest)) {
        return Verdict::No("explorer.drag.same_folder");
    }
    if !writable {
        return Verdict::No("explorer.drag.no_write");
    }
    Verdict::Go(op)
}

/// 판정에 쓸 경로. 작업 엔진처럼 놓을 폴더는 링크를 풀고, 원본은 항목 자체(링크면 링크)를 가리키도록
/// 부모 폴더만 푼다. 읽지 못한 경로는 그대로 비교한다. 작업 엔진도 같은 경우를 다시 막는다.
fn resolve(paths: &[PathBuf], dest: &Path) -> (Vec<PathBuf>, PathBuf) {
    let real = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let source = |p: &PathBuf| match (p.parent(), p.file_name()) {
        (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => real(parent).join(name),
        _ => p.clone(),
    };
    (
        paths.iter().map(|p| fold(source(p))).collect(),
        fold(real(dest)),
    )
}

#[cfg(windows)]
fn fold(path: PathBuf) -> PathBuf {
    PathBuf::from(fold_windows(&path.to_string_lossy()))
}

#[cfg(not(windows))]
fn fold(path: PathBuf) -> PathBuf {
    path
}

/// Windows 경로를 비교용으로 맞춘다. 파일시스템이 대소문자를 구분하지 않고, `canonicalize` 는 `\\?\`
/// 접두사를 붙이지만 읽지 못한 경로에는 붙지 않는다.
#[cfg(any(windows, test))]
fn fold_windows(text: &str) -> String {
    let plain = if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        rest.to_owned()
    } else {
        text.to_owned()
    };
    plain.to_lowercase()
}

#[cfg(unix)]
fn same_volume(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev(),
        // 읽지 못하면 작업이 이유와 함께 실패한다. 기본 동작은 원본을 남기는 복사로 둔다.
        _ => false,
    }
}

/// 링크·마운트 폴더·UNC 를 따라간 실제 볼륨의 일련번호로 비교한다. 읽지 못하면 복사가 기본이다.
#[cfg(windows)]
fn same_volume(a: &Path, b: &Path) -> bool {
    let serial = |p: &Path| {
        let file = win::open(p, 0).ok()?;
        win::volume(&file).map(|(serial, _)| serial)
    };
    match (serial(a), serial(b)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

#[cfg(not(any(unix, windows)))]
fn same_volume(_a: &Path, _b: &Path) -> bool {
    false
}

#[cfg(unix)]
fn writable(dir: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(dir.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: c 는 NUL 로 끝나는 유효한 경로 문자열이고 호출 동안 살아 있다.
    unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 }
}

/// 폴더에 항목을 만들 권한을 ACL 로 확인하고, 읽기 전용 볼륨이면 거절한다. 폴더의 읽기 전용 속성은
/// 쓰기를 막지 않으므로 보지 않는다.
#[cfg(windows)]
fn writable(dir: &Path) -> bool {
    use windows::Win32::Storage::FileSystem::{FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY};
    use windows::Win32::System::SystemServices::FILE_READ_ONLY_VOLUME;
    match win::open(dir, (FILE_ADD_FILE | FILE_ADD_SUBDIRECTORY).0) {
        Ok(file) => win::volume(&file).is_none_or(|(_, flags)| flags & FILE_READ_ONLY_VOLUME == 0),
        Err(_) => false,
    }
}

#[cfg(not(any(unix, windows)))]
fn writable(dir: &Path) -> bool {
    std::fs::metadata(dir).is_ok()
}

#[cfg(windows)]
mod win {
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use std::path::Path;

    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        GetVolumeInformationByHandleW,
    };

    /// 파일과 폴더를 모두 연다. `access` 가 0 이면 속성만 읽고, 권한이 없으면 `PermissionDenied` 다.
    pub(super) fn open(path: &Path, access: u32) -> std::io::Result<std::fs::File> {
        std::fs::OpenOptions::new()
            .access_mode(access)
            .share_mode((FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE).0)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0)
            .open(path)
    }

    /// 핸들이 가리키는 볼륨의 일련번호와 파일시스템 플래그.
    pub(super) fn volume(file: &std::fs::File) -> Option<(u32, u32)> {
        let (mut serial, mut flags) = (0u32, 0u32);
        // SAFETY: 핸들은 `file` 이 살아 있는 동안 유효하고, 출력 포인터는 이 함수의 지역 변수를 가리킨다.
        let read = unsafe {
            GetVolumeInformationByHandleW(
                HANDLE(file.as_raw_handle()),
                None,
                Some(&mut serial),
                None,
                Some(&mut flags),
                None,
            )
        };
        match read {
            Ok(()) => Some((serial, flags)),
            Err(e) => {
                tracing::debug!(error = %e, "explorer drag: volume information unavailable");
                None
            }
        }
    }
}

fn held(mods: egui::Modifiers, super_held: bool, flip: Option<Combo>) -> bool {
    let Some(flip) = flip else {
        return false;
    };
    let (alt, option) = tasty_key_match::token_axes_egui(&mods, super_held);
    let now = Combo {
        ctrl: mods.ctrl,
        alt,
        option,
        shift: mods.shift,
    };
    now.contains_all(flip)
}

fn start(
    ctx: &egui::Context,
    me: egui::Id,
    view: &mut ExplorerView,
    spots: &[Spot],
    body: egui::Rect,
) {
    let (dragging, origin) = ctx.input(|i| {
        (
            i.pointer.primary_down() && i.pointer.is_decidedly_dragging(),
            i.pointer.press_origin(),
        )
    });
    if !ctx.input(|i| i.pointer.primary_down()) {
        view.ops.drag.started = false;
    }
    if !dragging || view.ops.drag.started || ctx.dragged_id().is_some() {
        return;
    }
    let Some(origin) = origin else {
        return;
    };
    let Some(spot) = spots
        .iter()
        .rev()
        .find(|s| matches!(s.kind, Kind::Entry { .. }) && area(s, body).contains(origin))
    else {
        return;
    };
    let paths = if view.selected.contains(&spot.path) {
        let mut paths: Vec<PathBuf> = view.selected.owned().collect();
        paths.sort();
        paths
    } else {
        view.select_only(&spot.path);
        vec![spot.path.clone()]
    };
    let single_dir = paths.len() == 1 && spot.is_dir;
    let volume = first_parent(&paths).and_then(|dir| view.remote_volume_of(dir));
    view.ops.drag.started = true;
    view.ops.drag.source_widget = Some(me);
    egui::DragAndDrop::set_payload(
        ctx,
        Payload {
            paths,
            single_dir,
            source: me,
            origin: view.mirror_ws_id,
            volume,
        },
    );
}

/// 대상 폴더의 디스크·쓰기 가능 여부와 비교할 경로. 대상이나 원본이 바뀔 때만 다시 읽는다.
fn probe<'a>(state: &'a mut DragState, dest: &Path, paths: &[PathBuf]) -> &'a Probe {
    let stale = state
        .probe
        .as_ref()
        .is_none_or(|p| p.dest != dest || p.paths != paths);
    if stale {
        let first = paths.first().map_or(Path::new(""), PathBuf::as_path);
        state.probe = Some(Probe {
            dest: dest.to_path_buf(),
            paths: paths.to_vec(),
            same_volume: same_volume(first, dest),
            writable: writable(dest),
            resolved: resolve(paths, dest),
        });
    }
    state.probe.as_ref().expect("probe was just filled")
}

/// 이 컴퓨터의 경로를 놓는 판정. 링크를 푼 경로로 비교한다.
fn local_verdict(
    state: &mut DragState,
    paths: &[PathBuf],
    dest: &Path,
    always_copy: bool,
    flip: bool,
) -> Verdict {
    let probed = probe(state, dest, paths);
    let (paths, dest) = &probed.resolved;
    let same_volume = !always_copy && probed.same_volume;
    verdict(paths, dest, same_volume, probed.writable, flip, None)
}

/// 닫힌 트리 폴더 위에 머문 시간을 재고, 다 되면 펼친다.
fn expand_after_hover(
    ctx: &egui::Context,
    theme: &Theme,
    view: &mut ExplorerView,
    over: Option<(&Path, Kind)>,
) {
    let delay = theme.explorer_drag_expand_delay().to_duration();
    let Some((dir, Kind::Tree { open: false })) = over else {
        view.ops.drag.hover = None;
        return;
    };
    let now = Instant::now();
    match &view.ops.drag.hover {
        Some((p, since)) if p == dir => {
            let waited = now.saturating_duration_since(*since);
            if waited >= delay {
                view.expanded.insert(dir.to_path_buf());
                view.ops.drag.hover = None;
                // The tree was drawn before this hover update. Paint its children next frame.
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after(delay - waited);
            }
        }
        _ => {
            view.ops.drag.hover = Some((dir.to_path_buf(), now));
            ctx.request_repaint_after(delay);
        }
    }
}

fn chip(
    ctx: &egui::Context,
    theme: &Theme,
    pos: egui::Pos2,
    paths: &[PathBuf],
    single_dir: bool,
    shown: Option<(&Path, Verdict)>,
) {
    let label = match paths {
        [one] => one
            .file_name()
            .map_or_else(|| one.to_string_lossy(), |n| n.to_string_lossy())
            .into_owned(),
        many => t_fmt("explorer.drag.items", &many.len().to_string()),
    };
    let folder = |dest: &Path| {
        dest.file_name()
            .map_or_else(|| dest.to_string_lossy(), |n| n.to_string_lossy())
            .into_owned()
    };
    let (line, reason, op) = match shown {
        Some((dest, Verdict::Go(OpKind::Copy))) => (
            t_fmt("explorer.drag.copy_to", &folder(dest)),
            None,
            DragOp::Copy,
        ),
        Some((dest, Verdict::Go(_))) => (
            t_fmt("explorer.drag.move_to", &folder(dest)),
            None,
            DragOp::Move,
        ),
        Some((_, Verdict::No(key))) => (
            t("explorer.drag.refused").to_owned(),
            Some(t_fmt("explorer.drag.refused_reason", t(key))),
            DragOp::Refused,
        ),
        None => (t("explorer.drag.refused").to_owned(), None, DragOp::Refused),
    };
    let item = if paths.len() > 1 {
        icons::LAYERS
    } else if single_dir {
        icons::FOLDER
    } else {
        icons::FILE
    };
    let op_icon = match op {
        DragOp::Move => icons::MOVE,
        DragOp::Copy => icons::PLUS,
        DragOp::Refused => icons::CLOSE,
    };
    let offset = theme.spacing_md.value();
    egui::Area::new(egui::Id::new("explorer_drag_chip"))
        .order(egui::Order::Tooltip)
        .fixed_pos(pos + egui::vec2(offset, offset))
        .interactable(false)
        .show(ctx, |ui| {
            drag_chip(
                ui,
                theme,
                &DragChipProps {
                    label: &label,
                    line: &line,
                    reason: reason.as_deref(),
                    op,
                },
                &glyph(item),
                &glyph(op_icon),
            );
        });
}

/// 대상 표시를 칠한다. 틴트는 잡아 둔 자리에, 링은 지금 칠해 행 위에 놓는다.
fn paint_target(
    ui: &egui::Ui,
    theme: &Theme,
    slot: Option<DropTintSlot>,
    ring: egui::Rect,
    radius: f32,
) {
    if let Some(slot) = slot {
        slot.fill(ui.ctx(), theme, ring, radius);
    }
    // 칸 자신의 레이어에 그려 칸 위에 뜬 popup·카드보다 아래에 둔다.
    let painter = ui.ctx().layer_painter(ui.layer_id()).with_clip_rect(ring);
    paint_drop_ring(&painter, theme, ring, radius);
}

/// 놓기 틴트의 자리와 모서리는 대상 종류가 정한다.
fn paint_drop(ui: &egui::Ui, theme: &Theme, slots: TintSlots, ring: egui::Rect, kind: Kind) {
    let (slot, radius) = match kind {
        Kind::Body => (slots.files, 0.0),
        Kind::Entry { .. } => (slots.files, theme.corner_radius.value()),
        Kind::Tree { .. } | Kind::Favorite => (slots.sidebar, theme.corner_radius.value()),
    };
    paint_target(ui, theme, slot, ring, radius);
}

/// 폴더 행 아래에 이름 입력이 열려 있으면 그 폴더에 놓을 대상 표시를 한다. 끌기 대상과 같은 링·틴트다.
fn create_target(
    ui: &egui::Ui,
    theme: &Theme,
    view: &mut ExplorerView,
    spots: &[Spot],
    body: egui::Rect,
    slot: Option<DropTintSlot>,
) {
    if super::super::create::place::target_row(view).is_none() {
        return;
    }
    let Some(dir) = view.create.as_ref().map(|c| c.dir.clone()) else {
        return;
    };
    let entry = |s: &&Spot| matches!(s.kind, Kind::Entry { .. }) && s.path == dir;
    if let Some(spot) = spots.iter().find(entry) {
        view.ops.drag.drop_tint = Some(DropTint::Row(dir.clone()));
        paint_target(
            ui,
            theme,
            slot,
            area(spot, body),
            theme.corner_radius.value(),
        );
    }
}

/// 칸의 드래그 처리. 본문과 상태줄을 그린 뒤 한 번 부른다.
pub(crate) fn frame(
    ui: &egui::Ui,
    theme: &Theme,
    view: &mut ExplorerView,
    body: egui::Rect,
    action: &mut Option<super::super::ExplorerAction>,
) {
    let ctx = ui.ctx().clone();
    // 자세히 보기의 넓은 열이 본문 rect 를 칸 밖으로 넓힐 수 있다. 칸에서 보이는 부분만 대상이다.
    // Row geometry and list drop targets must exclude the adjacent preview panel.
    let body = view
        .list_rect
        .unwrap_or(body)
        .intersect(body)
        .intersect(ui.clip_rect());
    let spots = ctx
        .data_mut(|d| d.remove_temp::<Spots>(spots_id()))
        .unwrap_or_default()
        .0;
    let slots = std::mem::take(&mut view.ops.drag.slots);
    view.ops.drag.drop_tint = None;
    create_target(ui, theme, view, &spots, body, slots.create);
    let me = ui.id();
    let drawn = drawn_entries(&spots, body);
    super::cursor::frame(ui, theme, view, &drawn);
    if view.ops.drag.blocked {
        view.marquee = Default::default();
    } else {
        super::marquee::frame(ui, theme, view, &drawn);
    }
    if view.ops.drag.blocked || view.marquee.active() {
        view.ops.drag.hover = None;
        return;
    }
    let Some(payload) = egui::DragAndDrop::payload::<Payload>(&ctx) else {
        view.ops.drag.hover = None;
        if let Some(os) = view.ops.drag.os.clone() {
            os_frame(ui, theme, view, &spots, body, &os, slots);
        } else {
            start(&ctx, me, view, &spots, body);
        }
        return;
    };
    let Some(pos) = ctx.input(|i| i.pointer.latest_pos()) else {
        return;
    };
    let pass = ctx.cumulative_pass_nr();
    let shown = view.loaded_dir().map(Path::to_path_buf);
    let Some((dest, ring, kind)) = target(&spots, pos, body, shown.as_deref()) else {
        expand_after_hover(&ctx, theme, view, None);
        let last = ctx
            .data(|d| d.get_temp::<Claim>(claim_id()))
            .unwrap_or_default();
        if payload.source == me && last.0 + 1 < pass {
            chip(&ctx, theme, pos, &payload.paths, payload.single_dir, None);
        }
        return;
    };
    ctx.data_mut(|d| d.insert_temp(claim_id(), Claim(pass)));
    expand_after_hover(&ctx, theme, view, Some((&dest, kind)));
    let super_held = crate::adapters::ui::input::shortcuts::super_held(&ctx);
    let flip = held(ctx.input(|i| i.modifiers), super_held, view.ops.drag.flip);
    let blocked = blocked(view, payload.origin);
    // 원격 경로는 이 컴퓨터에서 읽지 않는다. 서버가 알린 디스크 식별값이 같을 때만 이동이 기본이고,
    // 쓰기 거부는 서버가 알린다.
    let decided = if blocked.is_some() || view.is_remote() {
        let here = view.remote_volume_of(&dest);
        let same_volume = payload.volume.is_some() && payload.volume == here;
        verdict(&payload.paths, &dest, same_volume, true, flip, blocked)
    } else {
        local_verdict(&mut view.ops.drag, &payload.paths, &dest, false, flip)
    };
    if let Verdict::Go(_) = decided {
        view.ops.drag.note_drop_tint(kind, &dest);
        paint_drop(ui, theme, slots, ring, kind);
    }
    chip(
        &ctx,
        theme,
        pos,
        &payload.paths,
        payload.single_dir,
        Some((&dest, decided)),
    );
    if let Verdict::Go(kind) = decided
        && ctx.input(|i| i.pointer.primary_released())
        && action.is_none()
    {
        *action = Some(super::super::ExplorerAction::Ops(OpsAction::Drop {
            kind,
            paths: payload.paths.clone(),
            dest,
        }));
        egui::DragAndDrop::clear_payload(&ctx);
    }
}

/// OS 파일 hover. OS 에서 온 파일은 항상 복사해 넣는다(이동하지 않는다).
fn os_frame(
    ui: &egui::Ui,
    theme: &Theme,
    view: &mut ExplorerView,
    spots: &[Spot],
    body: egui::Rect,
    os: &OsHover,
    slots: TintSlots,
) {
    let shown = view.loaded_dir().map(Path::to_path_buf);
    let Some((dest, ring, kind)) = target(spots, os.pos, body, shown.as_deref()) else {
        view.ops.drag.os_target = None;
        return;
    };
    // OS 에서 온 파일은 이 컴퓨터의 경로다.
    let decided = match blocked(view, None) {
        Some(reason) => Verdict::No(reason),
        None => local_verdict(&mut view.ops.drag, &os.paths, &dest, true, false),
    };
    let ctx = ui.ctx();
    if let Verdict::Go(_) = decided {
        view.ops.drag.note_drop_tint(kind, &dest);
        paint_drop(ui, theme, slots, ring, kind);
    }
    let single_dir = matches!(os.paths.as_slice(), [one] if one.is_dir());
    chip(
        ctx,
        theme,
        os.pos,
        &os.paths,
        single_dir,
        Some((&dest, decided)),
    );
    view.ops.drag.os_target = Some((dest, decided));
}

/// OS 파일 hover 가 explorer 칸 위에 있는지. 창 전체의 drop overlay 는 이때 그리지 않는다.
pub(crate) fn os_hover_claimed(views: &super::ExplorerViewStore) -> bool {
    views.cached_surfaces().any(|sid| {
        views
            .get(sid)
            .is_some_and(|v| v.ops.drag.os_target.is_some())
    })
}

/// 놓은 OS 파일이 갈 explorer 칸과 폴더. 모든 칸의 기록을 비운다.
pub(crate) fn take_os_drop(
    views: &mut super::ExplorerViewStore,
) -> Option<(tasty_model::SurfaceId, PathBuf, Verdict)> {
    let mut found = None;
    for sid in views.cached_surfaces().collect::<Vec<_>>() {
        if let Some(view) = views.get_mut(sid)
            && let Some((dest, decided)) = view.ops.drag.os_target.take()
        {
            found = Some((sid, dest, decided));
        }
    }
    found
}

/// 놓은 좌표 아래가 explorer 칸이면 그 칸이 보고 있는 폴더와 판정.
pub(crate) fn shown_drop(
    views: &super::ExplorerViewStore,
    sid: tasty_model::SurfaceId,
    paths: &[PathBuf],
) -> Option<(PathBuf, Verdict)> {
    let view = views.get(sid)?;
    let dest = view.loaded_dir()?.to_path_buf();
    let decided = match blocked(view, None) {
        Some(reason) => Verdict::No(reason),
        None => {
            let (resolved, into) = resolve(paths, &dest);
            verdict(&resolved, &into, false, writable(&dest), false, None)
        }
    };
    Some((dest, decided))
}

#[cfg(test)]
#[path = "drag_tests.rs"]
mod tests;

/// Snapshot only the local source that owns the current internal gesture.
pub(crate) fn outbound_payload(
    ctx: &egui::Context,
    views: &super::ExplorerViewStore,
) -> Option<(u32, Vec<PathBuf>)> {
    let payload = egui::DragAndDrop::payload::<Payload>(ctx)?;
    if payload.origin.is_some() {
        return None;
    }
    views.cached_surfaces().find_map(|sid| {
        let view = views.get(sid)?;
        (view.ops.drag.source_widget == Some(payload.source) && !view.ops.drag.blocked)
            .then(|| (sid, payload.paths.clone()))
    })
}

#[cfg(test)]
#[path = "drag_scroll_tests.rs"]
mod scroll_tests;

#[cfg(test)]
#[path = "drag_flow_tests.rs"]
mod flow_tests;

#[cfg(test)]
#[path = "drag_preview_tests.rs"]
mod preview_tests;
