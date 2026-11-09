//! 프로세스 전역 상태를 같은 바이너리의 다른 시험이 건드려 잠금으로 막을 수 없는 시험을, 자기 바이너리의 새
//! 프로세스에서 혼자 다시 실행한다. tracing 콜사이트 관심 캐시가 그런 상태다
//! (`docs/dev-guide/unit-test-isolation.md`).

/// [`rerun_alone`]이 띄운 자식을 표시한다. 값은 그 자식이 실행할 시험 이름이다.
const ENV: &str = "TASTY_TEST_RERUN_ALONE";

/// 부모에서는 `test`만 `--test-threads=1`인 자식으로 실행해 통과를 확인하고 true를 반환한다.
/// 그 자식 안에서는 false를 반환하므로 호출한 시험은 이때 본문을 실행한다.
///
/// `module`은 호출한 곳의 `module_path!()`다. 이름 필터 오류로 0개를 실행한 자식을 통과로 세지 않도록
/// 자식의 성공 종료와 `test result: ok. 1 passed`를 함께 확인한다. 실패하면 자식의 출력을 메시지에 싣는다.
pub fn rerun_alone(module: &str, test: &str) -> bool {
    let module = module.split_once("::").map_or("", |(_, rest)| rest);
    let name = if module.is_empty() {
        test.to_string()
    } else {
        format!("{module}::{test}")
    };
    if std::env::var_os(ENV).is_some_and(|running| running == name.as_str()) {
        return false;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &name, "--test-threads=1"])
        .env(ENV, &name)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("test result: ok. 1 passed"),
        "the run of {name} alone failed: {}\nstdout:\n{stdout}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    true
}

/// [`rerun_alone`]의 자식 안에서만 참이다. 공유 도우미가 격리를 빠뜨린 호출을 거부할 때 쓴다.
pub fn is_rerun_alone() -> bool {
    std::env::var_os(ENV).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_body_runs_only_in_the_child_and_its_failure_reaches_the_parent() {
        if !rerun_alone(module_path!(), "child_body_for_the_rerun_check") {
            unreachable!("this test is not the one the child runs");
        }
        let failure = std::panic::catch_unwind(|| {
            rerun_alone(module_path!(), "child_body_that_fails_for_the_rerun_check")
        })
        .expect_err("a failing child fails its parent");
        let message = failure.downcast_ref::<String>().unwrap();
        assert!(message.contains("child failure marker"), "{message}");
    }

    #[test]
    fn a_name_that_matches_no_test_is_not_a_pass() {
        let failure = std::panic::catch_unwind(|| rerun_alone(module_path!(), "no_such_test"))
            .expect_err("zero tests run is not a pass");
        let message = failure.downcast_ref::<String>().unwrap();
        assert!(message.contains("no_such_test"), "{message}");
    }

    /// 자식 표지 없이 직접 실행되면 할 일이 없다.
    #[test]
    fn child_body_for_the_rerun_check() {
        if !is_rerun_alone() {
            return;
        }
        assert!(!rerun_alone(
            module_path!(),
            "child_body_for_the_rerun_check"
        ));
    }

    #[test]
    fn child_body_that_fails_for_the_rerun_check() {
        if std::env::var_os(ENV).is_some_and(|running| {
            running
                .to_string_lossy()
                .ends_with("fails_for_the_rerun_check")
        }) {
            panic!("child failure marker");
        }
    }
}
