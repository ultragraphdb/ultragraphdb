/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_client_core::{ClientErrorEnum, Reply, ServerScript};
use ultragraphdb_protocol_transport::NetworkFaults;
use ultragraphdb_protocol_wire::TermTable;

use crate::types::client_core::client_core_tests::counter;
use crate::utils::harness::{Harness, config, welcome};

/// Transactions until `commits` are acknowledged, each retried by the application on whatever failed but an
/// acknowledged commit: every acknowledged commit is applied once, whatever the network did.
fn workload(faults: NetworkFaults, seed: u64, commits: u64) -> (u64, u64) {
    let mut h = Harness::with(
        config(),
        welcome(),
        ServerScript {
            results: 2,
            ..ServerScript::default()
        },
        faults,
        seed,
    );
    let mut acknowledged = Vec::new();
    let mut session = None;
    for _ in 0..200 {
        if acknowledged.len() as u64 == commits {
            break;
        }
        if !h.core.is_ready() {
            h.connect();
            h.until(|h| h.core.is_ready() || !h.core.is_connected());
            if !h.core.is_ready() {
                continue;
            }
        }
        let s = match session {
            Some(s) => s,
            None => {
                let Ok(p) = h.core.register(h.now) else {
                    continue;
                };
                match h.settle(p) {
                    Ok(Reply::Registered(s)) => *session.insert(s),
                    _ => continue,
                }
            }
        };
        let Ok(p) = h.core.begin(0, false, h.now) else {
            continue;
        };
        let Ok(Reply::Begun(tx)) = h.settle(p) else {
            continue;
        };
        if h.core
            .operation(&tx, &counter(&mut TermTable::new()))
            .is_err()
        {
            continue;
        }
        let Ok(p) = h.core.commit(tx, s, h.now) else {
            continue;
        };
        match h.settle(p) {
            Ok(Reply::Committed(c)) => acknowledged.push(c.request()),
            Err(e) if matches!(e.0, ClientErrorEnum::RetriesExhausted { .. }) => {
                if let Ok(p) = h.core.resolve(s, h.now)
                    && let Ok(Reply::Committed(c)) = h.settle(p)
                {
                    acknowledged.push(c.request());
                }
            }
            Err(_) => {}
            Ok(other) => panic!("{other:?}"),
        }
    }
    let mut distinct = acknowledged.clone();
    distinct.dedup();
    assert_eq!(distinct, acknowledged, "no request acknowledged twice");
    assert_eq!(
        acknowledged,
        (1..=acknowledged.len() as u64).collect::<Vec<_>>()
    );
    (acknowledged.len() as u64, h.server.applied())
}

#[test]
fn test_each_acknowledged_commit_is_applied_once_under_every_fault() {
    let cases = [
        NetworkFaults::none(),
        NetworkFaults::none().with_split(500_000),
        NetworkFaults::none().with_delay(500_000, 5),
        NetworkFaults::none().with_duplicate(300_000),
        NetworkFaults::none().with_drop(50_000),
        NetworkFaults::none()
            .with_drop(30_000)
            .with_duplicate(100_000)
            .with_delay(200_000, 3)
            .with_split(200_000),
    ];
    for (i, faults) in cases.into_iter().enumerate() {
        for seed in 1..=4 {
            let (acknowledged, applied) = workload(faults, seed * 31 + i as u64, 12);
            assert_eq!(acknowledged, 12, "case {i} seed {seed}");
            assert_eq!(
                applied, acknowledged,
                "case {i} seed {seed}: applied once each"
            );
        }
    }
}
