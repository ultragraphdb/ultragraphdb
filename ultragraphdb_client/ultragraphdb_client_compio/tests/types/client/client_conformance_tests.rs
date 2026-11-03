/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The conformance suite through the compio shell: the golden frames of `ultragraphdb_protocol_wire` served over the
//! simulated network, plain and split and delayed. Every request the shell writes arrives as its golden frame, every
//! golden reply is read as its golden values, a reply repeated or lost is refused or timed out by the connection's
//! rule, and every bit flip and truncation of every golden reply is refused, never read as a value. Which refusal a
//! damaged frame earns is read off the transport's framer and the wire's validation, not off the client.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use ultragraphdb_client_compio::{
    Anonymous, Client, ClientConfig, ClientError, ClientErrorEnum, Login, ResultStream, Session,
    Transaction,
};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_scram::{ScramError, ScramLogin};
use ultragraphdb_protocol_transport::{
    BufferPool, FrameStep, Framer, NetworkFaults, SimNetwork, TransportError,
};
use ultragraphdb_protocol_wire::{
    Begin, BeginView, BodyBuilder, BodyView, Command, CommitView, DescribeView, DropCommand,
    DropCommandView, ErrorReplyView, ExecuteView, FrameView, Header, MAX_FRAME_SIZE, OperationView,
    PatternSlot, PrepareView, QuadPattern, Read, ReadView, RegisterView, ResultView, TermTable,
    WriteView,
};

use crate::utils::golden::{Golden, Step, golden, not_last, numbered, restream, serve_golden_sim};
use crate::utils::serve::{CLUSTER, SimConnect};
use ultragraphdb_tool_bdd::{Context as _, TestError, TestResult};

fn runtime() -> TestResult<compio::runtime::Runtime> {
    compio::runtime::Runtime::new().context("a runtime")
}

/// The client of the golden frames: their cluster, the release the golden Hello's header carries, and the client
/// name, credit granted two frames at a time (the golden Credit's).
fn config() -> TestResult<ClientConfig> {
    let hello = golden("hello")?;
    let release = FrameView::validate(&hello, CLUSTER, MAX_FRAME_SIZE)?
        .header()
        .release();
    Ok(ClientConfig::new(CLUSTER, "ultragraph-test")
        .with_release(release)
        .with_timeout(Duration::from_secs(2))
        .with_retry_delay(Duration::from_millis(5))
        .with_credit(2))
}

/// The networks the golden frames cross: plain, then split and delayed under three seeds.
fn networks() -> Vec<SimNetwork> {
    let faults = NetworkFaults::none()
        .with_split(500_000)
        .with_delay(300_000, 3);
    let mut networks = vec![SimNetwork::new(1, NetworkFaults::none())];
    networks.extend((1..=3).map(|seed| SimNetwork::new(seed, faults)));
    networks
}

/// `test` run against a server of `golden` over `network`.
fn served<F: Future<Output = TestResult>, L: Login>(
    network: SimNetwork,
    golden: &Rc<RefCell<Golden>>,
    login: L,
    test: impl FnOnce(Client<SimConnect, L>) -> F,
) -> TestResult {
    let rt = runtime()?;
    let connector = SimConnect::new(network);
    let accepted = Rc::clone(&connector.accepted);
    rt.block_on(async {
        let _server = compio::runtime::spawn(serve_golden_sim(Rc::clone(golden), accepted));
        test(Client::new(connector, config()?, login, 8)).await
    })
}

