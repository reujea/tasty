//! attach mirror가 받는 바이트에서 원격 프로그램의 OSC 52 쓰기가 오가는 동안을 본다.
//!
//! 복사 접수는 쓰기 본문의 첫 바이트가 이 client에 도착한 때다. 그 전(원격 프로그램이 복사를 정하고
//! 시퀀스를 보내기 시작하기까지)은 바이트로 관측할 수 없다. 경계 판정은 mirror 터미널의 termwiz와
//! 같은 `vtparse` 상태 기계를 surface마다 따로 돌려 얻고, 끝난 시퀀스의 종류도 termwiz의
//! `OperatingSystemCommand::parse`로 정한다. 그래서 C1 도입자·UTF-8로 쓴 C1·ESC·CAN·SUB 중단도
//! termwiz와 같이 본다.
//!
//! 끝난 쓰기는 '결과 대기'로 남고 client가 같은 surface의 `TerminalClipboardSet`을 처리할 때 오래된
//! 것부터 하나씩 닫힌다. termwiz가 받지 않은 시퀀스(잘못된 base64 등)는 끝나는 즉시 실패다. 질의·
//! 지우기나 다른 OSC로 끝나면(앞머리 관측이 쓰기로 잘못 본 경우) 실패 없이 접수만 거둔다.
//!
//! 열린 시퀀스가 OSC 52 쓰기인지는 마지막 기준점 뒤의 바이트(머리)를 새 `vtparse`에 다시 먹여 정한다.
//! 기준점마다 새 파서가 그 지점의 실제 파서와 같은 상태(OSC 버퍼 포함)에서 시작한다는 것이 근거다.
//! - ground로 돌아간 바이트: 머리를 비우고 그 바이트는 버린다. ground의 새 파서와 같다. OSC가 끝나고 ground로
//!   돌아갈 때 남은 OSC 버퍼는 다음 OSC 진입 동작(OscStart)이 비운다. 잘못된 UTF-8이 삼킨 바이트도
//!   마찬가지다(ground에서 `\xe4\x1b`의 ESC는 글자로 소비되고 파서는 ground다).
//! - ground가 아닌 곳에서 무언가를 dispatch한 바이트: 머리를 그 바이트부터 다시 시작한다. dispatch는 표가
//!   그 바이트를 처리해 상태를 떠날 때만 나고, 그런 바이트는 ESC·C1(UTF-8로 쓴 C1 포함)이 아니면 ground로
//!   간다. ESC·C1은 아래처럼 어느 상태에서나 같은 곳으로 간다.
//! - dispatch가 없어도 ESC·CAN·SUB·C1 바이트: `vtparse` 전이표에서 이 바이트들은 모든 상태가 `anywhere_or`로
//!   같은 (동작, 상태)로 보낸다. 다른 상태에서 들어가면 진입 동작이 버퍼를 비운다. 같은 상태로 가는
//!   전이(Escape의 ESC, CsiEntry의 0x9b, DcsEntry의 0x90, SosPm의 0x98·0x9e·0x9f)는 진입 동작이 돌지 않지만
//!   그 상태들에는 진입 뒤 모으는 OSC 판정 버퍼가 없다. 그래서 머리를 그 바이트부터 다시 시작한다. 예외는
//!   둘이다. UTF-8 글자 중간이면 바이트가 글자로 소비되므로 기준점이 아니다. 글자 중간인지는 `utf8parse`
//!   상태표대로 정확히 센다(`Utf8Tail`). OSC 문자열 안의 0x9d는 같은 상태로 가는 전이라 아무것도 하지
//!   않으므로 OSC 문자열 밖일 때만 기준점이다. 리드 바이트가 글자를 시작하는지와 0x9d의 경우는 파서가 OSC
//!   문자열 안인지에 달렸다. ground가 아니면 머리를 다시 파싱해 정하고, 머리를 닫은 뒤에는 닫을 때 기록한
//!   값을 쓴다.
//!
//! 쓰기 판정은 실제로 받은 바이트로만 한다. 머리 끝의 미완성 UTF-8이 C2 리드면 다음 바이트가 UTF-8로 쓴
//! C1 종결일 수 있으므로 0x9c로 먼저 끝내 본다. 다른 리드는 어떻게 끝나도 데이터에 글자 하나가 들어간다.
//! 사전 필터로 머리에 `52;`와 그 뒤 `;`가 나온 뒤에만 다시 파싱한다. 머리가 `HEAD_LIMIT`을 넘도록 정하지
//! 못하면 다음 기준점까지 보지 않는다. 그런 경우는 OSC 52 자신의 선택 인자와 데이터 머리가 한도를 넘을
//! 때다(진행 표시만 빠지고 기록·토스트는 그대로다). vtparse의 OSC 버퍼는 본문 크기만큼 자라고 surface 상태를
//! 지울 때 해제된다.

