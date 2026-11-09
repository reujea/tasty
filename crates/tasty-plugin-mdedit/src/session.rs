//! 편집기 surface 하나의 상태 — 기준점(base)·편집 버퍼·외부 변경 검토·conflict.
//!
//! - **base** 는 마지막으로 맞춘 디스크 내용이다(E3). 버퍼가 base 와 같으면 깨끗하다.
//! - 디스크가 바뀌면([`Session::on_disk_change`]) 깨끗한 버퍼는 다시 읽고, 더러운 버퍼는
//!   3-way 병합한다(E4). 겹치면 conflict 로 멈춘다. 어느 경우든 직전 내용을 스냅샷으로 남기고
//!   (E5) 검토 목록에 올린다(A3).
//! - 저장([`Session::save`])은 가드(A4)·비밀값(A5)을 지나야 하고, 직전 디스크 내용을 스냅샷으로
//!   남긴 뒤 원자적으로 쓴다(E2).
//!
//! 호스트·웹뷰를 모른다 — 무엇을 다시 그려야 하는지만 [`Changes`] 로 알린다.

use std::path::{Path, PathBuf};

use crate::core::blocks;
use crate::core::guard::{self, DocConfig};
use crate::core::leak::{self, SecretsMode};
use crate::core::merge::{Choice, Merge, merge3};
use crate::core::review::{self, AgentLines, Origin};
use crate::core::snapshot::{Reason, Retention, SnapshotStore};
use crate::core::textio::{self, Decoded, Eol};
use crate::core::units::{self, Entry, Format};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Preview,
    Source,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    None,
    Review,
    Snapshots,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
    Error,
}

/// 편집기 위쪽 알림 한 줄. 문구는 i18n 키 + 인자 하나.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Banner {
    pub level: Level,
    pub key: &'static str,
    pub arg: String,
}

impl Banner {
    fn new(level: Level, key: &'static str, arg: impl Into<String>) -> Self {
        Self {
            level,
            key,
            arg: arg.into(),
        }
    }
}

pub struct ConflictState {
    pub merge: Merge,
    theirs_raw: String,
    theirs: Decoded,
    pub choices: Vec<Option<Choice>>,
}

pub struct ReviewState {
    pub base: String,
    pub target: String,
}

/// 한 동작이 바꾼 것 — 호출자가 무엇을 다시 그릴지 정한다.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Changes {
    /// 편집 버퍼가 사용자 입력이 아닌 이유로 바뀌었다(원문 편집기에 밀어 넣어야 한다).
    pub buffer: bool,
    /// 상태줄·배너·패널이 바뀌었다.
    pub chrome: bool,
    /// 문서를 통째로 다시 실어야 한다(모드 전환 등).
    pub reload: bool,
}

impl Changes {
    fn chrome() -> Self {
        Self {
            chrome: true,
            ..Self::default()
        }
    }
    fn buffer() -> Self {
        Self {
            buffer: true,
            chrome: true,
            reload: false,
        }
    }
    pub fn reload() -> Self {
        Self {
            reload: true,
            ..Self::default()
        }
    }
}

/// 스냅샷 저장소와 보존 정책.
pub struct Ctx<'a> {
    pub store: &'a SnapshotStore,
    pub keep: Retention,
}

pub struct Session {
    pub path: PathBuf,
    pub mode: Mode,
    pub panel: Panel,
    /// 마지막으로 맞춘 디스크 원문. 파일이 없거나 못 읽으면 `None`.
    disk_raw: Option<String>,
    pub base: String,
    eol: Eol,
    bom: bool,
    pub buffer: String,
    pub load_error: Option<String>,
    pub conflict: Option<ConflictState>,
    pub review: Option<ReviewState>,
    pub agent: AgentLines,
    pub banner: Option<Banner>,
    /// 다음에 받을 웹뷰 메시지 번호.
    pub expect_seq: u64,
    pub chunks: crate::bridge::Chunks,
    /// 미리보기를 다시 그린 뒤 열 편집기 자리 — 한 번 쓰고 비운다.
    pub focus: Option<Focus>,
    /// 미리보기에서 열린 블록에 아직 적용하지 않은 고침이 있다(웹뷰가 알린다). 버퍼는 적용할 때
    /// 바뀌므로 그 사이의 고침은 이것으로만 보인다 — 저장 단추·`mdedit status` 의 dirty 가 본다.
    pub pending: bool,
}

/// 미리보기 편집 뒤 이어서 열 자리(docs/surfaces/mdedit/index.md#미리보기-편집-단위는-블록-원문-조각).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Focus {
    /// 그 줄에서 이 바이트 이후에 시작하는 첫 편집 단위를 연다.
    pub at: usize,
    /// 그 블록이 아니라 그 블록 뒤의 새 줄 입력칸을 연다.
    pub after: bool,
    /// 커서를 끝이 아니라 머리에 둔다(나눈 블록의 뒤쪽을 이어 쓸 때).
    pub start: bool,
}

/// 손잡이로 단위에 하는 일(docs/surfaces/mdedit/index.md#블록-손잡이는-원문-줄을-통째로-옮긴다).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnitOp {
    Up,
    Down,
    Move(Option<Entry>),
    Duplicate,
    Delete,
    Format(Format),
    /// 항목 뒤에 같은 수준의 빈 항목.
    NewItem,
    /// 최상위 단위 뒤에 새 블록. `chain` 이면 그 뒤에 새 줄 입력칸을 다시 연다.
    Insert {
        text: String,
        chain: bool,
    },
}

/// 미리보기 블록 편집이 끝난 뒤 이어서 할 일.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Then {
    Nothing,
    /// 같은 수준의 새 항목을 만들고 연다.
    NewItem,
    /// 그 블록 뒤에 새 줄 입력칸을 연다.
    NewLine,
}

impl Session {
    pub fn open(path: PathBuf) -> Self {
        let mut s = Self {
            path,
            mode: Mode::Preview,
            panel: Panel::None,
            disk_raw: None,
            base: String::new(),
            eol: Eol::Lf,
            bom: false,
            buffer: String::new(),
            load_error: None,
            conflict: None,
            review: None,
            agent: AgentLines::default(),
            banner: None,
            expect_seq: 1,
            chunks: Default::default(),
            focus: None,
            pending: false,
        };
        s.read_fresh();
        s
    }