/// The body of golden frame `name`, as `read` takes it from the wire's view.
fn decoded<T>(
    name: &str,
    read: impl FnOnce(&FrameView<'_>) -> Result<T, ProtocolError>,
) -> TestResult<T> {
    let bytes = golden(name)?;
    let frame = FrameView::validate(&bytes, CLUSTER, MAX_FRAME_SIZE).context("a golden frame")?;
    read(&frame).context("a golden body")
}

/// The Begin of a transaction for writing: not a golden frame, the reply the commits and the abort need, its read
/// version 0 as a read-write Begin's reply has it (`PROTOCOL.md` §6.2).
fn writable() -> TestResult<Vec<u8>> {
    Begin::new(0, 0, false)
        .frame(Header::new(CLUSTER, Command::Begin))
        .context("a Begin")
}

/// A Result's values: its columns, rows, every cell column by column, and its resume key.
type ResultValues = (u32, u32, Vec<Option<u32>>, Vec<u8>);

/// The values of the Result `v` reads.
fn result_values(v: &ResultView<'_>) -> ResultValues {
    let (columns, rows) = (v.columns(), v.rows());
    let cells = (0..columns)
        .flat_map(|col| (0..rows).map(move |row| (col, row)))
        .map(|(col, row)| v.cell(col, row))
        .collect();
    (columns, rows, cells, v.resume().to_vec())
}

/// The golden Result's values, read from the frame by the wire crate's view.
fn golden_result() -> TestResult<ResultValues> {
    decoded("result", |f| {
        ResultView::from_frame(f).map(|v| result_values(&v))
    })
}

/// The session the golden Register assigns.
fn golden_session() -> TestResult<u128> {
    decoded("register", |f| {
        RegisterView::from_frame(f).map(|v| u128::from_be_bytes(v.session()))
    })
}

/// The namespace, read version and mode the golden Begin answers.
fn golden_begin() -> TestResult<(u64, u64, bool)> {
    decoded("begin", |f| {
        BeginView::from_frame(f).map(|v| (v.namespace(), v.read_version(), v.read_only()))
    })
}

/// The script of the conversation, in the order the client sends its frames.
fn conversation_script(golden_server: &mut Golden) -> TestResult {
    let result = golden("result")?;
    for step in [
        Step::any().reply(golden("register")?),
        Step::expect("prepare").reply(golden("prepare_reply")?),
        Step::expect("describe").reply(golden("describe_reply")?),
        Step::any().reply(writable()?),
        Step::any().reply(golden("begin")?),
        Step::expect("read")
            .reply(not_last(&result))
            .reply(not_last(&result))
            .reply(not_last(&result))
            .reply(not_last(&result)),
        Step::expect("credit").reply(result.clone()),
        Step::expect("read"),
        Step::expect("cancel"),
        Step::expect("execute").reply(result),
        Step::expect("write"),
        Step::expect("operation"),
        Step::expect("operation_counter"),
        Step::expect("operation_deep"),
        Step::any().reply(golden("commit")?),
        Step::any().reply(writable()?),
        Step::expect("abort"),
        Step::any().reply(writable()?),
        Step::any().reply(golden("error")?),
        Step::any().reply(writable()?),
        Step::any().reply(golden("error_operation")?),
        Step::expect("drop").reply(golden("drop_reply")?),
    ] {
        golden_server.push(step);
    }
    Ok(())
}

/// Every golden request and reply in one conversation, its values from `generate_bodies.py`'s comments.
async fn conversation(mut client: Client<SimConnect, Anonymous>) -> TestResult {
    client.connect().await.context("connected")?;
    golden_catalog(&mut client).await?;
    let writer = golden_reads(&mut client).await?;
    let committed_through = golden_commit(&mut client, writer).await?;
    golden_refusals_and_drop(&mut client, committed_through).await?;
    client.close().await;
    Ok(())
}

/// The Welcome, a session registered, a statement prepared and the data dictionary, read as their golden values.
async fn golden_catalog(client: &mut Client<SimConnect, Anonymous>) -> TestResult {
    // The golden Welcome: protocol 1, max_frame at its floor of 524,288, 64 streams, initial credit 4.
    let w = client.core().welcome().context("welcomed")?;
    assert_eq!(
        (
            w.protocol(),
            w.max_frame(),
            w.max_streams(),
            w.initial_credit(),
            w.server()
        ),
        (1, 1 << 19, 64, 4, "ultragraph 0.1")
    );
    // Register: the session the golden Register assigns.
    let session = client.register().await.context("registered")?;
    assert_eq!(session.number(), golden_session()?);
    // Prepare's reply: the statement, schema and result types the golden reply names.
    let statement = client
        .prepare(&decoded("prepare", |f| {
            PrepareView::from_frame(f)?.to_body()
        })?)
        .await?;
    let answer = decoded("prepare_reply", |f| PrepareView::from_frame(f)?.to_body())?;
    assert_eq!(
        (statement.id(), statement.schema(), statement.result_types()),
        (answer.statement(), answer.schema(), answer.result_types())
    );
    // Describe's reply: the description the wire reads from the golden bytes.
    let described = client.describe(0).await?;
    assert_eq!(
        described,
        decoded("describe_reply", |f| DescribeView::from_frame(f)?.to_body())?
    );
    Ok(())
}

/// A writer begun, then a reader on stream 5, whose reads and execution stream the golden Results; the writer.
async fn golden_reads(client: &mut Client<SimConnect, Anonymous>) -> TestResult<Transaction> {
    let writer = client.begin(0, false).await?;
    // Begin: the namespace, read version and mode the golden Begin answers, on the stream the golden Read names.
    let reader = client.begin(0, true).await?;
    let read_stream = decoded("read", |f| ReadView::from_frame(f).map(|v| v.transaction()))?;
    assert_eq!(
        (
            (
                reader.namespace(),
                reader.read_version(),
                reader.read_only()
            ),
            reader.stream()
        ),
        (golden_begin()?, read_stream)
    );
    // Four Results spend the Welcome's credit of four; the client grants two more (the golden Credit), and the last
    // Result is flagged last.
    let stream = client
        .read(
            &reader,
            &decoded("read", |f| ReadView::from_frame(f)?.to_body())?,
        )
        .await?;
    for _ in 0..5 {
        let batch = client.next_batch(&stream, result_values).await?;
        assert_eq!(batch, Some(golden_result()?));
    }
    assert_eq!(client.next_batch(&stream, result_values).await?, None);
    let cancelled = client
        .read(
            &reader,
            &decoded("read", |f| ReadView::from_frame(f)?.to_body())?,
        )
        .await?;
    client.cancel(cancelled).await;
    let executed = client
        .execute(
            &reader,
            &decoded("execute", |f| ExecuteView::from_frame(f)?.to_body())?,
        )
        .await?;
    assert_eq!(
        client.next_batch(&executed, result_values).await?,
        Some(golden_result()?)
    );
    assert_eq!(client.next_batch(&executed, result_values).await?, None);
    Ok(writer)
}

/// The writer's changes and its commit through session 9, then a transaction aborted; session 9.
async fn golden_commit(
    client: &mut Client<SimConnect, Anonymous>,
    writer: Transaction,
) -> TestResult<Session> {
    client
        .write(
            &writer,
            &decoded("write", |f| WriteView::from_frame(f)?.to_body())?,
        )
        .await?;
    for name in ["operation", "operation_counter", "operation_deep"] {
        client
            .operation(
                &writer,
                &decoded(name, |f| OperationView::from_frame(f)?.to_body())?,
            )
            .await?;
    }
    // Commit: through the session and request the golden Commit answers, its version and results read from it.
    let reply = decoded("commit", |f| CommitView::from_frame(f)?.to_body())?;
    let resumed = client.resume_session(u128::from_be_bytes(reply.session()), reply.request());
    let committed = client.commit(writer, resumed).await?;
    assert_eq!(
        (committed.version(), committed.request()),
        (reply.commit_version(), reply.request())
    );
    assert_eq!(committed.results(), reply.results());
    let aborted = client.begin(0, false).await?;
    client.abort(aborted).await?;
    Ok(resumed)
}

/// The two golden refusals of a commit, and a drop through `session`, the session the golden Commit answered.
async fn golden_refusals_and_drop(
    client: &mut Client<SimConnect, Anonymous>,
    session_of_commit: Session,
) -> TestResult {
    // Error: the golden conflict and failed compare-and-set, their codes, details and value read from the frames.
    let conflict_reply = decoded("error", |f| ErrorReplyView::from_frame(f)?.to_body())?;
    let failed_reply = decoded("error_operation", |f| {
        ErrorReplyView::from_frame(f)?.to_body()
    })?;
    let tx = client.begin(0, false).await?;
    let session = client.resume_session(104, 1);
    let conflict = client.commit(tx, session).await.err().context("an error")?;
    assert!(
        matches!(
            &conflict.0,
            ClientErrorEnum::Refused {
                code,
                detail,
                value: None,
                ..
            } if *code == conflict_reply.code() && *detail == conflict_reply.detail()
        ),
        "{conflict:?}"
    );
    assert!(conflict.retryable(), "a conflict is retryable");
    let operation_tx = client.begin(0, false).await?;
    let operation_session = client.resume_session(105, 1);
    let failed = client
        .commit(operation_tx, operation_session)
        .await
        .err()
        .context("an error")?;

    assert!(
        matches!(&failed.0, ClientErrorEnum::Refused { code, detail, value: Some(v), .. }
            if *code == failed_reply.code() && *detail == failed_reply.detail() && Some(v) == failed_reply.value()),
        "{failed:?}"
    );
    // Drop: the graph and namespace the golden Drop names, through the commit's session; its reply's version and
    // request read from the golden reply.
    let request = decoded("drop", |f| DropCommandView::from_frame(f)?.to_body())?;
    let reply = decoded("drop_reply", |f| DropCommandView::from_frame(f)?.to_body())?;
    let graph = request
        .target()
        .context("the golden Drop names a graph")?
        .to_string();
    let dropped = client
        .drop_command(
            DropCommand::graph(request.namespace(), graph),
            session_of_commit,
        )
        .await?;
    assert_eq!(
        (dropped.version(), dropped.request()),
        (reply.commit_version(), reply.request())
    );
    Ok(())
}

/// The requests the client writes in the conversation, golden byte for byte but for stream and sequence, in order.
const REQUESTS: [&str; 14] = [
    "hello",
    "prepare",
    "describe",
    "read",
    "credit",
    "read",
    "cancel",
    "execute",
    "write",
    "operation",
    "operation_counter",
    "operation_deep",
    "abort",
    "drop",
];

#[test]
fn test_every_golden_frame_crosses_the_shell_over_a_plain_split_and_delayed_network() -> TestResult
{
    for network in networks() {
        let server = Golden::new("welcome_no_auth")?;
        conversation_script(&mut server.borrow_mut())?;
        served(network, &server, Anonymous, conversation)?;
        let server = server.borrow();
        assert_eq!(server.mismatches(), &[] as &[String]);
        assert_eq!(server.matched(), &REQUESTS[..]);
        assert_eq!((server.steps_left(), server.connections()), (0, 1));
    }
    Ok(())
}

/// RFC 7677 §3's client nonce, the one the golden Authenticate exchange was made with.
fn rfc_nonce() -> Result<String, ScramError> {
    Ok("rOprNGfwEbeRWgbNEkqO".to_string())
}

/// The golden Authenticate exchange through the shell: RFC 7677's user "user" with password "pencil" answers the golden
/// Welcome and the server's first message with the golden frames, and acts as principal 7 until the golden expiry.
#[test]
fn test_the_golden_authenticate_exchange_crosses_the_shell() -> TestResult {
    for network in networks() {
        let server = Golden::new("welcome")?;
        server.borrow_mut().push(
            Step::expect("authenticate_client_first").reply(golden("authenticate_server_first")?),
        );
        server.borrow_mut().push(
            Step::expect("authenticate_client_final").reply(golden("authenticate_server_final")?),
        );
        let login = ScramLogin::new(
            "user",
            "pencil",
            rfc_nonce as fn() -> Result<String, ScramError>,
        )?;
        served(network, &server, login, |mut client| async move {
            client.connect().await.context("authenticated")?;
            // The server's final: principal 7, expiry 1,798,761,600,000,000 µs.
            assert_eq!(client.core().principal(), Some(7));
            assert_eq!(client.core().expires(), Some(1_798_761_600_000_000));
            client.close().await;
            Ok::<(), TestError>(())
        })?;
        let server = server.borrow();
        assert_eq!(server.mismatches(), &[] as &[String]);
        assert_eq!(
            server.matched(),
            &[
                "hello",
                "authenticate_client_first",
                "authenticate_client_final"
            ]
        );
    }
    Ok(())
}

/// A golden reply the network repeats is read once as its values, its copy refused by the rule, which gives the
/// connection up: the next request fails as lost, and the one after goes on a new connection. A golden reply the
/// network loses times its request out, and the next goes on a new connection.
#[test]
fn test_a_repeated_or_lost_golden_reply_through_the_shell() -> TestResult {
    let server = Golden::new("welcome_no_auth")?;
    for step in [
        Step::any().reply(golden("register")?).copies(2),
        Step::any().reply(golden("begin")?),
        Step::any().reply(golden("begin")?),
        Step::any().reply(golden("register")?).copies(0),
        Step::any().reply(golden("register")?),
    ] {
        server.borrow_mut().push(step);
    }
    let network = SimNetwork::new(1, NetworkFaults::none());
    served(network, &server, Anonymous, |mut client| async move {
        let session = client.register().await.context("the first copy")?;
        assert_eq!(session.number(), golden_session()?);
        // The copy is the Register's again, sequence 2 where 3 was due: the rule that gave the connection up.
        assert_eq!(
            client.begin(0, true).await.err().context("an error")?,
            ClientError::connection_lost(Some(ClientError::protocol(
                ProtocolError::sequence_order(3, 2)
            )))
        );
        assert_eq!(
            client.begin(0, true).await?.read_version(),
            golden_begin()?.1
        );
        assert_eq!(
            client.register().await.err().context("an error")?,
            ClientError::timed_out()
        );
        assert_eq!(client.register().await?.number(), golden_session()?);
        client.close().await;
        Ok::<(), TestError>(())
    })?;
    let server = server.borrow();
    assert_eq!((server.steps_left(), server.connections()), (0, 3));
    Ok(())
}

/// What the transport's framer and the wire's validation make of a damaged frame, read without the client: refused by
/// the framer, by a rule of the frame's, or passing both.
enum Damage {
    Framer(TransportError),
    Rule(ProtocolError),
    Passes,
}

impl Damage {
    /// The framer's refusal; `None` for any other damage.
    fn framer(self) -> Option<TransportError> {
        if let Damage::Framer(refusal) = self {
            Some(refusal)
        } else {
            None
        }
    }

    /// The rule refusing the frame; `None` for any other damage.
    fn rule(self) -> Option<ProtocolError> {
        if let Damage::Rule(rule) = self {
            Some(rule)
        } else {
            None
        }
    }
}

fn damage(bad: &[u8]) -> TestResult<Damage> {
    let pool = BufferPool::new(1, MAX_FRAME_SIZE);
    let mut framer = Framer::new(CLUSTER, MAX_FRAME_SIZE);
    let mut buffer = pool.acquire().context("a buffer")?;
    loop {
        let mut data = buffer.take();
        let want = framer.want().min(bad.len());
        data.clear();
        data.extend_from_slice(&bad[..want]);
        let short = data.len() < framer.want();
        let got = data.len();
        buffer.restore(data);
        if short {
            return Ok(Damage::Framer(framer.closed(got)));
        }
        match framer.advance(&buffer) {
            Ok(FrameStep::Read(_)) => {}
            Ok(FrameStep::Frame) => break,
            Err(refusal) => return Ok(Damage::Framer(refusal)),
        }
    }
    Ok(match FrameView::validate(bad, CLUSTER, MAX_FRAME_SIZE) {
        Err(rule) => Damage::Rule(rule),
        Ok(_) => Damage::Passes,
    })
}

/// The golden replies, each answering a request of one kind.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// A reply the request fails without when the connection is given up.
    Plain,
    /// A commit's or a drop's, retried through its session; with one attempt allowed, in doubt.
    Retried,
    /// A result's batch.
    Batch,
}

