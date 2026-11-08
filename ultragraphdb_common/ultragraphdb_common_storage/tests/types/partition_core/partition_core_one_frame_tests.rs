/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! How a partition's core runs a one-frame write and a Drop (`PROTOCOL.md` §6.5, §6.6), played through its inbox.

use oxrdf::NamedNode;
use ultragraphdb_common_role::Answer;
use ultragraphdb_common_storage::{CoreMessage, FrontendAsk};
use ultragraphdb_core_catalog::CatalogContents;
use ultragraphdb_core_mapping::{Declaration, PredicateDeclaration};
use ultragraphdb_protocol_wire::{
    BindKind, BodyView, Command, CommitOperation, CommitView, ConditionalWrite, DirectWrite,
    DropCommand, DropCommandView, ErrorCode, ErrorReply, FrameView, MAX_FRAME_SIZE, TermTable,
    WriteChange,
};
use ultragraphdb_tool_bdd::{TestError, TestResult};

use crate::types::partition_core::played_cores::{
    CLUSTER, Connection, Driver, PRINCIPAL, TENANT, answered, bind, error_in, refusal_of, reply_of,
    served,
};

/// How many DirectWrites a test sends at most while a catalog version reaches the partition.
const PROBES: u32 = 200;

/// An insert of `subject`'s `predicate` edge to `object`, each an IRI.
fn insert(terms: &mut TermTable, subject: &str, predicate: &str, object: &str) -> WriteChange {
    WriteChange::Insert {
        graph: None,
        subject: terms.iri(subject),
        predicate: terms.iri(predicate),
        object: terms.iri(object),
    }
}

/// A DirectWrite of `request` of `session` inserting an edge from an IRI partition 1 owns to one `partition` owns.
fn direct(
    driver: &Driver,
    partition: u32,
    (session, request): ([u8; 16], u64),
) -> TestResult<DirectWrite> {
    let mut terms = TermTable::new();
    let change = insert(
        &mut terms,
        &driver.owned_by(1, "s")?,
        &driver.owned_by(1, "knows")?,
        &driver.owned_by(partition, "o")?,
    );
    Ok(DirectWrite::new(TENANT, terms, vec![change])
        .with_principal(PRINCIPAL)
        .with_session(session, request))
}

/// The Commit pushed to `connection`: its stream, session, request and the operations' results counted, or the Error pushed.
async fn committed(
    connection: &mut Connection,
    what: &str,
) -> TestResult<Result<(u64, [u8; 16], u64, usize), ErrorReply>> {
    let (stream, frame) = connection.pushed(what).await?;
    if let Some(refusal) = error_in(&frame)? {
        return Ok(Err(refusal));
    }
    let view = FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE)?;
    let reply = CommitView::from_frame(&view)?;
    Ok(Ok((
        stream,
        reply.session(),
        reply.request(),
        reply.result_count(),
    )))
}

/// A DirectWrite is executed and committed in one request, its Commit pushed on its stream with its session and request.
#[test]
fn test_a_direct_write_commits_in_one_request() -> TestResult {
    served(1, async |driver| {
        let mut connection = driver.connect(1, 1 << 20)?;
        let write = direct(&driver, 1, ([7; 16], 1))?;
        assert_eq!(driver.ask(&connection, 3, &write).await?, Answer::Open);
        let reply = committed(&mut connection, "the Commit").await?;
        let (stream, session, request, _) =
            reply.map_err(|refusal| TestError::new(refusal.message()))?;
        assert_eq!((stream, session, request), (3, [7; 16], 1));
        Ok(())
    })
}

/// Two DirectWrites inserting the same edge, executed at one watermark, both commit, whichever's commit came first.
#[test]
fn test_concurrent_identical_inserts_both_commit() -> TestResult {
    served(1, async |driver| {
        let mut connection = driver.connect(1, 1 << 20)?;
        let first = driver.send(&connection, 3, &direct(&driver, 1, ([0; 16], 0))?)?;
        let second = driver.send(&connection, 5, &direct(&driver, 1, ([0; 16], 0))?)?;
        assert_eq!(answered(first).await?, Answer::Open);
        assert_eq!(answered(second).await?, Answer::Open);
        let mut streams = Vec::new();
        for what in ["a first Commit", "a second Commit"] {
            let reply = committed(&mut connection, what).await?;
            let (stream, ..) = reply.map_err(|refusal| TestError::new(refusal.message()))?;
            streams.push(stream);
        }
        streams.sort_unstable();
        assert_eq!(streams, [3, 5]);
        Ok(())
    })
}

