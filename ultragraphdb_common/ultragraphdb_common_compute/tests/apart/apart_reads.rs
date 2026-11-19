/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What a test holds in a played partition, and a read-write transaction's reads of it through compute's executor, as a
//! client's session hands them over and the client reads their answers (`PROTOCOL.md` §11.12, *Reads*).

use std::time::Duration;

use ultragraphdb_cluster_placement::TenantRun;
use ultragraphdb_common_role::OpenRequests;
use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, Tenant, TypePrefix};
use ultragraphdb_core_keys::{Graph, QuadKey};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_wire::{
    BodyView, Command, ErrorCode, ErrorReplyView, FLAG_LAST, MAX_FRAME_SIZE_U32,
    ObjectRecordsBuilder, PatternSlot, QuadPattern, ReadLevel, ResultView, TermTable,
};
use ultragraphdb_store_quads::{id_key, term_key};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use super::apart_cluster::{ApartCluster, times};
use super::apart_partition::{PlayedPartition, VERSION};
use super::apart_session::{ReadSession, SessionFrame, SessionRead};

/// The tenant every read names.
pub const TENANT: u64 = 1;

/// The transaction the reads belong to, bound at partition 1.
pub const TRANSACTION: [u8; 16] = [7; 16];

/// How long the test waits on one frame of a read's: past every wait a test's times give compute.
pub const ANSWER_WAIT: Duration = Duration::from_secs(5);

/// The system ID of `sequence` in the tenant's run.
pub fn id(sequence: u64) -> TestResult<SystemId> {
    let run = TenantRun::of(Tenant::new(TENANT)).range();
    Ok(SystemId::new(
        TypePrefix::RESOURCE,
        Bucket::new(run.start())?,
        Sequence::new(sequence)?,
    ))
}

/// Has `partition`'s dictionary name `id` `iri`, by its text and its ID.
pub fn name(partition: &PlayedPartition, iri: &str, id: SystemId) {
    partition.put(term_key(TENANT, iri), id.to_be_bytes().to_vec());
    partition.put(id_key(TENANT, id), iri.as_bytes().to_vec());
}

/// A list holding `ids` once each, in the order given.
pub fn list_of(ids: &[SystemId]) -> Vec<u8> {
    let mut records = ObjectRecordsBuilder::new();
    ids.iter()
        .for_each(|id| records.push(EncodedTerm::id(*id).as_bytes(), &[]));
    records.value().to_vec()
}

/// The forward list of `subject` and `predicate` in `graph`.
pub fn forward_in(graph: Graph, predicate: SystemId, subject: SystemId) -> Vec<u8> {
    QuadKey::Forward {
        tenant: TENANT,
        graph,
        predicate,
        subject,
    }
    .to_bytes()
}

/// The forward list of `subject` and `predicate` in the default graph.
pub fn forward(predicate: SystemId, subject: SystemId) -> Vec<u8> {
    forward_in(Graph::Default, predicate, subject)
}

/// The list of the predicates `subject` has in the default graph.
pub fn subject_predicates(subject: SystemId) -> Vec<u8> {
    QuadKey::SubjectPredicates {
        tenant: TENANT,
        graph: Graph::Default,
        subject,
    }
    .to_bytes()
}

/// The read of a pattern whose object is unbound, the graph the default one unless `any_graph`, of at most `limit` rows from
/// `resume`, in the read-write transaction bound at partition 1.
pub fn read_of(
    (any_graph, subject, predicate): (bool, Option<&str>, Option<&str>),
    limit: u32,
    resume: Vec<u8>,
) -> SessionRead {
    let mut terms = TermTable::new();
    let graph = if any_graph {
        PatternSlot::Unbound
    } else {
        PatternSlot::DefaultGraph
    };
    let subject = subject.map_or(PatternSlot::Unbound, |iri| {
        PatternSlot::Bound(terms.iri(iri))
    });
    let predicate = predicate.map_or(PatternSlot::Unbound, |iri| {
        PatternSlot::Bound(terms.iri(iri))
    });
    let pattern = QuadPattern::new(graph, subject, predicate, PatternSlot::Unbound);
    SessionRead::new(TENANT, false, ReadLevel::Snapshot, terms, pattern, resume)
        .with_partition(1)
        .with_transaction(TRANSACTION)
        .with_read_version(VERSION)
        .with_principal(1)
        .with_deadline_ms(10_000)
        .with_client_limits(MAX_FRAME_SIZE_U32, 4)
        .with_limit(limit)
}