const REPLIES: [(&str, Kind); 9] = [
    ("register", Kind::Plain),
    ("begin", Kind::Plain),
    ("prepare_reply", Kind::Plain),
    ("describe_reply", Kind::Plain),
    ("commit", Kind::Retried),
    ("error", Kind::Retried),
    ("error_operation", Kind::Retried),
    ("drop_reply", Kind::Retried),
    ("result", Kind::Batch),
];

/// `answer` without its value.
fn ok<T>(answer: Result<T, ClientError>) -> Result<(), ClientError> {
    answer.map(|_| ())
}

/// The request golden reply `name` answers, sent once the server's script holds `last`, the reply's own step; the
/// answer, `Ok` for a value.
async fn ask(
    client: &mut Client<SimConnect, Anonymous>,
    server: &RefCell<Golden>,
    name: &str,
    session: u64,
    last: Step,
) -> TestResult<(Result<(), ClientError>, Option<ResultStream>)> {
    Ok(match name {
        "register" => {
            server.borrow_mut().push(last);
            (ok(client.register().await), None)
        }
        "begin" => {
            server.borrow_mut().push(last);
            (ok(client.begin(0, true).await), None)
        }
        "prepare_reply" => {
            server.borrow_mut().push(last);
            (
                ok(client
                    .prepare(&decoded("prepare", |f| {
                        PrepareView::from_frame(f)?.to_body()
                    })?)
                    .await),
                None,
            )
        }
        "describe_reply" => {
            server.borrow_mut().push(last);
            (ok(client.describe(0).await), None)
        }
        "drop_reply" => {
            server.borrow_mut().push(last);
            let session = client.resume_session(u128::from(session), 1);
            let command = DropCommand::graph(3, "http://ex.org/g");
            (ok(client.drop_command(command, session).await), None)
        }
        "result" => {
            server
                .borrow_mut()
                .push(Step::any().reply(golden("begin")?));
            server.borrow_mut().push(last);
            let reader = match client.begin(0, true).await {
                Ok(reader) => reader,
                Err(error) => return Ok((Err(error), None)),
            };
            let mut terms = TermTable::new();
            let a = terms.iri("http://ex.org/a");
            let pattern = QuadPattern::new(
                PatternSlot::Unbound,
                PatternSlot::Bound(a),
                PatternSlot::Unbound,
                PatternSlot::Unbound,
            );
            let read = Read::new(reader.stream(), 0, terms, pattern, Vec::new());
            let stream = client.read(&reader, &read).await?;
            (
                ok(client.next_batch(&stream, result_values).await),
                Some(stream),
            )
        }
        _ => {
            server.borrow_mut().push(Step::any().reply(writable()?));
            server.borrow_mut().push(last);
            let tx = match client.begin(0, false).await {
                Ok(tx) => tx,
                Err(error) => return Ok((Err(error), None)),
            };
            let session = client.resume_session(u128::from(session), 1);
            (ok(client.commit(tx, session).await), None)
        }
    })
}

