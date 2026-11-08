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
use crate::utils::hosts::{Host, on_hosts, sim_log, turn_all};

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
        let ticket = first(hosts)?.commit_drop((TENANT, 1), DropRequest::Data, (7, 1), 0)?;
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
