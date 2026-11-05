/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Recovery (7.9; write-ahead-log, "Recovery replays the log above the store", "A round's record is logical"): the
//! log's rounds above the store's applied log position replayed into the store through the derivation each names, the
//! session table and the dictionary with them, so a store replayed from empty holds what the first apply left; a store
//! ahead of the log, behind its checkpoint, or below a gap refused, naming the positions.

use crate::utils::fixture::Refused;
use futures::executor::block_on;
use ultragraphdb_core_mapping::Change;
use ultragraphdb_store_apply::DropRequest;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_log::{OrderedLog, Position, RoundRef};
use ultragraphdb_traits_store::Store;
use ultragraphdb_transactions_commit::{
    Pipeline, RecoveryErrorEnum, Replay, Reply, RoundBounds, Sessions,
};
use ultragraphdb_transactions_txn::{Prepared, TxnBuffers};

use crate::utils::fixture::{
    Applier, Log, TENANT, applier, applier_over, begin, delete_literal, held, insert,
    insert_literal, prepare, prepare_in, reopen_wal, sim_store, version, wal, write,
};

type Pipe<'a> = Pipeline<
    'a,
    Log,
    ultragraphdb_store_sim::SimStore,
    ultragraphdb_store_sim::SimListCache<ultragraphdb_store_posting::CachedList>,
    ultragraphdb_store_sim::SteppedExecutor,
    ultragraphdb_traits_blocks::TestBlockSource,
>;

fn unbounded() -> RoundBounds {
    RoundBounds::new(u32::MAX, u32::MAX)
}

/// Commits `changes` through `pipe` under `ticket`, in a round of their own: the reply.
fn commit(
    pipe: &mut Pipe<'_>,
    applier: &Applier,
    ticket: u64,
    changes: Vec<Change>,
) -> TestResult<Reply> {
    let mut txn = begin(applier)?;
    write(&mut txn, changes)?;
    pipe.commit(ticket, prepare(txn)?, 0)?;
    block_on(pipe.sync(0))?;
    pipe.replies_mut().pop().context("answered")
}

/// Replays `log` into `applier`'s store.
fn replay(
    applier: &Applier,
    log: &mut Log,
) -> Result<Replay, ultragraphdb_transactions_commit::RecoveryError> {
    block_on(Replay::run(applier, log, Sessions::DEFAULT_CAPACITY))
}

/// The applied log position `applier`'s store records.
fn position(applier: &Applier) -> TestResult<u64> {
    Ok(Replay::applied_position(applier.quads().store())
        .context("the position reads")?
        .get())
}

#[test]
fn test_rounds_the_store_lost_are_replayed() -> TestResult {
    let store = sim_store();
    let applier = applier_over(store.clone())?;
    let (storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded())?;
    for i in 1..=10u64 {
        let _reply: Reply = commit(
            &mut pipe,
            &applier,
            i,
            vec![insert(&format!("s{i}"), "knows", "b")],
        )?;
        if i == 7 {
            applier.quads().store().sync()?;
        }
    }
    drop(pipe);
    drop(applier);
    store.power_loss();
    store.recover();
    let recovered = applier_over(store)?;
    // One commit a round, round i at position i and version (1, 2(i − 1)). The store keeps a prefix of its batches at
    // least as long as the sync after round 7 covered: rounds 1 to 7 at least, the rest replayed.
    let lost = position(&recovered)?;
    assert!((7..=10).contains(&lost), "{lost}");
    assert_eq!(
        recovered.quads().lists().applied_ts(),
        version(1, 2 * (lost - 1))
    );
    let (mut reopened_log, _) = reopen_wal(&storage)?;
    let replayed = replay(&recovered, &mut reopened_log)?;
    assert_eq!(
        replayed.rounds() as u64,
        10 - lost,
        "the rounds after the store's position"
    );
    assert_eq!(replayed.applied(), version(1, 18));
    assert_eq!(position(&recovered)?, 10);
    for i in 1..=10 {
        assert_eq!(
            held(&recovered, &format!("s{i}"), "knows")?.len(),
            1,
            "s{i}"
        );
    }
    // Replaying again replays nothing: the store holds the log's last round.
    let (mut log_again, _) = reopen_wal(&storage)?;
    assert_eq!(replay(&recovered, &mut log_again)?.rounds(), 0);
    Ok(())
}