/// The loss a framer's refusal gives: a close between frames is a plain loss, with nothing more to say; any other
/// refusal is its cause.
fn lost(refusal: TransportError) -> ClientError {
    if refusal == TransportError::closed() {
        ClientError::connection_lost(None)
    } else {
        ClientError::connection_lost(Some(ClientError::transport(refusal)))
    }
}

/// Golden reply `good` as the server sends it, moved to stream `on` and numbered `sequence`, then with bit `flip % 8`
/// of its byte `flip` flipped, or cut to its first `cut` bytes.
fn as_sent(
    good: &[u8],
    on: u64,
    sequence: u128,
    flip: Option<usize>,
    cut: Option<usize>,
) -> Vec<u8> {
    let mut bad = numbered(&restream(good, on), sequence);
    if let Some(at) = flip {
        bad[at] ^= 1 << (at % 8);
    }
    if let Some(end) = cut {
        bad.truncate(end);
    }
    bad
}

/// Every bit flip of every byte, one bit a byte, and every truncation of every golden reply, served through the shell
/// over a split and delayed network, the connection closed after it: never read as a value, and refused as the framer
/// and the wire's validation say. A plain request fails as lost, its cause the framer's refusal or the frame's rule; a
/// commit or a drop, allowed one attempt, is in doubt; a read fails with its own frame's rule, or as lost when the
/// framer gave the connection up.
#[test]
fn test_every_damaged_golden_reply_through_the_shell_is_refused_with_the_connections_rule()
-> TestResult {
    let server = Golden::new("welcome_no_auth")?;
    let faults = NetworkFaults::none()
        .with_split(500_000)
        .with_delay(300_000, 3);
    let config = config()?.with_commit_attempts(1);
    let rt = runtime()?;
    let connector = SimConnect::new(SimNetwork::new(7, faults));
    let accepted = Rc::clone(&connector.accepted);
    let mut cases = 0;
    let mut passing = 0;
    rt.block_on(async {
        let _server = compio::runtime::spawn(serve_golden_sim(Rc::clone(&server), accepted));
        let mut client = Client::new(connector, config, Anonymous, 8);
        for (name, kind) in REPLIES {
            // Each case on a connection of its own, its streams from 1: the reply numbered after the Welcome, and after
            // the Begin a commit or a read needs; on the request's stream, a read's the one after its transaction's.
            let (sequence, on) = match (name, kind) {
                ("drop_reply", _) | (_, Kind::Plain) => (2, 1),
                (_, Kind::Retried) => (3, 1),
                (_, Kind::Batch) => (3, 2),
            };
            let good = golden(name)?;
            let len = good.len();
            let damaged = (0..len).map(|at| (Some(at), None));
            let cuts = (0..len).map(|end| (None, Some(end)));
            for (flip, cut) in damaged.chain(cuts) {
                cases += 1;
                let session = 10_000 + cases;
                let mut step = Step::any().reply(good.clone());
                step = match (flip, cut) {
                    (Some(at), _) => step.damaged(at),
                    (_, Some(end)) => step.cut(end),
                    _ => unreachable!(),
                };
                let (answer, stream) = ask(&mut client, &server, name, session, step).await?;
                let stream_id = stream.as_ref().map(ResultStream::stream);
                assert!(stream_id.is_none_or(|id| id == on), "the read's stream");
                let bad = as_sent(&good, on, sequence, flip, cut);
                let error = answer.err().context("a damaged frame read as a value")?;
                let expected = match (damage(&bad)?, kind) {
                    (Damage::Passes, _) => {
                        passing += 1;
                        continue;
                    }
                    (_, Kind::Retried) => ClientError::retries_exhausted(u128::from(session), 1, 1),
                    (Damage::Framer(refusal), _) => lost(refusal),
                    (Damage::Rule(rule), Kind::Plain) => {
                        ClientError::connection_lost(Some(ClientError::protocol(rule)))
                    }
                    (Damage::Rule(rule), Kind::Batch) => ClientError::protocol(rule),
                };
                assert_eq!(error, expected, "{name}: flip {flip:?}, cut {cut:?}");
            }
        }
        client.close().await;
        Ok::<(), TestError>(())
    })?;
    let server = server.borrow();
    assert_eq!(server.mismatches(), &[] as &[String]);
    assert_eq!(server.steps_left(), 0);
    // One flip and one truncation per byte of every golden reply walked: twice their lengths, summed from the frames.
    let expected = REPLIES
        .iter()
        .map(|(name, _)| golden(name).map(|bytes| 2 * bytes.len()))
        .sum::<TestResult<usize>>()?;
    assert_eq!(usize::try_from(cases)?, expected);
    assert_eq!(
        passing, 0,
        "every damaged frame is refused by the framer or a rule"
    );
    Ok(())
}

