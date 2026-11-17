/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The transactions a Bind opens on a host: their tickets, their session tokens, their resends and their ends.

use ultragraphdb_cluster_partition::{
    BoundKey, BoundStep, PartitionHost, PartitionMailbox, PartitionMessage,
};
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_cluster_sequencer::Close;
use ultragraphdb_core_catalog::CatalogContents;
use ultragraphdb_core_ids::{Epoch, Version};
use ultragraphdb_core_mapping::Changeset;
use ultragraphdb_store_apply::DropRequest;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_commit::{CloseRequest, Pipeline, RoundBounds};

use crate::utils::cluster::{
    Cluster, TENANT, block_on, changeset, epoch, insert, insert_literal, node, pid,
};
use crate::utils::hosts::{Host, holds, on_hosts, sim_log, turn_all};

/// A name and a predicate partition 1 of `count` owns, and a name partition 2 owns when there is one.
fn names(count: u32) -> TestResult<(String, String, Option<String>)> {
    let cluster = Cluster::even(count)?;
    let foreign = if count > 1 {
        Some(cluster.name_in(2, "b")?)
    } else {
        None
    };
    Ok((
        cluster.name_in(1, "a")?,
        cluster.name_in(1, "knows")?,
        foreign,
    ))
}

/// Partition 1's host.
fn first<'h, 'a>(hosts: &'h mut [Host<'a>]) -> TestResult<&'h mut Host<'a>> {
    hosts.first_mut().context("partition 1")
}