#[test]
fn test_a_store_ahead_of_the_log_refuses_to_start_naming_its_position() -> TestResult {
    let applier = applier()?;
    let (_storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded())?;
    for ticket in 1..=2 {
        let _reply: Reply = commit(&mut pipe, &applier, ticket, vec![insert("a", "knows", "b")])?;
    }
    drop(pipe);
    // Another log, holding one round: the store's position 2 lies beyond its tail.
    let other = applier_over(sim_store())?;
    let (_other_storage, other_log) = wal()?;
    let mut other_pipe: Pipe<'_> = Pipeline::new(&other, other_log, unbounded())?;
    let _reply: Reply = commit(&mut other_pipe, &other, 1, vec![insert("x", "knows", "y")])?;
    let mut short_log = other_pipe.into_log();
    let refused = replay(&applier, &mut short_log).refused("refused")?;
    assert_eq!(refused.0, RecoveryErrorEnum::StoreNotInLog { position: 2 });
    assert!(refused.to_string().contains("position 2"), "{refused}");
    Ok(())
}

#[test]
fn test_a_store_behind_its_checkpoint_refuses_to_start_naming_both_positions() -> TestResult {
    use ultragraphdb_traits_store::{PersistMode, WriteBatch};
    let applier = applier()?;
    let (_storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded())?;
    let _reply: Reply = commit(&mut pipe, &applier, 1, vec![insert("a", "knows", "b")])?;
    let _reply_2: Reply = commit(&mut pipe, &applier, 2, vec![insert("c", "knows", "b")])?;
    let mut written_log = pipe.into_log();
    let committed = written_log.committed();
    block_on(written_log.trim_through(committed))?;
    assert_eq!(written_log.checkpoint(), Position::new(2));
    // A store that applied round 1 alone, as one restored from before the checkpoint holds it.
    let behind = applier_over(sim_store())?;
    let mut batch = WriteBatch::new();
    batch.put_versionless(
        Replay::APPLIED_POSITION_KEY.to_vec(),
        1u64.to_le_bytes().to_vec(),
    );
    behind.quads().store().apply(batch, PersistMode::Buffer)?;
    let refused = replay(&behind, &mut written_log).refused("refused")?;
    assert_eq!(
        refused.0,
        RecoveryErrorEnum::BehindCheckpoint {
            applied: 1,
            checkpoint: 2
        }
    );
    let text = refused.to_string();
    assert!(
        text.contains("position 1") && text.contains("position 2"),
        "{text}"
    );
    assert_eq!(position(&behind)?, 1, "nothing replayed");
    Ok(())
}

#[test]
fn test_a_store_at_its_checkpoint_below_a_trimmed_log_replays_the_rest() -> TestResult {
    let store = sim_store();
    let applier = applier_over(store.clone())?;
    let (storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&applier, log, unbounded())?;
    let _reply: Reply = commit(&mut pipe, &applier, 1, vec![insert("a", "knows", "b")])?;
    applier.quads().store().sync()?;
    let mut written_log = pipe.into_log();
    block_on(written_log.trim_through(Position::new(1)))?;
    let mut reopened: Pipe<'_> = Pipeline::new(&applier, written_log, unbounded())?;
    let _reply_2: Reply = commit(&mut reopened, &applier, 2, vec![insert("c", "knows", "b")])?;
    drop(reopened);
    drop(applier);
    store.power_loss();
    store.recover();
    let recovered = applier_over(store)?;
    assert_eq!(position(&recovered)?, 1);
    let (mut reopened_log, _) = reopen_wal(&storage)?;
    assert_eq!(replay(&recovered, &mut reopened_log)?.rounds(), 1);
    assert_eq!(held(&recovered, "c", "knows")?.len(), 1);
    Ok(())
}