/// A compare-and-set whose expected value does not hold is refused with `OperationFailed`, which is an answer and not run again
/// (`PROTOCOL.md` §6.6): a DirectWrite behind it in commit order setting the value it expected leaves it refused.
#[test]
fn test_a_failed_condition_is_an_answer() -> TestResult {
    served(1, async |driver| {
        let mut connection = driver.connect(1, 1 << 20)?;
        let (subject, predicate) = (driver.owned_by(1, "s")?, driver.owned_by(1, "p")?);
        let expected = "http://ex.org/expected";
        let mut terms = TermTable::new();
        let operation = CommitOperation::CompareAndSet {
            graph: None,
            subject: terms.iri(&subject),
            predicate: terms.iri(&predicate),
            expected: terms.iri(expected),
            new: terms.iri("http://ex.org/new"),
        };
        let condition = ConditionalWrite::new(TENANT, terms, Vec::new(), vec![operation])
            .with_principal(PRINCIPAL)
            .with_session([8; 16], 1);
        let mut set_terms = TermTable::new();
        let set = insert(&mut set_terms, &subject, &predicate, expected);
        let setting = DirectWrite::new(TENANT, set_terms, vec![set]).with_principal(PRINCIPAL);
        let first = driver.send(&connection, 3, &condition)?;
        let second = driver.send(&connection, 5, &setting)?;
        assert_eq!(answered(first).await?, Answer::Open);
        assert_eq!(answered(second).await?, Answer::Open);
        let mut outcomes = Vec::new();
        for what in ["an outcome", "another outcome"] {
            let (stream, frame) = connection.pushed(what).await?;
            outcomes.push((stream, error_in(&frame)?.map(|refusal| refusal.code())));
        }
        outcomes.sort_unstable_by_key(|(stream, _)| *stream);
        assert_eq!(outcomes, [(3, Some(ErrorCode::OperationFailed)), (5, None)]);
        Ok(())
    })
}

/// One-frame writes of two connections on the same stream number, both waiting on a home, are executions of their own: each
/// commits once the home answers, with its own session and request.
#[test]
fn test_one_frame_writes_of_two_connections_park_apart() -> TestResult {
    served(2, async |mut driver| {
        let mut first = driver.connect(1, 1 << 20)?;
        let mut second = driver.connect(2, 1 << 20)?;
        let one = direct(&driver, 2, ([1; 16], 4))?;
        let two = direct(&driver, 2, ([2; 16], 9))?;
        assert_eq!(driver.ask(&first, 3, &one).await?, Answer::Open);
        assert_eq!(driver.ask(&second, 3, &two).await?, Answer::Open);
        assert!(first.parked() > 0 && second.parked() > 0, "each parks");
        driver.release_homes();
        for (connection, expected) in [(&mut first, ([1; 16], 4)), (&mut second, ([2; 16], 9))] {
            let reply = committed(connection, "a Commit once the home answered").await?;
            let (stream, session, request, _) =
                reply.map_err(|refusal| TestError::new(refusal.message()))?;
            assert_eq!((stream, (session, request)), (3, expected));
            assert_eq!(connection.parked(), 0);
        }
        Ok(())
    })
}

/// A one-frame write of operations alone whose first subject another partition owns is refused as a stale route, code 26, which
/// the frontend resends (`PROTOCOL.md` §6.6).
#[test]
fn test_operations_alone_route_by_their_first_subject() -> TestResult {
    served(2, async |driver| {
        let connection = driver.connect(1, 1 << 20)?;
        let mut terms = TermTable::new();
        let operation = CommitOperation::CounterAdd {
            graph: None,
            subject: terms.iri(&driver.owned_by(2, "counted")?),
            predicate: terms.iri(&driver.owned_by(1, "count")?),
            delta: 1,
        };
        let write = ConditionalWrite::new(TENANT, terms, Vec::new(), vec![operation])
            .with_principal(PRINCIPAL);
        let refused = refusal_of(driver.ask(&connection, 3, &write).await?)?;
        assert_eq!(
            refused.code(),
            ErrorCode::WrongPartition,
            "{}",
            refused.message()
        );
        Ok(())
    })
}

/// An operation waiting on its home goes on after it once answered: run once, its one result in the Commit.
#[test]
fn test_an_operation_waiting_on_its_home_runs_once() -> TestResult {
    served(2, async |mut driver| {
        let mut connection = driver.connect(1, 1 << 20)?;
        let mut terms = TermTable::new();
        let operation = CommitOperation::CounterAdd {
            graph: None,
            subject: terms.iri(&driver.owned_by(1, "counted")?),
            predicate: terms.iri(&driver.owned_by(2, "count")?),
            delta: 2,
        };
        let write = ConditionalWrite::new(TENANT, terms, Vec::new(), vec![operation])
            .with_principal(PRINCIPAL)
            .with_session([3; 16], 1);
        assert_eq!(driver.ask(&connection, 3, &write).await?, Answer::Open);
        driver.release_homes();
        let reply = committed(&mut connection, "the Commit once the home answered").await?;
        let (_, _, _, results) = reply.map_err(|refusal| TestError::new(refusal.message()))?;
        assert_eq!(results, 1, "the operation ran once");
        Ok(())
    })
}

