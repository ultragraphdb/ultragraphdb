/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What a host reads for a Scan: the dictionary, the watermark, the discard version and where a list resumes.

use ultragraphdb_cluster_partition::BoundStep;
use ultragraphdb_core_ids::{Bucket, Epoch, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_keys::Keyspace;
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_posting::PostingIdentity;
use ultragraphdb_store_quads::{id_key, term_key};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::utils::cluster::{Cluster, TENANT, changeset, ex, insert_literal};
use crate::utils::hosts::{Host, on_hosts, turn_all};

/// Partition 1's host.
fn first<'h, 'a>(hosts: &'h mut [Host<'a>]) -> TestResult<&'h mut Host<'a>> {
    hosts.first_mut().context("partition 1")
}

/// The ID a dictionary value of a term names: its 16 bytes.
fn id_of(value: &[u8]) -> TestResult<SystemId> {
    let bytes: [u8; 16] = value
        .try_into()
        .map_err(|_| TestError::new("an ID's 16 bytes"))?;
    Ok(SystemId::from_u128(u128::from_be_bytes(bytes))?)
}

/// After a commit the watermark is its version, a Bind reads there, the outcome carries the commit step's reply, and the
/// dictionary holds the term both ways: its ID under its text, its text under its ID.
#[test]
fn test_a_commit_moves_the_watermark_a_bind_reads_at_and_the_dictionary_holds() -> TestResult {
    let cluster = Cluster::even(1)?;
    let (a, knows) = (cluster.name_in(1, "a")?, cluster.name_in(1, "knows")?);
    on_hosts(1, |hosts| {
        let ticket = first(hosts)?.submit(changeset(vec![insert_literal(&a, &knows, "v")]), 0)?;
        let ended = turn_all(hosts, 3, 0)?;
        let outcome = ended
            .iter()
            .find(|o| o.ticket() == ticket)
            .context("the write's outcome")?;
        let version = outcome
            .outcome()
            .clone()
            .map_err(|e| TestError::new(e.to_string()))?;
        let reply = outcome.reply().context("the commit step's reply")?;
        assert_eq!(reply.ticket(), ticket);
        assert_eq!(reply.outcome(), &Ok(version));
        let host = first(hosts)?;
        assert_eq!(host.watermark(), version);
        let identity = [3; 16];
        let read_version = host.bind((1, 1), identity, (TENANT, 1, Epoch::ZERO), 0)?;
        assert_eq!(read_version, version);
        let read = host.bound_read(&identity).context("the transaction")?;
        assert_eq!(read.read_version(), version);
        let iri = ex(&a);
        let value = host
            .dictionary(&term_key(TENANT, iri.as_str()))?
            .context("the term's ID")?;
        let id = id_of(&value)?;
        assert_eq!(
            host.dictionary(&id_key(TENANT, id))?,
            Some(iri.as_str().as_bytes().to_vec())
        );
        Ok(())
    })
}

/// A transaction finds the ID of a term it created and has not committed under the term's dictionary key.
#[test]
fn test_a_transaction_finds_the_term_it_created() -> TestResult {
    let cluster = Cluster::even(1)?;
    let (fresh, knows) = (cluster.name_in(1, "fresh")?, cluster.name_in(1, "knows")?);
    on_hosts(1, |hosts| {
        let key = (1, 1);
        let _read = first(hosts)?.bind(key, [1; 16], (TENANT, 1, Epoch::ZERO), 0)?;
        let write = changeset(vec![insert_literal(&fresh, &knows, "v")]);
        if first(hosts)?.write_bound(key, &write, 0)? == BoundStep::Waiting {
            let _ended = turn_all(hosts, 4, 0)?;
            first(hosts)?.resume_write(key, &write, 0)?;
        }
        let host = first(hosts)?;
        let iri = ex(&fresh);
        let id = host
            .resolve_bound(key, iri.as_ref(), 0)?
            .context("the term's ID")?;
        let found = host.dictionary_bound(key, &term_key(TENANT, iri.as_str()), 0)?;
        assert_eq!(found, Some(id.to_be_bytes().to_vec()));
        assert_eq!(host.dictionary(&term_key(TENANT, iri.as_str()))?, None);
        Ok(())
    })
}

/// The discard version is the store's, as its lists raised it.
#[test]
fn test_the_discard_version_is_the_stores() -> TestResult {
    let cluster = Cluster::even(1)?;
    let (a, knows) = (cluster.name_in(1, "a")?, cluster.name_in(1, "knows")?);
    on_hosts(1, |hosts| {
        let _ticket = first(hosts)?.submit(changeset(vec![insert_literal(&a, &knows, "v")]), 0)?;
        let _ended = turn_all(hosts, 3, 0)?;
        let host = first(hosts)?;
        let watermark = host.watermark();
        host.partition()
            .applier()
            .quads()
            .lists()
            .set_discard_ts(watermark)?;
        assert_eq!(host.discard_ts(), watermark);
        Ok(())
    })
}