/// A read of no golden frame, its transaction begun by the golden Begin on stream 1, the read on stream 2, served by
/// `last`.
async fn cut_read(
    client: &mut Client<SimConnect, Anonymous>,
    server: &RefCell<Golden>,
    last: Step,
) -> TestResult<ResultStream> {
    server
        .borrow_mut()
        .push(Step::any().reply(golden("begin")?));
    server.borrow_mut().push(last);
    let reader = client.begin(0, true).await?;
    let mut terms = TermTable::new();
    let a = terms.iri("http://ex.org/a");
    let pattern = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Bound(a),
        PatternSlot::Unbound,
        PatternSlot::Unbound,
    );
    let read = Read::new(reader.stream(), 0, terms, pattern, Vec::new());
    Ok(client.read(&reader, &read).await?)
}

/// A read cut short by the framer: byte 10 lies in the header's checksummed bytes, so the framer refuses the second
/// batch's header, and the read fails as lost with that refusal.
async fn cut_by_the_framer(
    client: &mut Client<SimConnect, Anonymous>,
    script: &RefCell<Golden>,
    result: &[u8],
) -> TestResult {
    // Byte 10 lies in the header's checksummed bytes: the framer refuses the header, as the transport says of
    // these very bytes (numbered 4 after the Welcome, the Begin and the first batch, on the read's stream 2).
    let header = Step::any()
        .reply(not_last(result))
        .reply(not_last(result))
        .damaged(10);
    let stream = cut_read(client, script, header).await?;
    let mut bad = numbered(&restream(&not_last(result), 2), 4);
    bad[10] ^= 1 << 2;
    let refusal = damage(&bad)?.framer().context("refused by the framer")?;
    assert_eq!(
        client.next_batch(&stream, result_values).await?,
        Some(golden_result()?)
    );
    assert_eq!(
        client
            .next_batch(&stream, result_values)
            .await
            .err()
            .context("an error")?,
        ClientError::connection_lost(Some(ClientError::transport(refusal)))
    );
    Ok(())
}

