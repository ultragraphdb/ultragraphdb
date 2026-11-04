/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_partition::{PartitionError, PartitionErrorEnum, PartitionMessage};
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_cluster_sequencer::Close;
use ultragraphdb_core_ids::Version;
use ultragraphdb_store_quads::namespace_base;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transactions_txn::{TxnError, TxnErrorEnum};

use crate::utils::cluster::{Cluster, changeset, epoch, group, insert, insert_literal, pid};
use crate::utils::hosts::{on_hosts, turn_all};

/// The names `a` and `knows` in partition 1 and `b` in partition 2.
fn names() -> TestResult<(String, String, String)> {
    let cluster = Cluster::even(2)?;
    Ok((
        cluster.name_in(1, "a")?,
        cluster.name_in(1, "knows")?,
        cluster.name_in(2, "b")?,
    ))
}

/// The commit version `outcomes` give `ticket`.
fn version_of(
    outcomes: &[ultragraphdb_cluster_partition::PartitionOutcome],
    ticket: u64,
) -> TestResult<u64> {
    let outcome = outcomes
        .iter()
        .find(|o| o.ticket() == ticket)
        .context("the write's outcome")?;
    Ok(*outcome
        .outcome()
        .as_ref()
        .map_err(|e| ultragraphdb_tool_bdd::TestError::new(e.to_string()))?)
}

/// A write naming only what its partition holds or owns goes to the commit step at once, once `rdf:type` is got.
#[test]
fn test_a_local_write_commits_within_the_turns_it_takes() -> TestResult {
    let (a, knows, _) = names()?;
    on_hosts(2, |hosts| {
        let host = hosts.first_mut().context("partition 1")?;
        let ticket = host.submit(changeset(vec![insert_literal(&a, &knows, "v")]), 0)?;
        let ended = turn_all(hosts, 3, 0)?;
        let version = version_of(&ended, ticket)?;
        assert_eq!(Version::from_u64(version).epoch(), epoch(2)?);
        assert!(hosts.iter().all(|h| h.parked() == 0 && !h.needs_sync()));
        Ok(())
    })
}

/// A reference to another partition's entity waits for its home's durable answer, the home's durable member and copied
/// entry then found at both ends, and the outcome counts what it waited for.
#[test]
fn test_a_reference_waits_for_its_homes_answer() -> TestResult {
    let (a, knows, b) = names()?;
    on_hosts(2, |hosts| {
        let host = hosts.first_mut().context("partition 1")?;
        let ticket = host.submit(changeset(vec![insert(&a, &knows, &b)]), 0)?;
        assert_eq!(host.parked(), 1, "it waits for its answers");
        let ended = turn_all(hosts, 3, 500)?;
        let outcome = ended
            .iter()
            .find(|o| o.ticket() == ticket)
            .context("its outcome")?;
        assert!(outcome.outcome().is_ok(), "{outcome:?}");
        assert!(outcome.registrations() >= 1);
        assert_eq!(
            outcome.waited_us(),
            500,
            "from the submit to the last answer"
        );
        Ok(())
    })
}

/// A home that has seen a later close than the referrer answers with it, and the referrer's commit is held until that
/// close is delivered to it: no outcome before, an epoch above the close after.
#[test]
fn test_a_commit_is_held_until_the_homes_close_is_delivered() -> TestResult {
    let (a, knows, b) = names()?;
    on_hosts(2, |hosts| {
        let close = |at: u64| -> TestResult<Close> {
            Ok(Close::new(epoch(at)?, 0, Vec::new(), Vec::new())?)
        };
        let sender_two = hosts_sender(hosts, 2)?;
        for at in [2, 3] {
            sender_two.send(PartitionMessage::Close(close(at)?))?;
        }
        let _ended = turn_all(hosts, 1, 0)?;
        let host = hosts.first_mut().context("partition 1")?;
        let ticket = host.submit(changeset(vec![insert(&a, &knows, &b)]), 0)?;
        let ended = turn_all(hosts, 3, 0)?;
        assert!(
            ended.iter().all(|o| o.ticket() != ticket),
            "held for close 3: {ended:?}"
        );
        let sender_one = hosts_sender(hosts, 1)?;
        for at in [2, 3] {
            sender_one.send(PartitionMessage::Close(close(at)?))?;
        }
        let released = turn_all(hosts, 2, 0)?;
        let version = version_of(&released, ticket)?;
        assert_eq!(Version::from_u64(version).epoch(), epoch(4)?);
        Ok(())
    })
}

