/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What a test has its played partitions hold, in the store's own encodings: dictionary entries, lists of object records and the
//! keys they live under.

use oxrdf::{Literal, NamedNode};
use ultragraphdb_cluster_placement::{NodeId, PlacementMap, TenantRun};
use ultragraphdb_core_ids::{Bucket, Epoch, Sequence, SystemId, Tenant, TypePrefix};
use ultragraphdb_core_keys::{Graph, QuadKey};
use ultragraphdb_core_terms::{EncodedTerm, TermError};
use ultragraphdb_protocol_wire::ObjectRecordsBuilder;
use ultragraphdb_store_quads::{id_key, iri_bucket, literal_key, tenant_base, term_key};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use super::played_partition::PlayedPartition;

/// The tenants searched for one whose run the placement spreads as a test needs.
const TENANTS_SEARCHED: u64 = 1 << 16;

/// The even placement over `partitions`, as compute holds it.
pub fn placement(partitions: u32) -> TestResult<PlacementMap> {
    Ok(PlacementMap::even(partitions, NodeId::new(1))?)
}

/// The first tenant whose run `partitions` evenly placed spread over exactly `holders` partitions.
pub fn tenant_over(partitions: u32, holders: usize) -> TestResult<u64> {
    let map = placement(partitions)?;
    (1..TENANTS_SEARCHED)
        .find(|tenant| map.holders_of_run(Tenant::new(*tenant), Epoch::ZERO).len() == holders)
        .ok_or_else(|| {
            TestError::new(format!(
                "no tenant over {holders} of {partitions} partitions"
            ))
        })
}

/// The partition `iri` of `tenant` is homed at, `partitions` evenly placed.
pub fn home_of(partitions: u32, tenant: u64, iri: &str) -> TestResult<u32> {
    let bucket = iri_bucket(tenant, iri).context("a bucket of the IRI")?;
    Ok(placement(partitions)?
        .partition_of(bucket, Epoch::ZERO)
        .get())
}

/// An IRI of `tenant` homed at `partition`, `partitions` evenly placed, named from `stem`.
pub fn iri_at(partitions: u32, tenant: u64, partition: u32, stem: &str) -> TestResult<String> {
    for at in 0..TENANTS_SEARCHED {
        let iri = format!("http://ex.org/{stem}{at}");
        if home_of(partitions, tenant, &iri)? == partition {
            return Ok(iri);
        }
    }
    Err(TestError::new(format!(
        "no IRI of {stem} homed at {partition}"
    )))
}

/// A bucket of `tenant`'s run that `partition` holds, `partitions` evenly placed.
pub fn bucket_at(partitions: u32, tenant: u64, partition: u32) -> TestResult<u32> {
    placement(partitions)?
        .holders_of_run(Tenant::new(tenant), Epoch::ZERO)
        .into_iter()
        .find(|(holder, _)| holder.get() == partition)
        .map(|(_, first)| first)
        .ok_or_else(|| {
            TestError::new(format!(
                "no bucket of tenant {tenant} at partition {partition}"
            ))
        })
}

/// The resource ID of `sequence` in `bucket`.
pub fn id_in(bucket: u32, sequence: u64) -> TestResult<SystemId> {
    Ok(SystemId::new(
        TypePrefix::RESOURCE,
        Bucket::new(bucket)?,
        Sequence::new(sequence)?,
    ))
}

/// The resource ID of `sequence` in the first bucket of `tenant`'s run.
pub fn id_of(tenant: u64, sequence: u64) -> TestResult<SystemId> {
    id_in(TenantRun::of(Tenant::new(tenant)).range().start(), sequence)
}

/// The skolem IRI of `tenant` in `bucket` with `random` as its tail, as the store names a blank node or a reifier: the tenant's
/// base, `.well-known/genid/`, the bucket in 6 lowercase hexadecimal digits, `-` and 26 more.
pub fn skolem_iri(tenant: u64, bucket: u32, random: u64) -> String {
    format!(
        "{}.well-known/genid/{bucket:06x}-{random:026x}",
        tenant_base(tenant)
    )
}

/// Has `partition` hold the IRI `iri` of `tenant` under `id`, by its text and its ID.
pub fn name(partition: &PlayedPartition, tenant: u64, iri: &str, id: SystemId) {
    partition.put(term_key(tenant, iri), id.to_be_bytes().to_vec());
    partition.put(id_key(tenant, id), iri.as_bytes().to_vec());
}

