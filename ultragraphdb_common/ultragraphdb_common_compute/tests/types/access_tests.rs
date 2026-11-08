/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The keyspace a read takes in a partition by its bound positions (`PROTOCOL.md` §11.12, quad-reads "Access paths follow the
//! bound positions").

use ultragraphdb_protocol_wire::RequestTerm;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::played::played_cluster::{PlayedCluster, quiet_times, run};
use crate::played::played_data::{
    OneList, TwoLists, forward, object_predicates, plain, referrer_set, referrers, reverse,
    subject_predicates,
};
use crate::played::played_read::{Bound, read, snapshot_of};

/// A bound subject alone reads its predicates' list, then the forward list of each predicate it names.
#[test]
fn test_a_bound_subject_alone_reads_each_list_its_predicates_list_names() -> TestResult {
    run(async {
        let played = PlayedCluster::start(1, quiet_times()).await?;
        let partition = played.partition(1)?;
        let lists = TwoLists::new("subject")?;
        lists.hold_in(partition);
        let bound = Bound {
            subject: Some(lists.subject.iri.clone()),
            predicate: None,
            object: None,
        };
        let mut client = played.client().await?;
        let answer = read(&mut client, &snapshot_of(&bound)?).await?.answered()?;
        assert_eq!(answer.texts(), lists.rows());
        let firsts: Vec<Vec<u8>> = partition
            .scans()
            .into_iter()
            .flat_map(|scan| scan.ranges)
            .map(|range| range.first)
            .collect();
        assert!(
            firsts.contains(&subject_predicates(&lists.subject)),
            "{firsts:?}"
        );
        for list in &lists.lists {
            assert!(
                firsts.contains(&forward(&list.predicate, &list.subject)),
                "{firsts:?}"
            );
        }
        Ok(())
    })
}

/// A bound object alone reads, at each partition its referrer set names, its predicates' list, then the reverse list of
/// each predicate it names.
#[test]
fn test_a_bound_object_alone_reads_each_reverse_list_its_predicates_list_names() -> TestResult {
    run(async {
        let played = PlayedCluster::start(1, quiet_times()).await?;
        let partition = played.partition(1)?;
        let list = OneList::new("pointing", "points", 1)?;
        list.hold_in(partition);
        let object = list.objects.first().context("an object")?;
        let (predicates, subjects) = (
            object_predicates(object),
            reverse(&list.predicate, &object.id_term()),
        );
        partition.put(predicates.clone(), plain(&[&list.predicate]));
        partition.put(subjects.clone(), plain(&[&list.subject]));
        partition.put(referrers(object), referrer_set(&[&list.subject])?);
        let bound = Bound {
            subject: None,
            predicate: None,
            object: Some(RequestTerm::Iri(object.iri.clone())),
        };
        let mut client = played.client().await?;
        let answer = read(&mut client, &snapshot_of(&bound)?).await?.answered()?;
        let firsts: Vec<Vec<u8>> = partition
            .scans()
            .into_iter()
            .flat_map(|scan| scan.ranges)
            .map(|range| range.first)
            .collect();
        let texts = answer.texts();
        let row = texts.first().context("a row")?;
        assert_eq!(texts.len(), 1, "{texts:?}");
        assert!(row.contains(&Some(list.subject.iri.clone())), "{texts:?}");
        assert!(row.contains(&Some(list.predicate.iri.clone())), "{texts:?}");
        assert!(firsts.contains(&predicates), "{firsts:?}");
        assert!(firsts.contains(&subjects), "{firsts:?}");
        Ok(())
    })
}

/// The referrer set's key of `object` in `tenant` and its value, its one member the group of `bucket`.
fn referrers_in(
    tenant: u64,
    object: ultragraphdb_core_ids::SystemId,
    bucket: u32,
) -> TestResult<(Vec<u8>, Vec<u8>)> {
    let mut set = ultragraphdb_store_quads::ReferrerSet::new();
    set.set(
        ultragraphdb_core_ids::BucketGroup::of(ultragraphdb_core_ids::Bucket::new(bucket)?),
        std::num::NonZeroU64::MIN,
        ultragraphdb_core_ids::Epoch::new(1)?,
    );
    let mut value = Vec::new();
    set.encode(&mut value);
    let key = ultragraphdb_core_keys::QuadKey::Referrers {
        tenant,
        entity: object,
    }
    .to_bytes();
    Ok((key, value))
}

/// An unbound graph is walked in the keyspace of the access the bound positions take: a subject alone its predicates' lists, a
/// predicate and an object the reverse lists, an object alone its predicates' lists, a reifier the statements.
#[test]
fn test_an_unbound_graph_is_walked_in_the_accesss_keyspace() -> TestResult {
    use ultragraphdb_common_compute::read_times_of;
    use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
    use ultragraphdb_core_keys::{Keyspace, key_prefix};
    use ultragraphdb_protocol_wire::ReadLevel;

    use crate::played_reads::played_cluster::PlayedCluster as ReadsCluster;
    use crate::played_reads::played_data::{first_bucket, id_in, name, tenant_over};
    use crate::played_reads::read_answer::{Pattern, answer_on, iri};

    const S: &str = "http://ex.org/s";
    const P: &str = "http://ex.org/p";
    const O: &str = "http://ex.org/o";
    const R: &str = "http://ex.org/r";
    crate::played_reads::played_cluster::run(async {
        let cluster = ReadsCluster::start(1, read_times_of(None, None)).await?;
        let tenant = tenant_over(1, 1)?;
        let part = cluster.partition(1)?;
        let bucket = first_bucket(tenant);
        name(part, tenant, S, id_in(bucket, 1)?);
        name(part, tenant, P, id_in(bucket, 2)?);
        let object = id_in(bucket, 3)?;
        name(part, tenant, O, object);
        // The object's referrer set names its one partition, so the read asks there.
        let (key, value) = referrers_in(tenant, object, bucket)?;
        part.put(key, value);
        let reifier = SystemId::new(
            TypePrefix::STATEMENT,
            Bucket::new(bucket)?,
            Sequence::new(4)?,
        );
        name(part, tenant, R, reifier);
        let unbound =
            |subject: Option<&str>, predicate: Option<&str>, held: Option<RequestTerm>| {
                let mut pattern = Pattern::of(subject, predicate, held);
                pattern.graph = None;
                pattern
            };
        // A literal object is read at every partition of the run, an IRI object at its home's referrer set's members.
        let literal = RequestTerm::Literal {
            lexical: "x".to_string(),
            datatype: None,
        };
        let walks = [
            (unbound(Some(S), None, None), Keyspace::SubjectPredicates),
            (unbound(None, Some(P), Some(literal)), Keyspace::Reverse),
            (
                unbound(None, None, Some(iri(O))),
                Keyspace::ObjectPredicates,
            ),
            (unbound(Some(R), None, None), Keyspace::Statements),
        ];
        for (pattern, keyspace) in walks {
            let asked = part.read_scans().len();
            let read = pattern.read(tenant, true, ReadLevel::Latest)?;
            let answer = answer_on(&mut cluster.client().await?, &read).await?;
            answer.ended()?;
            assert!(answer.rows().is_empty(), "nothing is held in any graph");
            let prefix = key_prefix(keyspace, false, tenant, None, None);
            let walked = part
                .read_scans()
                .into_iter()
                .skip(asked)
                .flat_map(|scan| scan.ranges)
                .any(|range| range.first.starts_with(&prefix));
            assert!(walked, "{keyspace:?} walked");
        }
        Ok(())
    })
}
