/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A one-frame write's body, a DirectWrite's or a ConditionalWrite's, read alike (`PROTOCOL.md` §§6.6, 6.7), through what a
//! partition's core makes of it: its session and request in its reply, its tenant, its changes and its operations.

use ultragraphdb_core_keys::{Keyspace, key_prefix};
use ultragraphdb_protocol_wire::{
    CommitOperation, ConditionalWrite, DirectWrite, ErrorCode, OperationResult, ScanAt, TermTable,
    WriteChange,
};
use ultragraphdb_protocol_wire::{RoundHeader, RoundView};
use ultragraphdb_store_quads::term_key;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_traits_log::{OrderedLog, Position};
use ultragraphdb_transactions_log::{CompioStorage, Wal, WalConfig};

use crate::types::partition_core::core_rig::{
    CLUSTER, PRINCIPAL, RigSetup, TENANT, with_core, with_core_then,
};
use crate::types::partition_core::core_writes::{
    alone, declare_unique, edges, ex, one_frame, plain, scanned,
};

/// The session of the writes that name one.
const SESSION: [u8; 16] = [0x5e; 16];
/// A tenant beside the tests' own, taking neither the value 0 nor 1.
const OTHER_TENANT: u64 = 5;

/// An insert of `<subject> <predicate> <object>` into `terms`.
fn insert(terms: &mut TermTable, subject: &str, predicate: &str, object: &str) -> WriteChange {
    WriteChange::Insert {
        graph: None,
        subject: terms.iri(&ex(subject)),
        predicate: terms.iri(&ex(predicate)),
        object: terms.iri(&ex(object)),
    }
}

/// A counter add of `delta` to `<subject> <counter>`.
fn counter_add(terms: &mut TermTable, subject: &str, delta: i64) -> CommitOperation {
    CommitOperation::CounterAdd {
        graph: None,
        subject: terms.iri(&ex(subject)),
        predicate: terms.iri(&ex("counter")),
        delta,
    }
}

/// A DirectWrite's reply names the session and request the write named.
#[test]
fn test_a_direct_writes_reply_names_its_session_and_request() -> TestResult {
    with_core("one-frame-session", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        let write = edges(TENANT, &[("a", "p", "b")]).with_session(SESSION, 7);
        let committed = one_frame(&mut client, &write)
            .await?
            .map_err(|refusal| TestError::new(format!("{refusal:?}")))?;
        assert_eq!((committed.session, committed.request), (SESSION, 7));
        assert!(committed.results.is_empty());
        Ok(())
    })
}

/// A ConditionalWrite's reply names its session and request, and the outcome of each of its operations in order.
#[test]
fn test_a_conditional_writes_operations_are_each_decided_in_order() -> TestResult {
    with_core("one-frame-operations", RigSetup::default(), async |core| {
        let mut client = core.dial().await?;
        let mut terms = TermTable::new();
        let operations = vec![
            counter_add(&mut terms, "c1", 3),
            counter_add(&mut terms, "c2", 4),
        ];
        let write = ConditionalWrite::new(TENANT, terms, Vec::new(), operations)
            .with_principal(PRINCIPAL)
            .with_session(SESSION, 9);
        let committed = one_frame(&mut client, &write)
            .await?
            .map_err(|refusal| TestError::new(format!("{refusal:?}")))?;
        assert_eq!((committed.session, committed.request), (SESSION, 9));
        assert_eq!(
            committed.results,
            vec![OperationResult::counter(3), OperationResult::counter(4)]
        );
        Ok(())
    })
}

