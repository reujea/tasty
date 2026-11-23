//! mirror 출력 적용에 원격 프로그램의 OSC 52 쓰기 진행 추적을 잇는다.
use super::MirrorHost;
use crate::remote::copy_progress::CopyProgress;

impl<'a, 'engine> MirrorHost<'a, 'engine> {
    pub(super) fn with_copies(mut self, copies: &'a mut CopyProgress) -> Self {
        self.copies = Some(copies);
        self
    }

    /// 스트림이 끊겼다. 이 surface에서 본문을 받던 쓰기는 실패로 닫는다.
    pub(super) fn break_copies(&mut self, local: u32) {
        if let Some(copies) = self.copies.as_deref_mut() {
            copies.break_stream(local);
        }
    }
}