/// A log holding one round, `<a> <knows> <b>` committed by a pipeline over a fresh store, and the log's storage.
fn one_round_log() -> TestResult<(ultragraphdb_transactions_log::SimStorage, Log)> {
    let original = applier()?;
    let (storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&original, log, unbounded())?;
    // The first commit after an empty store: epoch 1, sequence 0.
    let reply = commit(&mut pipe, &original, 1, vec![insert("a", "knows", "b")])?;
    assert_eq!(reply.outcome(), &Ok(version(1, 0)));
    Ok((storage, pipe.into_log()))
}

#[test]
fn test_an_empty_store_below_a_trimmed_log_refuses_to_start_naming_the_gap() -> TestResult {
    let (_storage, mut log) = one_round_log()?;
    block_on(log.trim_through(Position::new(1)))?;
    let empty = applier_over(sim_store())?;
    assert_eq!(
        replay(&empty, &mut log).refused("refused")?.0,
        RecoveryErrorEnum::BehindCheckpoint {
            applied: 0,
            checkpoint: 1
        }
    );
    Ok(())
}

#[test]
fn test_a_round_naming_an_unknown_derivation_is_refused_naming_it_applying_nothing() -> TestResult {
    let (_storage, mut log) = one_round_log()?;
    let (_, parent) = log.tail();
    let derivation = std::num::NonZeroU32::new(2).context("2")?;
    // A body the Round layout reads, holding no decision; its derivation is past the store's, 1.
    let body = {
        use ultragraphdb_protocol_wire::{BodyBuilder, RoundBody};
        RoundBody::new(2, parent, 0, version(1, 2), derivation)
            .body()
            .to_vec()
    };
    let _ticket = log
        .append(RoundRef::new(
            Position::new(2),
            parent,
            version(1, 2),
            0,
            derivation,
            &body,
        ))
        .context("the log takes it")?;
    let _synced: Position = block_on(log.sync()).context("synced")?;
    let replica = applier_over(sim_store())?;
    let refused = replay(&replica, &mut log).refused("refused")?;
    assert_eq!(
        refused.0,
        RecoveryErrorEnum::Derivation {
            position: 2,
            derivation: 2
        }
    );
    assert!(refused.to_string().contains("derivation 2"), "{refused}");
    assert_eq!(position(&replica)?, 1, "round 1 replayed, round 2 not");
    Ok(())
}

/// Every kind of round replayed into an empty store gives the store the first apply left (the differential check of
/// task 4.4): inserts and literals, deletes, a delete by pattern, an edge with properties, a session's committed
/// request with its stored reply, and a drop, the dictionary entries each round created, and the applied position.
#[test]
fn test_every_kind_of_round_replays_into_an_empty_store_as_the_first_apply_left_it() -> TestResult {
    let original = applier()?;
    let (_storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&original, log, unbounded())?;
    let _r1 = commit(
        &mut pipe,
        &original,
        1,
        vec![insert("a", "knows", "b"), insert_literal("a", "name", "A")],
    )?;
    let _r2 = commit(
        &mut pipe,
        &original,
        2,
        vec![
            delete_literal("a", "name", "A"),
            insert_literal("a", "name", "B"),
        ],
    )?;
    let edge = Change::InsertEdge {
        graph: oxrdf::GraphName::DefaultGraph,
        subject: crate::utils::fixture::ex("c").into(),
        predicate: crate::utils::fixture::ex("knows"),
        object: crate::utils::fixture::ex("a").into(),
        properties: vec![(
            crate::utils::fixture::ex("since"),
            oxrdf::Literal::new_simple_literal("2020").into(),
        )],
    };
    let _r3 = commit(&mut pipe, &original, 3, vec![edge])?;
    let pattern = Change::DeletePattern {
        graph: oxrdf::GraphName::DefaultGraph,
        subject: crate::utils::fixture::ex("a").into(),
        predicate: crate::utils::fixture::ex("knows"),
    };
    let _r4 = commit(&mut pipe, &original, 4, vec![pattern])?;
    let session = 0x51_u128 << 64;
    let mut txn = begin(&original)?;
    write(&mut txn, vec![insert("s", "knows", "b")])?;
    pipe.commit(5, prepare_in(txn, session, 7)?, 0)?;
    block_on(pipe.sync(0))?;
    let drop = Prepared::drop(
        &original,
        TENANT,
        DropRequest::Predicate(crate::utils::fixture::ex("name").as_ref()),
        0,
        0,
        0,
        TxnBuffers::new(),
    )?;
    pipe.commit(6, drop, 0)?;
    block_on(pipe.sync(0))?;
    let mut written = pipe.into_log();
    let replica = applier_over(sim_store())?;
    let replayed = replay(&replica, &mut written)?;
    assert_eq!(replayed.rounds(), 6);
    assert_eq!(
        original.quads().store().contents(),
        replica.quads().store().contents(),
        "the replayed store holds every key and value the first apply left"
    );
    assert_retry_answered_from_the_table(&replica, written, session)
}