/// Every change of a DirectWrite is written, in the tenant it names.
#[test]
fn test_a_direct_write_writes_every_change_in_its_tenant() -> TestResult {
    let setup = RigSetup {
        tenant: OTHER_TENANT,
        ..RigSetup::default()
    };
    with_core("one-frame-changes", setup, async |core| {
        let mut client = core.dial().await?;
        let mut terms = TermTable::new();
        let changes = vec![
            insert(&mut terms, "a", "p", "b"),
            insert(&mut terms, "c", "p", "d"),
        ];
        let write = DirectWrite::new(OTHER_TENANT, terms, changes).with_principal(PRINCIPAL);
        let _committed = one_frame(&mut client, &write)
            .await?
            .map_err(|refusal| TestError::new(format!("{refusal:?}")))?;
        let first = key_prefix(Keyspace::Forward, false, OTHER_TENANT, None, None);
        let (lists, refused) = scanned(
            &mut client,
            &plain(ScanAt::Latest, 0, 0, &[(first, Vec::new())]),
        )
        .await?;
        assert!(refused.is_none(), "{refused:?}");
        let held: usize = lists.iter().map(|batch| batch.entries.len()).sum();
        assert_eq!(held, 2, "a forward list for each subject");
        let keys = ["a", "c", "d"].map(|name| alone(term_key(OTHER_TENANT, &ex(name))));
        let (ids, _) = scanned(&mut client, &plain(ScanAt::Latest, 0, 0, &keys)).await?;
        let found: usize = ids.iter().map(|batch| batch.entries.len()).sum();
        assert_eq!(found, 3, "every term in the tenant the write named");
        Ok(())
    })
}

/// A DirectWrite inserting a predicate declared unique is refused, naming ConditionalWrite; a ConditionalWrite inserting it
/// commits (`PROTOCOL.md` §6.6).
#[test]
fn test_only_a_conditional_write_inserts_a_unique_predicate() -> TestResult {
    with_core("one-frame-unique", RigSetup::default(), async |mut core| {
        let unique = ex("u");
        declare_unique(&mut core, 1, &unique).await?;
        let mut client = core.dial().await?;
        let mut direct_terms = TermTable::new();
        let inserted = insert(&mut direct_terms, "a", "u", "b");
        let direct =
            DirectWrite::new(TENANT, direct_terms, vec![inserted]).with_principal(PRINCIPAL);
        let refused = one_frame(&mut client, &direct)
            .await?
            .err()
            .context("the DirectWrite refused")?;
        assert_eq!(refused.0, ErrorCode::Refused);
        assert!(
            refused.1.contains(&unique) && refused.1.contains("ConditionalWrite"),
            "{}",
            refused.1
        );
        let mut conditional_terms = TermTable::new();
        let change = insert(&mut conditional_terms, "a", "u", "b");
        let operation = counter_add(&mut conditional_terms, "c", 1);
        let conditional =
            ConditionalWrite::new(TENANT, conditional_terms, vec![change], vec![operation])
                .with_principal(PRINCIPAL);
        let committed = one_frame(&mut client, &conditional).await?;
        assert!(committed.is_ok(), "{committed:?}");
        Ok(())
    })
}

/// The principal of every write-set record the log in `wal` holds, in order.
async fn principals_logged(wal: std::path::PathBuf) -> TestResult<Vec<u64>> {
    let (mut log, _) = Wal::open(CompioStorage, wal, WalConfig::new(CLUSTER, false)).await?;
    let rounds = log.read(Position::new(1), u32::MAX).await?;
    let mut principals = Vec::new();
    for round in &rounds {
        let header = RoundHeader::new(
            round.position().get(),
            round.parent(),
            0,
            round.first_version(),
            round.transactions(),
            round.derivation(),
        );
        let view = RoundView::from_body(header, round.body())?;
        principals.extend(view.records().iter().map(|record| record.principal()));
    }
    Ok(principals)
}

/// A one-frame write's record in the log names the principal its frame names (`PROTOCOL.md` §§6.6, 12.2).
#[test]
fn test_a_one_frame_writes_record_names_its_principal() -> TestResult {
    const ACTING: u64 = 9;
    let mut principals = Vec::new();
    with_core_then(
        "one-frame-principal",
        RigSetup::default(),
        async |core| {
            let mut client = core.dial().await?;
            let write = edges(TENANT, &[("a", "p", "b")]).with_principal(ACTING);
            let committed = one_frame(&mut client, &write).await?;
            assert!(committed.is_ok(), "{committed:?}");
            Ok(())
        },
        async |wal| {
            principals = principals_logged(wal).await?;
            Ok(())
        },
    )?;
    assert!(principals.contains(&ACTING), "{principals:?}");
    Ok(())
}
