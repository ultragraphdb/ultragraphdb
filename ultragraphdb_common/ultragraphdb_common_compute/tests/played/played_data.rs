/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What a played partition holds, in the store's own keys and values: dictionary entries both ways, lists of object records,
//! predicates' lists, statements' records and referrer sets, each built by the crate that defines it.

use oxrdf::{LiteralRef, NamedNodeRef};
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_core_ids::{BucketGroup, Epoch, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_keys::{Graph, QuadKey};
use ultragraphdb_core_terms::{EncodedTerm, TermError};
use ultragraphdb_protocol_wire::ObjectRecordsBuilder;
use ultragraphdb_store_quads::{ReferrerSet, StatementRecord, id_key, iri_bucket, term_key};
use ultragraphdb_tool_bdd::{Context, TestResult};

use super::played_partition::PlayedPartition;

/// The played tenant.
pub const TENANT: u64 = 1;

/// An IRI and the system ID its home gave it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Term {
    /// The IRI.
    pub iri: String,
    /// Its ID.
    pub id: SystemId,
}

impl Term {
    /// The resource `iri`, of sequence `sequence` in its bucket.
    pub fn resource(iri: &str, sequence: u64) -> TestResult<Self> {
        let bucket = iri_bucket(TENANT, iri).context("an IRI's bucket")?;
        Ok(Self {
            iri: iri.to_string(),
            id: SystemId::new(TypePrefix::RESOURCE, bucket, Sequence::new(sequence)?),
        })
    }

    /// The reifier `iri` of a statement of `subject`, of sequence `sequence`.
    pub fn statement(iri: &str, subject: &Self, sequence: u64) -> TestResult<Self> {
        Ok(Self {
            iri: iri.to_string(),
            id: SystemId::statement(subject.id, Sequence::new(sequence)?),
        })
    }

    /// The term as the store encodes it.
    pub fn id_term(&self) -> EncodedTerm {
        EncodedTerm::id(self.id)
    }

    /// The term's encoding as a list holds it.
    pub fn encoding(&self) -> Vec<u8> {
        self.id_term().as_bytes().to_vec()
    }

    /// Its dictionary entries, both ways, in `played`.
    pub fn name_in(&self, played: &PlayedPartition) {
        played.put(term_key(TENANT, &self.iri), self.id.to_be_bytes().to_vec());
        played.put(id_key(TENANT, self.id), self.iri.as_bytes().to_vec());
    }
}

/// The key of `object`'s referrer set.
pub fn referrers(object: &Term) -> Vec<u8> {
    QuadKey::Referrers {
        tenant: TENANT,
        entity: object.id,
    }
    .to_bytes()
}

/// A referrer set naming the groups of `subjects`, each added at epoch 1.
pub fn referrer_set(subjects: &[&Term]) -> TestResult<Vec<u8>> {
    let mut set = ReferrerSet::new();
    for subject in subjects {
        let _registered = set.register(BucketGroup::of(subject.id.bucket()), 0, Epoch::new(1)?);
    }
    let mut value = Vec::new();
    set.encode(&mut value);
    Ok(value)
}

/// The key of the record of the statement `reifier` in the default graph.
pub fn statement_key(reifier: &Term) -> Vec<u8> {
    QuadKey::Statement {
        tenant: TENANT,
        graph: Graph::Default,
        statement: reifier.id,
    }
    .to_bytes()
}

/// The record of the statement `subject` `predicate` `object` with `properties`, each a predicate and its value.
pub fn statement_record(
    subject: &Term,
    predicate: &Term,
    object: &Term,
    properties: &[(&Term, &Term)],
) -> Vec<u8> {
    let held = properties
        .iter()
        .map(|(property, value)| (property.id, EncodedTerm::id(value.id)))
        .collect();
    StatementRecord::new(subject.id, predicate.id, EncodedTerm::id(object.id), held).encode()
}

/// The partition of `partitions` placed evenly that is the home of `iri`.
pub fn home_of(iri: &str, partitions: u32) -> TestResult<u32> {
    let bucket = iri_bucket(TENANT, iri).context("an IRI's bucket")?;
    Ok(PlacementMap::even(partitions, NodeId::new(1))?
        .partition_of(bucket, Epoch::ZERO)
        .get())
}

/// The first IRI `http://example.org/{stem}{n}` whose home among `partitions` placed evenly is `partition`.
pub fn iri_homed(stem: &str, partition: u32, partitions: u32) -> TestResult<String> {
    for n in 0..1_000 {
        let iri = format!("http://example.org/{stem}{n}");
        if home_of(&iri, partitions)? == partition {
            return Ok(iri);
        }
    }
    Err(ultragraphdb_tool_bdd::TestError::new("no IRI homed there"))
}

/// A list of object records: each object's encoding and the statement IDs it is held under.
pub fn records(objects: &[(Vec<u8>, Vec<SystemId>)]) -> Vec<u8> {
    let mut list = ObjectRecordsBuilder::new();
    for (encoding, statements) in objects {
        let ids: Vec<[u8; 16]> = statements.iter().map(|id| id.to_be_bytes()).collect();
        list.push(encoding, &ids);
    }
    list.value().to_vec()
}

