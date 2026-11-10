//! `remote attach --into-gui --ssh|--profile`: 실행 중인 GUI가 터널을 열어 mirror를 만들게 한다.
//! 기존 workspace는 접수 뒤 `run.result`로 연결 결과를 기다린다. 새 workspace는 응답에 run이 없어
//! 생성 결과 한 줄로 끝난다. `remote connect`도 저장된 원격 행의 연결 결과를 같은 방식으로 기다린다.
//! 출력·종료 코드 처리는 [`super::run_wait`]를 따른다.

use std::time::Duration;

use anyhow::Result;

use crate::dispatch::{ClientCommand, ClientCtx};

/// 응답 없는 호스트의 포트 발견이 최악 45초이고 그 뒤에 터널과 attach 연결이 이어진다.
/// 이 시간을 넘겨도 연결은 계속되며 결과는 출력한 run으로 다시 읽을 수 있다.
const ATTACH_WAIT: Duration = Duration::from_secs(90);

pub struct RemoteGuiAttach<'a> {
    pub command: &'a crate::Commands,
}

impl ClientCommand for RemoteGuiAttach<'_> {
    fn run(self: Box<Self>, ctx: &ClientCtx<'_>) -> Result<()> {
        let request = crate::request::command_to_request(self.command);
        super::run_wait::dispatch_and_wait(ctx.port_file, &request, "run.result", |_| ATTACH_WAIT)
    }
}