/// Declares `predicate` unique in the tenant's catalog, and waits until a DirectWrite inserting it is refused: that refusal.
async fn declared_unique(
    driver: &mut Driver,
    connection: &mut Connection,
    predicate: &str,
) -> TestResult<ErrorReply> {
    let declared = PredicateDeclaration::new(NamedNode::new(predicate)?).with_unique(true);
    let contents = CatalogContents::new(TENANT, 1, vec![Declaration::Predicate(declared)]);
    driver.tell(CoreMessage::Catalog(contents))?;
    let report = driver.asked("the catalog version's report").await?;
    assert!(matches!(report, FrontendAsk::Report(_)), "{report:?}");
    for request in 1..=PROBES {
        let mut terms = TermTable::new();
        let changes = vec![
            insert(
                &mut terms,
                &driver.owned_by(1, "s")?,
                "http://ex.org/knows",
                "http://ex.org/b",
            ),
            insert(
                &mut terms,
                &driver.owned_by(1, "s")?,
                predicate,
                "http://ex.org/o",
            ),
        ];
        let write = DirectWrite::new(TENANT, terms, changes).with_principal(PRINCIPAL);
        let stream = u64::from(request) * 2 + 1;
        match driver.ask(connection, stream, &write).await? {
            Answer::Refuse(refused) => return Ok(refused),
            Answer::Open => {
                let _committed = connection
                    .pushed("a DirectWrite before the version")
                    .await?;
            }
            other @ (Answer::Reply(_) | Answer::Done) => {
                return Err(TestError::new(format!("a DirectWrite answered {other:?}")));
            }
        }
    }
    Err(TestError::new(
        "the catalog version never reached the partition",
    ))
}

/// A DirectWrite inserting a predicate the catalog declares unique is refused with code 7, `detail` its term's index, naming it and
/// ConditionalWrite; one inserting only predicates not unique commits.
#[test]
fn test_a_direct_write_of_a_unique_predicate_is_refused() -> TestResult {
    const UNIQUE: &str = "http://ex.org/id";
    served(1, async |mut driver| {
        let mut connection = driver.connect(1, 1 << 20)?;
        let refused = declared_unique(&mut driver, &mut connection, UNIQUE).await?;
        assert_eq!(refused.code(), ErrorCode::Refused);
        assert!(
            refused.message().contains(UNIQUE) && refused.message().contains("ConditionalWrite"),
            "{}",
            refused.message()
        );
        // The term table holds the subject, `knows`, `b`, the subject again and then the unique predicate, at index 3.
        assert_eq!(refused.detail(), 3);
        let write = direct(&driver, 1, ([0; 16], 0))?;
        assert_eq!(driver.ask(&connection, 1_001, &write).await?, Answer::Open);
        let reply = committed(&mut connection, "a DirectWrite of predicates not unique").await?;
        let (stream, ..) = reply.map_err(|refusal| TestError::new(refusal.message()))?;
        assert_eq!(stream, 1_001);
        Ok(())
    })
}

/// A Drop on a stream a Bind of kind 2 opened commits in its own round, and its reply echoes it with the commit version.
#[test]
fn test_a_drop_commits_and_answers_with_its_version() -> TestResult {
    served(1, async |driver| {
        let mut connection = driver.connect(1, 1 << 20)?;
        let _bound = reply_of(driver.ask(&connection, 3, &bind(BindKind::Drop, 1)).await?)?;
        let drop = DropCommand::tenant_data(TENANT).with_session([5; 16], 2);
        assert_eq!(driver.ask(&connection, 3, &drop).await?, Answer::Open);
        let (stream, frame) = connection.pushed("the Drop's reply").await?;
        assert_eq!(stream, 3);
        let view = FrameView::validate(&frame, CLUSTER, MAX_FRAME_SIZE)?;
        assert_eq!(view.command(), Command::Drop, "{:?}", error_in(&frame)?);
        assert!(
            view.header().flags() & ultragraphdb_protocol_wire::FLAG_LAST != 0,
            "it ends the stream"
        );
        let reply = DropCommandView::from_frame(&view)?;
        assert_eq!(
            (reply.tenant(), reply.session(), reply.request()),
            (TENANT, [5; 16], 2)
        );
        assert!(reply.commit_version() > 0, "the commit version");
        Ok(())
    })
}

/// A Drop naming a tenant other than its Bind's is refused naming both.
#[test]
fn test_a_drop_names_its_binds_tenant() -> TestResult {
    served(1, async |driver| {
        let connection = driver.connect(1, 1 << 20)?;
        let _bound = reply_of(driver.ask(&connection, 3, &bind(BindKind::Drop, 1)).await?)?;
        let other = TENANT + 1;
        let refused = refusal_of(
            driver
                .ask(&connection, 3, &DropCommand::tenant_data(other))
                .await?,
        )?;
        assert_eq!(refused.code(), ErrorCode::Protocol);
        assert_eq!(
            refused.message(),
            format!("a Drop of tenant {other} on a stream bound to tenant {TENANT}")
        );
        Ok(())
    })
}