/// A read cut short by a rule of its own frame: byte 400 lies in the body, so the second batch passes the framer and
/// breaks a rule, and the read fails with it.
async fn cut_by_a_rule(
    client: &mut Client<SimConnect, Anonymous>,
    script: &RefCell<Golden>,
    result: &[u8],
) -> TestResult {
    // Byte 400 lies in the body: the frame passes the framer and breaks a rule of its own.
    let body = Step::any()
        .reply(not_last(result))
        .reply(not_last(result))
        .damaged(400);
    let stream = cut_read(client, script, body).await?;
    let mut bad = numbered(&restream(&not_last(result), 2), 4);
    bad[400] ^= 1;
    let rule = damage(&bad)?.rule().context("a rule refuses the body")?;
    assert_eq!(
        client.next_batch(&stream, result_values).await?,
        Some(golden_result()?)
    );
    assert_eq!(
        client
            .next_batch(&stream, result_values)
            .await
            .err()
            .context("an error")?,
        ClientError::protocol(rule),
        "the read's own frame"
    );
    Ok(())
}

/// A read whose connection is lost after some batches and before its last fails as lost, carrying what gave the
/// connection up, and never finishes: the server closing after two batches, a plain loss; the framer refusing the
/// second batch's header; the core refusing the second batch's body, the read's own frame, by its rule. A request whose
/// reply breaks a rule fails as lost with that rule.
#[test]
fn test_a_read_cut_short_through_the_shell_fails_as_lost_with_its_cause() -> TestResult {
    let result = golden("result")?;
    let server = Golden::new("welcome_no_auth")?;
    let network = SimNetwork::new(2, NetworkFaults::none().with_split(500_000));
    let script = Rc::clone(&server);
    served(network, &server, Anonymous, |mut client| async move {
        let two = Step::any()
            .reply(not_last(&result))
            .reply(not_last(&result))
            .close();
        let stream = cut_read(&mut client, &script, two).await?;
        for _ in 0..2 {
            assert_eq!(
                client.next_batch(&stream, result_values).await?,
                Some(golden_result()?)
            );
        }
        assert_eq!(
            client
                .next_batch(&stream, result_values)
                .await
                .err()
                .context("an error")?,
            ClientError::connection_lost(None),
            "cut short, not finished"
        );
        cut_by_the_framer(&mut client, &script, &result).await?;
        cut_by_a_rule(&mut client, &script, &result).await?;
        // A Register answered by the golden Describe reply with a body byte flipped, numbered 2 on stream 1.
        script
            .borrow_mut()
            .push(Step::any().reply(golden("describe_reply")?).damaged(400));
        let mut bad = numbered(&restream(&golden("describe_reply")?, 1), 2);
        bad[400] ^= 1;
        let rule = damage(&bad)?.rule().context("a rule refuses the body")?;
        assert_eq!(
            client.register().await.err().context("an error")?,
            ClientError::connection_lost(Some(ClientError::protocol(rule)))
        );
        client.close().await;
        Ok::<(), TestError>(())
    })?;
    let server = server.borrow();
    assert_eq!(server.mismatches(), &[] as &[String]);
    assert_eq!((server.steps_left(), server.connections()), (0, 4));
    Ok(())
}