    fn read_fresh(&mut self) {
        match std::fs::read_to_string(&self.path) {
            Ok(raw) => {
                let d = textio::decode(&raw);
                if d.eol == Eol::Mixed {
                    self.banner = Some(Banner::new(Level::Warn, "mdedit.banner.mixed_eol", ""));
                }
                self.adopt(raw, d);
                self.buffer = self.base.clone();
                self.load_error = None;
            }
            Err(e) => {
                self.load_error = Some(e.to_string());
                self.disk_raw = None;
            }
        }
    }

    fn adopt(&mut self, raw: String, d: Decoded) {
        self.base = d.text;
        self.eol = d.eol;
        self.bom = d.bom;
        self.disk_raw = Some(raw);
    }

    /// 저장할 것이 있는가 — 버퍼가 base 와 다르거나, 미리보기에 적용 전 고침이 있다. 저장 단추·
    /// 상태줄·`mdedit status`·닫기 전 확인이 모두 이것을 본다(docs/surfaces/mdedit/index.md#적용-전-고침도-dirty-로-센다).
    pub fn dirty(&self) -> bool {
        self.edited() || self.pending
    }

    /// 버퍼가 base 와 다른가 — 병합·저장의 판단은 버퍼만 본다(적용 전 고침은 아직 원문이 아니다).
    pub fn edited(&self) -> bool {
        self.buffer != self.base
    }

    /// 웹뷰가 알린 적용 전 고침 여부. dirty 가 바뀌면 상태줄을 다시 그린다.
    pub fn set_pending(&mut self, pending: bool) -> Changes {
        let was = self.dirty();
        self.pending = pending;
        if was != self.dirty() {
            Changes::chrome()
        } else {
            Changes::default()
        }
    }

    pub fn disk_hash(&self) -> Option<String> {
        self.disk_raw.as_deref().map(textio::content_hash)
    }

    pub fn review_hunks(&self) -> Vec<review::ReviewHunk> {
        self.review
            .as_ref()
            .map(|r| review::hunks(&r.base, &r.target, &self.agent))
            .unwrap_or_default()
    }

    /// 사용자가 원문 편집기에서 고친 내용(웹뷰 → plugin)을 버퍼에 반영한다.
    pub fn user_edit(&mut self, next: String) -> Changes {
        let was_dirty = self.dirty();
        self.buffer = next;
        if was_dirty != self.dirty() {
            Changes::chrome()
        } else {
            Changes::default()
        }
    }

    /// 미리보기에서 편집을 받을 수 있는가 — 원문 편집기·conflict·읽기 실패 중에는 아니다.
    fn preview_editable(&self) -> bool {
        self.mode == Mode::Preview && self.conflict.is_none() && self.load_error.is_none()
    }

    /// 미리보기 편집 결과를 버퍼에 넣는다. 저장은 하지 않는다 — 원문 편집기와 같이 버퍼만
    /// 바뀌고, 디스크 변경은 기존 병합 경로를 탄다.
    fn preview_apply(&mut self, r: Result<(String, Option<Focus>), blocks::EditError>) -> Changes {
        // 열린 블록의 고침이 적용됐다(또는 거부돼 다시 그린다) — 적용 전 고침은 남지 않는다.
        self.pending = false;
        match r {
            Ok((next, focus)) => {
                self.buffer = next;
                self.focus = focus;
            }
            Err(e) => {
                self.banner = Some(Banner::new(
                    Level::Warn,
                    "mdedit.banner.inline_rejected",
                    e.to_string(),
                ));
            }
        }
        Changes::reload()
    }

    /// 미리보기 블록 `id` 의 원문 `old` 를 `new` 로(docs/surfaces/mdedit/index.md#미리보기-편집-단위는-블록-원문-조각).
    pub fn preview_replace(&mut self, id: usize, old: &str, new: &str, then: Then) -> Changes {
        if !self.preview_editable() {
            return Changes::reload();
        }
        let src = self.buffer.clone();
        let r = blocks::replace(&src, id, old, new).and_then(|next| {
            let start = blocks::blocks(&src)[id].range.start;
            match then {
                Then::Nothing => Ok((next, None)),
                Then::NewLine => Ok((
                    next,
                    Some(Focus {
                        at: start,
                        after: true,
                        start: false,
                    }),
                )),
                Then::NewItem => {
                    // 바꾼 뒤의 원문에서 같은 번호가 같은 항목이다(블록 수가 안 바뀌는 한).
                    let after = blocks::blocks(&next);
                    let same = after
                        .get(id)
                        .is_some_and(|b| b.kind == blocks::Kind::Item && b.range.start == start);
                    if !same {
                        return Ok((next, None));
                    }
                    let cur = next[after[id].range.clone()].to_string();
                    blocks::new_item(&next, id, &cur).map(|(n, at)| {
                        (
                            n,
                            Some(Focus {
                                at,
                                after: false,
                                start: false,
                            }),
                        )
                    })
                }
            }
        });
        self.preview_apply(r)
    }

    /// 블록 `id` 를 커서 자리에서 나눈다(docs/surfaces/mdedit/index.md#enter-는-커서-자리에서-나눈다). 그 블록을 앞쪽 `before` 로 바꾸고, 뒤쪽
    /// `rest` 는 항목이면 같은 수준 새 항목에, 문단·제목이면 그 뒤 새 문단에 넣어 머리에서 이어
    /// 연다. 문단·제목의 뒤쪽이 비면 새 줄 입력칸을 연다. 한 단계라도 거부되면 버퍼는 그대로다.
    pub fn preview_split(&mut self, id: usize, old: &str, before: &str, rest: &str) -> Changes {
        if !self.preview_editable() {
            return Changes::reload();
        }
        let src = self.buffer.clone();
        let rest = rest.trim_start();
        let r = blocks::replace(&src, id, old, before).and_then(|next| {
            let all = blocks::blocks(&src);
            let (kind, start) = (all[id].kind, all[id].range.start);
            if kind != blocks::Kind::Item {
                if rest.trim().is_empty() {
                    return Ok((
                        next,
                        Some(Focus {
                            at: start,
                            after: true,
                            start: false,
                        }),
                    ));
                }
                let (n, at) = blocks::insert_after(&next, Some(id), rest.trim_end())?;
                return Ok((
                    n,
                    Some(Focus {
                        at,
                        after: false,
                        start: true,
                    }),
                ));
            }
            // 바꾼 뒤의 원문에서 같은 번호가 같은 항목이다(블록 수가 안 바뀌는 한).
            let after = blocks::blocks(&next);
            if !after
                .get(id)
                .is_some_and(|b| b.kind == blocks::Kind::Item && b.range.start == start)
            {
                return Err(blocks::EditError::Stale);
            }
            let cur = next[after[id].range.clone()].to_string();
            let (mut n, at) = blocks::new_item(&next, id, &cur)?;
            n.insert_str(at, rest);
            Ok((
                n,
                Some(Focus {
                    at,
                    after: false,
                    start: true,
                }),
            ))
        });
        self.preview_apply(r)
    }