use std::collections::{HashMap, VecDeque};
use std::time::Instant;
use termwiz::escape::osc::OperatingSystemCommand;

/// OSC 52 머리(도입자·`52;`·선택 인자·`;`·데이터 첫 두 바이트)를 판정하기에 충분한 길이.
const HEAD_LIMIT: usize = 64;

#[derive(Default)]
pub(crate) struct CopyProgress {
    surfaces: HashMap<u32, SurfaceScan>,
    /// 실패로 끝난 접수 건의 surface. 알릴 곳이 정해지면 꺼낸다.
    failures: Vec<u32>,
}

struct SurfaceScan {
    parser: vtparse::VTParser,
    /// 마지막 기준점 뒤의 바이트.
    head: Vec<u8>,
    /// 쓰기로 정했거나 한도를 넘어 다음 기준점까지 머리를 보지 않는 동안, 파서가 OSC 문자열 안인지.
    closed: Option<bool>,
    /// 이 머리를 다시 파싱해 파서가 OSC 문자열 안임을 알았다. 머리 안에서 OSC 문자열을 떠나는 바이트는 모두
    /// 기준점이라 다음 기준점까지 그대로다.
    in_osc_head: bool,
    /// 사전 필터. 머리에서 `52;`를 어디까지 찾았는지(3이면 찾음), 4면 그 뒤 `;`도 찾았다.
    marks: u8,
    /// 파서가 UTF-8 글자 중간이면 남은 연속 바이트.
    utf8: Utf8Tail,
    /// 본문을 받는 중인 쓰기의 접수 시각.
    receiving: Option<Instant>,
    /// 끝났고 client 기록 결과를 기다리는 쓰기의 접수 시각. 바이트 순서다.
    awaiting: VecDeque<Instant>,
}

impl SurfaceScan {
    /// 기준점에서 머리를 비우고, ground가 아니면 그 바이트부터 다시 시작한다.
    fn restart(&mut self, byte: Option<u8>) {
        self.head.clear();
        self.closed = None;
        self.in_osc_head = false;
        self.marks = 0;
        if let Some(byte) = byte {
            self.head.push(byte);
            self.marks = next_mark(0, byte);
        }
    }
}

impl Default for SurfaceScan {
    fn default() -> Self {
        Self {
            parser: vtparse::VTParser::new(),
            head: Vec::new(),
            closed: None,
            in_osc_head: false,
            marks: 0,
            utf8: Utf8Tail::default(),
            receiving: None,
            awaiting: VecDeque::new(),
        }
    }
}

/// 끝난 OSC가 OSC 52였을 때 termwiz가 어떻게 받는지.
enum Ended {
    Write,
    /// 질의·지우기. 쓰기가 아니지만 실패도 아니다.
    NotWrite,
    /// termwiz가 받지 않았다(잘못된 base64 등).
    Invalid,
    /// OSC 52가 아닌 OSC.
    Other,
}

/// 파서가 낸 OSC 끝과, 이 바이트가 기준점인지(무언가를 dispatch했는지).
#[derive(Default)]
struct Dispatches {
    ends: Vec<Ended>,
    dispatched: bool,
}