/// A read whose transaction ended normally is finished: `StreamFinished`, not a loss.
#[test]
fn test_a_read_whose_transaction_ended_still_answers_stream_finished() -> TestResult {
    let server = Golden::new("welcome_no_auth")?;
    let network = SimNetwork::new(1, NetworkFaults::none());
    let script = Rc::clone(&server);
    served(network, &server, Anonymous, |mut client| async move {
        script
            .borrow_mut()
            .push(Step::any().reply(golden("begin")?));
        script.borrow_mut().push(Step::any());
        script.borrow_mut().push(Step::expect("abort"));
        let reader = client.begin(0, true).await?;
        let mut terms = TermTable::new();
        let a = terms.iri("http://ex.org/a");
        let pattern = QuadPattern::new(
            PatternSlot::Unbound,
            PatternSlot::Bound(a),
            PatternSlot::Unbound,
            PatternSlot::Unbound,
        );
        let stream = client
            .read(
                &reader,
                &Read::new(reader.stream(), 0, terms, pattern, Vec::new()),
            )
            .await?;
        client.abort(reader).await?;
        assert_eq!(
            client
                .next_batch(&stream, result_values)
                .await
                .err()
                .context("an error")?,
            ClientError::stream_finished(stream.stream())
        );
        client.close().await;
        Ok::<(), TestError>(())
    })?;
    Ok(())
}