    /// 블록 `after` 뒤(없으면 문서 끝)에 새 블록 `text`. `chain` 이면 그 뒤에 새 줄 입력칸을
    /// 다시 연다.
    pub fn preview_insert(&mut self, after: Option<usize>, text: &str, chain: bool) -> Changes {
        if !self.preview_editable() {
            return Changes::reload();
        }
        let r = blocks::insert_after(&self.buffer, after, text).map(|(n, at)| {
            (
                n,
                chain.then_some(Focus {
                    at,
                    after: true,
                    start: false,
                }),
            )
        });
        self.preview_apply(r)
    }

    /// 셀 `id` 의 행 위·아래에 빈 행을 넣고 그 첫 셀을 연다.
    pub fn preview_row(&mut self, id: usize, below: bool) -> Changes {
        if !self.preview_editable() {
            return Changes::reload();
        }
        let r = blocks::add_row(&self.buffer, id, below).map(|(n, at)| {
            (
                n,
                Some(Focus {
                    at,
                    after: false,
                    start: false,
                }),
            )
        });
        self.preview_apply(r)
    }

    /// 빈 항목 `id` 에서 Enter — 목록을 빠져나간다(docs/surfaces/mdedit/index.md#블록-손잡이는-원문-줄을-통째로-옮긴다). 하위 항목은 한 단계 내어써 그 자리에서
    /// 이어 쓰고, 최상위 항목은 그 줄을 지우고 앞 항목 뒤에 새 줄 입력칸을 연다.
    pub fn preview_exit_item(&mut self, id: usize) -> Changes {
        if !self.preview_editable() {
            return Changes::reload();
        }
        let r = blocks::exit_item(&self.buffer, id).map(|(n, e)| match e {
            blocks::Exit::Outdent(at) => (
                n,
                Some(Focus {
                    at,
                    after: false,
                    start: true,
                }),
            ),
            blocks::Exit::Removed(prev) => (
                n,
                prev.map(|at| Focus {
                    at,
                    after: true,
                    start: false,
                }),
            ),
        });
        self.preview_apply(r)
    }

    /// 손잡이 연산(docs/surfaces/mdedit/index.md#블록-손잡이는-원문-줄을-통째로-옮긴다) — 단위 `u` 의 원문이 화면을 그릴 때의 해시 `hash` 와 같을 때만.
    pub fn preview_unit(&mut self, u: usize, hash: &str, op: UnitOp) -> Changes {
        if !self.preview_editable() {
            return Changes::reload();
        }
        let src = self.buffer.clone();
        let old = units::units(&src)
            .units
            .get(u)
            .map(|x| src[x.range.clone()].to_string())
            .filter(|t| textio::content_hash(t) == hash);
        let r = match old {
            None => Err(blocks::EditError::Stale),
            Some(old) => {
                let at = |at: usize, after: bool, start: bool| Some(Focus { at, after, start });
                match op {
                    UnitOp::Up | UnitOp::Down => {
                        units::shift(&src, u, &old, op == UnitOp::Up).map(|(n, _)| (n, None))
                    }
                    UnitOp::Move(before) => {
                        units::move_to(&src, u, &old, before).map(|(n, _)| (n, None))
                    }
                    UnitOp::Duplicate => units::duplicate(&src, u, &old).map(|(n, _)| (n, None)),
                    UnitOp::Delete => units::delete(&src, u, &old).map(|n| (n, None)),
                    UnitOp::Format(f) => units::convert(&src, u, &old, f).map(|n| (n, None)),
                    UnitOp::NewItem => {
                        units::new_item_after(&src, u, &old).map(|(n, a)| (n, at(a, false, true)))
                    }
                    UnitOp::Insert { ref text, chain } => {
                        units::insert_after_unit(&src, u, &old, text)
                            .map(|(n, a)| (n, if chain { at(a, true, false) } else { None }))
                    }
                }
            }
        };
        self.preview_apply(r)
    }

    pub fn set_mode(&mut self, mode: Mode) -> Changes {
        if self.mode == mode {
            return Changes::default();
        }
        self.mode = mode;
        self.pending = false;
        Changes::reload()
    }

    pub fn toggle_panel(&mut self, panel: Panel) -> Changes {
        self.panel = if self.panel == panel {
            Panel::None
        } else {
            panel
        };
        Changes::chrome()
    }

    pub fn dismiss_banner(&mut self) -> Changes {
        self.banner = None;
        Changes::chrome()
    }

