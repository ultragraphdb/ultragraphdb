/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The client's conformance suite against the binaries: the conversation its crates play against `ultragraphdb_protocol_wire`'s
//! golden frames, sent to a real frontend, compute and storage. What a request says comes from its golden frame, read by the
//! wire's own view; what the answer must be comes from the protocol's rule for that request and from the frame itself, so
//! nothing restates a golden value, and where a golden frame's values are an example a real server need not repeat, the rule
//! decides.

use cucumber::{then, when};
use ultragraphdb_client_compio::{ClientError, ClientErrorEnum, Committed, Session, Transaction};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{
    Begin, BeginView, BodyView, Command, CommitOperation, Declared, DeclaredKind,
    DeclaredPredicate, DropCommand, DropCommandView, ErrorCode, Execute, ExecuteView, FrameView,
    MAX_FRAME_FLOOR, MAX_FRAME_SIZE, OperationView, Prepare, PrepareView, QuadPattern, Read,
    ReadView, RequestTerm, TermTable, Write, WriteChange, WriteView,
};
use ultragraphdb_tests::CLUSTER;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::Steps;
use crate::client_reads::{Answer, Pattern, TENANT, batch_of, drain, refusal_of, string};
use crate::cluster::shared_client;
use crate::endpoint_steps::declare;
use crate::lossy_link::LossyLink;
use crate::request_steps::latest_read;
use crate::shells::{ClientAt, Knobs, TestClient, run};
use crate::transaction_steps::{committed, in_shared};

/// Golden frame `name`, read from the wire crate's `testdata/golden/bodies/`.
fn golden(name: &str) -> TestResult<Vec<u8>> {
    // The repository's root, three levels above this crate's `testdata/`, taken lexically: the directory need not exist.
    let testdata = ultragraphdb_tool_bdd::testdata_dir()?;
    let root = testdata
        .ancestors()
        .nth(3)
        .context("the repository's root")?;
    let path = root
        .join("ultragraphdb_protocol/ultragraphdb_protocol_wire/testdata/golden/bodies")
        .join(format!("{name}.frame"));
    Ok(std::fs::read(path)?)
}

/// The body of golden frame `name`, as `read` takes it from the wire's view.
fn decoded<T>(
    name: &str,
    read: impl FnOnce(&FrameView<'_>) -> Result<T, ProtocolError>,
) -> TestResult<T> {
    let bytes = golden(name)?;
    let frame = FrameView::validate(&bytes, CLUSTER, MAX_FRAME_SIZE)?;
    Ok(read(&frame)?)
}

/// What a change of the golden Write must earn alone in a transaction of its own, by `PROTOCOL.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Earns {
    /// It commits.
    Commit,
    /// §4.4: refused with code 7, the rule a term or a statement's IRI breaks.
    Refused,
}

/// The golden Write's changes, each with what it earns: a triple term in a change and a statement named by a skolem IRI no
/// dictionary holds are refused (§4.4), a vector index waits for stage 8 (§11.13), and the rest commit. A declaration's terms
/// are its own, so it travels by itself.
fn earned(change: &WriteChange, terms: &TermTable) -> TestResult<Earns> {
    let triple = |index: u32| matches!(terms.get(index), Some(RequestTerm::Triple { .. }));
    Ok(match change {
        WriteChange::Insert { object, .. } if triple(*object) => Earns::Refused,
        WriteChange::DeleteStatement { .. } => Earns::Refused,
        WriteChange::SetProperty { .. } => Earns::Refused,
        WriteChange::Declare(Declared::Predicate(declared))
            if declared.vector_index().is_some() =>
        {
            Earns::Refused
        }
        WriteChange::Insert { .. }
        | WriteChange::InsertEdge { .. }
        | WriteChange::Delete { .. }
        | WriteChange::DeletePattern { .. }
        | WriteChange::DeleteProperty { .. }
        | WriteChange::Declare(_) => Earns::Commit,
    })
}

