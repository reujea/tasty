//! 설치된 플러그인의 IPC namespace 소유 상태 표.
//!
//! 플러그인 관리자가 설치 목록과 설정(비활성)으로 계산해 발행한다. 플러그인
//! 프로세스의 실행 여부와 무관하므로 프로세스가 준비되기 전에도 같은 답을 준다. 러너 스레드처럼
//! 플러그인 관리자에 접근할 수 없는 곳이 namespace 를 가진 플러그인의 자격을 판정할 때 읽는다.

use std::collections::BTreeMap;
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;

/// namespace 하나의 소유 플러그인 상태.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamespaceOwner {
    pub plugin_id: String,
    /// 사용자가 비활성화하지 않았다.
    pub enabled: bool,
}

/// namespace prefix → 소유 상태. 발행 전에는 `None`.
static TABLE: RwLock<Option<BTreeMap<String, NamespaceOwner>>> = RwLock::new(None);
static POISON_REPORTED: AtomicBool = AtomicBool::new(false);
const WHAT: &str = "the plugin namespace owner table";

/// 표 전체를 바꾼다. 이후 판정부터 적용된다.
pub fn publish(table: BTreeMap<String, NamespaceOwner>) {
    *crate::poison::recover_write(TABLE.write(), WHAT, &POISON_REPORTED) = Some(table);
}

/// prefix 조회 결과.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    /// 플러그인 관리자가 아직 표를 발행하지 않았다.
    NotPublished,
    /// 설치된 플러그인 중 이 prefix 를 소유한 것이 없다.
    NoOwner,
    Owner(NamespaceOwner),
}

pub fn lookup(prefix: &str) -> Lookup {
    let guard = crate::poison::recover_read(TABLE.read(), WHAT, &POISON_REPORTED);
    match guard.as_ref() {
        None => Lookup::NotPublished,
        Some(table) => match table.get(prefix) {
            None => Lookup::NoOwner,
            Some(owner) => Lookup::Owner(owner.clone()),
        },
    }
}

/// 현재 발행된 표의 사본. 발행 전에는 `None`.
pub fn snapshot() -> Option<BTreeMap<String, NamespaceOwner>> {
    crate::poison::recover_read(TABLE.read(), WHAT, &POISON_REPORTED).clone()
}