/// A retry of `session`'s request 7 is answered from the table replay rebuilt without committing again, through a
/// Recall and a Drop resent under the number alike.
fn assert_retry_answered_from_the_table(
    replica: &Applier,
    written: Log,
    session: u128,
) -> TestResult {
    let mut again: Pipe<'_> = Pipeline::new(replica, written, unbounded())?;
    assert_eq!(again.retry(session, 7)?.version(), version(1, 8));
    let resend = Prepared::drop(
        replica,
        TENANT,
        DropRequest::Predicate(crate::utils::fixture::ex("name").as_ref()),
        session,
        7,
        0,
        TxnBuffers::new(),
    )?;
    again.commit(7, resend, 0)?;
    assert_eq!(
        again.replies_mut().pop().map(|r| r.outcome().clone()),
        Some(Ok(version(1, 8)))
    );
    Ok(())
}

/// A store whose session table is corrupted refuses recovery before any round is replayed.
#[test]
fn test_a_corrupted_session_table_refuses_recovery() -> TestResult {
    use ultragraphdb_error_store::StoreError;
    use ultragraphdb_traits_store::{PersistMode, WriteBatch};
    use ultragraphdb_transactions_commit::SessionEntry;
    let (_storage, mut log) = one_round_log()?;
    let replica = applier_over(sim_store())?;
    let mut batch = WriteBatch::new();
    batch.put_versionless(SessionEntry::INDEX_KEY.to_vec(), vec![1; 17]);
    replica.quads().store().apply(batch, PersistMode::Buffer)?;
    let refused = replay(&replica, &mut log).refused("refused")?;
    assert_eq!(
        refused.0,
        RecoveryErrorEnum::Sessions {
            cause: StoreError::corrupted(
                SessionEntry::INDEX_KEY.to_vec(),
                "a session index whose length is not a multiple of 16".to_string()
            )
        }
    );
    assert_eq!(position(&replica)?, 0, "nothing replayed");
    Ok(())
}

/// A log whose storage fails the read refuses recovery with the log's error.
#[test]
fn test_a_log_that_cannot_be_read_refuses_recovery() -> TestResult {
    use ultragraphdb_error_log::LogErrorEnum;
    use ultragraphdb_transactions_log::FaultMode;
    let (storage, log) = one_round_log()?;
    drop(log);
    let (mut reopened_log, _) = reopen_wal(&storage)?;
    storage.fail_at(storage.trace().len(), FaultMode::Before);
    let replica = applier_over(sim_store())?;
    let refused = replay(&replica, &mut reopened_log).refused("refused")?;
    assert!(
        matches!(&refused.0, RecoveryErrorEnum::Log { cause } if matches!(cause.0, LogErrorEnum::Io { .. })),
        "{refused:?}"
    );
    assert_eq!(position(&replica)?, 0, "nothing replayed");
    Ok(())
}

/// A round the log holds whose body is no Round body refuses recovery naming its position, after the rounds before it
/// were replayed.
#[test]
fn test_a_round_that_is_no_round_body_refuses_recovery_naming_it() -> TestResult {
    let (_storage, mut log) = one_round_log()?;
    let (tail, parent) = log.tail();
    assert_eq!(tail, Position::new(1));
    // Eight bytes of 0xff: a section table pointing far past the body.
    let garbage = [0xffu8; 8];
    let _ticket = log
        .append(RoundRef::new(
            Position::new(2),
            parent,
            version(1, 2),
            1,
            std::num::NonZeroU32::MIN,
            &garbage,
        ))
        .context("the log takes any body")?;
    let _synced: Position = block_on(log.sync()).context("synced")?;
    let replica = applier_over(sim_store())?;
    let refused = replay(&replica, &mut log).refused("refused")?;
    assert!(
        matches!(refused.0, RecoveryErrorEnum::Round { position: 2, .. }),
        "{refused:?}"
    );
    assert_eq!(position(&replica)?, 1, "round 1 was replayed first");
    Ok(())
}