/// The connection's first frames: what the Welcome states, against the protocol's rule for each field, then a session.
async fn the_handshake(client: &mut TestClient) -> TestResult<Session> {
    let hello = decoded("hello", |frame| {
        ultragraphdb_protocol_wire::HelloView::from_frame(frame)?.to_body()
    })?;
    let welcome = client.welcome().context("welcomed")?;
    assert!(welcome.authenticates(), "{welcome:?}");
    assert!(welcome.protocol() >= hello.protocol_min(), "{welcome:?}");
    assert!(welcome.protocol() <= hello.protocol_max(), "{welcome:?}");
    assert!(
        welcome.max_frame() as usize >= MAX_FRAME_FLOOR,
        "{welcome:?}"
    );
    assert_eq!(
        welcome.initial_credit(),
        ultragraphdb_protocol_wire::DEFAULT_INITIAL_CREDIT
    );
    assert_eq!(
        welcome.max_streams(),
        ultragraphdb_protocol_wire::DEFAULT_MAX_STREAMS
    );
    assert!(!welcome.server().is_empty(), "{welcome:?}");
    let first = client.register().await?;
    let second = client.register().await?;
    assert_ne!(first.number(), 0, "a session has an identity");
    assert_ne!(first.number(), second.number(), "each its own");
    Ok(first)
}

/// Sends each change of the golden Write in a transaction of its own, checking what the protocol says it earns.
async fn the_golden_write(client: &mut TestClient, session: Session) -> TestResult {
    let body = decoded("write", |frame| WriteView::from_frame(frame)?.to_body())?;
    // Whole, it mixes declarations with changes, refused by §11.13's first rule: code 7, the first declaration's record.
    let whole = client.begin(TENANT, false).await?;
    client.write(&whole, &body).await?;
    let refused = client
        .commit(whole, None)
        .await
        .err()
        .context("the mixed Write refused")?;
    assert_eq!(refusal_of(&refused)?.0, ErrorCode::Refused, "{refused:?}");
    for change in body.changes() {
        let earns = earned(change, body.terms())?;
        let single = Write::new(body.terms().clone(), vec![change.clone()]);
        let own = client.begin(TENANT, false).await?;
        client.write(&own, &single).await?;
        let through = if matches!(change, WriteChange::Declare(_)) {
            None
        } else {
            Some(session)
        };
        let answered = client.commit(own, through).await;
        match (earns, answered) {
            (Earns::Commit, Ok(_)) => {}
            (Earns::Refused, Err(answer)) => {
                assert_eq!(
                    refusal_of(&answer)?.0,
                    ErrorCode::Refused,
                    "{change:?} {answer:?}"
                );
            }
            (earns, answered) => {
                return Err(TestError::new(format!(
                    "{change:?} earns {earns:?}, answered {answered:?}"
                )));
            }
        }
    }
    Ok(())
}

/// The three golden Operations, in a transaction through `session`: the first creates its node, the counter takes its golden
/// delta, the third's value breaks no rule of an operation: its results, in the order asked.
async fn the_golden_operations(client: &mut TestClient, session: Session) -> TestResult<Committed> {
    let bodies = ["operation", "operation_counter"]
        .map(|name| decoded(name, |frame| OperationView::from_frame(frame)?.to_body()));
    let [upsert, counter] = bodies;
    let (upsert, counter) = (upsert?, counter?);
    let unique = match upsert.operation() {
        CommitOperation::UpsertAndLink { predicate, .. } => upsert
            .terms()
            .get(*predicate)
            .context("the unique predicate's term")?,
        other @ (CommitOperation::InsertIfAbsent { .. }
        | CommitOperation::CompareAndSet { .. }
        | CommitOperation::CounterAdd { .. }) => {
            return Err(TestError::new(format!("an upsert-and-link: {other:?}")));
        }
    };
    let RequestTerm::Iri(unique) = unique else {
        return Err(TestError::new(format!("an IRI: {unique:?}")));
    };
    declare(
        client,
        DeclaredPredicate::new(unique)
            .with_kind(DeclaredKind::Property)
            .with_single(true)
            .with_unique(true),
    )
    .await?;
    let own = client.begin(TENANT, false).await?;
    client.operation(&own, &upsert).await?;
    client.operation(&own, &counter).await?;
    Ok(client.commit(own, session).await?)
}

/// The golden Prepare at this cluster's tenant, then the golden Execute of it in `reader`: the statement's result types and
/// the Execute's rows.
async fn the_golden_statement(
    client: &mut TestClient,
    reader: &Transaction,
) -> TestResult<(Vec<ultragraphdb_protocol_wire::ColumnType>, Answer)> {
    let asked = decoded("prepare", |frame| PrepareView::from_frame(frame)?.to_body())?;
    let pattern: QuadPattern = *asked.pattern().context("the golden pattern")?;
    let prepare = Prepare::new(
        TENANT,
        0,
        0,
        asked.terms().clone(),
        pattern,
        asked.parameter_types().to_vec(),
        Vec::new(),
    );
    let statement = client.prepare(&prepare).await?;
    let execute = decoded("execute", |frame| ExecuteView::from_frame(frame)?.to_body())?;
    let execute = Execute::new(
        statement.id(),
        reader.stream(),
        execute.limit(),
        execute.terms().clone(),
        execute.parameters().to_vec(),
        Vec::new(),
    );
    let stream = client.execute(reader, &execute).await?;
    Ok((
        statement.result_types().to_vec(),
        drain(client, &stream).await?,
    ))
}