impl vtparse::VTActor for Dispatches {
    fn print(&mut self, _: char) {}
    fn execute_c0_or_c1(&mut self, _: u8) {}
    fn dcs_hook(&mut self, _: u8, _: &[i64], _: &[u8], _: bool) {}
    fn dcs_put(&mut self, _: u8) {}
    fn dcs_unhook(&mut self) {
        self.dispatched = true;
    }
    fn esc_dispatch(&mut self, _: &[i64], _: &[u8], _: bool, _: u8) {
        self.dispatched = true;
    }
    fn csi_dispatch(&mut self, _: &[vtparse::CsiParam], _: bool, _: u8) {
        self.dispatched = true;
    }
    fn osc_dispatch(&mut self, params: &[&[u8]]) {
        self.dispatched = true;
        self.ends.push(ended_osc(params));
    }
    fn apc_dispatch(&mut self, _: Vec<u8>) {
        self.dispatched = true;
    }
}

fn ended_osc(params: &[&[u8]]) -> Ended {
    if params.first() != Some(&b"52".as_slice()) {
        return Ended::Other;
    }
    match OperatingSystemCommand::parse(params) {
        OperatingSystemCommand::SetSelection(..) => Ended::Write,
        OperatingSystemCommand::QuerySelection(..) | OperatingSystemCommand::ClearSelection(..) => {
            Ended::NotWrite
        }
        _ => Ended::Invalid,
    }
}

/// 기준점 뒤 `head`를 새 파서에 먹인 뒤 열린 시퀀스가 OSC 문자열인지와, OSC 52 쓰기 본문을 받는 중인지.
/// 0x9c와 BEL 둘을 덧붙여 처음 끝난 OSC를 본다. 0x9c는 OSC를 끝내거나, 머리 끝이 C2 리드면 그것과 함께
/// UTF-8 ST가 된다. 다른 미완성 UTF-8은 어떻게 끝나도 글자 하나를 데이터에 남기므로 BEL로 끝낸다.
/// 쓰기는 `52;<선택>;<데이터>`이고 데이터가 비지 않았으며 질의 `?`가 아닌 것이다.
fn osc_head(head: &[u8]) -> (bool, bool) {
    #[derive(Default)]
    struct Write(Option<bool>);
    impl vtparse::VTActor for Write {
        fn print(&mut self, _: char) {}
        fn execute_c0_or_c1(&mut self, _: u8) {}
        fn dcs_hook(&mut self, _: u8, _: &[i64], _: &[u8], _: bool) {}
        fn dcs_put(&mut self, _: u8) {}
        fn dcs_unhook(&mut self) {}
        fn esc_dispatch(&mut self, _: &[i64], _: &[u8], _: bool, _: u8) {}
        fn csi_dispatch(&mut self, _: &[vtparse::CsiParam], _: bool, _: u8) {}
        fn osc_dispatch(&mut self, params: &[&[u8]]) {
            let data = params.get(2..).map(|data| data.join(&b';'));
            self.0.get_or_insert(
                params.first() == Some(&b"52".as_slice())
                    && data.is_some_and(|data| !data.is_empty() && data != b"?"),
            );
        }
        fn apc_dispatch(&mut self, _: Vec<u8>) {}
    }
    let mut parser = vtparse::VTParser::new();
    // 머리 안에서 끝난 OSC는 없다(끝난 바이트가 기준점이다). 그래도 덧붙인 바이트의 첫 OSC만 본다.
    parser.parse(head, &mut Write::default());
    let mut write = Write::default();
    parser.parse(b"\x9c\x07\x07", &mut write);
    (write.0.is_some(), write.0 == Some(true))
}

