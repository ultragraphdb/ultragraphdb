/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::GraphNameRef;
use ultragraphdb_cluster_partition::{
    BoundStep, MAILBOX_PER_TURN_MAX, PartitionError, PartitionErrorEnum, PartitionMessage,
};
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_cluster_sequencer::Close;
use ultragraphdb_core_ids::{Epoch, Version};
use ultragraphdb_store_quads::tenant_base;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transactions_txn::{TxnError, TxnErrorEnum};

use crate::utils::cluster::{
    Cluster, TENANT, begin, changeset, epoch, ex, group, insert, insert_literal, pid,
};
use crate::utils::hosts::{Host, on_hosts, turn_all};

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
                crate::utils::cluster::TENANT,
                crate::utils::cluster::ex(&b).as_str(),
            )?
            .context("the home holds the ID")?;
        let set = hosts
            .get(1)
            .context("partition 2")?
            .partition()
            .applier()
            .quads()
            .referrers(crate::utils::cluster::TENANT, id, u64::MAX)?;
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
        .context("a bucket of partition 2 in the tenant's run")?;
    let forged = format!(
        "{}.well-known/genid/{bucket:06x}-{}",
        tenant_base(crate::utils::cluster::TENANT),
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

/// Unanswered registrations go again at the delay and not before, each time it passes; a write ends past its limit and
/// not at it.
#[test]
fn test_resends_and_the_limit_fall_at_their_boundaries() -> TestResult {
    let (a, knows, b) = names()?;
    on_hosts(2, |hosts| {
        let host = hosts.first_mut().context("partition 1")?;
        let ticket = host.submit(changeset(vec![insert(&a, &knows, &b)]), 0)?;
        crate::utils::cluster::block_on(host.turn(999_999))??;
        assert_eq!(host.resends(), 0, "before the delay");
        crate::utils::cluster::block_on(host.turn(1_000_000))??;
        assert_eq!(host.resends(), 1, "at the delay");
        crate::utils::cluster::block_on(host.turn(1_999_999))??;
        assert_eq!(host.resends(), 1, "the delay counts from the last send");
        crate::utils::cluster::block_on(host.turn(2_000_000))??;
        assert_eq!(host.resends(), 2);
        crate::utils::cluster::block_on(host.turn(10_000_000))??;
        assert!(
            host.take_outcomes().is_empty(),
            "at the limit it still waits"
        );
        assert_eq!(host.parked(), 1);
        crate::utils::cluster::block_on(host.turn(10_000_001))??;
        let ended = host.take_outcomes();
        assert_eq!(ended.first().map(|o| o.ticket()), Some(ticket));
        assert_eq!(host.parked(), 0);
        Ok(())
    })
}

/// What was decided waits for a sync until a turn syncs it, and the outcome names how many registrations the write waited
/// for: every reference the partition's own scan of the write names.
#[test]
fn test_a_turn_syncs_what_was_decided_and_an_outcome_counts_its_registrations() -> TestResult {
    let cluster = Cluster::even(2)?;
    let (a, knows, b) = names()?;
    let c = cluster.name_where("c", |n| Ok(n != b && cluster.owner(n)? == 2))?;
    let changes = vec![insert(&a, &knows, &b), insert(&a, &knows, &c)];
    let expected = cluster.references(1, changes.clone())?.len();
    assert!(expected >= 2, "both objects are partition 2's");
    on_hosts(2, |hosts| {
        // A first write gets what the tenant's catalog names; a second has nothing to register.
        let _first = hosts
            .first_mut()
            .context("partition 1")?
            .submit(changeset(Vec::new()), 0)?;
        let _settled = turn_all(hosts, 3, 0)?;
        let local = hosts.first_mut().context("partition 1")?;
        let _second = local.submit(changeset(Vec::new()), 0)?;
        assert!(local.needs_sync(), "decided at once, and not yet synced");
        let _ended = turn_all(hosts, 1, 0)?;
        assert!(!hosts.first().context("partition 1")?.needs_sync());
        let ticket = hosts
            .first_mut()
            .context("partition 1")?
            .submit(changeset(changes), 0)?;
        let ended = turn_all(hosts, 3, 0)?;
        let outcome = ended
            .iter()
            .find(|o| o.ticket() == ticket)
            .context("its outcome")?;
        assert!(outcome.outcome().is_ok(), "{outcome:?}");
        assert_eq!(outcome.registrations(), expected);
        Ok(())
    })
}

/// A Bind's identity names one transaction while the partition holds it: a repeat is refused, and a commit or an abort frees it.
#[test]
fn test_a_bound_transactions_identity_is_held_until_it_ends() -> TestResult {
    on_hosts(1, |hosts| {
        let host = hosts.first_mut().context("the host")?;
        let (key, later, identity) = ((1, 1), (1, 3), [5; 16]);
        let read_version = host.bind(key, identity, (0, 9, epoch(0)?), 0)?;
        let read = host.bound_read(&identity).context("the transaction")?;
        assert_eq!(read.key(), key);
        assert_eq!(read.read_version(), read_version);
        assert_eq!(read.principal(), 9);
        assert_eq!(read.tenant(), 0);
        assert_eq!(
            host.bind(later, identity, (0, 9, epoch(0)?), 0),
            Err(PartitionError::transaction_held(identity))
        );
        host.abort_bound(key);
        assert_eq!(host.bound_read(&identity), None);
        let _again = host.bind(later, identity, (0, 9, epoch(0)?), 0)?;
        let _ticket = host.commit_bound(later, 0, 0, 0)?;
        assert_eq!(host.bound_read(&identity), None);
        let _once_more = host.bind(key, identity, (0, 9, epoch(0)?), 0)?;
        Ok(())
    })
}

/// A home's refusal ends the transaction waiting on it, and names it with the refusal.
#[test]
fn test_a_homes_refusal_ends_the_transaction_waiting_on_it() -> TestResult {
    let (_, knows, _) = names()?;
    let foreign = Cluster::even(2)?.name_in(2, &knows)?;
    on_hosts(2, |hosts| {
        let host = hosts.first_mut().context("partition 1")?;
        let (key, identity) = ((1, 1), [5; 16]);
        let _read = host.bind(key, identity, (0, 9, epoch(0)?), 0)?;
        let step = host.read_terms_bound(key, &[ex(&foreign)], 0)?;
        assert_eq!(step, BoundStep::Waiting);
        // The home's answer never comes: what its mailbox delivers instead is a refusal of the ticket the transaction asked under.
        let ticket = host.bound_ticket(key).context("its references' ticket")?;
        let sender = host.mailbox().sender(pid(1)?).context("the mailbox")?;
        sender.send(PartitionMessage::Answered {
            write: ticket,
            index: 0,
            answer: Err(PartitionError::refused(7)),
        })?;
        let _ended = turn_all(hosts, 1, 0)?;
        let turned = hosts.first_mut().context("partition 1")?;
        assert_eq!(turned.take_bound_ready(), []);
        assert_eq!(
            turned.take_bound_refused(),
            [(key, PartitionError::refused(7))]
        );
        assert_eq!(turned.bound_read(&identity), None);
        Ok(())
    })
}

/// A skolem IRI of another partition's bucket that its home holds none of is what one of the partition's own is: no entry, the
/// transaction open, recording nothing, and no second asking. A write naming the same IRI is refused, its statement being forged.
#[test]
fn test_an_unheld_foreign_skolem_iri_answers_no_entry_and_leaves_a_read_open() -> TestResult {
    let (_, knows, _) = names()?;
    let homed = Cluster::even(2)?.name_in(2, &knows)?;
    let bucket = crate::utils::cluster::bucket(&homed)?.get();
    let forged = format!(
        "{}.well-known/genid/{bucket:06x}-{}",
        tenant_base(crate::utils::cluster::TENANT),
        "1".repeat(26)
    );
    let iri = oxrdf::NamedNode::new(&forged)?;
    on_hosts(2, |hosts| {
        let host = hosts.first_mut().context("partition 1")?;
        let (key, identity) = ((1, 1), [5; 16]);
        let _read = host.bind(key, identity, (0, 9, epoch(0)?), 0)?;
        assert_eq!(
            host.read_terms_bound(key, std::slice::from_ref(&iri), 0)?,
            BoundStep::Waiting
        );
        let _ended = turn_all(hosts, 4, 0)?;
        let turned = hosts.first_mut().context("partition 1")?;
        assert_eq!(turned.take_bound_refused(), []);
        assert_eq!(turned.take_bound_ready(), [key]);
        assert_eq!(
            turned.read_terms_bound(key, std::slice::from_ref(&iri), 0)?,
            BoundStep::Done,
            "its home is not asked again"
        );
        assert_eq!(turned.resolve_bound(key, iri.as_ref(), 0)?, None);
        assert!(turned.bound_read(&identity).is_some(), "it goes on");
        Ok(())
    })
}

/// Reads of a transaction waiting on a home wait together, the later one asking nothing until the earlier's answers came, and
/// nothing an answer brings is lost to a later asking.
#[test]
fn test_a_read_waits_with_the_read_already_waiting_on_its_home() -> TestResult {
    let (_, knows, b) = names()?;
    let cluster = Cluster::even(2)?;
    let (first, second) = (cluster.name_in(2, &knows)?, cluster.name_in(2, &b)?);
    on_hosts(2, |hosts| {
        let host = hosts.first_mut().context("partition 1")?;
        let key = (1, 1);
        let _read = host.bind(key, [5; 16], (0, 9, epoch(0)?), 0)?;
        assert_eq!(
            host.read_terms_bound(key, &[ex(&first)], 0)?,
            BoundStep::Waiting
        );
        let ticket = host.bound_ticket(key);
        assert_eq!(
            host.read_terms_bound(key, &[ex(&second)], 0)?,
            BoundStep::Waiting
        );
        assert_eq!(host.bound_ticket(key), ticket, "it asked nothing");
        let _ended = turn_all(hosts, 4, 0)?;
        let turned = hosts.first_mut().context("partition 1")?;
        assert_eq!(turned.take_bound_ready(), [key]);
        assert_eq!(
            turned.read_terms_bound(key, &[ex(&first)], 0)?,
            BoundStep::Done
        );
        assert_eq!(
            turned.read_terms_bound(key, &[ex(&second)], 0)?,
            BoundStep::Waiting,
            "now it asks"
        );
        let _ended_too = turn_all(hosts, 4, 0)?;
        let last = hosts.first_mut().context("partition 1")?;
        assert_eq!(last.take_bound_ready(), [key]);
        assert!(last.resolve_bound(key, ex(&first).as_ref(), 0)?.is_some());
        assert!(last.resolve_bound(key, ex(&second).as_ref(), 0)?.is_some());
        Ok(())
    })
}

/// A term the partition holds no ID for, foreign and asked of no home, is refused rather than given an ID of its own.
#[test]
fn test_a_foreign_term_never_takes_a_local_id() -> TestResult {
    let (_, knows, _) = names()?;
    let foreign = Cluster::even(2)?.name_in(2, &knows)?;
    on_hosts(2, |hosts| {
        let host = hosts.first_mut().context("partition 1")?;
        let (key, identity) = ((1, 1), [5; 16]);
        let _read = host.bind(key, identity, (0, 9, epoch(0)?), 0)?;
        assert!(host.resolve_bound(key, ex(&foreign).as_ref(), 0).is_err());
        assert_eq!(host.bound_read(&identity), None, "the refusal ends it");
        let held = host
            .partition()
            .applier()
            .quads()
            .id_of(0, ex(&foreign).as_str())?;
        assert_eq!(held, None);
        Ok(())
    })
}

/// A transaction's objects read of a predicate another partition owns, asked of no home, mints no ID either.
#[test]
fn test_a_transactions_read_of_a_foreign_predicate_mints_no_id() -> TestResult {
    let cluster = Cluster::even(2)?;
    let (subject, foreign) = (cluster.name_in(1, "a")?, cluster.name_in(2, "knows")?);
    let part = cluster.part(1)?;
    let mut txn = begin(part)?;
    let mut out = Vec::new();
    let refused = txn.objects_into(
        GraphNameRef::DefaultGraph,
        ex(&subject).as_ref().into(),
        ex(&foreign).as_ref(),
        0,
        &mut out,
    );
    assert!(refused.is_err(), "{refused:?}");
    assert!(out.is_empty());
    assert_eq!(
        part.applier().quads().id_of(0, ex(&foreign).as_str())?,
        None
    );
    Ok(())
}

/// A partition's plain reads: closed epochs, the watermark and a pin of a version it retains.
#[test]
fn test_a_host_answers_what_a_plain_read_asks() -> TestResult {
    on_hosts(1, |hosts| {
        let host = hosts.first_mut().context("the host")?;
        assert_eq!(host.closed(), epoch(1)?);
        let pin = host.pin(host.watermark())?;
        assert_eq!(pin.version(), host.watermark());
        assert_eq!(host.dictionary(b"\x1bnot a key")?, None);
        assert!(host.recall(7, 1).is_err(), "no such session");
        Ok(())
    })
}

/// Partition 1's host.
fn first_host<'h, 'a>(hosts: &'h mut [Host<'a>]) -> TestResult<&'h mut Host<'a>> {
    hosts.first_mut().context("partition 1")
}

