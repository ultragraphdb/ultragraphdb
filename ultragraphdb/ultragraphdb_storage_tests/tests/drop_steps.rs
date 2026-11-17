/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Drops sent by a client through the frontend to a storage process hosting the system group, the partition's keys read back
//! over its role address: a Scan of each keyspace's range of the tenant, at the latest version, the keys decoded with
//! `QuadKey::parse` and a predicates' list with `ObjectRecords`, as compute reads one.

use cucumber::{then, when};
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{DEFAULT_GRAPH, Graph, Keyspace, QuadKey};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{
    Command, DirectWrite, DropCommand, ObjectRecords, RequestTerm, ScanAt, TermTable, WriteChange,
};
use ultragraphdb_tests::{
    CLIENT_ADDRESS, CLUSTER, FRONTEND_BINARY, FRONTEND_OBSERVABILITY, answering, frontend_holding,
    role, scratch, storage_holding, until_answering, with,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::read_kernel_steps::{answer, dial, id_of, object_of, objects_of, plain, run};
use crate::{Steps, TENANT};

/// The administrator the hosting storage process bootstraps, and its password.
const ADMIN: &str = "admin";
const PASSWORD: &str = "the administrator's password";

const A: &str = "http://ex.org/a";
const B: &str = "http://ex.org/b";
const C: &str = "http://ex.org/c";
const KNOWS: &str = "http://ex.org/knows";
const NAME: &str = "http://ex.org/name";
const SINCE: &str = "http://ex.org/since";
const G1: &str = "http://ex.org/g1";
const G2: &str = "http://ex.org/g2";

/// What a drop scenario's When read of the partition, for its Then.
#[derive(Debug, Default)]
pub(crate) struct DropSteps {
    /// The tenant's keys before the Drop, and after it.
    before: Vec<(Vec<u8>, Vec<u8>)>,
    after: Vec<(Vec<u8>, Vec<u8>)>,
    /// The IDs the partition holds for the scenario's terms, by their IRIs.
    ids: Vec<(&'static str, SystemId)>,
}

impl DropSteps {
    fn id(&self, iri: &str) -> TestResult<SystemId> {
        self.ids
            .iter()
            .find_map(|(held, id)| (*held == iri).then_some(*id))
            .context("an ID read before the Drop")
    }
}

/// The frontend and one partition's storage process hosting the system group, which bootstraps the administrator; ready.
fn started(w: &mut Steps) -> TestResult {
    let util = crate::services()?;
    let file = scratch("drops-bootstrap-secret");
    std::fs::write(&file, PASSWORD)?;
    let hosting = with(
        storage_holding()?,
        "metadata.bootstrap_secret_file",
        &file.display().to_string(),
    );
    let _storage = crate::start_storage(&util, &hosting, &scratch("storage-drops"))?;
    let _frontend: u32 = util.start_service_from_config(role(
        FRONTEND_BINARY,
        &frontend_holding()?,
        &scratch(FRONTEND_BINARY),
        answering(FRONTEND_OBSERVABILITY, "/health"),
    ))?;
    w.roles = Some(util);
    until_answering(FRONTEND_OBSERVABILITY, "/ready")
}

/// The administrator's connection to the frontend's client endpoint.
async fn admin() -> TestResult<RoleClient> {
    let target = Address::parse(&format!("tcp://{CLIENT_ADDRESS}"))?;
    Ok(RoleClient::dial(&target, CLUSTER, ADMIN, PASSWORD).await?)
}

/// A literal term of `text`.
fn literal(text: &str) -> RequestTerm {
    RequestTerm::Literal {
        lexical: text.to_string(),
        datatype: None,
    }
}

/// The insert of `subject` `predicate` `object` in `graph`, or the default graph, its terms in `terms`.
fn insert(
    terms: &mut TermTable,
    graph: Option<&str>,
    (subject, predicate, object): (&str, &str, RequestTerm),
) -> TestResult<WriteChange> {
    let graph: Option<u32> = graph.map(|graph| terms.iri(graph));
    Ok(WriteChange::Insert {
        graph,
        subject: terms.iri(subject),
        predicate: terms.iri(predicate),
        object: terms.intern(object)?,
    })
}

/// Commits `changes`, their terms in `terms`, as one DirectWrite of the tenant through no session.
async fn wrote(client: &mut RoleClient, terms: TermTable, changes: Vec<WriteChange>) -> TestResult {
    let write = DirectWrite::new(TENANT, terms, changes);
    let answered = client.request(&write).await?;
    let command = answered.view()?.command();
    if command == Command::Commit {
        return Ok(());
    }
    Err(TestError::new(format!("the write answered {command:?}")))
}

/// Sends `command` through the frontend, through no session, and waits for its reply, a Drop.
async fn dropped(client: &mut RoleClient, command: &DropCommand) -> TestResult {
    let answered = client.request(command).await?;
    let replied = answered.view()?.command();
    if replied == Command::Drop {
        return Ok(());
    }
    Err(TestError::new(format!("the Drop answered {replied:?}")))
}

/// The range of the tenant in each keyspace a Scan reads whole, its lists unsplit, its statement records and its referrers
/// (`PROTOCOL.md` §13.13): the keyspace's byte, then the tenant (`key_prefix`). A list's split parts, the statistics and the
/// other keyspaces no Scan reads are not read back here, nor the dictionary, which a Scan reads one key at a time and no drop
/// changes.
fn tenant_ranges() -> Vec<(Vec<u8>, Vec<u8>)> {
    (0..=u8::MAX)
        .filter(|byte| {
            Keyspace::from_byte(*byte).is_some_and(|(keyspace, split)| {
                !split
                    && (keyspace.is_list()
                        || matches!(keyspace, Keyspace::Statements | Keyspace::Referrers))
            })
        })
        .map(|byte| {
            (
                [&[byte][..], &TENANT.to_be_bytes()].concat(),
                [&[byte][..], &(TENANT + 1).to_be_bytes()].concat(),
            )
        })
        .collect()
}

/// Every key the partition answers of the tenant at the latest version, with its value.
async fn tenant_keys(client: &mut RoleClient) -> TestResult<Vec<(Vec<u8>, Vec<u8>)>> {
    let scan = plain(ScanAt::Latest, 0, 0, 0, &tenant_ranges());
    let (batches, refused) = answer(client, &scan, true).await?;
    if let Some(refused) = refused {
        return Err(TestError::new(format!(
            "the tenant's Scan was refused: {refused:?}"
        )));
    }
    Ok(batches
        .into_iter()
        .flat_map(|batch| batch.entries)
        .collect())
}

/// Whether `key` names `predicate`.
fn names_predicate(key: &[u8], predicate: SystemId) -> bool {
    QuadKey::parse(key).is_ok_and(|(parsed, _)| parsed.predicate() == Some(predicate))
}

/// Whether `key` is of `graph`.
fn of_graph(key: &[u8], graph: Graph) -> bool {
    QuadKey::parse(key).is_ok_and(|(parsed, _)| parsed.graph() == Some(graph))
}

/// Whether `key` is a statement record's.
fn is_statement(key: &[u8]) -> bool {
    matches!(
        key.first().and_then(|byte| Keyspace::from_byte(*byte)),
        Some((Keyspace::Statements, _))
    )
}

/// The predicates `subject`'s list in the default graph holds among `keys`, none when no such key remains.
fn subject_predicates(keys: &[(Vec<u8>, Vec<u8>)], subject: SystemId) -> TestResult<Vec<SystemId>> {
    let list = QuadKey::SubjectPredicates {
        tenant: TENANT,
        graph: DEFAULT_GRAPH,
        subject,
    }
    .to_bytes();
    let Some((_, value)) = keys.iter().find(|(key, _)| *key == list) else {
        return Ok(Vec::new());
    };
    ObjectRecords::read(value)?
        .iter()
        .map(|record| {
            EncodedTerm::from_bytes(record.encoding())
                .ok()
                .and_then(|term| term.as_id())
                .context("a predicates' list names a system ID")
        })
        .collect()
}

#[when("`knows` is held for `a` and `b` and is dropped")]
fn knows_dropped(w: &mut Steps) -> TestResult {
    started(w)?;
    w.drops = run(async {
        let mut client = admin().await?;
        let mut terms = TermTable::new();
        let changes = [
            (A, KNOWS, RequestTerm::Iri(C.to_string())),
            (B, KNOWS, RequestTerm::Iri(C.to_string())),
            (A, NAME, literal("Ann")),
            (B, NAME, literal("Bob")),
        ]
        .into_iter()
        .map(|quad| insert(&mut terms, None, quad))
        .collect::<TestResult<Vec<_>>>()?;
        wrote(&mut client, terms, changes).await?;
        // The edge again with a property, so it holds a statement ID and a statement record.
        let mut edge_terms = TermTable::new();
        let edge = WriteChange::InsertEdge {
            graph: None,
            subject: edge_terms.iri(A),
            predicate: edge_terms.iri(KNOWS),
            object: edge_terms.iri(C),
            properties: vec![(edge_terms.iri(SINCE), edge_terms.intern(literal("2020"))?)],
        };
        wrote(&mut client, edge_terms, vec![edge]).await?;
        let mut partition = dial().await?;
        let mut ids = Vec::new();
        for iri in [A, B, KNOWS, NAME] {
            ids.push((iri, id_of(&mut partition, iri).await?));
        }
        let before = tenant_keys(&mut partition).await?;
        dropped(&mut client, &DropCommand::predicate(TENANT, KNOWS)).await?;
        let after = tenant_keys(&mut partition).await?;
        Ok(DropSteps { before, after, ids })
    })?;
    Ok(())
}

#[then(
    "no key of `knows` remains, and neither the subject predicates of `a` nor of `b` holds `knows`"
)]
fn knows_gone(w: &mut Steps) -> TestResult {
    let seen = &w.drops;
    let (knows, name) = (seen.id(KNOWS)?, seen.id(NAME)?);
    // The partition held `knows` before: its keys, its statement record and both lists naming it.
    assert!(
        seen.before
            .iter()
            .any(|(key, _)| names_predicate(key, knows)),
        "keys of `knows` before the Drop"
    );
    assert!(
        seen.before.iter().any(|(key, _)| is_statement(key)),
        "the edge's statement record before the Drop"
    );
    for subject in [A, B] {
        assert!(
            subject_predicates(&seen.before, seen.id(subject)?)?.contains(&knows),
            "{subject}'s list before the Drop"
        );
    }
    let left: Vec<&Vec<u8>> = seen
        .after
        .iter()
        .map(|(key, _)| key)
        .filter(|key| names_predicate(key, knows))
        .collect();
    assert!(left.is_empty(), "keys of `knows` remain: {left:?}");
    assert!(
        !seen.after.iter().any(|(key, _)| is_statement(key)),
        "the edge's statement record is gone"
    );
    for subject in [A, B] {
        assert_eq!(
            subject_predicates(&seen.after, seen.id(subject)?)?,
            vec![name],
            "{subject}"
        );
    }
    w.roles = None;
    Ok(())
}