/// A store refusing a replayed round refuses recovery naming the round's position.
#[test]
fn test_a_round_the_store_refuses_refuses_recovery_naming_it() -> TestResult {
    let (_storage, mut log) = one_round_log()?;
    let replica = applier_over(sim_store())?;
    replica
        .quads()
        .store()
        .inject(ultragraphdb_store_sim::Fault::FailNextBatch);
    let refused = replay(&replica, &mut log).refused("refused")?;
    assert!(
        matches!(refused.0, RecoveryErrorEnum::Apply { position: 1, .. }),
        "{refused:?}"
    );
    assert_eq!(position(&replica)?, 0, "nothing applied");
    Ok(())
}

/// A store whose applied position does not read refuses recovery naming the key.
#[test]
fn test_an_applied_position_that_does_not_read_refuses_recovery() -> TestResult {
    use ultragraphdb_error_store::StoreError;
    use ultragraphdb_traits_store::{PersistMode, WriteBatch};
    let (_storage, mut log) = one_round_log()?;
    let replica = applier_over(sim_store())?;
    let mut batch = WriteBatch::new();
    batch.put_versionless(Replay::APPLIED_POSITION_KEY.to_vec(), vec![1; 3]);
    replica.quads().store().apply(batch, PersistMode::Buffer)?;
    let refused = replay(&replica, &mut log).refused("refused")?;
    assert_eq!(
        refused.0,
        RecoveryErrorEnum::AppliedPosition {
            cause: StoreError::corrupted(
                Replay::APPLIED_POSITION_KEY.to_vec(),
                "an applied log position that is not 8 bytes".to_string()
            )
        }
    );
    assert_eq!(
        Replay::APPLIED_POSITION_KEY,
        b"\xffultragraphdb/applied_position"
    );
    Ok(())
}

/// A catalog version removing a predicate, an entity type and a relation round-trips: replayed into an empty store from
/// the versions the metadata group holds, the store holds every key and value the first apply left, byte for byte, and
/// the catalog the same declarations.
#[test]
fn test_a_versions_removals_replay_as_the_first_apply_made_them() -> TestResult {
    use ultragraphdb_core_catalog::CatalogContents;
    use ultragraphdb_core_mapping::{
        Declaration, PredicateDeclaration, RelationDeclaration, TypeDeclaration,
    };
    use ultragraphdb_traits_catalog::{HeldVersion, TestCatalogSource};
    let ex = crate::utils::fixture::ex;
    let original = applier()?;
    let (_storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&original, log, unbounded())?;
    let _r1 = commit(
        &mut pipe,
        &original,
        1,
        vec![insert_literal("a", "name", "big apple")],
    )?;
    let group = TestCatalogSource::new();
    let two = CatalogContents::new(
        TENANT,
        2,
        vec![
            Declaration::Predicate(
                PredicateDeclaration::new(ex("name")).with_tokenizers(vec!["exact".into()]),
            ),
            Declaration::EntityType(TypeDeclaration::new(ex("Person"), None, false)),
            Declaration::Relation(RelationDeclaration::new(ex("Marriage"), vec![ex("spouse")])),
        ],
    );
    let three = CatalogContents::new(TENANT, 3, Vec::new());
    for (ticket, contents) in [(2, &two), (3, &three)] {
        group.commit(HeldVersion::new(
            TENANT,
            contents.version(),
            1,
            contents.encode()?,
        ));
        let refusals = pipe.install(ticket, contents, 0)?;
        assert!(refusals.is_empty(), "{refusals:?}");
        block_on(pipe.sync(0))?;
        original.executor().run_until_idle();
        pipe.poll_builds(0)?;
        block_on(pipe.sync(0))?;
    }
    assert!(!original.rebuilding());
    let catalog = original.catalog(TENANT)?;
    assert_eq!(
        catalog.entity_types().count() + catalog.relations().count(),
        0
    );
    let mut written = pipe.into_log();
    let replica = applier_over(sim_store())?;
    replica.sequence_builds();
    let _replayed = block_on(Replay::run_with_catalog(
        0,
        &replica,
        &mut written,
        Sessions::DEFAULT_CAPACITY,
        &group,
    ))?;
    replica.executor().run_until_idle();
    assert_eq!(
        original.quads().store().contents(),
        replica.quads().store().contents(),
        "the replayed store holds every key and value the first apply left"
    );
    assert_eq!(*replica.catalog(TENANT)?, *original.catalog(TENANT)?);
    Ok(())
}