/// What a read answered: its rows, each cell by its text, the last batch's resume key, and its refusal.
#[derive(Debug, Default)]
pub struct ReadAnswer {
    pub rows: Vec<Vec<Option<String>>>,
    pub resume: Vec<u8>,
    pub batches: usize,
    pub refused: Option<(ErrorCode, u64, String)>,
}

impl ReadAnswer {
    /// The rows, failing on a refusal.
    pub fn whole(&self) -> TestResult<&[Vec<Option<String>>]> {
        match &self.refused {
            Some(refused) => Err(TestError::new(format!("the read was refused: {refused:?}"))),
            None => Ok(&self.rows),
        }
    }
}

/// The next frame of `client`, failing past [`ANSWER_WAIT`].
pub async fn next_frame(client: &mut ReadSession) -> TestResult<SessionFrame> {
    compio::time::timeout(ANSWER_WAIT, client.next_frame())
        .await
        .map_err(|_| TestError::new(format!("compute gave no frame within {ANSWER_WAIT:?}")))?
}

/// Starts `read` on a stream of its own: the stream.
pub fn send_read(client: &mut ReadSession, read: &SessionRead) -> TestResult<u64> {
    client.send_only(read)?;
    Ok(client.last_stream())
}

/// Cancels the read on `stream`.
pub fn cancel(client: &mut ReadSession, stream: u64) -> TestResult {
    client.cancel(stream)
}

/// Starts `read` and reads its answer to its end, granting a frame of credit per batch.
pub async fn answer_on(client: &mut ReadSession, read: &SessionRead) -> TestResult<ReadAnswer> {
    let stream = send_read(client, read)?;
    let mut answer = ReadAnswer::default();
    loop {
        let frame = next_frame(client).await?;
        let view = frame.view()?;
        if view.command() == Command::Error {
            let error = ErrorReplyView::from_frame(&view)?;
            answer.refused = Some((error.code(), error.detail(), error.message().to_string()));
            return Ok(answer);
        }
        let result = ResultView::from_frame(&view)?;
        for row in 0..result.rows() {
            let cells = (0..result.columns())
                .map(|column| {
                    let cell = result.cell(column, row).context("a cell")?;
                    Ok(result
                        .term(cell)
                        .and_then(|term| term.text().map(str::to_string)))
                })
                .collect::<TestResult<Vec<_>>>()?;
            answer.rows.push(cells);
        }
        answer.resume = result.resume().to_vec();
        answer.batches += 1;
        if view.header().flags() & FLAG_LAST != 0 {
            return Ok(answer);
        }
        client.credit(stream, 1)?;
    }
}

/// `pairs` as rows of a read binding neither subject nor object: its subject, its object and an absent reifier.
pub fn rows_of(pairs: &[(&str, &str)]) -> Vec<Vec<Option<String>>> {
    pairs
        .iter()
        .map(|(subject, object)| {
            vec![
                Some((*subject).to_string()),
                Some((*object).to_string()),
                None,
            ]
        })
        .collect()
}