/// The encoding of `literal`, its datatype by `datatype` where the store names one by ID, and the texts it names by hash.
pub fn literal_encoding(
    literal: &Literal,
    datatype: Option<SystemId>,
) -> TestResult<(EncodedTerm, Vec<(ultragraphdb_core_terms::Hash128, String)>)> {
    let encoded = EncodedTerm::literal(literal.as_ref(), |iri: oxrdf::NamedNodeRef<'_>| {
        datatype.ok_or_else(|| TermError::malformed(iri.as_str().as_bytes().to_vec(), "unnamed"))
    })?;
    let texts = encoded
        .strings()
        .map(|(hash, text)| (hash, text.to_string()))
        .collect();
    Ok((encoded.into_term(), texts))
}

/// Has `partition` hold what `literal` of `tenant` names by hash; its encoding.
pub fn literal_held(
    partition: &PlayedPartition,
    tenant: u64,
    literal: &Literal,
    datatype: Option<SystemId>,
) -> TestResult<EncodedTerm> {
    let (encoded, texts) = literal_encoding(literal, datatype)?;
    for (hash, text) in texts {
        partition.put(literal_key(tenant, &hash), text.into_bytes());
    }
    Ok(encoded)
}

/// A typed literal of `datatype`.
pub fn typed(lexical: &str, datatype: &str) -> Literal {
    Literal::new_typed_literal(lexical, NamedNode::new_unchecked(datatype))
}

/// A list's value: one object record per term, each with its statements.
pub fn list_of(objects: &[(EncodedTerm, Vec<SystemId>)]) -> Vec<u8> {
    let mut records = ObjectRecordsBuilder::new();
    for (object, statements) in objects {
        let ids: Vec<[u8; 16]> = statements.iter().map(|id| id.to_be_bytes()).collect();
        records.push(object.as_bytes(), &ids);
    }
    records.value().to_vec()
}

/// A list's value of IDs alone.
pub fn ids_list(ids: &[SystemId]) -> Vec<u8> {
    let objects: Vec<(EncodedTerm, Vec<SystemId>)> = ids
        .iter()
        .map(|id| (EncodedTerm::id(*id), Vec::new()))
        .collect();
    list_of(&objects)
}

/// The forward list's key of `subject` and `predicate` in the default graph.
pub fn forward_key(tenant: u64, predicate: SystemId, subject: SystemId) -> Vec<u8> {
    QuadKey::Forward {
        tenant,
        graph: Graph::Default,
        predicate,
        subject,
    }
    .to_bytes()
}

/// The reverse list's key of `object` and `predicate` in the default graph.
pub fn reverse_key(tenant: u64, predicate: SystemId, object: &EncodedTerm) -> Vec<u8> {
    QuadKey::Reverse {
        tenant,
        graph: Graph::Default,
        predicate,
        object: object.clone(),
    }
    .to_bytes()
}

/// The predicates' list key of `subject` in the default graph.
pub fn subject_predicates_key(tenant: u64, subject: SystemId) -> Vec<u8> {
    QuadKey::SubjectPredicates {
        tenant,
        graph: Graph::Default,
        subject,
    }
    .to_bytes()
}

/// The dictionary key naming `id`, as compute asks for its text.
pub fn name_key(tenant: u64, id: SystemId) -> Vec<u8> {
    id_key(tenant, id)
}

/// The IDs of a forward list a test placed: its subject's, its predicate's and its objects'.
#[derive(Debug, Clone)]
pub struct Placed {
    /// The subject's ID.
    pub subject: SystemId,
    /// The predicate's ID.
    pub predicate: SystemId,
    /// The objects' IDs, in the list's order.
    pub objects: Vec<SystemId>,
}

/// Has `partition` hold `<subject> <predicate>` listing `objects` in the default graph, every IRI named, its IDs in the bucket
/// `bucket` from `sequence` on, in the order subject, predicate, objects.
pub fn place(
    partition: &PlayedPartition,
    tenant: u64,
    (bucket, sequence): (u32, u64),
    (subject, predicate): (&str, &str),
    objects: &[&str],
) -> TestResult<Placed> {
    let subject_id = id_in(bucket, sequence)?;
    let predicate_id = id_in(bucket, sequence + 1)?;
    name(partition, tenant, subject, subject_id);
    name(partition, tenant, predicate, predicate_id);
    let mut ids = Vec::with_capacity(objects.len());
    for (at, object) in objects.iter().enumerate() {
        let id = id_in(bucket, sequence + 2 + at as u64)?;
        name(partition, tenant, object, id);
        ids.push(id);
    }
    partition.put(
        forward_key(tenant, predicate_id, subject_id),
        ids_list(&ids),
    );
    Ok(Placed {
        subject: subject_id,
        predicate: predicate_id,
        objects: ids,
    })
}

/// The first bucket of `tenant`'s run.
pub fn first_bucket(tenant: u64) -> u32 {
    TenantRun::of(Tenant::new(tenant)).range().start()
}