/// `one_round_log` with a second round committing `record` alone at (1, 2): the log and the position of that round, 2.
fn log_with(record: Vec<u8>) -> TestResult<(ultragraphdb_transactions_log::SimStorage, Log)> {
    log_of(|body| body.commit(record), 1)
}

/// `one_round_log` with a second round at (1, 2) of `transactions` decisions, which `fill` adds to its body.
fn log_of(
    fill: impl FnOnce(&mut ultragraphdb_protocol_wire::RoundBody),
    transactions: u32,
) -> TestResult<(ultragraphdb_transactions_log::SimStorage, Log)> {
    use ultragraphdb_protocol_wire::{BodyBuilder, RoundBody};
    let (storage, mut log) = one_round_log()?;
    let (_, parent) = log.tail();
    let derivation = std::num::NonZeroU32::MIN;
    let mut body = RoundBody::new(2, parent, 0, version(1, 2), derivation);
    fill(&mut body);
    let body = body.body().to_vec();
    let _ticket = log
        .append(RoundRef::new(
            Position::new(2),
            parent,
            version(1, 2),
            transactions,
            derivation,
            &body,
        ))
        .context("the log takes it")?;
    let _synced: Position = block_on(log.sync()).context("synced")?;
    Ok((storage, log))
}

/// A build record whose operation is a drop's names nothing a build does: the round does not read, and recovery is
/// refused naming the round and the record's operation, round 1 replayed before it.
#[test]
fn test_a_build_record_naming_a_drop_refuses_recovery_naming_its_operation() -> TestResult {
    use ultragraphdb_error_protocol::ProtocolError;
    use ultragraphdb_protocol_wire::{RecordOperation, WriteSetRecord};
    for drop in [RecordOperation::DropTenantData, RecordOperation::DropTenant] {
        let (_storage, mut log) = log_with(WriteSetRecord::build(TENANT, drop).encode())?;
        let replica = applier_over(sim_store())?;
        let refused = replay(&replica, &mut log).refused("refused")?;
        let cause = ProtocolError::record(
            0,
            ProtocolError::field_value(17, "operation", u64::from(drop.code())),
        );
        assert_eq!(
            refused.0,
            RecoveryErrorEnum::Round { position: 2, cause },
            "{drop:?}"
        );
        assert_eq!(position(&replica)?, 1, "round 1 replayed, round 2 not");
    }
    Ok(())
}

/// A round starting a catalog version the replay's catalog source does not give refuses recovery naming the round, the
/// tenant and the version: a replay given no source holds none.
#[test]
fn test_a_catalog_version_the_source_does_not_give_refuses_recovery() -> TestResult {
    use ultragraphdb_protocol_wire::{RecordOperation, WriteSetRecord};
    let mut record = WriteSetRecord::build(TENANT, RecordOperation::CatalogTakesEffect(5));
    record.set_catalog(5);
    let (_storage, mut log) = log_with(record.encode())?;
    let replica = applier_over(sim_store())?;
    let refused = replay(&replica, &mut log).refused("refused")?;
    assert_eq!(
        refused.0,
        RecoveryErrorEnum::Catalog {
            position: 2,
            tenant: TENANT,
            version: 5,
            reason: "the metadata group does not hold it".to_string()
        }
    );
    assert_eq!(position(&replica)?, 1);
    Ok(())
}

