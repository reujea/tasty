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
//! - ground로 돌아간 바이트: 머리를 비우고 그 바이트는 버린다. ground의 새 파서와 같다. 잘못된 UTF-8이
//!   삼킨 바이트도 마찬가지다(ground에서 `\xe4\x1b`의 ESC는 글자로 소비되고 파서는 ground다).
//! - ground가 아닌 곳에서 무언가를 dispatch한 바이트: 머리를 그 바이트부터 다시 시작한다. dispatch는 표가
//!   그 바이트를 처리해 상태를 떠날 때만 나고, 그런 바이트는 ESC·C1(UTF-8로 쓴 C1 포함)이 아니면 ground로
//!   간다. ESC·C1은 아래처럼 어느 상태에서나 같은 곳으로 간다.
//! - dispatch가 없어도 ESC·CAN·SUB·C1 바이트: `vtparse` 전이표에서 이 바이트들은 모든 상태가 `anywhere_or`로
//!   같은 (동작, 상태)로 보낸다. 들어간 상태의 진입 동작이 버퍼를 비우므로 머리를 그 바이트부터 다시
//!   시작한다. 예외는 둘이다. UTF-8 글자 중간이면 바이트가 글자로 소비되므로 기준점이 아니다(리드 바이트
//!   뒤의 연속 바이트 수를 넉넉히 세어 그럴 수 있는 동안은 쓰지 않는다). OSC 문자열 안의 0x9d는 같은 상태로
//!   가는 전이라 아무것도 하지 않으므로, 파서가 OSC 문자열 밖임을 알 때만 기준점이다.
//!
//! 쓰기 판정은 실제로 받은 바이트로만 한다. 머리 끝의 미완성 UTF-8이 C2 리드면 다음 바이트가 UTF-8로 쓴
//! C1 종결일 수 있으므로 0x9c로 먼저 끝내 본다. 다른 리드는 어떻게 끝나도 데이터에 글자 하나가 들어간다.
//! 머리가 `HEAD_LIMIT`을 넘도록 정하지 못하면 다음 기준점까지 보지 않는다. 그런 경우는 OSC 52 자신의
//! 선택 인자와 데이터 머리가 한도를 넘을 때다(진행 표시만 빠지고 기록·토스트는 그대로다). vtparse의 OSC
//! 버퍼는 본문 크기만큼 자라고 surface 상태를 지울 때 해제된다.

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
    /// 파서가 UTF-8 글자 중간일 수 있으면 남은 연속 바이트 수. 실제보다 작게 세지 않는다.
    utf8_left: u8,
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
        self.head.extend(byte);
    }
}

impl Default for SurfaceScan {
    fn default() -> Self {
        Self {
            parser: vtparse::VTParser::new(),
            head: Vec::new(),
            closed: None,
            utf8_left: 0,
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

/// 실제 파서가 UTF-8 글자 중간일 수 있는 남은 연속 바이트 수. 리드 바이트는 ground나 OSC 문자열에서만
/// 글자를 시작하지만 그 밖일 수도 있으면(`may_start`) 시작한 것으로 센다. 연속 바이트 범위는 넓게 본다.
fn next_utf8_left(left: u8, byte: u8, may_start: bool) -> u8 {
    match byte {
        0x80..=0xbf => left.saturating_sub(1),
        0xc2..=0xdf if may_start => 1,
        0xe0..=0xef if may_start => 2,
        0xf0..=0xf4 if may_start => 3,
        _ => 0,
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
            // 직전까지 UTF-8 글자 중간일 수 있었으면 이 바이트는 글자로 소비됐을 수 있다.
            let maybe_in_char = scan.utf8_left > 0;
            scan.utf8_left = next_utf8_left(
                scan.utf8_left,
                byte,
                was_ground || scan.closed != Some(false),
            );
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
            let anywhere = !maybe_in_char
                && match byte {
                    0x1b | 0x18 | 0x1a | 0x80..=0x9c | 0x9e | 0x9f => true,
                    0x9d => scan.closed == Some(false),
                    _ => false,
                };
            if dispatched || anywhere {
                scan.restart(Some(byte));
            } else if scan.closed.is_none() {
                scan.head.push(byte);
            } else {
                continue;
            }
            let (in_osc, write) = osc_head(&scan.head);
            if write {
                scan.receiving.get_or_insert(now);
                scan.closed = Some(in_osc);
            } else if scan.head.len() >= HEAD_LIMIT {
                scan.closed = Some(in_osc);
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
        scan.utf8_left = 0;
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