/// Writes `changes` in the transaction `key` of partition 1, its references answered first when it has any.
fn write_now(hosts: &mut [Host<'_>], key: BoundKey, changes: &Changeset) -> TestResult {
    if first(hosts)?.write_bound(key, changes, 0)? == BoundStep::Waiting {
        let _ended = turn_all(hosts, 4, 0)?;
        first(hosts)?.resume_write(key, changes, 0)?;
    }
    Ok(())
}

/// The commit version the outcomes `ended` give `ticket`.
fn version_of(
    ended: &[ultragraphdb_cluster_partition::PartitionOutcome],
    ticket: u64,
) -> TestResult<u64> {
    let outcome = ended
        .iter()
        .find(|o| o.ticket() == ticket)
        .context("the outcome")?;
    outcome
        .outcome()
        .clone()
        .map_err(|e| TestError::new(e.to_string()))
}

/// Delivers the close of `at` to partition 1.
fn deliver(hosts: &[Host<'_>], at: u64) -> TestResult {
    let sender = hosts
        .first()
        .and_then(|h| h.mailbox().sender(pid(1).ok()?))
        .context("a sender")?;
    sender.send(PartitionMessage::Close(Close::new(
        epoch(at)?,
        0,
        Vec::new(),
        Vec::new(),
    )?))?;
    Ok(())
}

/// A Bind naming an epoch holds its commit until that epoch is open, and stamps it there (`PROTOCOL.md` §6.5: a session
/// token's epoch is the earliest a commit takes); epoch 0 names none.
#[test]
fn test_a_binds_epoch_holds_its_commit_until_that_epoch_opens() -> TestResult {
    let (a, knows, _) = names(1)?;
    on_hosts(1, |hosts| {
        // Epoch 1 is closed, so 2 is open: a token of 3 waits for the close of 2.
        let key = (1, 1);
        let _read = first(hosts)?.bind(key, [1; 16], (TENANT, 1, epoch(3)?), 0)?;
        write_now(
            hosts,
            key,
            &changeset(vec![insert_literal(&a, &knows, "v")]),
        )?;
        let ticket = first(hosts)?.commit_bound(key, 0, 0, 0)?;
        let held = turn_all(hosts, 3, 0)?;
        assert!(
            held.iter().all(|o| o.ticket() != ticket),
            "held for epoch 3: {held:?}"
        );
        deliver(hosts, 2)?;
        let released = turn_all(hosts, 3, 0)?;
        let version = version_of(&released, ticket)?;
        assert_eq!(Version::from_u64(version).epoch(), epoch(3)?);
        // Epoch 0 holds nothing: the next commit takes the open epoch, 3.
        let plain = (1, 2);
        let _plain_read = first(hosts)?.bind(plain, [2; 16], (TENANT, 1, Epoch::ZERO), 0)?;
        write_now(
            hosts,
            plain,
            &changeset(vec![insert_literal(&a, &knows, "w")]),
        )?;
        let next = first(hosts)?.commit_bound(plain, 0, 0, 0)?;
        let ended = turn_all(hosts, 3, 0)?;
        assert_eq!(
            Version::from_u64(version_of(&ended, next)?).epoch(),
            epoch(3)?
        );
        Ok(())
    })
}

/// A one-frame commit comes under the host's next ticket, and the session table keeps its reply for a resend (`PROTOCOL.md`
/// §6.6).
#[test]
fn test_a_one_frame_commit_comes_under_its_ticket_and_its_reply_is_kept() -> TestResult {
    let (a, knows, _) = names(1)?;
    on_hosts(1, |hosts| {
        let before = first(hosts)?.submit(changeset(vec![insert_literal(&a, &knows, "u")]), 0)?;
        let _settled = turn_all(hosts, 3, 0)?;
        let key = (1, 1);
        let _read = first(hosts)?.bind(key, [1; 16], (TENANT, 1, Epoch::ZERO), 0)?;
        write_now(
            hosts,
            key,
            &changeset(vec![insert_literal(&a, &knows, "v")]),
        )?;
        let session = 7_u128;
        assert!(
            first(hosts)?.resend_of(session, 1)?.is_none(),
            "nothing kept yet"
        );
        let ticket = first(hosts)?.commit_one_frame(key, session, 1, 0)?;
        assert!(ticket > before, "a ticket after the write's");
        assert_eq!(first(hosts)?.last_ticket(), ticket);
        let ended = turn_all(hosts, 3, 0)?;
        let version = version_of(&ended, ticket)?;
        let kept = first(hosts)?
            .resend_of(session, 1)?
            .context("the kept reply")?;
        assert_eq!(kept.version(), version);
        Ok(())
    })
}

/// A Drop comes under the host's next ticket, its outcome the version it commits at.
#[test]
fn test_a_drop_comes_under_its_ticket() -> TestResult {
    let (a, knows, _) = names(1)?;
    on_hosts(1, |hosts| {
        let before = first(hosts)?.submit(changeset(vec![insert_literal(&a, &knows, "v")]), 0)?;
        let written = turn_all(hosts, 3, 0)?;
        let first_version = version_of(&written, before)?;
        let ticket = first(hosts)?.commit_drop((TENANT, 1), DropRequest::Data, 0, (7, 1), 0)?;
        assert!(ticket > before, "a ticket after the write's");
        assert_eq!(first(hosts)?.last_ticket(), ticket);
        let ended = turn_all(hosts, 3, 0)?;
        assert!(version_of(&ended, ticket)? > first_version);
        Ok(())
    })
}

/// A catalog version installed comes under the host's next ticket, and a version declaring nothing is refused nothing.
#[test]
fn test_an_installed_catalog_version_comes_under_its_ticket() -> TestResult {
    let (a, knows, _) = names(1)?;
    on_hosts(1, |hosts| {
        let before = first(hosts)?.submit(changeset(vec![insert_literal(&a, &knows, "v")]), 0)?;
        let _settled = turn_all(hosts, 3, 0)?;
        let contents = CatalogContents::new(TENANT, 1, Vec::new());
        let (ticket, refusals) = first(hosts)?.install_catalog(&contents, 0)?;
        assert_eq!(ticket, before + 1, "tickets are numbered in turn from 1");
        assert!(refusals.is_empty(), "{refusals:?}");
        Ok(())
    })
}

/// A closing connection aborts its own transactions and no other's, and each ended one is named once.
#[test]
fn test_a_closed_connection_ends_its_own_transactions() -> TestResult {
    on_hosts(1, |hosts| {
        let host = first(hosts)?;
        let (own, other) = ((1, 1), (2, 1));
        let _own = host.bind(own, [1; 16], (TENANT, 1, Epoch::ZERO), 0)?;
        let _other = host.bind(other, [2; 16], (TENANT, 1, Epoch::ZERO), 0)?;
        host.abort_connection(1);
        assert_eq!(host.bound_read(&[1; 16]), None);
        assert_eq!(
            host.bound_read(&[2; 16]).map(|read| read.key()),
            Some(other)
        );
        assert_eq!(host.take_bound_ended(), vec![own]);
        assert_eq!(host.take_bound_ended(), Vec::<BoundKey>::new());
        Ok(())
    })
}

/// A read-write transaction lives 10 s by default (`TxnLimits`, the transactions spec's limit).
#[test]
fn test_a_bound_transaction_lives_ten_seconds() -> TestResult {
    on_hosts(1, |hosts| {
        assert_eq!(first(hosts)?.bound_limit_ms(), 10_000);
        Ok(())
    })
}

/// A bound transaction asks its references under a ticket of its own, after the tickets before it; one that asked nothing
/// holds none.
#[test]
fn test_a_bound_transaction_asks_under_a_ticket_of_its_own() -> TestResult {
    let (a, knows, b) = names(2)?;
    let b = b.context("a name of partition 2")?;
    on_hosts(2, |hosts| {
        let host = first(hosts)?;
        let parked = host.submit(changeset(vec![insert_literal(&a, &knows, "v")]), 0)?;
        let (key, idle) = ((1, 1), (1, 2));
        let _read = host.bind(key, [1; 16], (TENANT, 1, Epoch::ZERO), 0)?;
        let _idle = host.bind(idle, [2; 16], (TENANT, 1, Epoch::ZERO), 0)?;
        let step = host.write_bound(key, &changeset(vec![insert(&a, &knows, &b)]), 0)?;
        assert_eq!(step, BoundStep::Waiting);
        let asked = host.bound_ticket(key).context("its ticket")?;
        assert!(asked > parked, "after the write's");
        assert_eq!(host.bound_ticket(idle), Some(0));
        assert_eq!(host.bound_ticket((9, 9)), None);
        Ok(())
    })
}

/// Answers to a parked write and to a bound transaction reach each its own asker, whichever comes first.
#[test]
fn test_answers_reach_the_write_or_transaction_that_asked() -> TestResult {
    let cluster = Cluster::even(2)?;
    let (a, knows) = (cluster.name_in(1, "a")?, cluster.name_in(1, "knows")?);
    let (b, c) = (cluster.name_in(2, "b")?, cluster.name_in(2, "c")?);
    on_hosts(2, |hosts| {
        let parked = first(hosts)?.submit(changeset(vec![insert(&a, &knows, &b)]), 0)?;
        let key = (1, 1);
        let _read = first(hosts)?.bind(key, [1; 16], (TENANT, 1, Epoch::ZERO), 0)?;
        let step = first(hosts)?.write_bound(key, &changeset(vec![insert(&a, &knows, &c)]), 0)?;
        assert_eq!(step, BoundStep::Waiting);
        let ended = turn_all(hosts, 4, 0)?;
        assert!(version_of(&ended, parked).is_ok(), "{ended:?}");
        let host = first(hosts)?;
        assert_eq!(host.take_bound_ready(), vec![key]);
        assert!(host.take_bound_refused().is_empty());
        Ok(())
    })
}

/// A bound transaction's unanswered registrations go again at the delay and not before, each time it passes.
#[test]
fn test_a_bound_transactions_registrations_go_again_at_the_delay() -> TestResult {
    let (a, knows, b) = names(2)?;
    let b = b.context("a name of partition 2")?;
    on_hosts(2, |hosts| {
        let host = first(hosts)?;
        let key = (1, 1);
        let _read = host.bind(key, [1; 16], (TENANT, 1, Epoch::ZERO), 0)?;
        let step = host.write_bound(key, &changeset(vec![insert(&a, &knows, &b)]), 0)?;
        assert_eq!(step, BoundStep::Waiting);
        // Partition 2 never turns, so nothing is answered.
        for (now_us, resends) in [(999_999, 0), (1_000_000, 1), (1_999_999, 1), (2_000_000, 2)] {
            block_on(host.turn(now_us))??;
            assert_eq!(host.resends(), resends, "at {now_us} µs");
        }
        Ok(())
    })
}

/// A host whose store applied the last sequence of epoch 2, epoch 1 its latest close: its open epoch is full.
fn on_a_full_epoch(body: impl FnOnce(&mut Host<'_>) -> TestResult) -> TestResult {
    let placement = PlacementMap::even(1, NodeId::new(1))?;
    let id = pid(1)?;
    let part = node(id, &placement, epoch(1)?)?;
    let applier = part.applier();
    let nothing = applier
        .begin(TENANT, applier.quads().lists().applied_ts())?
        .finish()?;
    let last = Version::commit(epoch(2)?, Version::COMMIT_SEQUENCE_MAX)?;
    let _applied = applier.apply_round(vec![(last.to_u64(), nothing)])?;
    let pipeline = Pipeline::new(applier, sim_log(id)?, RoundBounds::new(u32::MAX, u32::MAX))?
        .with_partition(id.get(), epoch(1)?, CloseRequest::default(), &[])
        .bound(id, part.placement());
    let mailbox = PartitionMailbox::mesh(&[id])
        .into_iter()
        .next()
        .context("a mailbox")?;
    let mut host = PartitionHost::new(&part, pipeline, mailbox);
    body(&mut host)
}

/// A partition whose open epoch is full asks the sequencer to close it, once (commit-rounds, "A full epoch asks for a close
/// and waits").
#[test]
fn test_a_full_epoch_asks_for_its_close_once() -> TestResult {
    let (a, knows, _) = names(1)?;
    on_a_full_epoch(|host| {
        assert_eq!(host.take_close_request(), None, "nothing asked yet");
        let _ticket = host.submit(changeset(vec![insert_literal(&a, &knows, "v")]), 0)?;
        for _ in 0..3 {
            block_on(host.turn(0))??;
        }
        assert_eq!(host.take_close_request(), Some(epoch(2)?));
        assert_eq!(host.take_close_request(), None, "handed out once");
        Ok(())
    })
}

/// A counter on a bound transaction's own subject, naming nothing the partition does not hold, is done at once; one whose
/// subject another partition owns is refused, ending the transaction; one whose predicate is homed on another partition and
/// not yet held waits for its home's answer, and is then ready.
#[test]
fn test_a_bound_operation_is_done_refused_or_waits_for_its_references() -> TestResult {
    use crate::utils::cluster::ex;
    use oxrdf::GraphName;
    use ultragraphdb_store_apply::Operation;
    let cluster = Cluster::even(2)?;
    let (a, knows) = (cluster.name_in(1, "a")?, cluster.name_in(1, "knows")?);
    let (b, elsewhere) = (cluster.name_in(2, "b")?, cluster.name_in(2, "p")?);
    let counter = |subject: &str, predicate: &str| Operation::CounterAdd {
        graph: GraphName::DefaultGraph,
        subject: ex(subject).into(),
        predicate: ex(predicate),
        delta: 1,
    };
    on_hosts(2, |hosts| {
        let _held = first(hosts)?.submit(changeset(vec![insert_literal(&a, &knows, "v")]), 0)?;
        let _ended = turn_all(hosts, 4, 0)?;
        let (own, foreign, waiting) = ((1, 1), (1, 2), (1, 3));
        for (key, identity) in [(own, 1), (foreign, 2), (waiting, 3)] {
            let _read = first(hosts)?.bind(key, [identity; 16], (TENANT, 1, Epoch::ZERO), 0)?;
        }
        let late = (1, 4);
        let _read = first(hosts)?.bind(late, [4; 16], (TENANT, 1, Epoch::ZERO), 0)?;
        let host = first(hosts)?;
        assert_eq!(
            host.operate_bound(own, counter(&a, &knows), 0)?,
            BoundStep::Done
        );
        assert_eq!(
            host.operate_bound(own, counter(&a, &knows), 0)?,
            BoundStep::Done,
            "a second"
        );
        let past_limit = 3_600 * 1_000_000;
        assert!(
            host.operate_bound(late, counter(&a, &knows), past_limit)
                .is_err()
        );
        assert!(
            host.bound_read(&[4; 16]).is_none(),
            "past its limit it ended"
        );
        assert!(host.operate_bound(foreign, counter(&b, &knows), 0).is_err());
        assert!(host.bound_read(&[2; 16]).is_none(), "the refusal ended it");
        let step = host.operate_bound(waiting, counter(&a, &elsewhere), 0)?;
        assert_eq!(step, BoundStep::Waiting);
        let _answered = turn_all(hosts, 4, 0)?;
        assert_eq!(first(hosts)?.take_bound_ready(), vec![waiting]);
        Ok(())
    })
}

/// A Drop whose predicate the store fails to read is refused with the store's error, under no ticket.
#[test]
fn test_a_drop_the_store_fails_under_is_refused() -> TestResult {
    use ultragraphdb_cluster_partition::PartitionErrorEnum;
    use ultragraphdb_store_sim::Fault;
    let (_, knows, _) = names(1)?;
    on_hosts(1, |hosts| {
        let host = first(hosts)?;
        let before = host.last_ticket();
        host.partition()
            .applier()
            .quads()
            .store()
            .inject(Fault::FailNextRead);
        let predicate = crate::utils::cluster::ex(&knows);
        let refused = host.commit_drop(
            (TENANT, 1),
            DropRequest::Predicate(predicate.as_ref()),
            0,
            (7, 1),
            0,
        );
        assert!(
            matches!(&refused, Err(e) if matches!(e.0, PartitionErrorEnum::Txn(_))),
            "{refused:?}"
        );
        assert_eq!(host.last_ticket(), before, "no ticket taken");
        Ok(())
    })
}

/// A refusal answering a registration a bound transaction did not ask, at an index past its references, still ends the
/// transaction, which is named refused with it.
#[test]
fn test_a_refusal_at_an_index_past_the_references_ends_the_transaction() -> TestResult {
    use ultragraphdb_cluster_partition::PartitionError;
    use ultragraphdb_protocol_wire::ErrorCode;
    let (a, knows, b) = names(2)?;
    let b = b.context("a name of partition 2")?;
    on_hosts(2, |hosts| {
        let host = first(hosts)?;
        let key = (1, 1);
        let _read = host.bind(key, [1; 16], (TENANT, 1, Epoch::ZERO), 0)?;
        let step = host.write_bound(key, &changeset(vec![insert(&a, &knows, &b)]), 0)?;
        assert_eq!(step, BoundStep::Waiting);
        let asked = host.bound_ticket(key).context("its ticket")?;
        let refusal = PartitionError::refused(ErrorCode::Busy.number());
        host.mailbox()
            .sender(pid(1)?)
            .context("a sender")?
            .send(PartitionMessage::Answered {
                write: asked,
                index: usize::MAX,
                answer: Err(refusal.clone()),
            })?;
        block_on(host.turn(0))??;
        assert_eq!(host.take_bound_refused(), vec![(key, refusal)]);
        assert!(host.bound_read(&[1; 16]).is_none(), "the refusal ended it");
        Ok(())
    })
}

/// A bound operation waiting for its references whose transaction is past its limit when they are answered is refused
/// then, ending the transaction.
#[test]
fn test_a_bound_operation_past_its_limit_at_its_answer_is_refused() -> TestResult {
    use crate::utils::cluster::ex;
    use oxrdf::GraphName;
    use ultragraphdb_store_apply::Operation;
    let cluster = Cluster::even(2)?;
    let (a, knows) = (cluster.name_in(1, "a")?, cluster.name_in(1, "knows")?);
    let elsewhere = cluster.name_in(2, "p")?;
    let counter = Operation::CounterAdd {
        graph: GraphName::DefaultGraph,
        subject: ex(&a).into(),
        predicate: ex(&elsewhere),
        delta: 1,
    };
    on_hosts(2, |hosts| {
        let _held = first(hosts)?.submit(changeset(vec![insert_literal(&a, &knows, "v")]), 0)?;
        let _ended = turn_all(hosts, 4, 0)?;
        let key = (1, 1);
        let host = first(hosts)?;
        let _read = host.bind(key, [1; 16], (TENANT, 1, Epoch::ZERO), 0)?;
        assert_eq!(host.operate_bound(key, counter, 0)?, BoundStep::Waiting);
        let limit_us = host.bound_limit_ms() * 1_000;
        let _answered = turn_all(hosts, 4, limit_us + 1)?;
        let late = first(hosts)?;
        let refused: Vec<BoundKey> = late
            .take_bound_refused()
            .into_iter()
            .map(|(refused, _)| refused)
            .collect();
        assert_eq!(refused, vec![key]);
        assert!(late.take_bound_ready().is_empty());
        assert!(late.bound_read(&[1; 16]).is_none(), "the refusal ended it");
        Ok(())
    })
}

/// Delivers to partition 1 the closes from 2 through `through`, the close of `at - 1` announcing `(version, at)` of `history`
/// for the tenant, so that version takes effect at epoch `at` (`PROTOCOL.md` §11.12: a close's catalog versions take effect at
/// the epoch one above it). Epoch 1 is closed already, so `at` is 3 or above.
fn deliver_history(hosts: &mut [Host<'_>], history: &[(u64, u64)], through: u64) -> TestResult {
    use ultragraphdb_cluster_sequencer::CatalogVersion;
    use ultragraphdb_core_ids::Tenant;
    let sender = hosts
        .first()
        .and_then(|h| h.mailbox().sender(pid(1).ok()?))
        .context("a sender")?;
    for at in 2..=through {
        let announced = history
            .iter()
            .filter(|(_, effective)| *effective == at + 1)
            .map(|(version, _)| CatalogVersion::new(Tenant::new(TENANT), *version))
            .collect();
        sender.send(PartitionMessage::Close(Close::new(
            epoch(at)?,
            0,
            announced,
            Vec::new(),
        )?))?;
        let _settled = turn_all(hosts, 2, 0)?;
    }
    Ok(())
}

/// A Drop's commit version or its refusal's code and detail, and whether it removed the fact written.
type Dropped = (Result<u64, (u16, u64)>, bool);

/// What a Drop of `drop` naming `catalog` came to at partition 1, its round in the open epoch, and whether it removed the fact
/// [`written`] wrote: the commit version, or the code and detail its refusal answers with on the wire.
fn dropped(hosts: &mut [Host<'_>], drop: DropRequest<'_>, catalog: u64) -> TestResult<Dropped> {
    let (a, knows, _) = names(1)?;
    let before = holds(first(hosts)?, &a, &knows)?;
    assert!(before, "the fact written is held before the Drop");
    let ticket = first(hosts)?.commit_drop((TENANT, 1), drop, catalog, (0, 0), 0)?;
    let ended = turn_all(hosts, 3, 0)?;
    let outcome = ended
        .iter()
        .find(|o| o.ticket() == ticket)
        .context("the Drop's outcome")?;
    let answered = match outcome.outcome() {
        Ok(version) => Ok(*version),
        Err(refusal) => {
            let reply = ultragraphdb_common_storage::error_reply(refusal);
            Err((reply.code().number(), reply.detail()))
        }
    };
    let after = holds(first(hosts)?, &a, &knows)?;
    Ok((answered, before && !after))
}

/// Versions 1 to 4 of the tenant, taking effect at epochs 3 to 6.
const HISTORY: [(u64, u64); 4] = [(1, 3), (2, 4), (3, 5), (4, 6)];

/// Code 2 (Conflict), kind 5 (Catalog): `PROTOCOL.md` §6.5 and §9.4's table of conflict kinds.
const CATALOG_CONFLICT: (u16, u64) = (2, 5);

/// Writes a fact for a Drop to find, in the default graph and in `<g>`, its round settled: a Drop of a predicate or a graph
/// that finds nothing to remove records no drop (`Prepared::drop`).
fn written(hosts: &mut [Host<'_>]) -> TestResult {
    let (a, knows, _) = names(1)?;
    let in_graph = ultragraphdb_core_mapping::Change::Insert {
        graph: oxrdf::GraphName::NamedNode(crate::utils::cluster::ex("g")),
        subject: crate::utils::cluster::ex(&a).into(),
        predicate: crate::utils::cluster::ex(&knows),
        object: oxrdf::Literal::new_simple_literal("w").into(),
    };
    let _ticket = first(hosts)?.submit(
        changeset(vec![insert_literal(&a, &knows, "v"), in_graph]),
        0,
    )?;
    let _settled = turn_all(hosts, 3, 0)?;
    Ok(())
}

/// C4.11: version 4 in effect at the round's epoch, past the epoch it took effect at; a Drop of kind 3 and one of kind 4
/// naming `catalog` 4 commit and remove what they drop.
#[test]
fn test_a_drop_naming_the_version_in_effect_commits() -> TestResult {
    for drop in [DropRequest::Data, DropRequest::Tenant] {
        on_hosts(1, |hosts| {
            // Written before the versions take effect: a write executes against the version its partition installed.
            written(hosts)?;
            deliver_history(hosts, &HISTORY, 7)?;
            let (answered, changed) = dropped(hosts, drop, 4)?;
            assert!(answered.is_ok(), "{answered:?}");
            assert!(changed, "the Drop removed what it names");
            Ok(())
        })?;
    }
    Ok(())
}

/// C4.12 and C4.13: version 4 in effect; a Drop of kind 3 or 4 naming version 3, one below, or 5, one above, is refused with
/// code 2, kind 5, and removes nothing; so is one naming 0, a tenant holding none, the `catalog` a client leaving it unset sends.
#[test]
fn test_a_drop_naming_another_version_is_refused_and_removes_nothing() -> TestResult {
    for drop in [DropRequest::Data, DropRequest::Tenant] {
        for catalog in [0, 3, 5] {
            on_hosts(1, |hosts| {
                written(hosts)?;
                deliver_history(hosts, &HISTORY, 7)?;
                let (answered, changed) = dropped(hosts, drop, catalog)?;
                assert_eq!(answered, Err(CATALOG_CONFLICT), "catalog {catalog}");
                assert!(!changed, "a refused Drop removes nothing");
                Ok(())
            })?;
        }
    }
    Ok(())
}

/// C4.14: version 4 takes effect at exactly the round's epoch, 6: a Drop of kind 4 naming 4 commits, and one naming 3, the
/// version in effect one epoch before, is refused.
#[test]
fn test_a_drop_at_the_epoch_a_version_takes_effect_names_that_version() -> TestResult {
    on_hosts(1, |hosts| {
        written(hosts)?;
        deliver_history(hosts, &HISTORY, 5)?;
        assert_eq!(
            first(hosts)?.delivered(),
            epoch(5)?,
            "the round's epoch is 6"
        );
        let (stale, removed) = dropped(hosts, DropRequest::Tenant, 3)?;
        assert_eq!(stale, Err(CATALOG_CONFLICT));
        assert!(!removed, "a refused Drop removes nothing");
        let (current, changed) = dropped(hosts, DropRequest::Tenant, 4)?;
        assert!(current.is_ok(), "{current:?}");
        assert!(changed);
        Ok(())
    })
}

/// C4.15: a Drop of kind 1 or 2 names `catalog` 0 and is not checked, whatever version is in effect.
#[test]
fn test_a_drop_of_a_predicate_or_a_graph_is_not_checked() -> TestResult {
    let (_, knows, _) = names(1)?;
    let predicate = crate::utils::cluster::ex(&knows);
    let graph = crate::utils::cluster::ex("g");
    for drop in [
        DropRequest::Predicate(predicate.as_ref()),
        DropRequest::Graph(oxrdf::GraphNameRef::NamedNode(graph.as_ref())),
    ] {
        on_hosts(1, |hosts| {
            // Written before the versions take effect: a write executes against the version its partition installed.
            written(hosts)?;
            deliver_history(hosts, &HISTORY, 7)?;
            let (answered, _) = dropped(hosts, drop, 0)?;
            assert!(answered.is_ok(), "{drop:?}: {answered:?}");
            Ok(())
        })?;
    }
    Ok(())
}

/// A tenant holding no catalog version has version 0 in effect: a Drop of kind 3 naming 0 commits, naming 1 is refused.
#[test]
fn test_a_tenant_holding_no_version_drops_under_version_zero() -> TestResult {
    on_hosts(1, |hosts| {
        written(hosts)?;
        let (none, removed) = dropped(hosts, DropRequest::Data, 1)?;
        assert_eq!(none, Err(CATALOG_CONFLICT));
        assert!(!removed, "a refused Drop removes nothing");
        let (zero, changed) = dropped(hosts, DropRequest::Data, 0)?;
        assert!(zero.is_ok(), "{zero:?}");
        assert!(changed);
        Ok(())
    })
}

/// C4.15 for a Drop that finds nothing to remove: version 4 in effect, a Drop of kind 1 naming a predicate the tenant does not
/// hold and one of kind 2 naming a graph it does not hold are checked against no version and commit, as a Drop removing
/// something does, removing nothing (`PROTOCOL.md` §6.5; §11.13, *Drops*: a Drop run again "removes nothing more").
#[test]
fn test_a_drop_of_a_predicate_or_a_graph_finding_nothing_is_not_checked() -> TestResult {
    let predicate = crate::utils::cluster::ex("no-such-predicate");
    let graph = crate::utils::cluster::ex("no-such-graph");
    for drop in [
        DropRequest::Predicate(predicate.as_ref()),
        DropRequest::Graph(oxrdf::GraphNameRef::NamedNode(graph.as_ref())),
    ] {
        on_hosts(1, |hosts| {
            written(hosts)?;
            deliver_history(hosts, &HISTORY, 7)?;
            let (answered, changed) = dropped(hosts, drop, 0)?;
            assert!(answered.is_ok(), "{drop:?}: {answered:?}");
            assert!(!changed, "{drop:?} removed nothing");
            Ok(())
        })?;
    }
    Ok(())
}

/// C4.18: a Drop handed over with a write behind it, both in one turn, is the last of its round whether it removes
/// something or finds nothing to remove (`PROTOCOL.md` §6.5; §12.2): the write lands in the next round, and the log holds the
/// two rounds of one decision each.
#[test]
fn test_a_drop_finding_nothing_closes_its_round_as_one_removing_something() -> TestResult {
    use ultragraphdb_traits_log::{OrderedLog, Position};
    let (a, knows, _) = names(1)?;
    let held = crate::utils::cluster::ex(&knows);
    let predicate = crate::utils::cluster::ex("no-such-predicate");
    let graph = crate::utils::cluster::ex("no-such-graph");
    for drop in [
        DropRequest::Predicate(held.as_ref()),
        DropRequest::Predicate(predicate.as_ref()),
        DropRequest::Graph(oxrdf::GraphNameRef::NamedNode(graph.as_ref())),
    ] {
        on_hosts(1, |hosts| {
            written(hosts)?;
            let host = first(hosts)?;
            host.hold_rounds(0)?;
            let tickets = [
                host.commit_drop((TENANT, 1), drop, 0, (0, 0), 0)?,
                host.submit(changeset(vec![insert_literal(&a, "other", "v")]), 0)?,
            ];
            host.release_rounds(0)?;
            let ended = turn_all(hosts, 3, 0)?;
            for ticket in tickets {
                let version = version_of(&ended, ticket)?;
                assert!(version > 0, "{drop:?}: ticket {ticket} committed");
            }
            let (_, mut log) = hosts.pop().context("partition 1")?.into_parts();
            let rounds = block_on(log.read(Position::new(1), u32::MAX))??;
            let last: Vec<u32> = rounds
                .iter()
                .rev()
                .take(tickets.len())
                .map(|r| r.transactions())
                .collect();
            assert_eq!(
                last,
                vec![1; tickets.len()],
                "{drop:?}: the Drop's round closed after it, the write in the next"
            );
            Ok(())
        })?;
    }
    Ok(())
}