/// The golden Read's terms and pattern in `reader`, its limit and no key; with its golden key too, which no server gave: refused
/// naming it.
async fn the_golden_read(
    client: &mut TestClient,
    reader: &Transaction,
) -> TestResult<(Answer, ClientError)> {
    let asked = decoded("read", |frame| ReadView::from_frame(frame)?.to_body())?;
    let read = Read::new(
        reader.stream(),
        asked.limit(),
        asked.terms().clone(),
        *asked.pattern(),
        Vec::new(),
    );
    let stream = client.read(reader, &read).await?;
    let answer = drain(client, &stream).await?;
    let keyed = Read::new(
        reader.stream(),
        asked.limit(),
        asked.terms().clone(),
        *asked.pattern(),
        asked.resume().to_vec(),
    );
    let keyed_stream = client.read(reader, &keyed).await?;
    let refused = client
        .next_batch(&keyed_stream, batch_of)
        .await
        .err()
        .context("the golden key refused")?;
    Ok((answer, refused))
}

/// Everything the conversation's replies said, for its `then` step.
#[derive(Debug, Default)]
pub(crate) struct Conversation {
    read: Answer,
    executed: Answer,
    key_refused: Option<ClientError>,
    result_types: Vec<ultragraphdb_protocol_wire::ColumnType>,
    operations: Option<Committed>,
    conflict: Option<ClientError>,
    dropped: Option<Committed>,
    level_kept: bool,
    gone: Answer,
}

/// The conversation of the conformance suite, every request from its golden frame.
async fn converse(client: &mut TestClient, tail: &str) -> TestResult<Conversation> {
    let session = the_handshake(client).await?;
    // Golden values name one example's cluster: every IRI of these frames is written fresh in a cluster the scenarios share by
    // keeping its golden name, so one run sends the conversation once per cluster.
    the_golden_write(client, session).await?;
    let operations = the_golden_operations(client, session).await?;
    let asked = decoded("begin", |frame| BeginView::from_frame(frame)?.to_body())?;
    let begin = Begin::new(TENANT, 0, true)
        .with_level(asked.level())
        .with_deadline_ms(5_000);
    let reader = client.begin_with(begin, None).await?;
    let level_kept = reader.level() == asked.level() && reader.read_only() == asked.read_only();
    let (read, key_refused) = the_golden_read(client, &reader).await?;
    let (result_types, executed) = the_golden_statement(client, &reader).await?;
    client.abort(reader).await?;
    let conflict = a_real_conflict(client, session, tail).await?;
    let drop_asked = decoded("drop", |frame| {
        DropCommandView::from_frame(frame)?.to_body()
    })?;
    let graph = drop_asked
        .target()
        .context("the golden Drop names a graph")?;
    let dropped = client
        .drop_command(DropCommand::graph(TENANT, graph), session)
        .await?;
    let gone = latest_read(
        client,
        &Pattern::of(None, None).in_graph(crate::client_reads::GraphAt::Named(graph.to_string())),
    )
    .await?;
    Ok(Conversation {
        read,
        executed,
        key_refused: Some(key_refused),
        result_types,
        operations: Some(operations),
        conflict: Some(conflict),
        dropped: Some(dropped),
        level_kept,
        gone,
    })
}

/// A commit a later write conflicts with: the refusal a real partition gives where the golden Error is an example of one.
async fn a_real_conflict(
    client: &mut TestClient,
    session: Session,
    tail: &str,
) -> TestResult<ClientError> {
    let (a, p) = (format!("{tail}a"), format!("{tail}p"));
    let own = client.begin(TENANT, false).await?;
    let found =
        crate::client_reads::read_whole(client, &own, &Pattern::of(Some(&a), Some(&p))).await?;
    crate::transaction_steps::nothing(&found)?;
    committed(client, &a, &p, &string("x")).await?;
    client
        .write(
            &own,
            &crate::client_reads::write_of(&format!("{tail}other"), &p, &string("x"))?,
        )
        .await?;
    client
        .commit(own, session)
        .await
        .err()
        .context("a conflict")
}

#[when("the client's conformance conversation goes to the three binaries")]
fn conversation(w: &mut Steps) -> TestResult {
    let tail = w.iri("");
    w.conversation = Some(in_shared(w, async |client| converse(client, &tail).await)?);
    Ok(())
}