    /// 디스크가 바뀌었을 수 있다 — 감시 스레드·`mdedit` 쓰기 연산·저장 직전 확인이 부른다.
    pub fn on_disk_change(&mut self, origin: Origin, ctx: &Ctx) -> Changes {
        let raw_now = match std::fs::read_to_string(&self.path) {
            Ok(r) => r,
            Err(e) => {
                if self.disk_raw.is_some() {
                    self.banner = Some(Banner::new(
                        Level::Warn,
                        "mdedit.banner.read_failed",
                        e.to_string(),
                    ));
                    return Changes::chrome();
                }
                return Changes::default();
            }
        };
        if self.disk_raw.as_deref() == Some(raw_now.as_str()) {
            return Changes::default();
        }
        let Some(old_raw) = self.disk_raw.clone() else {
            // 처음 생긴 파일 — 비교할 base 가 없다.
            let was_error = self.load_error.is_some();
            self.read_fresh();
            return if was_error {
                Changes::reload()
            } else {
                Changes::buffer()
            };
        };
        let reason = match origin {
            Origin::Agent => Reason::Agent,
            Origin::External => Reason::External,
        };
        if let Err(e) = ctx.store.save(&self.path, &old_raw, reason, ctx.keep) {
            tracing::warn!("mdedit: snapshot before external change failed: {e}");
        }
        let theirs = textio::decode(&raw_now);
        self.warn_about_external(&theirs.text, origin);
        match self.review.as_mut() {
            Some(r) => r.target = theirs.text.clone(),
            None => {
                self.review = Some(ReviewState {
                    base: self.base.clone(),
                    target: theirs.text.clone(),
                })
            }
        }
        self.drop_empty_review();

        if self.conflict.is_none() && !self.edited() {
            self.buffer = theirs.text.clone();
            self.adopt(raw_now, theirs);
            return Changes::buffer();
        }
        let merge = merge3(&self.base, &self.buffer, &theirs.text);
        if let Some(text) = merge.clean_text() {
            self.buffer = text;
            self.adopt(raw_now, theirs);
            self.conflict = None;
            if self.banner.as_ref().is_none_or(|b| b.level != Level::Warn) {
                self.banner = Some(Banner::new(Level::Info, "mdedit.banner.merged", ""));
            }
            return Changes::buffer();
        }
        let n = merge.conflict_count();
        self.conflict = Some(ConflictState {
            merge,
            theirs_raw: raw_now,
            theirs,
            choices: vec![None; n],
        });
        self.banner = Some(Banner::new(
            Level::Error,
            "mdedit.banner.conflict",
            n.to_string(),
        ));
        Changes::reload()
    }

    /// 직접 쓰기(파일 도구)로 들어온 변경이 가드·비밀값 규칙을 어겼으면 경고만 한다 — 이미
    /// 디스크에 있으므로 막을 수 없다(docs/surfaces/mdedit/index.md#방어선은-쓰기-이후에-둔다). 되돌리기는 검토 목록에서 한다.
    fn warn_about_external(&mut self, theirs: &str, origin: Origin) {
        if origin == Origin::Agent {
            return;
        }
        let cfg = DocConfig::load(&self.path);
        if let Some(v) = guard::check(&self.base, theirs, &cfg, None).first() {
            self.banner = Some(Banner::new(
                Level::Warn,
                "mdedit.banner.external_guard",
                v.heading.clone(),
            ));
        } else if let Some(f) = leak::scan_added(&self.base, theirs, &cfg.secrets).first() {
            self.banner = Some(Banner::new(
                Level::Warn,
                "mdedit.banner.external_secret",
                f.line.to_string(),
            ));
        }
    }

    fn drop_empty_review(&mut self) {
        if self.review.as_ref().is_some_and(|r| r.base == r.target) {
            self.review = None;
            if self.panel == Panel::Review {
                self.panel = Panel::None;
            }
        }
    }

    /// 편집 버퍼를 저장한다. `restore_from` 은 검토 되돌리기·스냅샷 복원처럼 "가드 섹션을 그
    /// 텍스트로 되돌리는" 저장에 준다(가드가 그 복원을 막지 않게).
    pub fn save(&mut self, ctx: &Ctx, restore_from: Option<&str>) -> Changes {
        if self.conflict.is_some() {
            self.banner = Some(Banner::new(Level::Error, "mdedit.banner.resolve_first", ""));
            return Changes::chrome();
        }
        // 마지막으로 맞춘 뒤 디스크가 바뀌었으면 덮어쓰지 않는다 — 먼저 병합하고 멈춘다.
        let disk_now = std::fs::read_to_string(&self.path).ok();
        if disk_now != self.disk_raw && disk_now.is_some() {
            let mut c = self.on_disk_change(Origin::External, ctx);
            if self.conflict.is_none() {
                self.banner = Some(Banner::new(Level::Warn, "mdedit.banner.disk_moved", ""));
            }
            c.chrome = true;
            return c;
        }
        if !self.edited() && self.disk_raw.is_some() {
            return Changes::default();
        }
        let cfg = DocConfig::load(&self.path);
        if let Some(v) = guard::check(&self.base, &self.buffer, &cfg, restore_from).first() {
            self.banner = Some(Banner::new(
                Level::Error,
                "mdedit.banner.guard_blocked",
                format!("{} ({})", v.heading, v.line),
            ));
            return Changes::chrome();
        }
        let leaks = leak::scan_added(&self.base, &self.buffer, &cfg.secrets);
        if let Some(f) = leaks.first()
            && cfg.secrets.mode == SecretsMode::Block
        {
            self.banner = Some(Banner::new(
                Level::Error,
                "mdedit.banner.secret_blocked",
                format!("{} ({})", f.line, f.kind),
            ));
            return Changes::chrome();
        }
        if let Some(old) = &self.disk_raw
            && let Err(e) = ctx.store.save(&self.path, old, Reason::Save, ctx.keep)
        {
            tracing::warn!("mdedit: snapshot before save failed: {e}");
        }
        let eol = if self.eol == Eol::Mixed {
            Eol::Lf
        } else {
            self.eol
        };
        let raw = textio::encode(&self.buffer, eol, self.bom);
        if let Err(e) = textio::save_atomic(&self.path, raw.as_bytes()) {
            self.banner = Some(Banner::new(
                Level::Error,
                "mdedit.banner.save_failed",
                e.to_string(),
            ));
            return Changes::chrome();
        }
        self.rebase_review_after_save();
        let saved = self.buffer.clone();
        self.adopt(
            raw,
            Decoded {
                text: saved,
                eol,
                bom: self.bom,
            },
        );
        self.load_error = None;
        self.banner = leaks.first().map(|f| {
            Banner::new(
                Level::Warn,
                "mdedit.banner.secret_warned",
                format!("{} ({})", f.line, f.kind),
            )
        });
        Changes::chrome()
    }

    /// 저장한 내용에 든 사용자 편집을 검토 base 에도 옮긴다 — 그래야 검토 diff 에 외부 변경만
    /// 남는다. 사용자가 외부 hunk 안을 고쳤으면 그 hunk 는 검토가 끝난 것으로 본다.
    fn rebase_review_after_save(&mut self) {
        let Some(r) = self.review.as_mut() else {
            return;
        };
        let m = merge3(&r.target, &self.buffer, &r.base);
        let choices = vec![Choice::Ours; m.conflict_count()];
        r.base = m.resolve(&choices);
        r.target = self.buffer.clone();
        self.drop_empty_review();
    }