/// 사전 필터의 다음 값. OSC 52 쓰기라면 OSC 버퍼가 `52;`로 시작하고 그 뒤 `;`가 있다. 버퍼의 ASCII는 머리의
/// 같은 바이트에서만 오고, 그 사이에는 OSC 문자열이 무시하는 바이트만 올 수 있다. 그래서 무시되는 바이트를
/// 건너뛰며 `52;`를 찾고, 찾은 뒤에는 `;`만 기다린다. 이 필터를 통과하지 못하면 `osc_head`도 쓰기가 아니다.
fn next_mark(marks: u8, byte: u8) -> u8 {
    let ignored = matches!(
        byte,
        0x00..=0x06 | 0x08..=0x17 | 0x19 | 0x1c..=0x1f | 0x9d | 0xa0..=0xc1 | 0xf5..=0xff
    );
    match (marks, byte) {
        (3, b';') => 4,
        (3.., _) => marks,
        (_, _) if ignored => marks,
        (0..=2, b'5') => 1,
        (1, b'2') => 2,
        (2, b';') => 3,
        _ => 0,
    }
}

/// `vtparse`가 UTF-8 글자 중간일 때 남은 연속 바이트 수와 다음 바이트의 허용 범위. `utf8parse` 0.2의
/// 상태표와 같다. 범위 밖 바이트는 잘못된 시퀀스로 소비되고 글자가 끝난다.
#[derive(Clone, Copy, Default)]
struct Utf8Tail {
    left: u8,
    next: (u8, u8),
}

impl Utf8Tail {
    /// 이 바이트를 받은 뒤. `starts`는 파서가 글자를 시작할 수 있는 상태(ground·OSC 문자열)인지다.
    fn after(self, byte: u8, starts: bool) -> Self {
        let tail = |left, lo, hi| Self {
            left,
            next: (lo, hi),
        };
        if self.left > 0 {
            return match (self.next.0..=self.next.1).contains(&byte) && self.left > 1 {
                true => tail(self.left - 1, 0x80, 0xbf),
                false => Self::default(),
            };
        }
        if !starts {
            return Self::default();
        }
        match byte {
            0xc2..=0xdf => tail(1, 0x80, 0xbf),
            0xe0 => tail(2, 0xa0, 0xbf),
            0xe1..=0xec | 0xee..=0xef => tail(2, 0x80, 0xbf),
            0xed => tail(2, 0x80, 0x9f),
            0xf0 => tail(3, 0x90, 0xbf),
            0xf1..=0xf3 => tail(3, 0x80, 0xbf),
            0xf4 => tail(3, 0x80, 0x8f),
            _ => Self::default(),
        }
    }
}

impl CopyProgress {
    /// mirror 터미널에 넣기 전의 바이트를 본다.
    pub(crate) fn feed(&mut self, surface: u32, bytes: &[u8], now: Instant) {
        let Self { surfaces, failures } = self;
        let scan = surfaces.entry(surface).or_default();
        let mut step = Dispatches::default();
        for &byte in bytes {
            let was_ground = scan.parser.is_ground();
            // UTF-8 글자 중간이면 이 바이트는 표를 거치지 않고 글자로 소비된다.
            let in_char = scan.utf8.left > 0;
            // 리드 바이트가 글자를 시작하는지와 0x9d가 새 OSC를 여는지는 파서가 OSC 문자열 안인지에 달렸다.
            // ground가 아니고 머리를 보는 중이면 머리를 다시 파싱해 정한다. 이런 바이트 조합에서만 돈다.
            let needs_osc = !in_char && !was_ground && matches!(byte, 0x9d | 0xc2..=0xf4);
            let in_osc = needs_osc
                && scan.closed.unwrap_or_else(|| {
                    scan.in_osc_head = scan.in_osc_head || osc_head(&scan.head).0;
                    scan.in_osc_head
                });
            scan.utf8 = scan.utf8.after(byte, was_ground || in_osc);
            scan.parser.parse_byte(byte, &mut step);
            for end in step.ends.drain(..) {
                let receiving = scan.receiving.take();
                match end {
                    Ended::Write => scan.awaiting.push_back(receiving.unwrap_or(now)),
                    Ended::Invalid if receiving.is_some() => failures.push(surface),
                    Ended::Invalid | Ended::NotWrite | Ended::Other => {}
                }
            }
            let dispatched = std::mem::take(&mut step.dispatched);
            if scan.parser.is_ground() {
                scan.restart(None);
                continue;
            }
            let anywhere = !in_char
                && match byte {
                    0x1b | 0x18 | 0x1a | 0x80..=0x9c | 0x9e | 0x9f => true,
                    0x9d => !in_osc,
                    _ => false,
                };
            if dispatched || anywhere {
                scan.restart(Some(byte));
            } else if scan.closed.is_none() {
                scan.head.push(byte);
                scan.marks = next_mark(scan.marks, byte);
            } else {
                continue;
            }
            if scan.marks == 4 {
                let (in_osc, write) = osc_head(&scan.head);
                if write {
                    scan.receiving.get_or_insert(now);
                    scan.closed = Some(in_osc);
                    continue;
                }
            }
            if scan.head.len() >= HEAD_LIMIT {
                scan.closed = Some(osc_head(&scan.head).0);
            }
        }
        if scan.receiving.is_none() && scan.awaiting.is_empty() && scan.parser.is_ground() {
            surfaces.remove(&surface);
        }
    }

