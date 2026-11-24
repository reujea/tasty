use crate::runtime::engine_access::EngineMut;

impl EngineMut<'_> {
    /// 지정한 surface의 사용자 mark 이후 출력. 로컬 Terminal이 없으면 빈 문자열이다.
    pub fn read_since_mark_of(&mut self, surface_id: u32, strip_ansi: bool) -> String {
        self.runtime
            .terminals
            .get_mut(surface_id)
            .map(|t| t.read_since_mark(strip_ansi))
            .unwrap_or_default()
    }

    /// scanner mark 이후 출력을 읽고 커서를 옮긴다. 다른 surface나 포커스로 대체하지 않는다.
    /// 로컬 Terminal이 없으면 None이다. `with_origins`면 출처 정보도 계산한다.
    pub fn take_since_output_scan_mark(
        &mut self,
        surface_id: u32,
        strip_ansi: bool,
        with_origins: bool,
    ) -> Option<tasty_terminal::ScanRead> {
        self.runtime
            .terminals
            .get_mut(surface_id)
            .map(|t| t.take_since_output_scan_mark(strip_ansi, with_origins))
    }

    /// 호출자의 커서로 지정 surface의 출력을 읽는다. Terminal 부재(None)와 읽기 실패(Err)를 구분한다.
    pub fn read_output(
        &mut self,
        surface_id: u32,
        req: &tasty_terminal::OutputReadRequest,
    ) -> Option<Result<tasty_terminal::OutputRead, tasty_terminal::OutputReadError>> {
        self.runtime
            .terminals
            .get_mut(surface_id)
            .map(|t| t.read_output(req))
    }
}