/// A bound transaction's first request whose first subject another partition owns names that owner, and a later request's subject
/// there is cross-partition, each ending the transaction.
#[test]
fn test_a_first_subject_outside_is_a_stale_map_and_a_later_one_cross_partition() -> TestResult {
    let (own, _, foreign) = names()?;
    on_hosts(2, |hosts| {
        let host = hosts.first_mut().context("partition 1")?;
        let key = (1, 1);
        let _read = host.bind(key, [1; 16], (TENANT, 1, epoch(0)?), 0)?;
        let outside = changeset(vec![insert_literal(&foreign, "p", "x")]);
        let refused = host.write_bound(key, &outside, 0);
        let PartitionError(PartitionErrorEnum::WrongPartition { owner, .. }) =
            refused.err().context("a refusal")?
        else {
            return Err(TestError::new("not code 26"));
        };
        assert_eq!(owner, 2);
        assert_eq!(host.bound_read(&[1; 16]), None, "it ended the transaction");
        let later = (1, 2);
        let _later_read = host.bind(later, [2; 16], (TENANT, 1, epoch(0)?), 0)?;
        let inside = changeset(vec![insert_literal(&own, "p", "x")]);
        if host.write_bound(later, &inside, 0)? == BoundStep::Waiting {
            let _ended = turn_all(hosts, 4, 0)?;
            first_host(hosts)?.resume_write(later, &inside, 0)?;
        }
        let later_refused = match first_host(hosts)?.write_bound(later, &outside, 0) {
            Ok(_) => {
                let _ended = turn_all(hosts, 4, 0)?;
                first_host(hosts)?.resume_write(later, &outside, 0).err()
            }
            Err(refusal) => Some(refusal),
        };
        assert!(
            matches!(
                later_refused.context("a refusal")?.0,
                PartitionErrorEnum::Txn(TxnError(TxnErrorEnum::CrossPartition { .. }))
            ),
            "a later subject"
        );
        Ok(())
    })
}