#[then(
    "each golden request earns what the protocol says of it, and each reply is read as the protocol's values"
)]
fn conversation_read(w: &mut Steps) -> TestResult {
    let over = w.over();
    let said = w.conversation.as_ref().context("the conversation")?;
    // The golden Prepare's reply names the result columns of the golden pattern: this server's answer must name the same.
    let reply = decoded("prepare_reply", |frame| {
        PrepareView::from_frame(frame)?.to_body()
    })?;
    assert_eq!(said.result_types, reply.result_types(), "{over}");
    assert!(said.level_kept, "the Begin's level and mode {over}");
    let operations = said.operations.as_ref().context("the operations' commit")?;
    let results = operations.results();
    assert_eq!(results.len(), 2, "{operations:?} {over}");
    let created = results.first().context("the upsert's result")?;
    assert!(
        created.created() || operations.node(0).is_some(),
        "{operations:?}"
    );
    let refused = said
        .key_refused
        .as_ref()
        .context("the golden key's refusal")?;
    assert!(
        refused.to_string().contains("resume key"),
        "{refused:?} {over}"
    );
    let conflict = said.conflict.as_ref().context("the conflict")?;
    assert_eq!(refusal_of(conflict)?.0, ErrorCode::Conflict, "{conflict:?}");
    assert!(conflict.retryable(), "a conflict runs again {over}");
    let dropped = said.dropped.as_ref().context("the drop's reply")?;
    assert!(dropped.version() > 0, "{dropped:?}");
    assert!(said.gone.rows.is_empty(), "{:?} {over}", said.gone);
    // What the golden Read names, `<a> <name>`, no golden Write holds, so a real server finds no row, in either form.
    assert!(said.read.rows.is_empty(), "{:?} {over}", said.read);
    assert!(said.executed.rows.is_empty(), "{:?} {over}", said.executed);
    Ok(())
}

/// A client of `at`, one attempt at each commit.
async fn once_client(at: &ClientAt) -> TestResult<TestClient> {
    let mut client = TestClient::new(at, &Knobs::one_attempt())?;
    client.connect().await?;
    Ok(client)
}

#[when("a real reply is damaged on its way to the client")]
fn damaged_reply(w: &mut Steps) -> TestResult {
    let at = shared_client(w.carrier)?;
    let link = LossyLink::to(&at)?;
    let [a, p] = ["a", "p"].map(|name| w.iri(name));
    let outcomes = run(async {
        let mut client = once_client(&link.client_at()).await?;
        let mut outcomes = Vec::new();
        // In the header's checksummed bytes, and in the body's: the framer's refusal, then a rule of the frame's.
        for byte in [10_usize, 300] {
            link.flip_reply(Command::Begin, byte);
            outcomes.push(client.begin(TENANT, false).await.map(|_| ()));
        }
        let own = client.begin(TENANT, false).await?;
        client
            .write(&own, &crate::client_reads::write_of(&a, &p, &string("x"))?)
            .await?;
        let session = client.register().await?;
        link.flip_reply(Command::Commit, 300);
        outcomes.push(client.commit(own, session).await.map(|_| ()));
        Ok(outcomes)
    })?;
    w.seen.facts = outcomes
        .iter()
        .map(|outcome| format!("{outcome:?}"))
        .collect();
    w.committed = outcomes.iter().all(Result::is_err);
    w.seen.refusals = outcomes.into_iter().filter_map(Result::err).collect();
    Ok(())
}

#[then(
    "the client refuses it by the connection's rule, never reading it as a value, and a commit so answered stays in doubt"
)]
fn damaged_refused(w: &mut Steps) -> TestResult {
    assert!(
        w.committed,
        "a damaged reply read as a value: {:?} {}",
        w.seen.facts,
        w.over()
    );
    let [header, body, commit] = w.seen.refusals.as_slice() else {
        return Err(TestError::new(format!(
            "three refusals: {:?}",
            w.seen.refusals
        )));
    };
    for lost in [header, body] {
        assert!(
            matches!(
                &lost.0,
                ClientErrorEnum::ConnectionLost { .. }
                    | ClientErrorEnum::Protocol { .. }
                    | ClientErrorEnum::Transport { .. }
            ),
            "{lost:?} {}",
            w.over()
        );
    }
    assert!(
        matches!(&commit.0, ClientErrorEnum::InDoubt { .. }),
        "{commit:?} {}",
        w.over()
    );
    Ok(())
}
