//! MockProcessSpawner — 외부 process 를 띄우지 않고 성공한 child 를 돌려준다.

use std::path::Path;

use crate::ports::process::{ExitStatus, ProcessChild, ProcessSpawner};

#[derive(Debug, Default)]
pub struct MockProcessSpawner;

impl ProcessSpawner for MockProcessSpawner {
    fn spawn(
        &self,
        _command: &str,
        _args: &[&str],
        _env: &[(String, String)],
        _cwd: Option<&Path>,
    ) -> anyhow::Result<Box<dyn ProcessChild>> {
        Ok(Box::new(MockProcessChild { pid: 1 }))
    }
}

pub struct MockProcessChild {
    pid: u32,
}

impl ProcessChild for MockProcessChild {
    fn pid(&self) -> u32 {
        self.pid
    }

    fn try_wait(&mut self) -> anyhow::Result<Option<ExitStatus>> {
        Ok(Some(ExitStatus::Exited(0)))
    }

    fn kill(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}