/// A read waiting on its home says so while it waits and no longer, and a write's wait is no read's.
#[test]
fn test_a_read_waits_only_while_its_home_has_not_answered() -> TestResult {
    let knows = Cluster::even(2)?.name_in(2, "knows")?;
    on_hosts(2, |hosts| {
        let host = hosts.first_mut().context("partition 1")?;
        let key = (1, 1);
        let _read = host.bind(key, [1; 16], (TENANT, 1, epoch(0)?), 0)?;
        assert!(!host.read_waits(key), "nothing asked");
        assert_eq!(
            host.read_terms_bound(key, &[ex(&knows)], 0)?,
            BoundStep::Waiting
        );
        assert!(host.read_waits(key));
        let _ended = turn_all(hosts, 4, 0)?;
        let answered = hosts.first_mut().context("partition 1")?;
        assert_eq!(answered.take_bound_ready(), [key]);
        assert!(!answered.read_waits(key), "answered");
        Ok(())
    })
}

/// A registration a Reference brought from another process is answered to the shell, under the asking's number and index,
/// once durable (`PROTOCOL.md` §13.15).
#[test]
fn test_a_registration_from_another_process_is_answered_to_the_shell() -> TestResult {
    use ultragraphdb_cluster_partition::{
        PartitionHost, PartitionMailbox, Reference, RemoteMessage,
    };
    use ultragraphdb_transactions_commit::{CloseRequest, Pipeline, RoundBounds};
    let b = Cluster::even(1)?.name_in(1, "b")?;
    let placement = PlacementMap::even(1, NodeId::new(1))?;
    let id = pid(1)?;
    let part = crate::utils::cluster::node(id, &placement, epoch(1)?)?;
    let pipeline = Pipeline::new(
        part.applier(),
        crate::utils::hosts::sim_log(id)?,
        RoundBounds::new(u32::MAX, u32::MAX),
    )?
    .with_partition(id.get(), epoch(1)?, CloseRequest::default(), &[])
    .bound(id, part.placement());
    let (shell, remote) = std::sync::mpsc::channel();
    let mailbox = PartitionMailbox::mesh(&[id])
        .into_iter()
        .next()
        .context("a mailbox")?
        .with_remote(shell);
    let sender = mailbox.sender(id).context("a sender")?;
    let mut host = PartitionHost::new(&part, pipeline, mailbox);
    let reference = Reference::new(id, TENANT, ex(&b).as_str().to_string(), None, None, 0);
    sender.send(PartitionMessage::Register {
        from: None,
        write: 42,
        batch: vec![(0, reference)],
    })?;
    for _ in 0..3 {
        crate::utils::cluster::block_on(host.turn(0))??;
    }
    let answered = remote.try_recv()?;
    let RemoteMessage::Answered {
        write,
        index,
        answer,
    } = answered
    else {
        return Err(TestError::new(format!("not an answer: {answered:?}")));
    };
    assert_eq!((write, index), (42, 0));
    assert!(answer.is_ok(), "{answer:?}");
    Ok(())
}