/// A transaction record whose statement record's key is no graph and ID refuses recovery naming the round, the store's
/// corruption naming the key, for each way the key fails: too short, a graph that is no ID, an ID that is none.
#[test]
fn test_a_statement_record_whose_key_is_no_graph_and_id_refuses_recovery() -> TestResult {
    use ultragraphdb_error_store::StoreError;
    use ultragraphdb_protocol_wire::WriteSetRecord;
    use ultragraphdb_store_apply::ChangesetError;
    let id = crate::utils::proposal_double::id(9)?.to_be_bytes().to_vec();
    let keys = [
        vec![0u8; 15],
        [vec![0xFFu8; 16], id.clone()].concat(),
        [vec![0u8; 16], vec![0xFF; 16]].concat(),
    ];
    for key in keys {
        let mut record = WriteSetRecord::transaction(TENANT);
        record.push_statement_record(key.clone(), None);
        let (_storage, mut log) = log_with(record.encode())?;
        let replica = applier_over(sim_store())?;
        let refused = replay(&replica, &mut log).refused("refused")?;
        let cause: ChangesetError = StoreError::corrupted(
            key.clone(),
            "a logged statement record's key is no graph and ID".to_string(),
        )
        .into();
        assert_eq!(
            refused.0,
            RecoveryErrorEnum::Apply { position: 2, cause },
            "{key:?}"
        );
    }
    Ok(())
}

/// A build's completion replayed with no build running, a drop having cancelled it, derives nothing: the round is
/// replayed and the store's position passes it, holding what round 1 left.
#[test]
fn test_a_builds_completion_with_no_build_running_replays_as_nothing() -> TestResult {
    use ultragraphdb_protocol_wire::{BuildOutcome, RecordOperation, WriteSetRecord};
    for outcome in [BuildOutcome::Swap, BuildOutcome::Undo] {
        let mut record = WriteSetRecord::build(
            TENANT,
            RecordOperation::CompleteBuild {
                version: 2,
                outcome,
            },
        );
        record.set_catalog(2);
        let (_storage, mut log) = log_with(record.encode())?;
        let replica = applier_over(sim_store())?;
        let _replayed: Replay = replay(&replica, &mut log)?;
        assert_eq!(position(&replica)?, 2, "{outcome:?}");
        assert_eq!(held(&replica, "a", "knows")?.len(), 1, "{outcome:?}");
    }
    Ok(())
}

/// A round holding an aborted decision before a committed one replays the committed one alone, at its own version; a
/// registration a home accepted derives no keys on replay; a statement record of a named graph's statement replays; and
/// each drop a transaction record names replays as the drop of its target.
#[test]
fn test_aborted_decisions_registrations_named_graph_records_and_every_drop_replay() -> TestResult {
    use ultragraphdb_protocol_wire::{ErrorCode, RecordOperation, WriteSetRecord};
    let id = crate::utils::proposal_double::id(9)?;
    let dropping = |operation| {
        let mut record = WriteSetRecord::transaction(TENANT);
        record.push_operation(operation);
        record.encode()
    };
    let mut named = WriteSetRecord::transaction(TENANT);
    named.set_read_version(version(1, 0));
    named.push_statement_record([id.to_be_bytes(), id.to_be_bytes()].concat(), None);
    // Each case: the round's second entry, and how many of round 1's statements are held after it.
    let cases = [
        (crate::utils::proposal_double::registration(7, 3, 1)?, 1),
        (named.encode(), 1),
        (dropping(RecordOperation::DropPredicate(id)), 1),
        (dropping(RecordOperation::DropGraph(id)), 1),
        (dropping(RecordOperation::DropTenantData), 0),
        (dropping(RecordOperation::DropTenant), 0),
    ];
    for (record, statements) in cases {
        let (_storage, mut log) = log_of(
            |body| {
                body.abort(ErrorCode::Conflict, 1);
                body.commit(record);
            },
            2,
        )?;
        let replica = applier_over(sim_store())?;
        let _replayed: Replay = replay(&replica, &mut log)?;
        assert_eq!(position(&replica)?, 2);
        assert_eq!(held(&replica, "a", "knows")?.len(), statements);
    }
    Ok(())
}