    pub fn choose(&mut self, index: usize, choice: Choice) -> Changes {
        if let Some(c) = self.conflict.as_mut()
            && let Some(slot) = c.choices.get_mut(index)
        {
            *slot = Some(choice);
            return Changes::chrome();
        }
        Changes::default()
    }

    /// 모든 conflict 에 선택이 있으면 병합 결과를 버퍼로 삼는다(저장은 사용자가 한다).
    pub fn apply_resolution(&mut self) -> Changes {
        let Some(c) = self.conflict.as_ref() else {
            return Changes::default();
        };
        let Some(choices) = c.choices.iter().copied().collect::<Option<Vec<_>>>() else {
            self.banner = Some(Banner::new(Level::Error, "mdedit.banner.choose_all", ""));
            return Changes::chrome();
        };
        let Some(c) = self.conflict.take() else {
            return Changes::default();
        };
        self.buffer = c.merge.resolve(&choices);
        self.adopt(c.theirs_raw, c.theirs);
        self.banner = Some(Banner::new(Level::Info, "mdedit.banner.resolved", ""));
        Changes::reload()
    }

    /// 미리보기의 `index` 번째 체크박스를 버퍼에서 토글한다(E6). 다른 미리보기 편집과 같이
    /// 저장하지 않는다 — 파일에는 사용자가 저장할 때 쓴다(에이전트의 `mdedit task` 는 IPC 쪽
    /// 경로라 바로 쓴다). `count` 는 미리보기가 그린 체크박스 수 — 플러그인이 센 수와 다르면
    /// 어느 줄인지 확정할 수 없어 거부한다.
    pub fn toggle_task(&mut self, index: usize, count: usize) -> Changes {
        if !self.preview_editable() {
            return Changes::reload();
        }
        let offs = crate::preview::task_offsets(&self.buffer);
        if offs.len() != count || index >= offs.len() {
            self.banner = Some(Banner::new(
                Level::Error,
                "mdedit.banner.task_mismatch",
                format!("{count}/{}", offs.len()),
            ));
            return Changes::reload();
        }
        let at = offs[index];
        let ch = if &self.buffer[at..at + 1] == " " {
            "x"
        } else {
            " "
        };
        self.buffer.replace_range(at..at + 1, ch);
        Changes::reload()
    }

    pub fn review_keep(&mut self, index: usize) -> Changes {
        let Some(r) = self.review.as_mut() else {
            return Changes::default();
        };
        if let Some(b) = review::keep(&r.base, &r.target, index) {
            r.base = b;
        }
        self.drop_empty_review();
        Changes::chrome()
    }

    pub fn review_keep_all(&mut self) -> Changes {
        self.review = None;
        if self.panel == Panel::Review {
            self.panel = Panel::None;
        }
        Changes::chrome()
    }

    /// 외부 hunk 하나를 되돌리고 저장한다. 편집 중인 내용과 겹치면 거부한다.
    pub fn review_revert(&mut self, index: usize, ctx: &Ctx) -> Changes {
        let Some(r) = self.review.as_ref() else {
            return Changes::default();
        };
        let Some(new_target) = review::revert(&r.base, &r.target, index) else {
            return Changes::default();
        };
        let m = merge3(&r.target, &self.buffer, &new_target);
        let Some(next) = m.clean_text() else {
            self.banner = Some(Banner::new(
                Level::Error,
                "mdedit.banner.revert_overlaps",
                "",
            ));
            return Changes::chrome();
        };
        let restore = r.base.clone();
        self.buffer = next;
        if let Some(r) = self.review.as_mut() {
            r.target = new_target;
        }
        let mut c = self.save(ctx, Some(&restore));
        c.buffer = true;
        c
    }

    /// 스냅샷 하나를 버퍼로 불러와 저장한다(직전 내용은 저장 경로가 다시 스냅샷으로 남긴다).
    pub fn restore_snapshot(&mut self, id: &str, ctx: &Ctx) -> Changes {
        match ctx.store.read(&self.path, id) {
            Ok(raw) => {
                let d = textio::decode(&raw);
                self.buffer = d.text.clone();
                let mut c = self.save(ctx, Some(&d.text));
                c.buffer = true;
                c.reload |= self.mode == Mode::Preview;
                c
            }
            Err(e) => {
                self.banner = Some(Banner::new(
                    Level::Error,
                    "mdedit.banner.snapshot_failed",
                    e.to_string(),
                ));
                Changes::chrome()
            }
        }
    }

    pub fn file_name(&self) -> String {
        file_name(&self.path)
    }
}