/// The forward lists of one predicate: each subject's IRI and its objects' IRIs.
pub type Lists<'a> = &'a [(&'a str, &'a [&'a str])];

/// Has `partition` hold `predicate`'s forward lists `lists` in `graph`, every term named, the IDs drawn in the order the IRIs
/// first appear from `first`, so the store's key order is the order listed: the predicate's ID.
pub fn hold_lists(
    partition: &PlayedPartition,
    (graph, predicate): (Graph, &str),
    lists: Lists<'_>,
    first: u64,
) -> TestResult<SystemId> {
    let mut next = first;
    let mut named = |iri: &str| -> TestResult<SystemId> {
        let held = id(next)?;
        next += 1;
        name(partition, iri, held);
        Ok(held)
    };
    let predicate = named(predicate)?;
    for (subject, objects) in lists {
        let subject = named(subject)?;
        let objects = objects
            .iter()
            .map(|object| named(object))
            .collect::<TestResult<Vec<_>>>()?;
        partition.put(forward_in(graph, predicate, subject), list_of(&objects));
    }
    Ok(predicate)
}

/// The rows of a read of `lists` binding the predicate alone.
pub fn rows_listed(lists: Lists<'_>) -> Vec<Vec<Option<String>>> {
    let pairs: Vec<(&str, &str)> = lists
        .iter()
        .flat_map(|(subject, objects)| objects.iter().map(move |object| (*subject, *object)))
        .collect();
    rows_of(&pairs)
}

/// The predicate every predicate read binds.
pub const P: &str = "http://ex.org/p";

/// A read binding `<p>` alone, of at most `limit` rows from `resume`.
pub fn of_p(limit: u32, resume: Vec<u8>) -> SessionRead {
    read_of((false, None, Some(P)), limit, resume)
}

/// A cluster of one partition holding `lists` of `<p>`, framed per key when `per_key`, and a client's reads on its compute core.
pub async fn holding(lists: Lists<'_>, per_key: bool) -> TestResult<(ApartCluster, ReadSession)> {
    let cluster = ApartCluster::start(1).await?;
    let partition = cluster.partition(1)?;
    let _p = hold_lists(partition, (Graph::Default, P), lists, 100)?;
    if per_key {
        partition.frame_per_key();
    }
    let client = cluster.session(times(Duration::from_secs(1)), OpenRequests::default());
    Ok((cluster, client))
}

/// Two subjects of `<p>` with two objects each.
pub const TWO_BY_TWO: Lists<'static> = &[
    (
        "http://ex.org/s1",
        &["http://ex.org/o1", "http://ex.org/o2"],
    ),
    (
        "http://ex.org/s2",
        &["http://ex.org/o3", "http://ex.org/o4"],
    ),
];

/// The subject every subject read binds.
pub const S: &str = "http://ex.org/s";

/// A subject with two predicates: their predicates' list, each predicate's forward list and every term named. The IRIs of the
/// predicates and objects, in key order.
pub fn hold_two_predicates(partition: &PlayedPartition) -> TestResult<[&'static str; 4]> {
    let iris = [
        "http://ex.org/p1",
        "http://ex.org/o1",
        "http://ex.org/p2",
        "http://ex.org/o2",
    ];
    let subject = id(200)?;
    name(partition, S, subject);
    let ids: Vec<_> = (201..205).map(id).collect::<TestResult<_>>()?;
    for (iri, held) in iris.iter().zip(&ids) {
        name(partition, iri, *held);
    }
    let [p1, o1, p2, o2] = <[_; 4]>::try_from(ids).map_err(|_| TestError::new("four IDs"))?;
    partition.put(subject_predicates(subject), list_of(&[p1, p2]));
    partition.put(forward(p1, subject), list_of(&[o1]));
    partition.put(forward(p2, subject), list_of(&[o2]));
    Ok(iris)
}

/// A read of `<s>` alone.
pub fn of_s(limit: u32, resume: Vec<u8>) -> SessionRead {
    read_of((false, Some(S), None), limit, resume)
}

/// Has `partition` hold, in tenant `tenant`, `subject`'s statements of `predicate` in the default graph, its `objects` in order,
/// every term named and the subject's predicates listed, IDs drawn from `first`: what a read of the subject with its graph and
/// predicate unbound reads.
pub fn hold_subject(
    partition: &PlayedPartition,
    tenant: u64,
    (subject, predicate): (&str, &str),
    objects: &[&str],
    first: u64,
) -> TestResult {
    let run = TenantRun::of(Tenant::new(tenant)).range();
    let mut next = first;
    let mut named = |iri: &str| -> TestResult<SystemId> {
        let held = SystemId::new(
            TypePrefix::RESOURCE,
            Bucket::new(run.start())?,
            Sequence::new(next)?,
        );
        next += 1;
        partition.put(term_key(tenant, iri), held.to_be_bytes().to_vec());
        partition.put(id_key(tenant, held), iri.as_bytes().to_vec());
        Ok(held)
    };
    let subject = named(subject)?;
    let predicate = named(predicate)?;
    let objects = objects
        .iter()
        .map(|object| named(object))
        .collect::<TestResult<Vec<_>>>()?;
    let listed = QuadKey::SubjectPredicates {
        tenant,
        graph: Graph::Default,
        subject,
    };
    partition.put(listed.to_bytes(), list_of(&[predicate]));
    let forward = QuadKey::Forward {
        tenant,
        graph: Graph::Default,
        predicate,
        subject,
    };
    partition.put(forward.to_bytes(), list_of(&objects));
    Ok(())
}