/// A home whose pipeline stopped under a registration's round takes the next registration and answers neither: the
/// writes asking wait.
#[test]
fn test_a_home_whose_pipeline_stopped_answers_no_registration() -> TestResult {
    use ultragraphdb_store_sim::Fault;
    let (a, knows, b) = names()?;
    on_hosts(2, |hosts| {
        hosts
            .get(1)
            .context("partition 2")?
            .partition()
            .applier()
            .quads()
            .store()
            .inject(Fault::FailNextBatch);
        let write = changeset(vec![insert(&a, &knows, &b)]);
        let mut failed = Vec::new();
        for _ in 0..2 {
            let _ticket = hosts
                .first_mut()
                .context("partition 1")?
                .submit(write.clone(), 0)?;
            for host in hosts.iter_mut() {
                if let Err(error) = crate::utils::cluster::block_on(host.turn(0))? {
                    failed.push(error);
                }
            }
        }
        let home = hosts.get(1).context("partition 2")?;
        assert!(home.is_stopped());
        assert_eq!(failed.len(), 1, "the failed round's turn alone: {failed:?}");
        assert_eq!(
            hosts.first().context("partition 1")?.parked(),
            2,
            "both writes wait"
        );
        Ok(())
    })
}

/// The closes after epoch 1, `count` of them in order, sent to the one host's mailbox; the last epoch sent.
fn send_closes(hosts: &[Host<'_>], count: usize) -> TestResult<u64> {
    let sender = hosts_sender(hosts, 1)?;
    let last = 1 + u64::try_from(count)?;
    for at in 2..last + 1 {
        sender.send(PartitionMessage::Close(Close::new(
            epoch(at)?,
            0,
            Vec::new(),
            Vec::new(),
        )?))?;
    }
    Ok(last)
}

/// The latest close delivered to the one host after it turns once.
fn delivered_after_a_turn(hosts: &mut [Host<'_>]) -> TestResult<Epoch> {
    let _ended = turn_all(hosts, 1, 0)?;
    Ok(hosts.first().context("partition 1")?.delivered())
}

/// A turn takes a mailbox holding as many messages as its bound whole: every close is delivered.
#[test]
fn test_a_turn_takes_a_mailbox_at_its_bound_whole() -> TestResult {
    on_hosts(1, |hosts| {
        let last = send_closes(hosts, MAILBOX_PER_TURN_MAX)?;
        assert_eq!(delivered_after_a_turn(hosts)?, epoch(last)?);
        Ok(())
    })
}

/// A turn takes its bound of a mailbox holding one message more, and the next turn takes the one left: the last close is
/// delivered a turn later.
#[test]
fn test_the_message_past_the_mailbox_bound_waits_for_the_next_turn() -> TestResult {
    on_hosts(1, |hosts| {
        let last = send_closes(hosts, MAILBOX_PER_TURN_MAX + 1)?;
        assert_eq!(
            delivered_after_a_turn(hosts)?,
            epoch(last - 1)?,
            "the last close waits"
        );
        assert_eq!(delivered_after_a_turn(hosts)?, epoch(last)?);
        Ok(())
    })
}