/// A Scan's `after` naming an ID resumes at that ID's posting, read itself (quad-reads, a resume key holds terms).
#[test]
fn test_a_resume_at_an_id_takes_its_postings_identity() -> TestResult {
    on_hosts(1, |hosts| {
        let id = SystemId::new(TypePrefix::RESOURCE, Bucket::new(1)?, Sequence::new(3)?);
        assert_eq!(
            first(hosts)?.resume_at(Keyspace::Forward, EncodedTerm::id(id).as_bytes())?,
            PostingIdentity::new(id.to_u128(), Vec::new()).included()
        );
        Ok(())
    })
}

/// A Scan of keyspace `keyspace` from its start at `version`, a page of at most 16.
fn scan_of(keyspace: Keyspace, version: u64) -> TestResult<ultragraphdb_store_quads::KernelScan> {
    let limit = std::num::NonZeroUsize::new(16).context("a limit")?;
    Ok(ultragraphdb_store_quads::KernelScan::new(
        keyspace,
        TENANT,
        Vec::new(),
        version,
        limit,
    ))
}

/// A host pages its lists and its values at a version with no transaction: a committed object is one row of the forward
/// lists, and a keyspace of no values is refused.
#[test]
fn test_a_host_pages_lists_and_values_unbound() -> TestResult {
    use std::ops::ControlFlow;
    let cluster = Cluster::even(1)?;
    let (a, knows) = (cluster.name_in(1, "a")?, cluster.name_in(1, "knows")?);
    on_hosts(1, |hosts| {
        let _ticket = first(hosts)?.submit(changeset(vec![insert_literal(&a, &knows, "v")]), 0)?;
        let _ended = turn_all(hosts, 3, 0)?;
        let host = first(hosts)?;
        let version = host.watermark();
        let mut objects = 0;
        let _page = host.page(&scan_of(Keyspace::Forward, version)?, |_, _, ids| {
            objects += ids.len().max(1);
            ControlFlow::Continue(())
        })?;
        assert_eq!(objects, 1);
        let mut values = 0;
        let _values_page = host.values_page(&scan_of(Keyspace::Statements, version)?, |_, _| {
            values += 1;
            ControlFlow::Continue(())
        })?;
        assert_eq!(values, 0, "no statement was written");
        let refused = host.values_page(&scan_of(Keyspace::Forward, version)?, |_, _| {
            ControlFlow::Continue(())
        });
        assert!(refused.is_err());
        Ok(())
    })
}

/// A bound transaction's page, values page or dictionary read that its transaction refuses ends the transaction.
#[test]
fn test_a_refused_bound_read_ends_the_transaction() -> TestResult {
    use std::ops::ControlFlow;
    let cluster = Cluster::even(1)?;
    on_hosts(1, |hosts| {
        let host = first(hosts)?;
        let (paged, valued, looked) = ((1, 1), (1, 2), (1, 3));
        for (key, identity) in [(paged, 1), (valued, 2), (looked, 3)] {
            let _read = host.bind(key, [identity; 16], (TENANT, 1, Epoch::ZERO), 0)?;
        }
        let version = host.watermark();
        let of_marks = scan_of(Keyspace::Marks, version)?;
        assert!(
            host.page_bound(paged, &of_marks, 0, |_, _, _| ControlFlow::Continue(()))
                .is_err()
        );
        let of_lists = scan_of(Keyspace::Forward, version)?;
        assert!(
            host.values_page_bound(valued, &of_lists, 0, |_, _| ControlFlow::Continue(()))
                .is_err()
        );
        let late = 3_600 * 1_000_000;
        let key = term_key(
            TENANT,
            crate::utils::cluster::ex(&cluster.name_in(1, "a")?).as_str(),
        );
        assert!(host.dictionary_bound(looked, &key, late).is_err());
        for identity in [1, 2, 3] {
            assert!(host.bound_read(&[identity; 16]).is_none(), "{identity}");
        }
        Ok(())
    })
}

/// A store failing under the dictionary read of a term homed elsewhere ends the bound read naming it, and the transaction.
#[test]
fn test_a_store_failing_under_a_read_terms_lookup_ends_the_transaction() -> TestResult {
    use ultragraphdb_cluster_partition::PartitionErrorEnum;
    use ultragraphdb_store_sim::Fault;
    let cluster = Cluster::even(2)?;
    let foreign = ex(&cluster.name_in(2, "q")?);
    on_hosts(2, |hosts| {
        let host = first(hosts)?;
        let key = (1, 1);
        let _read = host.bind(key, [1; 16], (TENANT, 1, Epoch::ZERO), 0)?;
        host.partition()
            .applier()
            .quads()
            .store()
            .inject(Fault::FailNextRead);
        let refused = host.read_terms_bound(key, &[foreign], 0);
        assert!(
            matches!(&refused, Err(e) if matches!(e.0, PartitionErrorEnum::Quad(_))),
            "{refused:?}"
        );
        assert!(host.bound_read(&[1; 16]).is_none(), "the refusal ended it");
        Ok(())
    })
}