pub fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fx {
        _dir: tempfile::TempDir,
        path: PathBuf,
        store: SnapshotStore,
    }

    fn fx(content: &str) -> Fx {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("일지.md");
        std::fs::write(&path, content).unwrap();
        let store = SnapshotStore::new(dir.path().join("snaps"));
        Fx {
            _dir: dir,
            path,
            store,
        }
    }

    fn ctx(f: &Fx) -> Ctx<'_> {
        Ctx {
            store: &f.store,
            keep: Retention::default(),
        }
    }

    const DOC: &str = "# 할 일\n- [ ] a\n\n# 2026.10.22\n- x\n\n# 2026.10.21\n- y\n";

    #[test]
    fn one_line_edit_saves_byte_exact() {
        let f = fx(DOC);
        let mut s = Session::open(f.path.clone());
        s.user_edit(DOC.replace("- x", "- x2"));
        s.save(&ctx(&f), None);
        assert_eq!(
            std::fs::read_to_string(&f.path).unwrap(),
            DOC.replace("- x", "- x2")
        );
        assert!(!s.dirty());
        assert_eq!(f.store.list(&f.path).unwrap().len(), 1, "pre-save snapshot");
    }

    #[test]
    fn clean_buffer_reloads_dirty_buffer_merges() {
        let f = fx(DOC);
        let mut s = Session::open(f.path.clone());
        std::fs::write(&f.path, DOC.replace("- y", "- y\n> [!agent] t")).unwrap();
        let c = s.on_disk_change(Origin::External, &ctx(&f));
        assert!(c.buffer);
        assert_eq!(s.buffer, DOC.replace("- y", "- y\n> [!agent] t"));
        assert_eq!(s.review_hunks().len(), 1);
        assert_eq!(s.review_hunks()[0].origin, Origin::Agent);

        s.user_edit(s.buffer.replace("- x", "- mine"));
        std::fs::write(&f.path, s.base.replace("- [ ] a", "- [x] a")).unwrap();
        s.on_disk_change(Origin::External, &ctx(&f));
        assert!(s.conflict.is_none());
        assert!(s.buffer.contains("- mine") && s.buffer.contains("- [x] a"));
        assert!(s.dirty());
    }

    #[test]
    fn same_line_conflict_then_resolve() {
        let f = fx(DOC);
        let mut s = Session::open(f.path.clone());
        s.user_edit(DOC.replace("- x", "- mine"));
        std::fs::write(&f.path, DOC.replace("- x", "- disk")).unwrap();
        let c = s.on_disk_change(Origin::External, &ctx(&f));
        assert!(c.reload);
        assert_eq!(s.conflict.as_ref().unwrap().merge.conflict_count(), 1);
        // 해결 전 저장은 거부.
        s.save(&ctx(&f), None);
        assert_eq!(
            std::fs::read_to_string(&f.path).unwrap(),
            DOC.replace("- x", "- disk")
        );
        s.choose(0, Choice::Both);
        s.apply_resolution();
        s.save(&ctx(&f), None);
        assert_eq!(
            std::fs::read_to_string(&f.path).unwrap(),
            DOC.replace("- x", "- mine\n- disk")
        );
    }

    #[test]
    fn save_refuses_to_overwrite_unseen_disk_change() {
        let f = fx(DOC);
        let mut s = Session::open(f.path.clone());
        s.user_edit(DOC.replace("- y", "- mine"));
        std::fs::write(&f.path, DOC.replace("- x", "- agent")).unwrap();
        s.save(&ctx(&f), None);
        // 저장하지 않고 병합만 했다.
        assert_eq!(
            std::fs::read_to_string(&f.path).unwrap(),
            DOC.replace("- x", "- agent")
        );
        assert!(s.buffer.contains("- agent") && s.buffer.contains("- mine"));
        s.save(&ctx(&f), None);
        let disk = std::fs::read_to_string(&f.path).unwrap();
        assert!(disk.contains("- agent") && disk.contains("- mine"));
    }

    #[test]
    fn snapshot_restores_pre_agent_content() {
        let f = fx(DOC);
        let mut s = Session::open(f.path.clone());
        std::fs::write(&f.path, DOC.replace("- x", "- agent wrote")).unwrap();
        s.on_disk_change(Origin::Agent, &ctx(&f));
        let snap = f.store.list(&f.path).unwrap()[0].clone();
        assert_eq!(snap.reason, "agent");
        s.restore_snapshot(&snap.id, &ctx(&f));
        assert_eq!(std::fs::read_to_string(&f.path).unwrap(), DOC);
    }

    #[test]
    fn review_revert_undoes_one_external_hunk_and_rebases() {
        let f = fx(DOC);
        let mut s = Session::open(f.path.clone());
        let ext = DOC.replace("- x", "- ext1").replace("- y", "- ext2");
        std::fs::write(&f.path, &ext).unwrap();
        s.on_disk_change(Origin::External, &ctx(&f));
        assert_eq!(s.review_hunks().len(), 2);
        s.review_revert(0, &ctx(&f));
        assert_eq!(
            std::fs::read_to_string(&f.path).unwrap(),
            DOC.replace("- y", "- ext2")
        );
        assert_eq!(s.review_hunks().len(), 1);
        s.review_keep(0);
        assert!(s.review.is_none());
    }

    #[test]
    fn guard_and_secret_block_human_save() {
        let f = fx(DOC);
        std::fs::write(
            f.path.with_file_name(".일지.md.tasty.toml"),
            "[[guard]]\nheading = '^# 2026\\.10\\.21$'\nallow = [\"checkbox\"]\n",
        )
        .unwrap();
        let mut s = Session::open(f.path.clone());
        s.user_edit(DOC.replace("- y", "- changed"));
        s.save(&ctx(&f), None);
        assert_eq!(
            s.banner.as_ref().unwrap().key,
            "mdedit.banner.guard_blocked"
        );
        assert_eq!(std::fs::read_to_string(&f.path).unwrap(), DOC);
        s.user_edit(DOC.replace("- x", "- root/Passw0rd!"));
        s.save(&ctx(&f), None);
        assert_eq!(
            s.banner.as_ref().unwrap().key,
            "mdedit.banner.secret_blocked"
        );
        assert_eq!(std::fs::read_to_string(&f.path).unwrap(), DOC);
    }

    #[test]
    fn preview_checkbox_toggle_changes_one_byte_and_waits_for_save() {
        let f = fx(DOC);
        let mut s = Session::open(f.path.clone());
        s.toggle_task(0, 1);
        assert_eq!(s.buffer, DOC.replace("- [ ] a", "- [x] a"));
        assert!(s.dirty());
        assert_eq!(
            std::fs::read_to_string(&f.path).unwrap(),
            DOC,
            "no write before save"
        );
        s.save(&ctx(&f), None);
        let disk = std::fs::read_to_string(&f.path).unwrap();
        assert_eq!(disk, DOC.replace("- [ ] a", "- [x] a"));
        s.toggle_task(0, 2);
        assert_eq!(
            s.banner.as_ref().unwrap().key,
            "mdedit.banner.task_mismatch"
        );
    }

    fn block_id(s: &Session, text: &str) -> usize {
        blocks::blocks(&s.buffer)
            .iter()
            .position(|b| b.text == text)
            .unwrap_or_else(|| panic!("no block {text:?}"))
    }

    /// Enter 로 이어 쓰기(docs/surfaces/mdedit/index.md#enter-는-커서-자리에서-나눈다): 항목을 커서 자리에서 나누고 · 새 항목에 이어 쓰고 · 끝에서 다시
    /// Enter 로 빈 항목을 만들어 쓰고 · 문단을 나눈 뒤 저장하면 그 줄들만 바뀐다. 저장 전에는 파일
    /// 그대로다.
    #[test]
    fn enter_splits_and_typing_continues_until_save_changes_only_those_lines() {
        const SRC: &str = "# 할 일\n- [ ] 첫째 둘째\n- [ ] c\n\n문단 하나 둘\n\n# 끝\n";
        let f = fx(SRC);
        let mut s = Session::open(f.path.clone());

        let id = block_id(&s, "첫째 둘째");
        s.preview_split(id, "첫째 둘째", "첫째", " 둘째");
        assert_eq!(s.buffer, SRC.replace("첫째 둘째", "첫째\n- [ ] 둘째"));
        let f1 = s.focus.expect("새 항목을 열어야 한다");
        assert!(f1.start && !f1.after);
        assert_eq!(
            &s.buffer[f1.at..f1.at + "둘째".len()],
            "둘째",
            "커서 자리가 새 항목 머리가 아니다"
        );

        let id = block_id(&s, "둘째");
        s.preview_replace(id, "둘째", "새 둘째", Then::Nothing);
        let id = block_id(&s, "새 둘째");
        s.preview_split(id, "새 둘째", "새 둘째", "");
        let empty = blocks::blocks(&s.buffer)
            .iter()
            .position(|b| b.kind == blocks::Kind::Item && b.range.is_empty())
            .expect("빈 새 항목");
        s.preview_replace(empty, "", "셋째", Then::Nothing);

        let id = block_id(&s, "문단 하나 둘");
        s.preview_split(id, "문단 하나 둘", "문단 하나", " 둘");
        assert!(s.focus.is_some_and(|f| f.start));
        let id = block_id(&s, "둘");
        s.preview_split(id, "둘", "둘", "");
        assert!(
            s.focus.is_some_and(|f| f.after),
            "끝에서 Enter 는 새 줄 입력칸"
        );

        assert_eq!(
            std::fs::read_to_string(&f.path).unwrap(),
            SRC,
            "저장 전에 썼다"
        );
        assert!(s.dirty());
        s.save(&ctx(&f), None);
        let want =
            "# 할 일\n- [ ] 첫째\n- [ ] 새 둘째\n- [ ] 셋째\n- [ ] c\n\n문단 하나\n\n둘\n\n# 끝\n";
        assert_eq!(std::fs::read_to_string(&f.path).unwrap(), want);
    }

    fn unit_of(s: &Session, starts: &str) -> (usize, String) {
        let us = units::units(&s.buffer);
        let u = us
            .units
            .iter()
            .position(|x| s.buffer[x.range.clone()].starts_with(starts))
            .unwrap_or_else(|| panic!("no unit {starts:?}"));
        let h = textio::content_hash(&s.buffer[us.units[u].range.clone()]);
        (u, h)
    }

    /// 손잡이(docs/surfaces/mdedit/index.md#블록-손잡이는-원문-줄을-통째로-옮긴다): 위로 · 아래로 · 복제 · 삭제 · 형식 변경을 한 뒤 저장하면 그 줄들만 바뀐다.
    /// 저장 전에는 파일 그대로이고, 낡은 해시는 거부한다.
    #[test]
    fn handle_moves_and_deletes_change_only_those_lines_on_save() {
        const SRC: &str =
            "# 할 일\n\n- [ ] a\n- b\n  - b1\n- c\n\n문단\n\n> [!agent] 10:00 기록\n\n## 끝\n";
        let f = fx(SRC);
        let mut s = Session::open(f.path.clone());
        let (u, h) = unit_of(&s, "- c");
        s.preview_unit(u, &h, UnitOp::Up);
        let (u, h) = unit_of(&s, "> [!agent]");
        s.preview_unit(u, &h, UnitOp::Up);
        let (u, h) = unit_of(&s, "- [ ] a");
        s.preview_unit(u, &h, UnitOp::Duplicate);
        let (u, h) = unit_of(&s, "## 끝");
        s.preview_unit(u, &h, UnitOp::Delete);
        let (u, h) = unit_of(&s, "문단");
        s.preview_unit(u, &h, UnitOp::Format(Format::H2));
        let (u, _) = unit_of(&s, "- b");
        s.preview_unit(u, "0000000000000000", UnitOp::Delete);
        assert!(s.banner.is_some(), "낡은 해시를 받아들였다");
        assert_eq!(
            std::fs::read_to_string(&f.path).unwrap(),
            SRC,
            "저장 전에 썼다"
        );
        s.save(&ctx(&f), None);
        let want =
            "# 할 일\n\n- [ ] a\n- [ ] a\n- c\n- b\n  - b1\n\n> [!agent] 10:00 기록\n\n## 문단\n";
        assert_eq!(std::fs::read_to_string(&f.path).unwrap(), want);
    }

    /// 빈 항목에서 Enter(docs/surfaces/mdedit/index.md#블록-손잡이는-원문-줄을-통째로-옮긴다): 하위 항목은 한 단계 내어쓰고, 최상위 항목은 그 줄을 지운 뒤 앞
    /// 항목 뒤에 새 줄 입력칸을 연다 — 거기 쓴 문단은 빈 줄로 띄워져 목록을 나눈다.
    #[test]
    fn enter_on_an_empty_item_leaves_the_list() {
        const SRC: &str = "- a\n  - a1\n  - \n- b\n- \n- c\n";
        let f = fx(SRC);
        let mut s = Session::open(f.path.clone());
        let empty = |s: &Session, nth: usize| {
            blocks::blocks(&s.buffer)
                .iter()
                .enumerate()
                .filter(|(_, b)| b.kind == blocks::Kind::Item && b.range.is_empty())
                .nth(nth)
                .unwrap()
                .0
        };
        let id = empty(&s, 0);
        s.preview_exit_item(id);
        assert_eq!(s.buffer, "- a\n  - a1\n- \n- b\n- \n- c\n");
        assert!(s.focus.is_some_and(|f| f.start && !f.after));
        let id = empty(&s, 1);
        s.preview_exit_item(id);
        assert_eq!(s.buffer, "- a\n  - a1\n- \n- b\n- c\n");
        let f2 = s.focus.expect("앞 항목 뒤 새 줄");
        assert!(f2.after);
        let prev = blocks::blocks(&s.buffer)
            .iter()
            .position(|b| b.text == "b")
            .unwrap();
        s.preview_insert(Some(prev), "문단", false);
        assert_eq!(s.buffer, "- a\n  - a1\n- \n- b\n\n문단\n\n- c\n");
    }

    /// 나누기가 낡았으면(원문 조각이 다르면) 아무것도 바꾸지 않는다.
    #[test]
    fn a_stale_split_changes_nothing() {
        let f = fx(DOC);
        let mut s = Session::open(f.path.clone());
        let id = block_id(&s, "x");
        s.preview_split(id, "x 다름", "x", "y");
        assert_eq!(s.buffer, DOC);
        assert!(s.banner.is_some());
    }

    /// 미리보기에서 열린 블록의 적용 전 고침은 dirty 를 켠다(저장 단추 · status 가 같은 값을 본다).
    /// 바뀔 때만 상태줄을 다시 그리고, 적용하면 지워진다. 병합·저장 판단은 버퍼만 본다.
    #[test]
    fn pending_preview_edit_marks_dirty_until_applied() {
        let f = fx(DOC);
        let mut s = Session::open(f.path.clone());
        assert!(!s.dirty());
        assert!(
            s.set_pending(true).chrome,
            "dirty 가 켜졌는데 상태줄을 안 그렸다"
        );
        assert!(s.dirty() && !s.edited());
        assert!(!s.set_pending(true).chrome, "바뀌지 않았는데 다시 그렸다");
        assert!(s.set_pending(false).chrome);
        assert!(!s.dirty());

        s.set_pending(true);
        let id = blocks::blocks(&s.buffer)
            .iter()
            .position(|b| b.text == "x")
            .unwrap();
        s.preview_replace(id, "x", "x 고침", Then::Nothing);
        assert!(
            !s.pending && s.edited(),
            "적용한 뒤에도 적용 전 고침이 남았다"
        );

        // 적용 전 고침만 있을 때 외부 쓰기는 그냥 싣는다(병합 배너 없음), 저장은 파일을 안 건드린다.
        let mut s = Session::open(f.path.clone());
        s.set_pending(true);
        std::fs::write(&f.path, DOC.replace("- y", "- y (agent)")).unwrap();
        s.on_disk_change(Origin::Agent, &ctx(&f));
        assert_eq!(s.buffer, DOC.replace("- y", "- y (agent)"));
        assert!(s.banner.is_none(), "{:?}", s.banner.as_ref().map(|b| b.key));
        let before = std::fs::metadata(&f.path).unwrap().modified().unwrap();
        s.save(&ctx(&f), None);
        assert_eq!(
            std::fs::metadata(&f.path).unwrap().modified().unwrap(),
            before
        );
    }

    /// 미리보기 블록 편집(docs/surfaces/mdedit/index.md#미리보기-편집-단위는-블록-원문-조각)은 버퍼만 바꾸고, 그 사이 에이전트 쓰기는 기존 병합 경로를
    /// 탄다 — 저장 결과는 두 줄만 다르다.
    #[test]
    fn preview_edit_merges_with_an_agent_write_and_saves_two_lines() {
        let f = fx(DOC);
        let mut s = Session::open(f.path.clone());
        let id = blocks::blocks(&s.buffer)
            .iter()
            .position(|b| b.text == "x")
            .unwrap();
        let c = s.preview_replace(id, "x", "x 고침", Then::Nothing);
        assert!(c.reload && s.dirty() && s.banner.is_none());
        std::fs::write(&f.path, DOC.replace("- y", "- y (agent)")).unwrap();
        s.on_disk_change(Origin::Agent, &ctx(&f));
        assert!(s.conflict.is_none());
        s.save(&ctx(&f), None);
        assert_eq!(
            std::fs::read_to_string(&f.path).unwrap(),
            DOC.replace("- x", "- x 고침").replace("- y", "- y (agent)")
        );
    }

    #[test]
    fn preview_edit_on_the_line_the_agent_changed_stops_at_conflict() {
        let f = fx(DOC);
        let mut s = Session::open(f.path.clone());
        let id = blocks::blocks(&s.buffer)
            .iter()
            .position(|b| b.text == "x")
            .unwrap();
        s.preview_replace(id, "x", "mine", Then::Nothing);
        std::fs::write(&f.path, DOC.replace("- x", "- theirs")).unwrap();
        s.on_disk_change(Origin::Agent, &ctx(&f));
        assert!(s.conflict.is_some());
        // conflict 동안 미리보기 편집은 받지 않는다.
        let before = s.buffer.clone();
        s.preview_replace(0, "할 일", "z", Then::Nothing);
        assert_eq!(s.buffer, before);
    }

    #[test]
    fn stale_preview_edit_is_refused_with_a_banner_and_no_change() {
        let f = fx(DOC);
        let mut s = Session::open(f.path.clone());
        let id = blocks::blocks(&s.buffer)
            .iter()
            .position(|b| b.text == "x")
            .unwrap();
        let c = s.preview_replace(id, "not what is there", "z", Then::Nothing);
        assert!(c.reload);
        assert_eq!(s.buffer, DOC);
        assert_eq!(
            s.banner.as_ref().map(|b| b.key),
            Some("mdedit.banner.inline_rejected")
        );
        // 원문 편집기 모드에서는 아무것도 안 한다.
        s.set_mode(Mode::Source);
        s.preview_insert(None, "new", false);
        assert_eq!(s.buffer, DOC);
    }

    #[test]
    fn enter_on_an_item_creates_the_next_item_and_focuses_it() {
        let f = fx(DOC);
        let mut s = Session::open(f.path.clone());
        let id = blocks::blocks(&s.buffer)
            .iter()
            .position(|b| b.text == "x")
            .unwrap();
        s.preview_replace(id, "x", "x2", Then::NewItem);
        assert_eq!(s.buffer, DOC.replace("- x\n", "- x2\n- \n"));
        let at = s.focus.expect("focus").at;
        assert_eq!(&s.buffer[at - 2..at], "- ");
        assert!(!s.focus.unwrap().after);
    }

    #[test]
    fn crlf_and_bom_survive_save() {
        let raw = "\u{feff}# a\r\n- x\r\n";
        let f = fx(raw);
        let mut s = Session::open(f.path.clone());
        assert_eq!(s.buffer, "# a\n- x\n");
        s.user_edit("# a\n- y\n".into());
        s.save(&ctx(&f), None);
        assert_eq!(
            std::fs::read_to_string(&f.path).unwrap(),
            "\u{feff}# a\r\n- y\r\n"
        );
    }
}