#[when("graphs `<g1>` and `<g2>` hold statements and `<g1>` is dropped")]
fn g1_dropped(w: &mut Steps) -> TestResult {
    started(w)?;
    w.drops = run(async {
        let mut client = admin().await?;
        let mut terms = TermTable::new();
        let changes = [
            (G1, (A, KNOWS, RequestTerm::Iri(B.to_string()))),
            (G1, (A, NAME, literal("Ann"))),
            (G2, (A, KNOWS, RequestTerm::Iri(C.to_string()))),
            (G2, (A, NAME, literal("Ann"))),
        ]
        .into_iter()
        .map(|(graph, quad)| insert(&mut terms, Some(graph), quad))
        .collect::<TestResult<Vec<_>>>()?;
        wrote(&mut client, terms, changes).await?;
        let mut partition = dial().await?;
        let mut ids = Vec::new();
        for iri in [A, C, KNOWS, NAME, G1, G2] {
            ids.push((iri, id_of(&mut partition, iri).await?));
        }
        let before = tenant_keys(&mut partition).await?;
        dropped(&mut client, &DropCommand::graph(TENANT, G1)).await?;
        let after = tenant_keys(&mut partition).await?;
        Ok(DropSteps { before, after, ids })
    })?;
    Ok(())
}

#[then("no key of `<g1>` remains, and every statement of `<g2>` is still held")]
fn g1_gone(w: &mut Steps) -> TestResult {
    let seen = &w.drops;
    let (g1, g2) = (Graph::Named(seen.id(G1)?), Graph::Named(seen.id(G2)?));
    let keys_of = |keys: &[(Vec<u8>, Vec<u8>)], graph: Graph| -> Vec<Vec<u8>> {
        keys.iter()
            .map(|(key, _)| key.clone())
            .filter(|key| of_graph(key, graph))
            .collect()
    };
    assert!(
        !keys_of(&seen.before, g1).is_empty(),
        "keys of `<g1>` before the Drop"
    );
    let left = keys_of(&seen.after, g1);
    assert!(left.is_empty(), "keys of `<g1>` remain: {left:?}");
    // `<g2>`'s keys are those it held before, and its forward lists hold each statement's object.
    assert_eq!(keys_of(&seen.after, g2), keys_of(&seen.before, g2));
    let forward = |predicate: SystemId| -> TestResult<Vec<Vec<u8>>> {
        let key = QuadKey::Forward {
            tenant: TENANT,
            graph: g2,
            predicate,
            subject: seen.id(A)?,
        }
        .to_bytes();
        let (_, value) = seen
            .after
            .iter()
            .find(|(held, _)| *held == key)
            .context("a forward list of `<g2>`")?;
        objects_of(value)
    };
    assert_eq!(forward(seen.id(KNOWS)?)?, vec![object_of(seen.id(C)?)]);
    assert_eq!(forward(seen.id(NAME)?)?.len(), 1, "`<a> <name> \"Ann\"`");
    w.roles = None;
    Ok(())
}
