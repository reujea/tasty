//! 메모리 저장소의 권한·quota·만료·SQL 설정 시험.

mod expired_rows;

use super::*;

fn store() -> MemoryStore {
    MemoryStore::open_in_memory().unwrap()
}

fn text(s: &str) -> MemoryValue {
    MemoryValue::Text(s.into())
}

const PLUGIN_A: &str = "com.tasty.a";
const PLUGIN_B: &str = "com.tasty.b";

// ---- Regular ----

#[test]
fn put_get_delete_roundtrip() {
    let mut s = store();
    let scope = Scope::Surface(1);

    let v1 = s
        .put(PLUGIN_A, &scope, "a", &text("hello"), &PutOpts::default())
        .unwrap();
    assert_eq!(v1, 1);

    let entry = s.get(&scope, "a").unwrap().unwrap();
    assert_eq!(entry.value, text("hello"));
    assert_eq!(entry.version, 1);
    assert_eq!(entry.owner.as_deref(), Some(PLUGIN_A));

    let v2 = s
        .put(PLUGIN_A, &scope, "a", &text("world"), &PutOpts::default())
        .unwrap();
    assert_eq!(v2, 2);

    s.delete(PLUGIN_A, &scope, "a", None).unwrap();
    assert!(s.get(&scope, "a").unwrap().is_none());
}

#[test]
fn scopes_are_isolated() {
    let mut s = store();
    s.put(
        HOST_OWNER,
        &Scope::Surface(1),
        "k",
        &text("s1"),
        &PutOpts::default(),
    )
    .unwrap();
    s.put(
        HOST_OWNER,
        &Scope::Surface(2),
        "k",
        &text("s2"),
        &PutOpts::default(),
    )
    .unwrap();
    s.put(
        HOST_OWNER,
        &Scope::Global,
        "k",
        &text("g"),
        &PutOpts::default(),
    )
    .unwrap();
    assert_eq!(
        s.get(&Scope::Surface(1), "k").unwrap().unwrap().value,
        text("s1")
    );
    assert_eq!(
        s.get(&Scope::Surface(2), "k").unwrap().unwrap().value,
        text("s2")
    );
    assert_eq!(
        s.get(&Scope::Global, "k").unwrap().unwrap().value,
        text("g")
    );
}

#[test]
fn cas_conflict_blocks_update() {
    let mut s = store();
    let scope = Scope::Workspace(1);
    s.put(PLUGIN_A, &scope, "k", &text("v1"), &PutOpts::default())
        .unwrap();

    let err = s
        .put(
            PLUGIN_A,
            &scope,
            "k",
            &text("v2"),
            &PutOpts {
                cas: Some(99),
                ..Default::default()
            },
        )
        .unwrap_err();
    assert!(matches!(
        err,
        MemoryError::CasConflict {
            actual: 1,
            expected: 99
        }
    ));

    s.put(
        PLUGIN_A,
        &scope,
        "k",
        &text("v2"),
        &PutOpts {
            cas: Some(1),
            ..Default::default()
        },
    )
    .unwrap();
}

#[test]
fn regular_owned_by_other_on_update() {
    let mut s = store();
    let scope = Scope::Global;
    s.put(PLUGIN_A, &scope, "k", &text("v1"), &PutOpts::default())
        .unwrap();

    let err = s
        .put(PLUGIN_B, &scope, "k", &text("v2"), &PutOpts::default())
        .unwrap_err();
    let owner = match err {
        MemoryError::OwnedByOther { owner } => owner,
        other => panic!("expected OwnedByOther, got {other:?}"),
    };
    assert_eq!(owner, PLUGIN_A);

    s.put(PLUGIN_A, &scope, "k", &text("v3"), &PutOpts::default())
        .unwrap();
    s.put(HOST_OWNER, &scope, "k", &text("v4"), &PutOpts::default())
        .unwrap();
}

#[test]
fn regular_owned_by_other_on_delete() {
    let mut s = store();
    let scope = Scope::Global;
    s.put(PLUGIN_A, &scope, "k", &text("v1"), &PutOpts::default())
        .unwrap();

    let err = s.delete(PLUGIN_B, &scope, "k", None).unwrap_err();
    assert!(matches!(err, MemoryError::OwnedByOther { .. }));

    s.delete(HOST_OWNER, &scope, "k", None).unwrap();
    assert!(s.get(&scope, "k").unwrap().is_none());
}

#[test]
fn read_is_shared_across_callers() {
    let mut s = store();
    let scope = Scope::Global;
    s.put(PLUGIN_A, &scope, "k", &text("v"), &PutOpts::default())
        .unwrap();
    let entry = s.get(&scope, "k").unwrap().unwrap();
    assert_eq!(entry.owner.as_deref(), Some(PLUGIN_A));

    let list = s.list(&scope, &ListOpts::default()).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].owner.as_deref(), Some(PLUGIN_A));
}