/// A list of `objects`, each held once.
pub fn plain(objects: &[&Term]) -> Vec<u8> {
    let held: Vec<(Vec<u8>, Vec<SystemId>)> = objects
        .iter()
        .map(|term| (term.encoding(), Vec::new()))
        .collect();
    records(&held)
}

/// The key of `subject`'s forward list of `predicate` in the default graph.
pub fn forward(predicate: &Term, subject: &Term) -> Vec<u8> {
    QuadKey::Forward {
        tenant: TENANT,
        graph: Graph::Default,
        predicate: predicate.id,
        subject: subject.id,
    }
    .to_bytes()
}

/// The key of the reverse list of `predicate` to `object` in the default graph.
pub fn reverse(predicate: &Term, object: &EncodedTerm) -> Vec<u8> {
    QuadKey::Reverse {
        tenant: TENANT,
        graph: Graph::Default,
        predicate: predicate.id,
        object: object.clone(),
    }
    .to_bytes()
}

/// The key of `subject`'s predicates' list in the default graph.
pub fn subject_predicates(subject: &Term) -> Vec<u8> {
    QuadKey::SubjectPredicates {
        tenant: TENANT,
        graph: Graph::Default,
        subject: subject.id,
    }
    .to_bytes()
}

/// The key of `object`'s predicates' list in the default graph.
pub fn object_predicates(object: &Term) -> Vec<u8> {
    QuadKey::ObjectPredicates {
        tenant: TENANT,
        graph: Graph::Default,
        object: object.id_term(),
    }
    .to_bytes()
}

/// One subject's list of one predicate, its objects in the list's order.
#[derive(Debug, Clone)]
pub struct OneList {
    /// The subject.
    pub subject: Term,
    /// The predicate.
    pub predicate: Term,
    /// The objects, ascending by encoding, as a list holds them.
    pub objects: Vec<Term>,
}

impl OneList {
    /// The list of `subject` and `predicate` holding `count` objects, each IRI under `http://example.org/`.
    pub fn new(subject: &str, predicate: &str, count: u64) -> TestResult<Self> {
        let mut objects = (0..count)
            .map(|n| Term::resource(&format!("http://example.org/{subject}-object-{n}"), 100 + n))
            .collect::<TestResult<Vec<_>>>()?;
        objects.sort_by_key(Term::encoding);
        Ok(Self {
            subject: Term::resource(&format!("http://example.org/{subject}"), 1)?,
            predicate: Term::resource(&format!("http://example.org/{predicate}"), 2)?,
            objects,
        })
    }

    /// Holds the list in `played`, every term named.
    pub fn hold_in(&self, played: &PlayedPartition) {
        self.subject.name_in(played);
        self.predicate.name_in(played);
        self.objects
            .iter()
            .for_each(|object| object.name_in(played));
        let held: Vec<&Term> = self.objects.iter().collect();
        played.put(forward(&self.predicate, &self.subject), plain(&held));
    }

    /// The objects' IRIs, in the list's order.
    pub fn object_iris(&self) -> Vec<Option<String>> {
        self.objects
            .iter()
            .map(|object| Some(object.iri.clone()))
            .collect()
    }
}

/// One subject's lists of two predicates, each of two objects, and its predicates' list naming both.
#[derive(Debug, Clone)]
pub struct TwoLists {
    /// The subject.
    pub subject: Term,
    /// The lists, ascending by predicate ID as the store's keys order them.
    pub lists: Vec<OneList>,
}

impl TwoLists {
    /// The lists of the subject `http://example.org/{stem}`.
    pub fn new(stem: &str) -> TestResult<Self> {
        let mut lists = vec![
            OneList::new(stem, &format!("{stem}-first"), 2)?,
            OneList::new(stem, &format!("{stem}-second"), 2)?,
        ];
        lists.sort_by_key(|list| list.predicate.id);
        let subject = lists
            .first()
            .map(|list| list.subject.clone())
            .context("a list")?;
        Ok(Self { subject, lists })
    }

    /// Holds the lists and the predicates' list in `played`, every term named.
    pub fn hold_in(&self, played: &PlayedPartition) {
        self.lists.iter().for_each(|list| list.hold_in(played));
        let predicates: Vec<&Term> = self.lists.iter().map(|list| &list.predicate).collect();
        played.put(subject_predicates(&self.subject), plain(&predicates));
    }

    /// Every row a read of the subject alone gives: its predicate's IRI and its object's, in the store's order.
    pub fn rows(&self) -> Vec<Vec<Option<String>>> {
        self.lists
            .iter()
            .flat_map(|list| {
                list.objects.iter().map(|object| {
                    vec![
                        Some(list.predicate.iri.clone()),
                        Some(object.iri.clone()),
                        None,
                    ]
                })
            })
            .collect()
    }
}

/// The store's encoding of the literal `lexical` of the known datatype `datatype`, or of `xsd:string` for none.
pub fn literal(lexical: &str, datatype: Option<&str>) -> TestResult<EncodedTerm> {
    let literal = match datatype {
        Some(iri) => LiteralRef::new_typed_literal(lexical, NamedNodeRef::new_unchecked(iri)),
        None => LiteralRef::new_simple_literal(lexical),
    };
    let encoded = EncodedTerm::literal(literal, |iri| {
        Err::<SystemId, TermError>(TermError::malformed(
            iri.as_str().as_bytes().to_vec(),
            "a known datatype",
        ))
    })?;
    Ok(encoded.into_term())
}