/// A round holding only two registrations a home accepted replays into an empty store as its first apply left it: each
/// member of the referrer set at the commit version the record lists for its registration, and the dictionary entries
/// of the IRIs the home created for them (write-ahead-log, "Replay derives a home's referrer-set deltas").
#[test]
fn test_a_round_of_two_registrations_replays_each_member_at_its_version() -> TestResult {
    use crate::utils::fixture::{member_at, registered};
    let original = applier()?;
    let (_storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&original, log, unbounded())?;
    // A commit's round in flight, so the two registrations wait for one round of their own.
    let mut txn = begin(&original)?;
    write(&mut txn, vec![insert("a", "knows", "b")])?;
    pipe.commit(1, prepare(txn)?, 0)?;
    let (o, o_executed, o_record) = registered(&original, "o", 7, 1)?;
    let (p, p_executed, p_record) = registered(&original, "p", 9, 3)?;
    pipe.registration(2, o_executed, o_record, 0)?;
    pipe.registration(3, p_executed, p_record, 0)?;
    block_on(pipe.sync(0))?;
    block_on(pipe.sync(0))?;
    let versions: Vec<(
        u64,
        Result<u64, ultragraphdb_transactions_commit::CommitError>,
    )> = pipe
        .replies_mut()
        .iter()
        .map(|r| (r.ticket(), r.outcome().clone()))
        .collect();
    assert_eq!(
        versions,
        vec![
            (1, Ok(version(1, 0))),
            (2, Ok(version(1, 2))),
            (3, Ok(version(1, 4)))
        ]
    );
    let mut written = pipe.into_log();
    let replica = applier_over(sim_store())?;
    let replayed = replay(&replica, &mut written)?;
    assert_eq!(replayed.rounds(), 2);
    assert_eq!(member_at(&replica, o, 7, version(1, 2))?, Some((1, 1)));
    assert_eq!(member_at(&replica, o, 7, version(1, 0))?, None);
    assert_eq!(
        member_at(&replica, p, 9, version(1, 4))?,
        Some((3, 1)),
        "the generation its home set"
    );
    assert_eq!(member_at(&replica, p, 9, version(1, 2))?, None);
    assert_eq!(
        original.quads().store().contents(),
        replica.quads().store().contents(),
        "the replayed store holds every key and value the first apply left"
    );
    Ok(())
}

/// A round holding a first reference to an entity another partition owns, with its registration mark and the copied
/// ID-to-IRI entry, replays into an empty store as its first apply left it, the mark's generation and the copied entry
/// included (write-ahead-log, "Replay derives the same keys").
#[test]
fn test_a_reference_with_its_mark_and_copied_entry_replays_the_same_keys() -> TestResult {
    use std::num::NonZeroU64;
    use ultragraphdb_core_ids::{BucketGroup, Epoch};
    let original = applier()?;
    let (_storage, log) = wal()?;
    let mut pipe: Pipe<'_> = Pipeline::new(&original, log, unbounded())?;
    let b = crate::utils::proposal_double::id(40)?;
    let g = BucketGroup::new(7)?;
    let four = NonZeroU64::new(4).context("4")?;
    let mut txn = begin(&original)?;
    txn.adopt(crate::utils::fixture::ex("b").as_str(), b, Epoch::ZERO, 0)?;
    txn.mark(b, g, four, Epoch::ZERO, 0)?;
    write(&mut txn, vec![insert("a", "knows", "b")])?;
    pipe.commit(1, prepare(txn)?, 0)?;
    block_on(pipe.sync(0))?;
    let at = pipe.watermark();
    assert_eq!(original.quads().mark(TENANT, b, g, at)?, Some(four));
    assert_eq!(
        original
            .quads()
            .id_of(TENANT, crate::utils::fixture::ex("b").as_str())?,
        Some(b),
        "the copied entry names the home's ID"
    );
    let mut written = pipe.into_log();
    let replica = applier_over(sim_store())?;
    let _replayed: Replay = replay(&replica, &mut written)?;
    assert_eq!(replica.quads().mark(TENANT, b, g, at)?, Some(four));
    assert_eq!(
        original.quads().store().contents(),
        replica.quads().store().contents(),
        "the replayed store holds every key and value the first apply left"
    );
    Ok(())
}