/// 저장소는 기존 항목의 owner만 보호한다. 호스트 예약 namespace의 새 키 생성 제한은
/// IPC 핸들러가 담당하며, 저장소만 직접 부르면 그런 키도 만들 수 있다.
#[test]
fn ownership_does_not_reserve_a_key_namespace() {
    let mut s = store();
    let scope = Scope::Global;
    s.put(
        HOST_OWNER,
        &scope,
        "tasty.audit.0001",
        &text("deny"),
        &PutOpts::default(),
    )
    .unwrap();

    assert!(matches!(
        s.put(
            PLUGIN_A,
            &scope,
            "tasty.audit.0001",
            &text("x"),
            &PutOpts::default()
        ),
        Err(MemoryError::OwnedByOther { .. })
    ));
    assert!(matches!(
        s.delete(PLUGIN_A, &scope, "tasty.audit.0001", None),
        Err(MemoryError::OwnedByOther { .. })
    ));

    s.put(
        PLUGIN_A,
        &scope,
        "tasty.audit.9999",
        &text("forged"),
        &PutOpts::default(),
    )
    .unwrap();
    let rows = s
        .list(
            &scope,
            &ListOpts {
                prefix: Some("tasty.audit.".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(rows.len(), 2, "심은 행이 호스트 조회에 섞이지 않는다");
    assert!(
        rows.iter().any(|e| e.owner.as_deref() == Some(PLUGIN_A)),
        "심은 행의 owner 가 plugin 인데도 같은 조회에 함께 나온다"
    );
}

#[test]
fn expired_keys_treated_as_missing() {
    let mut s = store();
    let scope = Scope::Surface(1);
    let past = unix_ms_now() - 1000;
    s.put(
        HOST_OWNER,
        &scope,
        "k",
        &text("v"),
        &PutOpts {
            expires_at: Some(past),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(s.get(&scope, "k").unwrap().is_none());
    assert_eq!(s.count(&scope, None).unwrap(), 0);
    assert!(s.list(&scope, &ListOpts::default()).unwrap().is_empty());
}

#[test]
fn value_size_cap_enforced() {
    let mut s = store();
    let cap = s.config().entry_max_bytes as usize;
    let big = vec![0u8; cap + 1];
    let err = s
        .put(
            HOST_OWNER,
            &Scope::Global,
            "k",
            &MemoryValue::Binary(big),
            &PutOpts::default(),
        )
        .unwrap_err();
    assert!(matches!(err, MemoryError::ValueTooLarge { .. }));
}

#[test]
fn entry_max_configurable() {
    let mut s = MemoryStore::open_in_memory_with_config(MemoryConfig {
        entry_max_bytes: 16,
        ..MemoryConfig::default()
    })
    .unwrap();
    s.put(
        HOST_OWNER,
        &Scope::Global,
        "k",
        &text("0123456789ab"),
        &PutOpts::default(),
    )
    .unwrap();
    let err = s
        .put(
            HOST_OWNER,
            &Scope::Global,
            "big",
            &text("0123456789abcdefghij"),
            &PutOpts::default(),
        )
        .unwrap_err();
    assert!(matches!(err, MemoryError::ValueTooLarge { actual, max } if max == 16 && actual == 20));
}

#[test]
fn regular_quota_exceeded() {
    let mut s = MemoryStore::open_in_memory_with_config(MemoryConfig {
        entry_max_bytes: 1024,
        regular_quota_total_bytes: 20,
        ..MemoryConfig::default()
    })
    .unwrap();
    // 12 byte 저장: 합산 12 ≤ 20 통과.
    s.put(
        HOST_OWNER,
        &Scope::Global,
        "a",
        &text("0123456789ab"),
        &PutOpts::default(),
    )
    .unwrap();
    // 새 entry 12 byte 추가 시 projected=24 → 거부.
    let err = s
        .put(
            HOST_OWNER,
            &Scope::Global,
            "b",
            &text("0123456789ab"),
            &PutOpts::default(),
        )
        .unwrap_err();
    match err {
        MemoryError::QuotaExceeded { area, used, limit } => {
            assert_eq!(area, MemoryArea::Regular);
            assert_eq!(used, 24);
            assert_eq!(limit, 20);
        }
        other => panic!("expected QuotaExceeded, got {other:?}"),
    }
    // 기존 entry 의 in-place 갱신은 existing_size 만큼 차감 후 평가 → 같은 크기면 통과.
    s.put(
        HOST_OWNER,
        &Scope::Global,
        "a",
        &text("ABCDEFGHIJKL"),
        &PutOpts::default(),
    )
    .unwrap();
}

/// 모든 변경 뒤 Regular 바이트 캐시를 실제 SUM(LENGTH(value))와 대조한다.
#[test]
fn regular_used_bytes_stays_consistent() {
    let mut s = store();
    fn assert_consistent(s: &MemoryStore) {
        assert_eq!(
            s.regular_used_bytes,
            MemoryStore::scan_regular_used(&s.conn),
            "regular_used_bytes drifted from actual SUM(LENGTH(value))"
        );
    }
    assert_consistent(&s);

    s.put(
        HOST_OWNER,
        &Scope::Global,
        "a",
        &text("hello"),
        &PutOpts::default(),
    )
    .unwrap();
    assert_consistent(&s);

    s.put(
        HOST_OWNER,
        &Scope::Global,
        "a",
        &text("hello world"),
        &PutOpts::default(),
    )
    .unwrap();
    assert_consistent(&s);

    s.put(
        HOST_OWNER,
        &Scope::Global,
        "a",
        &text("x"),
        &PutOpts::default(),
    )
    .unwrap();
    assert_consistent(&s);

    s.put(
        HOST_OWNER,
        &Scope::Surface(1),
        "b",
        &text("0123456789"),
        &PutOpts::default(),
    )
    .unwrap();
    assert_consistent(&s);

    s.delete(HOST_OWNER, &Scope::Global, "a", None).unwrap();
    assert_consistent(&s);

    s.purge_scope(&Scope::Surface(1)).unwrap();
    assert_consistent(&s);
    assert_eq!(s.regular_used_bytes, 0, "all entries removed → 0");
}

/// quota 거부 시 카운터가 변하지 않아야 한다 (commit 이전 return).
#[test]
fn regular_used_bytes_unchanged_on_quota_reject() {
    let mut s = MemoryStore::open_in_memory_with_config(MemoryConfig {
        entry_max_bytes: 1024,
        regular_quota_total_bytes: 20,
        ..MemoryConfig::default()
    })
    .unwrap();
    s.put(
        HOST_OWNER,
        &Scope::Global,
        "a",
        &text("0123456789ab"),
        &PutOpts::default(),
    )
    .unwrap();
    let before = s.regular_used_bytes;
    let err = s.put(
        HOST_OWNER,
        &Scope::Global,
        "b",
        &text("0123456789ab"),
        &PutOpts::default(),
    );
    assert!(matches!(err, Err(MemoryError::QuotaExceeded { .. })));
    assert_eq!(
        s.regular_used_bytes, before,
        "counter must not change on rejected put"
    );
    assert_eq!(
        s.regular_used_bytes,
        MemoryStore::scan_regular_used(&s.conn)
    );
}

/// count 기반 retention: 최근 N 개만 남기고 prefix 매칭 로그를 삭제. 비매칭 키는 보존,
/// 카운터 정합 유지, N 이하면 no-op.
#[test]
fn prune_prefix_keep_recent_caps_logs() {
    let mut s = store();
    // 시간순 정렬되는 zero-padded 키 (audit/telemetry 와 동형).
    for i in 0..10 {
        s.put(
            HOST_OWNER,
            &Scope::Global,
            &format!("tasty.audit.{i:04}"),
            &text("x"),
            &PutOpts::default(),
        )
        .unwrap();
    }
    // 비-로그 키는 prune 영향 없어야 함.
    s.put(
        HOST_OWNER,
        &Scope::Global,
        "real.data",
        &text("keep"),
        &PutOpts::default(),
    )
    .unwrap();

    // 최근 3개만 유지 → 7개 삭제.
    let deleted = s.prune_prefix_keep_recent("tasty.audit.", 3).unwrap();
    assert_eq!(deleted, 7);
    assert!(s.get(&Scope::Global, "tasty.audit.0009").unwrap().is_some());
    assert!(s.get(&Scope::Global, "tasty.audit.0007").unwrap().is_some());
    assert!(s.get(&Scope::Global, "tasty.audit.0006").unwrap().is_none());
    assert!(s.get(&Scope::Global, "tasty.audit.0000").unwrap().is_none());
    assert!(s.get(&Scope::Global, "real.data").unwrap().is_some());
    assert_eq!(
        s.regular_used_bytes,
        MemoryStore::scan_regular_used(&s.conn)
    );

    // 남은 개수(3) 이하 cap → no-op.
    let deleted2 = s.prune_prefix_keep_recent("tasty.audit.", 5).unwrap();
    assert_eq!(deleted2, 0);

    // 사실상 무제한 cap 도 no-op 이어야 한다 — i64 로 좁힐 때 음수가 되면 OFFSET 이
    // 무효가 되어 전량 삭제로 돌변한다.
    assert_eq!(
        s.prune_prefix_keep_recent("tasty.audit.", u64::MAX)
            .unwrap(),
        0
    );
    assert!(s.get(&Scope::Global, "tasty.audit.0009").unwrap().is_some());
}

#[test]
fn prune_prefix_older_than_cuts_by_timestamp_in_key() {
    let mut s = store();
    // audit/telemetry 와 동형인 `{prefix}{ts:013}.{seq:04}` 키.
    for ts in [1_000u64, 5_000, 9_000] {
        s.put(
            HOST_OWNER,
            &Scope::Global,
            &format!("tasty.audit.{ts:013}.0000"),
            &text("x"),
            &PutOpts::default(),
        )
        .unwrap();
    }
    s.put(
        HOST_OWNER,
        &Scope::Global,
        "tasty.telemetry.event.0000000001000.0000",
        &text("x"),
        &PutOpts::default(),
    )
    .unwrap();

    assert_eq!(s.prune_prefix_older_than("tasty.audit.", 5_000).unwrap(), 1);
    assert!(
        s.get(&Scope::Global, "tasty.audit.0000000005000.0000")
            .unwrap()
            .is_some(),
        "cutoff 와 같은 ts 는 만료가 아니다"
    );
    assert!(
        s.get(&Scope::Global, "tasty.audit.0000000001000.0000")
            .unwrap()
            .is_none()
    );
    assert!(
        s.get(&Scope::Global, "tasty.telemetry.event.0000000001000.0000")
            .unwrap()
            .is_some()
    );
    assert_eq!(
        s.regular_used_bytes,
        MemoryStore::scan_regular_used(&s.conn)
    );
    // cutoff 0 = 아무것도 만료되지 않음.
    assert_eq!(s.prune_prefix_older_than("tasty.audit.", 0).unwrap(), 0);
}

#[test]
fn secret_quota_exceeded_per_owner() {
    // entry 1개 = 12 byte. 한도 20 이면 1 통과, 2 거부.
    let mut s = MemoryStore::open_in_memory_with_config(MemoryConfig {
        entry_max_bytes: 1024,
        secret_quota_per_owner_bytes: 20,
        ..MemoryConfig::default()
    })
    .unwrap();
    s.put_secret(
        PLUGIN_A,
        &Scope::Global,
        "a",
        &text("0123456789ab"),
        &PutOpts::default(),
    )
    .unwrap();
    let err = s
        .put_secret(
            PLUGIN_A,
            &Scope::Global,
            "b",
            &text("0123456789ab"),
            &PutOpts::default(),
        )
        .unwrap_err();
    assert!(matches!(
        err,
        MemoryError::QuotaExceeded {
            area: MemoryArea::Secret,
            ..
        }
    ));
    s.put_secret(
        PLUGIN_B,
        &Scope::Global,
        "a",
        &text("0123456789ab"),
        &PutOpts::default(),
    )
    .unwrap();
}

#[test]
fn invalid_key_rejected() {
    let mut s = store();
    let err = s
        .put(
            HOST_OWNER,
            &Scope::Global,
            "BAD",
            &text("x"),
            &PutOpts::default(),
        )
        .unwrap_err();
    assert!(matches!(err, MemoryError::InvalidKey(_)));
}

#[test]
fn invalid_owner_rejected() {
    let mut s = store();
    let err = s
        .put("", &Scope::Global, "k", &text("x"), &PutOpts::default())
        .unwrap_err();
    assert!(matches!(err, MemoryError::InvalidOwner(_)));
}

#[test]
fn delete_missing_returns_not_found() {
    let mut s = store();
    let err = s
        .delete(HOST_OWNER, &Scope::Global, "ghost", None)
        .unwrap_err();
    assert!(matches!(err, MemoryError::NotFound { .. }));
}

#[test]
fn list_prefix_and_limit() {
    let mut s = store();
    let scope = Scope::Surface(1);
    for k in ["a.1", "a.2", "b.1", "b.2", "c.1"] {
        s.put(HOST_OWNER, &scope, k, &text(k), &PutOpts::default())
            .unwrap();
    }
    let all = s.list(&scope, &ListOpts::default()).unwrap();
    assert_eq!(all.len(), 5);
    let a_only = s
        .list(
            &scope,
            &ListOpts {
                prefix: Some("a.".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        a_only.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(),
        vec!["a.1", "a.2"]
    );
    let limited = s
        .list(
            &scope,
            &ListOpts {
                limit: Some(2),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(limited.len(), 2);
    assert_eq!(s.count(&scope, None).unwrap(), 5);
    assert_eq!(s.count(&scope, Some("b.")).unwrap(), 2);
}

// ---- Secret ----

#[test]
fn secret_isolated_between_owners() {
    let mut s = store();
    let scope = Scope::Global;
    s.put_secret(
        PLUGIN_A,
        &scope,
        "tok",
        &text("A-token"),
        &PutOpts::default(),
    )
    .unwrap();
    s.put_secret(
        PLUGIN_B,
        &scope,
        "tok",
        &text("B-token"),
        &PutOpts::default(),
    )
    .unwrap();

    // 같은 (scope, key)지만 owner별로 분리 — Plugin A는 자기 값만 본다.
    let a = s.get_secret(PLUGIN_A, &scope, "tok").unwrap().unwrap();
    assert_eq!(a.value, text("A-token"));
    assert!(a.owner.is_none(), "secret 응답에는 owner 노출 금지");

    let b = s.get_secret(PLUGIN_B, &scope, "tok").unwrap().unwrap();
    assert_eq!(b.value, text("B-token"));

    let list_a = s
        .list_secret(PLUGIN_A, &scope, &ListOpts::default())
        .unwrap();
    assert_eq!(list_a.len(), 1);

    let scopes_a = s.scopes_secret(PLUGIN_A).unwrap();
    assert_eq!(scopes_a, vec!["global"]);
}

#[test]
fn secret_delete_only_affects_owner() {
    let mut s = store();
    let scope = Scope::Workspace(1);
    s.put_secret(PLUGIN_A, &scope, "tok", &text("A"), &PutOpts::default())
        .unwrap();
    s.put_secret(PLUGIN_B, &scope, "tok", &text("B"), &PutOpts::default())
        .unwrap();
    s.delete_secret(PLUGIN_A, &scope, "tok", None).unwrap();
    assert!(s.get_secret(PLUGIN_A, &scope, "tok").unwrap().is_none());
    assert!(s.get_secret(PLUGIN_B, &scope, "tok").unwrap().is_some());
}

#[test]
fn secret_cas_and_versioning() {
    let mut s = store();
    let scope = Scope::Global;
    let v1 = s
        .put_secret(PLUGIN_A, &scope, "k", &text("v1"), &PutOpts::default())
        .unwrap();
    assert_eq!(v1, 1);
    let v2 = s
        .put_secret(
            PLUGIN_A,
            &scope,
            "k",
            &text("v2"),
            &PutOpts {
                cas: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(v2, 2);
    let err = s
        .put_secret(
            PLUGIN_A,
            &scope,
            "k",
            &text("v3"),
            &PutOpts {
                cas: Some(99),
                ..Default::default()
            },
        )
        .unwrap_err();
    assert!(matches!(err, MemoryError::CasConflict { .. }));
}

#[test]
fn secret_stats_per_owner() {
    let mut s = store();
    s.put_secret(
        PLUGIN_A,
        &Scope::Global,
        "a",
        &text("xx"),
        &PutOpts::default(),
    )
    .unwrap();
    s.put_secret(
        PLUGIN_B,
        &Scope::Global,
        "a",
        &text("zzzz"),
        &PutOpts::default(),
    )
    .unwrap();

    let a = s.stats_secret(PLUGIN_A, None).unwrap();
    assert_eq!(a.entries, 1);
    assert_eq!(a.bytes, 2);

    let b = s.stats_secret(PLUGIN_B, None).unwrap();
    assert_eq!(b.entries, 1);
    assert_eq!(b.bytes, 4);
}

/// Secret value 는 평문 BLOB 으로 저장된다. 보호는 IPC owner 분리까지만.
/// DB 파일을 직접 여는 행위자는 secret 을 평문으로 본다.
#[test]
fn secret_at_rest_is_plaintext() {
    let mut s = store();
    let secret = "hunter2-supersecret-token-zzz";
    s.put_secret(
        PLUGIN_A,
        &Scope::Global,
        "tok",
        &text(secret),
        &PutOpts::default(),
    )
    .unwrap();
    let blob: Vec<u8> = s
        .conn
        .query_row(
            "SELECT value FROM memory_secret WHERE owner=?1 AND scope='global' AND key='tok'",
            params![PLUGIN_A],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(blob, secret.as_bytes());
}

// ---- GC ----

#[test]
fn purge_expired_removes_only_expired_rows() {
    let mut s = store();
    let scope = Scope::Workspace(1);

    s.put(
        PLUGIN_A,
        &scope,
        "permanent",
        &text("p"),
        &PutOpts::default(),
    )
    .unwrap();
    s.put(
        PLUGIN_A,
        &scope,
        "expired_reg",
        &text("r"),
        &PutOpts {
            expires_at: Some(1),
            cas: None,
        },
    )
    .unwrap();
    s.put_secret(
        PLUGIN_B,
        &scope,
        "expired_sec",
        &text("s"),
        &PutOpts {
            expires_at: Some(1),
            cas: None,
        },
    )
    .unwrap();

    assert!(s.get(&scope, "expired_reg").unwrap().is_none());
    assert!(
        s.get_secret(PLUGIN_B, &scope, "expired_sec")
            .unwrap()
            .is_none()
    );

    // purge 전에는 row 가 디스크에 남아 있다
    let count_before: i64 = s
        .conn
        .query_row("SELECT COUNT(*) FROM memory", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count_before, 2);

    let stats = s.purge_expired().unwrap();
    assert_eq!(stats.regular, 1);
    assert_eq!(stats.secret, 1);

    let count_after: i64 = s
        .conn
        .query_row("SELECT COUNT(*) FROM memory", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count_after, 1, "permanent entry must remain");
    let sec_after: i64 = s
        .conn
        .query_row("SELECT COUNT(*) FROM memory_secret", [], |r| r.get(0))
        .unwrap();
    assert_eq!(sec_after, 0);
}

#[test]
fn purge_scope_clears_both_areas_for_that_scope_only() {
    let mut s = store();
    let target = Scope::Surface(7);
    let other = Scope::Surface(8);

    s.put(PLUGIN_A, &target, "a", &text("x"), &PutOpts::default())
        .unwrap();
    s.put_secret(PLUGIN_A, &target, "sa", &text("y"), &PutOpts::default())
        .unwrap();
    s.put_secret(PLUGIN_B, &target, "sb", &text("z"), &PutOpts::default())
        .unwrap();

    s.put(PLUGIN_A, &other, "keep", &text("k"), &PutOpts::default())
        .unwrap();

    let stats = s.purge_scope(&target).unwrap();
    assert_eq!(stats.regular, 1);
    assert_eq!(stats.secret, 2);

    assert!(s.get(&target, "a").unwrap().is_none());
    assert!(s.get_secret(PLUGIN_A, &target, "sa").unwrap().is_none());
    assert!(s.get_secret(PLUGIN_B, &target, "sb").unwrap().is_none());
    assert!(s.get(&other, "keep").unwrap().is_some());
}

// ---- Change events ----

#[test]
fn put_records_created_then_updated_change() {
    let mut s = store();
    let scope = Scope::Workspace(3);
    let _ = s.take_pending_changes();

    let v1 = s
        .put(PLUGIN_A, &scope, "k", &text("v1"), &PutOpts::default())
        .unwrap();
    let changes = s.take_pending_changes();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].kind, MemoryChangeKind::Created);
    assert_eq!(changes[0].key, "k");
    assert_eq!(changes[0].scope, scope.as_token());
    assert_eq!(changes[0].version, Some(v1));

    let v2 = s
        .put(PLUGIN_A, &scope, "k", &text("v2"), &PutOpts::default())
        .unwrap();
    let changes = s.take_pending_changes();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].kind, MemoryChangeKind::Updated);
    assert_eq!(changes[0].version, Some(v2));

    assert!(s.take_pending_changes().is_empty());
}

#[test]
fn delete_records_deleted_change() {
    let mut s = store();
    let scope = Scope::Workspace(3);
    s.put(PLUGIN_A, &scope, "k", &text("v"), &PutOpts::default())
        .unwrap();
    let _ = s.take_pending_changes();

    s.delete(PLUGIN_A, &scope, "k", None).unwrap();
    let changes = s.take_pending_changes();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].kind, MemoryChangeKind::Deleted);
    assert_eq!(changes[0].key, "k");
    assert!(changes[0].version.is_none());
}

#[test]
fn secret_change_does_not_emit_event() {
    let mut s = store();
    let scope = Scope::Workspace(3);
    let _ = s.take_pending_changes();

    s.put_secret(PLUGIN_A, &scope, "sk", &text("v"), &PutOpts::default())
        .unwrap();
    s.delete_secret(PLUGIN_A, &scope, "sk", None).unwrap();
    assert!(
        s.take_pending_changes().is_empty(),
        "secret changes must not be broadcast"
    );
}

#[test]
fn purge_expired_records_expired_changes_for_regular_only() {
    let mut s = store();
    let scope = Scope::Workspace(3);
    s.put(
        PLUGIN_A,
        &scope,
        "exp_r",
        &text("r"),
        &PutOpts {
            expires_at: Some(1),
            cas: None,
        },
    )
    .unwrap();
    s.put_secret(
        PLUGIN_A,
        &scope,
        "exp_s",
        &text("s"),
        &PutOpts {
            expires_at: Some(1),
            cas: None,
        },
    )
    .unwrap();
    let _ = s.take_pending_changes();

    let stats = s.purge_expired().unwrap();
    assert_eq!(stats.regular, 1);
    assert_eq!(stats.secret, 1);
    let changes = s.take_pending_changes();
    assert_eq!(changes.len(), 1, "only regular expired key emits event");
    assert_eq!(changes[0].kind, MemoryChangeKind::Expired);
    assert_eq!(changes[0].key, "exp_r");
}

// ---- Pagination + query + export/import ----

#[test]
fn list_supports_offset_limit_since_until() {
    let mut s = store();
    let scope = Scope::Workspace(9);
    for k in ["a", "b", "c", "d", "e"] {
        s.put(PLUGIN_A, &scope, k, &text(k), &PutOpts::default())
            .unwrap();
    }
    let all = s.list(&scope, &ListOpts::default()).unwrap();
    assert_eq!(
        all.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(),
        ["a", "b", "c", "d", "e"]
    );

    let page = s
        .list(
            &scope,
            &ListOpts {
                offset: Some(1),
                limit: Some(2),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(
        page.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(),
        ["b", "c"]
    );

    // since / until — 모두 같은 updated_at 이라 since=now+1 이면 0개
    let now = unix_ms_now();
    let future = s
        .list(
            &scope,
            &ListOpts {
                since: Some(now + 60_000),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(future.is_empty());
}

#[test]
fn query_filters_by_dot_path_equality() {
    let mut s = store();
    let scope = Scope::Workspace(11);
    let make = |status: &str| {
        MemoryValue::Json(serde_json::json!({
            "task": { "status": status, "id": 1 }
        }))
    };
    s.put(PLUGIN_A, &scope, "t.1", &make("open"), &PutOpts::default())
        .unwrap();
    s.put(
        PLUGIN_A,
        &scope,
        "t.2",
        &make("closed"),
        &PutOpts::default(),
    )
    .unwrap();
    s.put(PLUGIN_A, &scope, "t.3", &make("open"), &PutOpts::default())
        .unwrap();
    s.put(
        PLUGIN_A,
        &scope,
        "x",
        &text("not-json"),
        &PutOpts::default(),
    )
    .unwrap();

    let hits = s
        .query(
            &scope,
            "task.status",
            &serde_json::json!("open"),
            &ListOpts::default(),
        )
        .unwrap();
    let mut keys: Vec<&str> = hits.iter().map(|e| e.key.as_str()).collect();
    keys.sort();
    assert_eq!(keys, ["t.1", "t.3"]);

    let none = s
        .query(
            &scope,
            "task.nope",
            &serde_json::json!("open"),
            &ListOpts::default(),
        )
        .unwrap();
    assert!(none.is_empty());
}

#[test]
fn export_and_import_roundtrip() {
    let mut s = store();
    let ws = Scope::Workspace(20);
    let sf = Scope::Surface(20);
    s.put(PLUGIN_A, &ws, "alpha", &text("a"), &PutOpts::default())
        .unwrap();
    s.put(
        PLUGIN_A,
        &sf,
        "beta",
        &MemoryValue::Json(serde_json::json!({"v":1})),
        &PutOpts::default(),
    )
    .unwrap();
    s.put_secret(
        PLUGIN_A,
        &ws,
        "secret_k",
        &text("hidden"),
        &PutOpts::default(),
    )
    .unwrap();

    let exported = s.export_regular(None).unwrap();
    assert_eq!(exported.len(), 2);
    assert!(exported.iter().all(|e| e.scope != "secret"));

    let mut s2 = store();
    let stats = s2.import_regular(HOST_OWNER, &exported, false).unwrap();
    assert_eq!(stats.applied, 2);
    assert_eq!(stats.skipped, 0);

    assert!(s2.get(&ws, "alpha").unwrap().is_some());
    assert!(s2.get(&sf, "beta").unwrap().is_some());

    let stats = s2.import_regular(HOST_OWNER, &exported, false).unwrap();
    assert_eq!(stats.applied, 0);
    assert_eq!(stats.skipped, 2);

    let stats = s2.import_regular(HOST_OWNER, &exported, true).unwrap();
    assert_eq!(stats.applied, 2);
    assert_eq!(stats.skipped, 0);
}

#[test]
fn purge_scope_records_deleted_for_each_regular_key() {
    let mut s = store();
    let scope = Scope::Surface(11);
    s.put(PLUGIN_A, &scope, "a", &text("x"), &PutOpts::default())
        .unwrap();
    s.put(PLUGIN_A, &scope, "b", &text("y"), &PutOpts::default())
        .unwrap();
    s.put_secret(PLUGIN_A, &scope, "sa", &text("z"), &PutOpts::default())
        .unwrap();
    let _ = s.take_pending_changes();

    s.purge_scope(&scope).unwrap();
    let mut changes = s.take_pending_changes();
    changes.sort_by(|a, b| a.key.cmp(&b.key));
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0].kind, MemoryChangeKind::Deleted);
    assert_eq!(changes[0].key, "a");
    assert_eq!(changes[1].kind, MemoryChangeKind::Deleted);
    assert_eq!(changes[1].key, "b");
}

/// secret DELETE를 막는 trigger. regular DELETE 뒤 둘째 문장을 실패시킨다.
fn block_secret_delete(s: &MemoryStore) {
    s.conn
        .execute_batch(
            "CREATE TRIGGER block BEFORE DELETE ON memory_secret
             BEGIN SELECT RAISE(ABORT, 'blocked'); END;",
        )
        .unwrap();
}

#[test]
fn a_failed_secret_delete_rolls_back_the_whole_scope_purge() {
    let mut s = store();
    let scope = Scope::Surface(7);
    s.put(
        HOST_OWNER,
        &scope,
        "r",
        &text(&"x".repeat(100)),
        &PutOpts::default(),
    )
    .unwrap();
    s.put_secret(PLUGIN_A, &scope, "s", &text("y"), &PutOpts::default())
        .unwrap();
    s.take_pending_changes();
    block_secret_delete(&s);
    let commits = s.db_latency().snapshot().commits;

    assert!(s.purge_scope(&scope).is_err());
    assert!(
        s.get(&scope, "r").unwrap().is_some(),
        "regular 삭제가 롤백되지 않았다"
    );
    assert!(s.take_pending_changes().is_empty());
    assert_eq!(
        s.regular_used_bytes,
        MemoryStore::scan_regular_used(&s.conn)
    );
    assert!(s.regular_used_bytes > 0);
    assert_eq!(s.db_latency().snapshot().commits, commits);
}

#[test]
fn a_failed_secret_delete_rolls_back_the_whole_expired_purge() {
    let mut s = store();
    let scope = Scope::Workspace(4);
    let expired = PutOpts {
        expires_at: Some(1),
        cas: None,
    };
    s.put(HOST_OWNER, &scope, "r", &text(&"x".repeat(100)), &expired)
        .unwrap();
    s.put_secret(PLUGIN_A, &scope, "s", &text("y"), &expired)
        .unwrap();
    s.take_pending_changes();
    block_secret_delete(&s);
    let commits = s.db_latency().snapshot().commits;

    assert!(s.purge_expired().is_err());
    let left: i64 = s
        .conn
        .query_row("SELECT COUNT(*) FROM memory WHERE key='r'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(left, 1, "만료된 regular 행 삭제가 롤백되지 않았다");
    assert!(s.take_pending_changes().is_empty());
    assert_eq!(
        s.regular_used_bytes,
        MemoryStore::scan_regular_used(&s.conn)
    );
    assert!(s.regular_used_bytes > 0);
    assert_eq!(s.db_latency().snapshot().commits, commits);
}

#[test]
fn each_purge_and_prune_that_deletes_counts_one_commit() {
    let mut s = store();
    let commits = |s: &MemoryStore| s.db_latency().snapshot().commits;
    let scope = Scope::Surface(3);
    s.put(PLUGIN_A, &scope, "a", &text("x"), &PutOpts::default())
        .unwrap();
    s.put_secret(PLUGIN_A, &scope, "s", &text("y"), &PutOpts::default())
        .unwrap();
    let before = commits(&s);
    s.purge_scope(&scope).unwrap();
    assert_eq!(commits(&s) - before, 1, "purge_scope");

    let expired = PutOpts {
        expires_at: Some(1),
        cas: None,
    };
    s.put(PLUGIN_A, &scope, "e", &text("x"), &expired).unwrap();
    s.put_secret(PLUGIN_A, &scope, "es", &text("y"), &expired)
        .unwrap();
    let before = commits(&s);
    s.purge_expired().unwrap();
    assert_eq!(commits(&s) - before, 1, "purge_expired");

    for i in 0..3u64 {
        let key = format!("log.{i:013}");
        s.put(
            HOST_OWNER,
            &Scope::Global,
            &key,
            &text("l"),
            &PutOpts::default(),
        )
        .unwrap();
    }
    let before = commits(&s);
    assert_eq!(s.prune_prefix_keep_recent("log.", 2).unwrap(), 1);
    assert_eq!(commits(&s) - before, 1, "prune_prefix_keep_recent");
    let before = commits(&s);
    assert_eq!(s.prune_prefix_older_than("log.", 2).unwrap(), 1);
    assert_eq!(commits(&s) - before, 1, "prune_prefix_older_than");

    // 지운 행이 없는 정리는 확정된 쓰기가 없으므로 세지 않는다.
    let before = commits(&s);
    s.purge_scope(&Scope::Surface(99)).unwrap();
    s.purge_expired().unwrap();
    assert_eq!(s.prune_prefix_keep_recent("log.", 5).unwrap(), 0);
    assert_eq!(commits(&s), before, "nothing deleted");
}

// WAL 파일 회수 시험은 in-memory가 아닌 임시 파일 DB를 사용한다.

fn disk_store(dir: &std::path::Path) -> (MemoryStore, std::path::PathBuf) {
    let path = dir.join("memory.db");
    let s = MemoryStore::open(&path).unwrap();
    (s, path)
}

fn wal_len(db_path: &std::path::Path) -> u64 {
    let wal = db_path.with_extension("db-wal");
    std::fs::metadata(&wal).map(|m| m.len()).unwrap_or(0)
}

/// 적은 commit으로 WAL 파일을 충분히 키우기 위해 큰 값을 저장한다.
fn append_rows(s: &mut MemoryStore, scope: &Scope, from: usize, count: usize, bytes: usize) {
    let blob = "x".repeat(bytes);
    for i in from..from + count {
        s.put(
            PLUGIN_A,
            scope,
            &format!("k{i}"),
            &text(&blob),
            &PutOpts::default(),
        )
        .unwrap();
    }
}

#[test]
fn journal_size_limit_is_applied_to_disk_databases() {
    let tmp = tempfile::tempdir().unwrap();
    let (s, _path) = disk_store(tmp.path());
    let limit: i64 = s
        .conn
        .query_row("PRAGMA journal_size_limit", [], |r| r.get(0))
        .unwrap();
    assert_eq!(limit, WAL_SIZE_LIMIT_BYTES);

    // 선택한 상한을 실제 SQLite 기본 페이지 수·페이지 크기와 대조한다.
    let pages: i64 = s
        .conn
        .query_row("PRAGMA wal_autocheckpoint", [], |r| r.get(0))
        .unwrap();
    let page_size: i64 = s
        .conn
        .query_row("PRAGMA page_size", [], |r| r.get(0))
        .unwrap();
    assert_eq!(pages * page_size, WAL_SIZE_LIMIT_BYTES);
}

/// 실제 tracing 경고를 캡처한다.
///
/// 구독자 없는 다른 시험이 같은 콜사이트를 먼저 치면 프로세스 전역 관심 캐시에 `never`가 남아
/// 경고가 이 구독자까지 오지 않는다(docs/dev-guide/unit-test-isolation.md). 경고를 단정하는 시험은
/// 빠뜨린 경고로 실패하고, 빈 로그를 단정하는 시험은 있어야 할 실패를 놓친다. 그래서 호출자는
/// 모두 `rerun_alone`으로 혼자 돌아야 한다.
fn captured_log(body: impl FnOnce()) -> String {
    use std::io::Write;

    assert!(
        tasty_test_support::is_rerun_alone(),
        "로그 단정은 rerun_alone 으로 혼자 돌아야 한다"
    );

    #[derive(Clone)]
    struct Capture(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .expect("capture lock")
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let capture = Capture(std::sync::Arc::new(std::sync::Mutex::new(Vec::new())));
    let writer = capture.clone();
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    tracing::subscriber::with_default(subscriber, body);
    let bytes = capture.0.lock().expect("capture lock").clone();
    String::from_utf8(bytes).expect("UTF-8 log")
}

/// 읽기 전용 DB에서 적용되지 않은 설정이 경고로 남는지 확인한다.
#[test]
fn a_read_only_database_says_that_the_requested_pragmas_did_not_take() {
    if tasty_test_support::rerun_alone(
        module_path!(),
        "a_read_only_database_says_that_the_requested_pragmas_did_not_take",
    ) {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("readonly.db");
    // 먼저 평범하게 만든다 — 이 시점의 journal_mode 는 SQLite 기본값(delete)이다.
    rusqlite::Connection::open(&path).unwrap();

    let conn =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let log = captured_log(|| {
        crate::pragma::apply_connection_pragmas(&conn, &path);
    });

    assert!(
        log.contains("journal_mode is delete, not the requested WAL"),
        "요청과 다른 journal_mode 가 조용히 지나갔다:\n{log}"
    );
    assert!(log.contains("WARN"), "경고 수준이 아니다:\n{log}");
}

#[test]
fn a_healthy_database_logs_nothing_while_setting_its_pragmas() {
    if tasty_test_support::rerun_alone(
        module_path!(),
        "a_healthy_database_logs_nothing_while_setting_its_pragmas",
    ) {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("healthy.db");
    let conn = rusqlite::Connection::open(&path).unwrap();
    let log = captured_log(|| {
        crate::pragma::apply_connection_pragmas(&conn, &path);
    });
    assert!(log.is_empty(), "정상 열기에서 경고가 났다:\n{log}");
}

/// in-memory의 journal_mode=memory에는 경고하지 않는다.
#[test]
fn an_in_memory_database_does_not_warn_about_its_own_journal_mode() {
    if tasty_test_support::rerun_alone(
        module_path!(),
        "an_in_memory_database_does_not_warn_about_its_own_journal_mode",
    ) {
        return;
    }
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let log = captured_log(|| {
        crate::pragma::apply_connection_pragmas(&conn, std::path::Path::new(":memory:"));
    });
    assert!(log.is_empty(), "in-memory 정상 결과에 경고가 났다:\n{log}");
}

/// pragma 호출의 성공과 실제 적용값을 구분해 파일·in-memory 모드를 각각 확인한다.
#[test]
fn the_effective_journal_mode_differs_between_a_file_and_an_in_memory_database() {
    let tmp = tempfile::tempdir().unwrap();
    let (disk, _path) = disk_store(tmp.path());
    assert_eq!(
        crate::pragma::effective_journal_mode(&disk.conn).unwrap(),
        "wal",
        "파일 DB 가 요청한 WAL 로 안 섰다"
    );

    let mem = store();
    assert_eq!(
        crate::pragma::effective_journal_mode(&mem.conn).unwrap(),
        "memory",
        "in-memory DB 의 journal_mode 가 바뀌었다 — 이 모드의 허용 결과를 다시 정해야 한다"
    );
}

/// VACUUM으로 커진 WAL이 다음 재사용 때 줄어드는지 확인한다.
/// 일반 append만으로는 pragma가 없어도 파일 크기가 안정돼 이 차이를 검증할 수 없다.
#[test]
fn a_ballooned_wal_shrinks_back_under_the_limit() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut s, path) = disk_store(tmp.path());
    let scope = Scope::Surface(1);

    append_rows(&mut s, &scope, 0, 400, 64 * 1024);
    for i in 0..350 {
        s.delete(PLUGIN_A, &scope, &format!("k{i}"), None).unwrap();
    }
    // VACUUM 은 DB 를 통째로 다시 쓰므로 WAL 을 한도보다 훨씬 크게 부풀린다 —
    // 이 항목이 재현하려는 "한 번 커진 WAL" 의 실제 발생 경로 중 하나다.
    assert!(s.vacuum_if_fragmented(1).unwrap(), "VACUUM 이 돌지 않았다");
    let ballooned = wal_len(&path);
    assert!(
        ballooned > WAL_SIZE_LIMIT_BYTES as u64,
        "WAL 이 한도를 넘겨 부풀지 않아 회수를 검증할 수 없다: {ballooned}"
    );

    // 이후의 평범한 커밋 몇 개면 되감기가 일어나고, 그때 상한으로 잘린다.
    append_rows(&mut s, &scope, 1000, 5, 1024);
    let after = wal_len(&path);
    assert!(
        after <= WAL_SIZE_LIMIT_BYTES as u64,
        "부푼 WAL 이 회수되지 않고 고착됐다: {ballooned} -> {after}, limit={WAL_SIZE_LIMIT_BYTES}"
    );
}

/// 이미 커진 WAL 은 pragma 만으로는 즉시 줄지 않는다 — 부팅 시 되감기를 강제하는
/// 경로가 그것을 회수하고, 그 과정에서 레코드를 잃지 않는다.
#[test]
fn checkpoint_truncate_reclaims_the_wal_without_losing_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut s, path) = disk_store(tmp.path());
    let scope = Scope::Surface(1);
    append_rows(&mut s, &scope, 0, 300, 64 * 1024);

    let before: i64 = s
        .conn
        .query_row("SELECT COUNT(*) FROM memory", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, 300);
    assert!(wal_len(&path) > 0, "회수할 WAL 이 애초에 없다");

    assert!(s.checkpoint_truncate().unwrap(), "체크포인트가 busy 였다");

    assert_eq!(wal_len(&path), 0, "truncate 후에도 WAL 이 남아 있다");
    let after: i64 = s
        .conn
        .query_row("SELECT COUNT(*) FROM memory", [], |r| r.get(0))
        .unwrap();
    assert_eq!(after, before, "체크포인트가 레코드를 훼손했다");
    // 회수 후에도 정상적으로 읽힌다(본체로 흡수됐는지 값 수준에서 확인).
    assert!(s.get(&scope, "k0").unwrap().is_some());
    assert!(s.get(&scope, "k299").unwrap().is_some());
}

// ---- DB 지연 계측 ----

/// 거절된 쓰기는 commit 누계에 포함하지 않는다.
#[test]
fn only_a_write_that_committed_is_counted_as_a_commit() {
    let mut s = store();
    let gauge = s.db_latency();
    let scope = Scope::Surface(1);
    assert_eq!(gauge.snapshot().commits, 0, "열기만 해서는 commit 이 없다");

    s.put(PLUGIN_A, &scope, "a", &text("hello"), &PutOpts::default())
        .unwrap();
    assert_eq!(gauge.snapshot().commits, 1);

    // 없는 키에 cas 를 걸면 트랜잭션은 열리지만 commit 전에 돌아간다.
    let rejected = s.put(
        PLUGIN_A,
        &scope,
        "b",
        &text("nope"),
        &PutOpts {
            cas: Some(7),
            ..Default::default()
        },
    );
    assert!(rejected.is_err(), "cas 충돌이 거부되지 않았다");
    assert_eq!(
        gauge.snapshot().commits,
        1,
        "거부된 쓰기가 commit 으로 셌다"
    );

    s.delete(PLUGIN_A, &scope, "a", None).unwrap();
    assert_eq!(gauge.snapshot().commits, 2);
}

/// checkpoint를 commit과 별도로 집계한다.
#[test]
fn a_checkpoint_lands_in_its_own_population() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut s, _path) = disk_store(tmp.path());
    let gauge = s.db_latency();
    let scope = Scope::Surface(1);
    append_rows(&mut s, &scope, 0, 4, 1024);

    let before = gauge.snapshot();
    assert_eq!(before.checkpoints, 0);
    assert!(before.commits >= 4, "쓰기가 commit 으로 안 셌다");

    assert!(s.checkpoint_truncate().unwrap(), "체크포인트가 busy 였다");

    let after = gauge.snapshot();
    assert_eq!(after.checkpoints, 1);
    assert_eq!(after.checkpoints_busy, 0, "끝까지 갔는데 busy 로 셌다");
    assert_eq!(
        after.commits, before.commits,
        "checkpoint 가 commit 으로 셌다"
    );
}

/// 파일·in-memory DB가 각 모드의 허용값으로 적용됐는지 확인한다.
#[test]
fn an_open_store_carries_the_pragmas_that_took_in_each_mode() {
    let tmp = tempfile::tempdir().unwrap();
    let (disk, _path) = disk_store(tmp.path());
    let d = disk.applied_pragmas();
    assert!(!d.in_memory, "파일 DB 를 in-memory 로 판정했다");
    assert!(!d.degraded(), "정상 파일 DB 가 degraded 다: {d:?}");
    let j = d.get("journal_mode").unwrap();
    assert_eq!(j.requested, "WAL");
    assert_eq!(j.effective.as_deref(), Some("wal"));
    assert_eq!(
        d.get("synchronous").unwrap().effective.as_deref(),
        Some("NORMAL")
    );
    assert_eq!(
        d.get("foreign_keys").unwrap().effective.as_deref(),
        Some("ON")
    );
    assert_eq!(
        d.get("journal_size_limit").unwrap().effective.as_deref(),
        Some(WAL_SIZE_LIMIT_BYTES.to_string().as_str())
    );

    // 스토어가 들고 있는 값이 **지금 연결의 실제값**과 같은지 직접 대조한다.
    assert_eq!(
        j.effective.as_deref().unwrap(),
        crate::pragma::effective_journal_mode(&disk.conn).unwrap()
    );

    let mem = store();
    let m = mem.applied_pragmas();
    assert!(m.in_memory, "in-memory DB 를 파일로 판정했다");
    assert!(!m.degraded(), "in-memory 정상 결과가 degraded 다: {m:?}");
    assert_eq!(
        m.get("journal_mode").unwrap().effective.as_deref(),
        Some("memory")
    );
}

/// 설정 불일치는 경고와 degraded 상태에 모두 반영한다.
#[test]
fn a_read_only_database_reports_itself_degraded() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("readonly.db");
    rusqlite::Connection::open(&path).unwrap();
    let conn =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();

    let applied = crate::pragma::apply_connection_pragmas(&conn, &path);

    assert!(applied.degraded(), "안 선 요청이 degraded 로 안 나왔다");
    let j = applied.get("journal_mode").unwrap();
    assert!(!j.took);
    assert_eq!(j.effective.as_deref(), Some("delete"));
}

/// 파일 DB의 memory 모드를 in-memory의 정상값으로 잘못 허용하지 않는다.
#[test]
fn a_file_database_in_memory_journal_mode_is_degraded() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("m.db");
    let conn = rusqlite::Connection::open(&path).unwrap();
    // 파일 DB 에 직접 memory 모드를 건다 — 그 뒤 WAL 요청은 거절되지 않고 받아들여지므로,
    // 판정 함수만 떼어 대조한다.
    let mode: String = conn
        .query_row("PRAGMA journal_mode=MEMORY", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "memory");
    let applied = crate::pragma::apply_connection_pragmas(&conn, &path);
    assert!(!applied.in_memory);
    // WAL 로 바뀌었으면 정상이고, memory 로 남았으면 degraded 여야 한다 — 어느 쪽이든
    // "memory 인데 정상" 인 조합은 없어야 한다.
    let j = applied.get("journal_mode").unwrap();
    assert!(
        !(j.effective.as_deref() == Some("memory") && j.took),
        "파일 DB 의 memory 를 정상으로 봤다: {j:?}"
    );
}

/// DB 잠금으로 쓰기에 실패하면 busy로 분류하고 quota 캐시·변경 버퍼를 그대로 유지한다.
#[test]
fn a_write_to_a_locked_database_fails_as_busy_and_leaves_the_cache_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut s, path) = disk_store(tmp.path());
    let scope = Scope::Surface(1);
    s.put(PLUGIN_A, &scope, "before", &text("x"), &PutOpts::default())
        .unwrap();
    s.take_pending_changes();
    let used_before = s.regular_used_bytes;
    // 기본 busy_timeout(5 s)을 기다리지 않게 한다 — 판정하려는 것은 분류지 대기가 아니다.
    s.conn.busy_timeout(std::time::Duration::ZERO).unwrap();

    let holder = rusqlite::Connection::open(&path).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();

    let err = s
        .put(PLUGIN_A, &scope, "k", &text("value"), &PutOpts::default())
        .expect_err("잠긴 DB 에 쓰기가 성공으로 돌아왔다");
    assert_eq!(err.storage_failure(), Some(StorageFailure::Busy), "{err}");
    assert_eq!(
        s.regular_used_bytes, used_before,
        "실패한 쓰기가 카운터를 옮겼다"
    );
    assert!(
        s.take_pending_changes().is_empty(),
        "실패한 쓰기가 변경 알림을 남겼다"
    );

    holder.execute_batch("ROLLBACK").unwrap();
    assert!(
        s.get(&scope, "k").unwrap().is_none(),
        "실패한 쓰기가 디스크에 남았다"
    );
}

/// 용량이 찬 DB 에 쓰면 `disk_full` 로 갈린다. 볼륨을 채우는 대신 `max_page_count`
/// 로 DB 의 페이지 상한을 낮춘다 — SQLite 는 두 경우에 같은 `SQLITE_FULL` 을 낸다.
#[test]
fn a_write_past_the_page_ceiling_fails_as_disk_full_and_leaves_the_cache_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let (mut s, _path) = disk_store(tmp.path());
    let scope = Scope::Surface(1);
    let pages: i64 = s
        .conn
        .query_row("PRAGMA page_count", [], |r| r.get(0))
        .unwrap();
    s.conn
        .query_row(&format!("PRAGMA max_page_count={pages}"), [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap();
    let used_before = s.regular_used_bytes;

    let big = "x".repeat(64 * 1024);
    let err = s
        .put(PLUGIN_A, &scope, "big", &text(&big), &PutOpts::default())
        .expect_err("상한을 넘는 쓰기가 성공으로 돌아왔다");
    assert_eq!(
        err.storage_failure(),
        Some(StorageFailure::DiskFull),
        "{err}"
    );
    assert_eq!(s.regular_used_bytes, used_before);
    assert!(s.take_pending_changes().is_empty());
}

/// NotFound 같은 요청 거부는 저장 실패로 분류하지 않는다.
#[test]
fn a_refused_request_is_not_a_storage_failure() {
    let mut s = store();
    let err = s
        .delete(PLUGIN_A, &Scope::Surface(1), "missing", None)
        .unwrap_err();
    assert_eq!(err.storage_failure(), None);
}

/// 초기화와 저장이 같은 오류 분류를 사용한다.
#[test]
fn opening_a_file_that_is_not_a_database_is_classified_as_corrupt() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("memory.db");
    std::fs::write(&path, vec![0xAB; 8192]).unwrap();
    let err = match MemoryStore::open(&path) {
        Ok(_) => panic!("SQLite 가 아닌 파일이 열렸다"),
        Err(e) => e,
    };
    assert!(
        matches!(err, MemoryInitError::Corrupt(_)),
        "깨진 파일이 Corrupt 로 안 갔다: {err}"
    );
}
