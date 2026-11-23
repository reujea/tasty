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
//! 기준점은 파서가 ground로 돌아간 바이트와 무언가를 dispatch한 바이트다. 기준점에서 파서가 가는 상태는
//! ground의 새 파서가 그 바이트를 받은 상태와 같다(그 바이트는 ground이거나 어느 상태에서나 같은 전이를
//! 하는 ESC·C1이다). 그래서 머리 판정은 파서 상태를 바이트별로 흉내 내지 않고 파서 자체의 판정이 된다.
//! 머리가 `HEAD_LIMIT`을 넘도록 정하지 못하면 다음 기준점까지 보지 않는다(긴 SOS·PM·DCS 문자열 뒤 dispatch
//! 없이 시작한 쓰기는 진행 표시만 빠지고 기록·토스트는 그대로다). vtparse의 OSC 버퍼는 본문 크기만큼 자라고
//! surface 상태를 지울 때 해제된다.

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
    /// 마지막 기준점 뒤의 바이트. 쓰기로 정했거나 한도를 넘으면 다음 기준점까지 `None`이다.
    head: Option<Vec<u8>>,
    /// 본문을 받는 중인 쓰기의 접수 시각.
    receiving: Option<Instant>,
    /// 끝났고 client 기록 결과를 기다리는 쓰기의 접수 시각. 바이트 순서다.
    awaiting: VecDeque<Instant>,
}

impl Default for SurfaceScan {
    fn default() -> Self {
        Self {
            parser: vtparse::VTParser::new(),
            head: Some(Vec::new()),
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

/// 기준점 뒤 `head`로 열린 시퀀스가 OSC 52 쓰기 본문을 받는 중인지. 새 파서에 `head`와 BEL 둘을 먹여 끝난
/// OSC가 `52;<선택>;<데이터>`이고 데이터가 비지 않았으며 질의 `?`가 아닌지 본다. BEL이 둘인 것은 머리 끝의
/// 미완성 UTF-8이 첫 BEL을 삼켜도 OSC를 끝내기 위해서다.
fn opens_write(head: &[u8]) -> bool {
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
    parser.parse(b"\x07\x07", &mut write);
    write.0 == Some(true)
}

impl CopyProgress {
    /// mirror 터미널에 넣기 전의 바이트를 본다.
    pub(crate) fn feed(&mut self, surface: u32, bytes: &[u8], now: Instant) {
        let Self { surfaces, failures } = self;
        let scan = surfaces.entry(surface).or_default();
        let mut step = Dispatches::default();
        for &byte in bytes {
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
                scan.head = Some(Vec::new());
                continue;
            }
            if dispatched {
                scan.head = Some(vec![byte]);
            } else if let Some(head) = &mut scan.head {
                head.push(byte);
            }
            let Some(head) = &scan.head else {
                continue;
            };
            if opens_write(head) {
                scan.receiving.get_or_insert(now);
                scan.head = None;
            } else if head.len() >= HEAD_LIMIT {
                scan.head = None;
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
        scan.head = Some(Vec::new());
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