    /// client가 이 surface의 OSC 52 쓰기 결과를 처리했다. 가장 오래된 결과 대기 건을 닫는다.
    pub(crate) fn settle_write(&mut self, surface: u32) -> bool {
        let Some(scan) = self.surfaces.get_mut(&surface) else {
            return false;
        };
        let settled = scan.awaiting.pop_front().is_some();
        self.forget_if_idle(surface);
        settled
    }

    /// 이 surface의 사건 처리가 끝났는데 결과가 나오지 않은 쓰기를 실패로 닫는다.
    pub(crate) fn fail_unanswered(&mut self, surface: u32) {
        let Some(scan) = self.surfaces.get_mut(&surface) else {
            return;
        };
        let failed = scan.awaiting.len();
        scan.awaiting.clear();
        self.failures.extend(std::iter::repeat_n(surface, failed));
        self.forget_if_idle(surface);
    }

    /// 스트림이 끊겼다. 본문을 받던 쓰기는 실패로 닫고 파서 상태를 버린다. 이미 끝나 결과를 기다리는
    /// 쓰기는 남겨 사건 처리로 정산한다.
    pub(crate) fn break_stream(&mut self, surface: u32) {
        let Some(scan) = self.surfaces.get_mut(&surface) else {
            return;
        };
        if scan.receiving.take().is_some() {
            self.failures.push(surface);
        }
        scan.parser = vtparse::VTParser::new();
        scan.utf8 = Utf8Tail::default();
        scan.restart(None);
        self.forget_if_idle(surface);
    }

    /// surface가 사라졌다. 알리지 않고 버린다.
    pub(crate) fn forget(&mut self, surface: u32) {
        self.surfaces.remove(&surface);
    }

    /// 상태가 있는 surface.
    pub(crate) fn surfaces(&self) -> Vec<u32> {
        self.surfaces.keys().copied().collect()
    }

    /// 실패로 끝난 접수 건의 surface를 꺼낸다. 한 건마다 한 항목이다.
    pub(crate) fn take_failures(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.failures)
    }

    fn forget_if_idle(&mut self, surface: u32) {
        if self.surfaces.get(&surface).is_some_and(|scan| {
            scan.receiving.is_none() && scan.awaiting.is_empty() && scan.parser.is_ground()
        }) {
            self.surfaces.remove(&surface);
        }
    }

    /// 표시할 진행 중 복사의 (surface, 접수 시각).
    pub(crate) fn waits(&self) -> impl Iterator<Item = (u32, Instant)> + '_ {
        self.surfaces.iter().flat_map(|(&surface, scan)| {
            scan.receiving
                .iter()
                .chain(scan.awaiting.iter())
                .map(move |&since| (surface, since))
        })
    }
}

#[cfg(test)]
#[path = "copy_progress_tests.rs"]
mod tests;