/// A sender to partition `id`, as whoever delivers closes holds one.
fn hosts_sender(
    hosts: &[crate::utils::hosts::Host<'_>],
    id: u32,
) -> TestResult<std::sync::mpsc::Sender<PartitionMessage>> {
    hosts
        .first()
        .and_then(|h| h.mailbox().sender(pid(id).ok()?))
        .context("a sender")
}

/// A registration unanswered past the resend delay goes again; the home answers both, idempotently, and the write ends
/// once: one member, generation 1, one outcome.
#[test]
fn test_a_resent_registration_is_idempotent() -> TestResult {
    let (a, knows, b) = names()?;
    on_hosts(2, |hosts| {
        let host = hosts.first_mut().context("partition 1")?;
        let ticket = host.submit(changeset(vec![insert(&a, &knows, &b)]), 0)?;
        // Partition 1 turns past the delay before partition 2 has turned at all.
        crate::utils::cluster::block_on(host.turn(1_000_000))??;
        let ended = turn_all(hosts, 4, 1_000_001)?;
        assert_eq!(ended.iter().filter(|o| o.ticket() == ticket).count(), 1);
        let _version = version_of(&ended, ticket)?;
        let id = hosts
            .get(1)
            .context("partition 2")?
            .partition()
            .applier()
            .quads()
            .id_of(
                crate::utils::cluster::NS,
                crate::utils::cluster::ex(&b).as_str(),
            )?
            .context("the home holds the ID")?;
        let set = hosts
            .get(1)
            .context("partition 2")?
            .partition()
            .applier()
            .quads()
            .referrers(crate::utils::cluster::NS, id, u64::MAX)?;
        assert_eq!(
            set.member(group(&a)?).map(|m| m.generation().get()),
            Some(1),
            "the repeat changed nothing"
        );
        Ok(())
    })
}

/// A write whose answers never come ends at its limit, naming it.
#[test]
fn test_a_write_past_its_limit_ends() -> TestResult {
    let (a, knows, b) = names()?;
    on_hosts(2, |hosts| {
        let host = hosts.first_mut().context("partition 1")?;
        let ticket = host.submit(changeset(vec![insert(&a, &knows, &b)]), 0)?;
        crate::utils::cluster::block_on(host.turn(10_000_001))??;
        let ended = host.take_outcomes();
        let outcome = ended
            .iter()
            .find(|o| o.ticket() == ticket)
            .context("its outcome")?;
        assert_eq!(
            outcome.outcome(),
            &Err(PartitionError::from(TxnError::past_limit(10_000)))
        );
        assert!(matches!(
            outcome.outcome(),
            Err(PartitionError(PartitionErrorEnum::Txn(TxnError(
                TxnErrorEnum::PastLimit { .. }
            ))))
        ));
        assert_eq!(host.parked(), 0);
        Ok(())
    })
}

/// A home's refusal is the write's outcome: a skolem IRI of the home's buckets its dictionary does not hold.
#[test]
fn test_a_homes_refusal_ends_the_write() -> TestResult {
    let (a, knows, _) = names()?;
    let placement = PlacementMap::even(2, NodeId::new(1))?;
    let bucket = placement
        .ranges_at(epoch(1)?)
        .into_iter()
        .find(|(range, p)| {
            p.get() == 2
                && range.start() >> 16
                    == crate::utils::cluster::bucket(&a)
                        .map(|b| b.get() >> 16)
                        .unwrap_or(0)
        })
        .map(|(range, _)| range.start())
        .context("a bucket of partition 2 in the namespace's run")?;
    let forged = format!(
        "{}.well-known/genid/{bucket:06x}-{}",
        namespace_base(crate::utils::cluster::NS),
        "1".repeat(26)
    );
    let object = oxrdf::NamedNode::new(&forged)?;
    let change = ultragraphdb_core_mapping::Change::Insert {
        graph: oxrdf::GraphName::DefaultGraph,
        subject: crate::utils::cluster::ex(&a).into(),
        predicate: crate::utils::cluster::ex(&knows),
        object: object.into(),
    };
    on_hosts(2, |hosts| {
        let host = hosts.first_mut().context("partition 1")?;
        let ticket = host.submit(changeset(vec![change]), 0)?;
        let ended = turn_all(hosts, 4, 0)?;
        let outcome = ended
            .iter()
            .find(|o| o.ticket() == ticket)
            .context("its outcome")?;
        assert!(outcome.outcome().is_err(), "{outcome:?}");
        assert!(hosts.iter().all(|h| h.parked() == 0));
        Ok(())
    })
}
