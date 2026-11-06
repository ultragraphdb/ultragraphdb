/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The scenarios of a request's own rules, its transaction's order, its session, its tenant and its catalog step, and the helpers both
//! step files share.

use std::cell::Cell;
use std::future::Future;
use std::rc::Rc;
use std::time::Duration;

use compio::runtime::Runtime;
use cucumber::{then, when};
use oxrdf::Literal;
use ultragraphdb_common_frontend::{
    Authorizer, DoubleFrame, EndpointLimits, IriPlace, RoleDouble, batch_frame, empty_batch,
    iri_place, last_frame, partition_answer,
};
use ultragraphdb_common_role::{Answer, RoleClient};
use ultragraphdb_core_ids::{
    Bucket, Epoch, Sequence, SessionToken, SystemId, Tenant, TypePrefix, Version,
};
use ultragraphdb_core_terms::{EncodedTerm, SYSTEM_VOCABULARY, TermError};
use ultragraphdb_protocol_scram::{ScramClient, system_nonce};
use ultragraphdb_protocol_transport::{
    Address, AuthenticationStep, BufferPool, ClientAuthentication, Framer,
};
use ultragraphdb_protocol_transport_compio::{Carrier, Connection};
use ultragraphdb_protocol_wire::{
    Abort, Action, AuthenticateView, Begin, BindView, BodyBuilder, BodyView, Cancel, ColumnType,
    Command, Commit, CommitOperation, CommitView, ComputeReadView, ConditionalWrite, Declared,
    DeclaredKind, DeclaredPredicate, DefaultEntry, Describe, DescribeView, DirectWrite,
    DropCommand, ErrorCode, ErrorReply, Execute, FLAG_LAST, Header, Held, Hello, Join,
    MAX_FRAME_FLOOR, MAX_FRAME_SIZE, Operation, PatternSlot, Prepare, PrepareView, QuadPattern,
    Read, ReadLevel, ReferenceView, RequestTerm, RequestTermView, ResultBatch, ResultView,
    TenantRun, TermTable, Write, WriteChange, xxh3_64,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::Steps;
use crate::endpoint_stage::{
    BIND_VERSION, CLUSTER, Client, OBJECT, Options, PREDICATE, Stage, answer_empty, fresh,
    insert_into, pause, until, view_frame, write_of,
};

/// The longest a scenario runs before it counts as hung.
const SCENARIO: Duration = Duration::from_secs(30);

/// A `When`'s exchange, on a stage of `options` and a runtime of its own.
pub(crate) fn run<F, Fut>(w: &mut Steps, options: Options, scenario: F) -> TestResult
where
    F: FnOnce(Stage) -> Fut,
    Fut: Future<Output = TestResult>,
{
    Runtime::new()?.block_on(async {
        let stage = Stage::start(options).await?;
        compio::time::timeout(SCENARIO, scenario(stage))
            .await
            .map_err(|_| TestError::new("the scenario did not end"))?
    })?;
    w.ran = true;
    Ok(())
}

/// Denies every principal every action.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Deny;

impl Authorizer for Deny {
    fn allows(&self, _principal: u64, _action: Action, _tenant: u64) -> bool {
        false
    }
}

/// Limits taking a compute process for hung after `ms`.
pub(crate) fn stalling(ms: u64) -> EndpointLimits {
    EndpointLimits::new(1 << 22, Duration::from_millis(ms))
}

/// Limits of a read-write transaction living `ms`.
pub(crate) fn limited(ms: u32) -> EndpointLimits {
    EndpointLimits::new(1 << 22, Duration::from_secs(30)).with_transaction_ms(ms)
}

/// The bucket of an ordinary IRI in tenant 0.
pub(crate) fn bucket_of(iri: &str) -> TestResult<u32> {
    match iri_place(0, iri) {
        IriPlace::Ordinary(bucket) => Ok(bucket.get()),
        IriPlace::Skolem(_) | IriPlace::Nobody | IriPlace::TooLong => {
            Err(TestError::new("an ordinary IRI"))
        }
    }
}

/// Whether `frame` holds the text `iri`.
pub(crate) fn names(frame: Option<&DoubleFrame>, iri: &str) -> TestResult<bool> {
    let frame = frame.context("a frame")?;
    Ok(String::from_utf8_lossy(frame.bytes()).contains(iri))
}

/// Code 23 of partitions `one` and `two`.
pub(crate) fn cross_refusal(one: u32, two: u32) -> ErrorReply {
    ErrorReply::new(
        ErrorCode::CrossPartition,
        "the read reaches another partition",
    )
    .with_detail(u64::from(one) | (u64::from(two) << 32))
}

/// Sends `batch` on the read at `index` of `compute`.
pub(crate) fn answer_frame(
    compute: &RoleDouble,
    index: usize,
    batch: &ResultBatch,
    last: bool,
) -> TestResult {
    let reads = compute.frames(Command::ComputeRead);
    let read = reads.get(index).context("the read")?;
    read.answer_with(batch_frame(CLUSTER, read.stream(), batch, last)?);
    Ok(())
}

/// Refuses the read at `index` of `compute` with `refusal`.
pub(crate) fn answer_with(compute: &RoleDouble, index: usize, refusal: &ErrorReply) -> TestResult {
    let reads = compute.frames(Command::ComputeRead);
    let read = reads.get(index).context("the read")?;
    read.answer_with(last_frame(CLUSTER, read.stream(), refusal)?);
    Ok(())
}

/// Answers the Write at `index` of `partition` with Held.
pub(crate) fn answer_held(partition: &RoleDouble, index: usize) -> TestResult {
    let writes = partition.frames(Command::Write);
    let write = writes.get(index).context("the Write")?;
    let header = Header::new(CLUSTER, Command::Held).with_stream(write.stream());
    write.answer_with(Held.frame(header)?);
    Ok(())
}

/// A Write declaring `predicate`.
pub(crate) fn declaring(predicate: DeclaredPredicate) -> Write {
    Write::new(
        TermTable::new(),
        vec![WriteChange::Declare(Declared::Predicate(predicate))],
    )
}

/// Commits `predicate`'s declaration, answered once its epoch is assigned.
pub(crate) async fn declare(client: &mut Client, predicate: DeclaredPredicate) -> TestResult {
    let transaction = client.begin(0).await?;
    client.on(transaction, &declaring(predicate)).await?;
    client.commit(transaction).await?;
    assert_eq!(client.got(transaction).await?.command, Command::Commit);
    Ok(())
}

/// A Prepare in `tenant` of a pattern binding the predicate `predicate` and taking its object as its parameter.
pub(crate) fn prepared_in(tenant: u64, predicate: &str) -> Prepare {
    let mut terms = TermTable::new();
    let pattern = QuadPattern::new(
        PatternSlot::Unbound,
        PatternSlot::Unbound,
        PatternSlot::Bound(terms.iri(predicate)),
        PatternSlot::Parameter(0),
    );
    Prepare::new(
        tenant,
        0,
        0,
        terms,
        pattern,
        vec![ColumnType::Term],
        Vec::new(),
    )
}

/// The term table a counter add over two names.
pub(crate) fn counter_terms() -> TermTable {
    let mut terms = TermTable::new();
    let _subject = terms.iri(OBJECT);
    let _predicate = terms.iri(PREDICATE);
    terms
}

/// Two tenants whose hashes share their top 8 bits.
pub(crate) fn pair_sharing_a_run() -> TestResult<Vec<u64>> {
    let of = |tenant: u64| Tenant::new(tenant).bucket_bits();
    (1..4_096_u64)
        .find_map(|one| {
            ((one + 1)..4_096)
                .find(|two| of(one) == of(*two))
                .map(|two| vec![one, two])
        })
        .context("two tenants sharing a run")
}

/// The stage's directory, for a scenario restarting it.
pub(crate) fn held_directory(stage: &Stage) -> TestResult<std::path::PathBuf> {
    stage.directory().context("a directory")
}

/// The `s=…,i=…` of the server-first-message answering `name`.
pub(crate) async fn challenge(client: &Address, name: &str) -> TestResult<String> {
    let carrier = Carrier::connect(client).await?;
    let mut raw = Connection::new(carrier, Framer::new(CLUSTER, MAX_FRAME_SIZE));
    let pool = BufferPool::new(1, MAX_FRAME_SIZE);
    let _welcome = raw
        .hello(&pool, &Hello::new(1, 1, 1, MAX_FRAME_SIZE as u32, "client"))
        .await?;
    let nonce = system_nonce()?;
    let mechanism = ScramClient::new(name, "pw", &nonce)?;
    let mut exchange = ClientAuthentication::new(mechanism);
    let first = exchange.start()?;
    let mut frame = first.frame(Header::new(CLUSTER, Command::Authenticate))?;
    Header::stamp_sequence(&mut frame, 2)?;
    let _sent = raw.write_frame(frame).await?;
    let answer = raw.read_frame(&pool).await?;
    let view = answer.view()?;
    let message = AuthenticateView::from_frame(&view)?.message();
    let text = String::from_utf8_lossy(message).to_string();
    let (_nonce, salted) = text.split_once(",s=").context("a salt")?;
    let _step = AuthenticationStep::Send;
    Ok(salted.to_string())
}

/// The default entry of `predicate`, recorded through the frontend's role address.
pub(crate) async fn record_default(stage: &Stage, predicate: &str) -> TestResult {
    let mut storage = RoleClient::dial(
        &stage.role,
        CLUSTER,
        "storage",
        stage.cluster.secret().password(),
    )
    .await?;
    let reply = storage
        .request(&DefaultEntry::request(0, predicate))
        .await?;
    drop(reply);
    Ok(())
}

/// Describe's property keys, relationship types, declarations and read version.
pub(crate) async fn described(
    client: &mut Client,
) -> TestResult<(Vec<String>, Vec<String>, usize, u64)> {
    let describe = client.send(&Describe::request(0)).await?;
    let got = client.got(describe).await?;
    let view = view_frame(&got.bytes)?;
    let answer = DescribeView::from_frame(&view)?;
    Ok((
        answer.property_keys().map(str::to_string).collect(),
        answer.relationship_types().map(str::to_string).collect(),
        answer.declarations(),
        answer.read_version(),
    ))
}

/// Describe, once the version declaring `predicate` is in effect at the latest close.
pub(crate) async fn described_after(
    stage: &Stage,
    client: &mut Client,
    predicate: &str,
) -> TestResult<(Vec<String>, Vec<String>, usize, u64)> {
    let _stage = stage;
    for _ in 0..500 {
        let held = described(client).await?;
        if held.0.iter().any(|key| key == predicate) {
            return Ok(held);
        }
        pause(2).await;
    }
    Err(TestError::new("the version never took effect"))
}

/// A partition's answer to a frontend's claim: the predicate's system ID, `id`.
pub(crate) fn answer_claims(frame: &DoubleFrame, id: SystemId) -> Answer {
    if frame.command() != Command::Reference {
        return partition_answer(frame, CLUSTER, BIND_VERSION);
    }
    let reply = view_frame(frame.bytes())
        .ok()
        .and_then(|view| ReferenceView::from_frame(&view).ok()?.to_body().ok())
        .map(|asked| {
            let mut reply = asked.into_reply();
            let _accepted = reply.accept(0, id, 0, 0);
            reply
        });
    reply
        .and_then(|reply| last_frame(CLUSTER, frame.stream(), &reply).ok())
        .map_or(Answer::Done, Answer::Reply)
}

/// A DirectWrite through `session` naming only a new blank node as its subject.
pub(crate) fn blank_write(session: [u8; 16], request: u64) -> TestResult<DirectWrite> {
    let mut terms = TermTable::new();
    let change = WriteChange::Insert {
        graph: None,
        subject: terms.intern(RequestTerm::BlankNode("b".to_string()))?,
        predicate: terms.iri(PREDICATE),
        object: terms.iri(OBJECT),
    };
    Ok(DirectWrite::new(0, terms, vec![change]).with_session(session, request))
}

#[when(
    "a client sends a read of its transaction, and a write on another stream before the read is answered"
)]
fn a_write_waits_for_a_read_sent_before_it(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let subject = stage.on(1)?;
        let partition = stage.partition(1).await?;
        let compute = stage.compute_with(|_| Answer::Open).await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        let read = client.read(transaction, &subject).await?;
        until("the read", || compute.count(Command::ComputeRead) == 1).await?;
        client.write(transaction, &subject).await?;
        pause(20).await;
        assert_eq!(partition.count(Command::Write), 0, "held behind the read");
        answer_frame(&compute, 0, &empty_batch(4, 1), true)?;
        assert_eq!(client.got(read).await?.command, Command::Result);
        until("the Write", || partition.count(Command::Write) == 1).await?;
        Ok(())
    })
}

#[then(
    "the frontend sends the write only after the read is answered, and the read returns none of it"
)]
fn a_write_waits_for_a_read_sent_before_it_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a client sends a write and then a read of its transaction on another stream, without waiting for the write's reply"
)]
fn a_read_waits_for_a_write_sent_before_it(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let partition = stage
            .partition_with(1, |frame| {
                if frame.command() == Command::Write {
                    return Answer::Open;
                }
                partition_answer(frame, CLUSTER, BIND_VERSION)
            })
            .await?;
        let subject = stage.on(1)?;
        let compute = stage.compute().await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        client.write(transaction, &subject).await?;
        let read = client.read(transaction, &subject).await?;
        until("the Write", || partition.count(Command::Write) == 1).await?;
        pause(20).await;
        assert_eq!(
            compute.count(Command::ComputeRead),
            0,
            "held behind the write"
        );
        answer_held(&partition, 0)?;
        assert_eq!(client.got(read).await?.command, Command::Result);
        Ok(())
    })
}

#[then(
    "the frontend sends the read to compute only after the write is answered, and the read returns the write"
)]
fn a_read_waits_for_a_write_sent_before_it_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a read of a transaction is open, a Write the client sent after it is held behind it, and the client sends Commit"
)]
fn a_commit_takes_the_writes_held_behind_a_read(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let subject = stage.on(1)?;
        let partition = stage.partition(1).await?;
        let compute = stage.compute_with(|_| Answer::Open).await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        let read = client.read(transaction, &subject).await?;
        until("the read", || compute.count(Command::ComputeRead) == 1).await?;
        client.write(transaction, &subject).await?;
        client.commit(transaction).await?;
        // Compute answers the read as the Commit arrives, as a process that has not yet read the Cancel does: a frame the
        // frontend took before the Commit may reach the client before the Commit's reply, and none may after it. None is the
        // read's last, which would end it before the Commit cancels it.
        for _ in 0..3 {
            answer_frame(&compute, 0, &empty_batch(4, 1), false)?;
        }
        assert_eq!(client.got(transaction).await?.command, Command::Commit);
        let _before = client.taken(read);
        until("the Cancel", || compute.count(Command::Cancel) == 1).await?;
        assert_eq!(
            partition.commands(&[]),
            [Command::Bind, Command::Write, Command::Commit]
        );
        client.silent(read, 50).await?;
        Ok(())
    })
}

#[then(
    "the frontend cancels the read, sends the Write on the Bind stream, then the Commit; the Write is in what commits, and no further frame of the read reaches the client"
)]
fn a_commit_takes_the_writes_held_behind_a_read_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a read of a transaction is open, a Write is held behind it, and the client sends Abort")]
fn an_abort_drops_what_a_read_held_back(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let subject = stage.on(1)?;
        let partition = stage.partition(1).await?;
        let compute = stage.compute_with(|_| Answer::Open).await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        let _read = client.read(transaction, &subject).await?;
        until("the read", || compute.count(Command::ComputeRead) == 1).await?;
        client.write(transaction, &subject).await?;
        client.abort(transaction).await?;
        until("the Cancel", || compute.count(Command::Cancel) == 1).await?;
        until("the Bind stream's end", || {
            partition.count(Command::Cancel) == 1
        })
        .await?;
        assert_eq!(
            partition.count(Command::Write),
            0,
            "the partition never receives it"
        );
        Ok(())
    })
}

#[then(
    "the frontend cancels the read, the partition never receives the Write, and nothing of the transaction is held"
)]
fn an_abort_drops_what_a_read_held_back_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a read of a transaction is open, a Write is held behind it, and the client cancels the read"
)]
fn a_read_the_client_cancels_releases_the_write_behind_it(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let subject = stage.on(1)?;
        let partition = stage.partition(1).await?;
        let compute = stage.compute_with(|_| Answer::Open).await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        let read = client.read(transaction, &subject).await?;
        until("the read", || compute.count(Command::ComputeRead) == 1).await?;
        client.write(transaction, &subject).await?;
        client.on(read, &Cancel).await?;
        until("the Cancel", || compute.count(Command::Cancel) == 1).await?;
        until("the Write", || partition.count(Command::Write) == 1).await?;
        Ok(())
    })
}

#[then(
    "the frontend sends the read's Cancel and then the Write, without waiting for the read's `LAST`"
)]
fn a_read_the_client_cancels_releases_the_write_behind_it_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a Write of a transaction is unanswered, a read behind it is held, and the client cancels the read"
)]
fn a_read_cancelled_while_it_is_held_is_dropped_unsent(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let partition = stage
            .partition_with(1, |frame| {
                if frame.command() == Command::Write {
                    return Answer::Open;
                }
                partition_answer(frame, CLUSTER, BIND_VERSION)
            })
            .await?;
        let subject = stage.on(1)?;
        let compute = stage.compute().await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        client.write(transaction, &subject).await?;
        let read = client.read(transaction, &subject).await?;
        until("the Write", || partition.count(Command::Write) == 1).await?;
        client.on(read, &Cancel).await?;
        pause(20).await;
        answer_held(&partition, 0)?;
        client.commit(transaction).await?;
        until("the Commit", || partition.count(Command::Commit) == 1).await?;
        assert_eq!(
            compute.count(Command::ComputeRead),
            0,
            "no ComputeRead was sent"
        );
        Ok(())
    })
}

#[then("no ComputeRead is sent for it, and the transaction goes on")]
fn a_read_cancelled_while_it_is_held_is_dropped_unsent_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a read of a transaction is under way, the transaction passes its limit, and compute's Error 1 of `detail` 1 reaches the frontend before the Bind stream's code 4"
)]
fn a_read_under_way_when_its_transaction_ended_takes_the_stream_s_error(
    w: &mut Steps,
) -> TestResult {
    let options = Options {
        limits: limited(100),
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let subject = stage.on(1)?;
        let _partition = stage.partition(1).await?;
        let compute = stage
            .compute_with(|_| {
                Answer::Refuse(ErrorReply::transaction_not_held(
                    "the partition holds no such transaction",
                ))
            })
            .await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        let read = client.read(transaction, &subject).await?;
        until("the read", || compute.count(Command::ComputeRead) == 1).await?;
        client.silent(read, 20).await?;
        assert_eq!(client.refused(read).await?, (ErrorCode::PastLimit, 100));
        Ok(())
    })
}

#[then(
    "the frontend holds that Error until the Bind stream ends, and the client's read is refused with code 4"
)]
fn a_read_under_way_when_its_transaction_ended_takes_the_stream_s_error_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a read of a transaction has reached its `LAST`, and the transaction then ends with an Error"
)]
fn a_read_already_answered_takes_nothing_further(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let subject = stage.on(1)?;
        let _partition = stage
            .partition_with(1, |frame| {
                if frame.command() == Command::Write {
                    return Answer::Refuse(ErrorReply::new(
                        ErrorCode::Refused,
                        "the write is refused",
                    ));
                }
                partition_answer(frame, CLUSTER, BIND_VERSION)
            })
            .await?;
        let _compute = stage.compute().await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        let read = client.read(transaction, &subject).await?;
        assert_eq!(client.got(read).await?.command, Command::Result);
        client.write(transaction, &subject).await?;
        assert_eq!(client.refused(transaction).await?.0, ErrorCode::Refused);
        client.silent(read, 20).await?;
        Ok(())
    })
}

#[then("the frontend sends nothing further on that read's stream")]
fn a_read_already_answered_takes_nothing_further_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a transaction at latest level sends two reads beside each other, the first binding it to partition 1"
)]
fn a_later_latest_state_read_waits_for_the_first_read_s_partition(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let compute = stage.compute_with(|_| Answer::Open).await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin_read_only(ReadLevel::Latest).await?;
        let first = client.read(transaction, OBJECT).await?;
        let _second = client.read(transaction, OBJECT).await?;
        until("the first read", || {
            compute.count(Command::ComputeRead) == 1
        })
        .await?;
        pause(20).await;
        assert_eq!(compute.count(Command::ComputeRead), 1, "the second waits");
        answer_frame(&compute, 0, &empty_batch(4, 1), false)?;
        assert_eq!(client.got(first).await?.command, Command::Result);
        until("the second read", || {
            compute.count(Command::ComputeRead) == 2
        })
        .await?;
        let reads = compute.frames(Command::ComputeRead);
        let second = reads.get(1).context("the second read")?;
        let view = view_frame(second.bytes())?;
        assert_eq!(ComputeReadView::from_frame(&view)?.partition(), 1);
        Ok(())
    })
}

#[then(
    "the frontend sends the second only after the first's first Result names partition 1, with `partition` 1, and compute refuses it with code 25 when it reaches another partition"
)]
fn a_later_latest_state_read_waits_for_the_first_read_s_partition_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a transaction at latest level sends two reads, and the first is refused before any Result")]
fn a_first_read_refused_leaves_the_next_read_first(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let compute = stage.compute_with(|_| Answer::Open).await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin_read_only(ReadLevel::Latest).await?;
        let first = client.read(transaction, OBJECT).await?;
        let _second = client.read(transaction, OBJECT).await?;
        until("the first read", || {
            compute.count(Command::ComputeRead) == 1
        })
        .await?;
        answer_with(
            &compute,
            0,
            &ErrorReply::new(ErrorCode::TimedOut, "a Scan stalled").with_detail(1),
        )?;
        assert_eq!(client.refused(first).await?.0, ErrorCode::TimedOut);
        until("the second read", || {
            compute.count(Command::ComputeRead) == 2
        })
        .await?;
        let reads = compute.frames(Command::ComputeRead);
        let second = reads.get(1).context("the second read")?;
        let view = view_frame(second.bytes())?;
        assert_eq!(
            ComputeReadView::from_frame(&view)?.partition(),
            0,
            "bound by no first read"
        );
        answer_frame(&compute, 1, &empty_batch(4, 1), false)?;
        let _third = client.read(transaction, OBJECT).await?;
        until("the third read", || {
            compute.count(Command::ComputeRead) == 3
        })
        .await?;
        let reads_2 = compute.frames(Command::ComputeRead);
        let third = reads_2.get(2).context("the third read")?;
        let view_2 = view_frame(third.bytes())?;
        assert_eq!(
            ComputeReadView::from_frame(&view_2)?.partition(),
            1,
            "the second bound it"
        );
        Ok(())
    })
}

#[then("the second goes out as the transaction's first read and binds it")]
fn a_first_read_refused_leaves_the_next_read_first_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a statement binding a predicate and taking its object as a parameter is prepared, and executed in a snapshot transaction"
)]
fn an_executed_statement_reads_through_compute(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let compute = stage.compute_with(answer_empty).await?;
        let mut client = stage.ann().await?;
        let prepare = client.send(&prepared_in(0, "http://ex.org/p")).await?;
        let got = client.got(prepare).await?;
        let view = view_frame(&got.bytes)?;
        let statement = PrepareView::from_frame(&view)?.statement();
        let transaction = client.begin_read_only(ReadLevel::Snapshot).await?;
        let mut terms = TermTable::new();
        let object = "http://ex.org/o";
        let parameter = terms.iri(object);
        let execute = client
            .send(&Execute::new(
                statement,
                transaction,
                0,
                terms,
                vec![parameter],
                Vec::new(),
            ))
            .await?;
        assert_eq!(client.got(execute).await?.command, Command::Result);
        let sent = compute.frames(Command::ComputeRead);
        let [read] = sent.as_slice() else {
            return Err(TestError::new(format!("{} ComputeReads", sent.len())));
        };
        let frame = view_frame(read.bytes())?;
        let asked = ComputeReadView::from_frame(&frame)?;
        let bound = match asked.pattern().object() {
            PatternSlot::Bound(index) => asked.terms().get(index),
            PatternSlot::Unbound | PatternSlot::Parameter(_) | PatternSlot::DefaultGraph => None,
        };
        let bound_object = matches!(bound, Some(RequestTermView::Iri(iri)) if iri == object);
        assert!(bound_object, "the parameter is bound: {bound:?}");
        Ok(())
    })
}

#[then(
    "compute receives one ComputeRead of the statement's pattern with the parameter bound, and the client receives its batch"
)]
fn an_executed_statement_reads_through_compute_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "an Execute's parameters, bound into the pattern, make a ComputeRead larger than a frame within compute's Welcome"
)]
fn an_execute_too_large_for_compute_s_frame_is_refused(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let compute = stage.compute().await?;
        let mut client = stage.ann().await?;
        let long = format!("http://ex.org/{}", "a".repeat(600_000));
        let prepare = client.send(&prepared_in(0, &long)).await?;
        let got = client.got(prepare).await?;
        let view = view_frame(&got.bytes)?;
        let statement = PrepareView::from_frame(&view)?.statement();
        let transaction = client.begin_read_only(ReadLevel::Snapshot).await?;
        let mut terms = TermTable::new();
        let parameter = terms.intern(RequestTerm::Literal {
            lexical: "b".repeat(600_000),
            datatype: None,
        })?;
        let execute = client
            .send(&Execute::new(
                statement,
                transaction,
                0,
                terms,
                vec![parameter],
                Vec::new(),
            ))
            .await?;
        assert_eq!(
            client.refused(execute).await?,
            (ErrorCode::TooLarge, MAX_FRAME_SIZE as u64)
        );
        assert_eq!(
            compute.count(Command::ComputeRead),
            0,
            "no compute process receives it"
        );
        Ok(())
    })
}

#[then(
    "the frontend refuses it with code 6, `detail` that `max_frame`, and no compute process receives it"
)]
fn an_execute_too_large_for_compute_s_frame_is_refused_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "principal `alice` registers a session, and a connection authenticated as `bob` commits through it in a partition where `alice` has not yet committed"
)]
fn a_session_belongs_to_its_principal(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let partition = stage.partition(1).await?;
        let subject = stage.on(1)?;
        let mut ann = stage.ann().await?;
        let session = ann.register().await?;
        let mut bob = stage.as_principal("bob").await?;
        let taken = bob.begin(0).await?;
        bob.write(taken, &subject).await?;
        bob.on(taken, &Commit::new(session, 1, 0)).await?;
        assert_eq!(
            bob.refused(taken).await?,
            (ErrorCode::PermissionDenied, Action::Write.number())
        );
        assert_eq!(partition.count(Command::Commit), 0, "nothing is committed");
        let served = ann.begin(0).await?;
        ann.write(served, &subject).await?;
        ann.on(served, &Commit::new(session, 1, 0)).await?;
        assert_eq!(ann.got(served).await?.command, Command::Commit);
        Ok(())
    })
}

#[then(
    "the commit is refused with code 21, `detail` 2, nothing is committed, and `alice`'s next request through the session is served"
)]
fn a_session_belongs_to_its_principal_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a Commit through a session waits behind a Write its partition refuses, and the client then commits through that session again"
)]
fn a_transaction_ending_before_its_commit_lets_the_session_go(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _partition = stage
            .partition_with(1, |frame| {
                if frame.command() == Command::Write {
                    return Answer::Refuse(ErrorReply::new(ErrorCode::Refused, "a refused Write"));
                }
                partition_answer(frame, CLUSTER, BIND_VERSION)
            })
            .await?;
        let mut client = stage.ann().await?;
        let session = client.register().await?;
        let ended = client.begin(0).await?;
        client.on(ended, &write_of(&stage.on(1)?)).await?;
        client.on(ended, &Commit::new(session, 1, 0)).await?;
        assert_eq!(client.got(ended).await?.code, Some(ErrorCode::Refused));
        let again = client.begin(0).await?;
        client.on(again, &Commit::new(session, 1, 0)).await?;
        let got = client.got(again).await?;
        assert_eq!(
            got.command,
            Command::Commit,
            "{:?} {}",
            got.code,
            got.message
        );
        Ok(())
    })
}

#[then("the second Commit is answered, not refused as busy")]
fn a_transaction_ending_before_its_commit_lets_the_session_go_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a session is evicted from the full registry and its client sends its next request")]
fn an_evicted_session_is_told_so(w: &mut Steps) -> TestResult {
    let options = Options {
        sessions: 1,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let partition = stage.partition(1).await?;
        let mut client = stage.ann().await?;
        let session = client.register().await?;
        let _next = client.register().await?;
        let transaction = client.begin(0).await?;
        client.write(transaction, &stage.on(1)?).await?;
        client.on(transaction, &Commit::new(session, 1, 0)).await?;
        let got = client.got(transaction).await?;
        assert_eq!(got.code, Some(ErrorCode::SessionEvicted));
        assert!(got.message.contains("evicted"), "{}", got.message);
        assert_eq!(partition.count(Command::Commit), 0, "no partition sees it");
        Ok(())
    })
}

#[then(
    "the frontend refuses the request with an error saying the session was evicted, and no partition sees it"
)]
fn an_evicted_session_is_told_so_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a client sends request 8 of its session again, with no transaction open for it, and the session's last committed request is 7"
)]
fn a_retry_of_a_request_never_committed_is_told_so(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let partition = stage.partition(1).await?;
        let mut client = stage.ann().await?;
        let session = client.register().await?;
        let retried = client.send(&Commit::new(session, 8, 0)).await?;
        assert_eq!(client.refused(retried).await?, (ErrorCode::NotCommitted, 8));
        assert_eq!(partition.count(Command::Commit), 0, "nothing is committed");
        Ok(())
    })
}

#[then("the request is refused as not committed, `detail` 8, and nothing is committed")]
fn a_retry_of_a_request_never_committed_is_told_so_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a retried Commit is recalled over two partitions, partition 1 answers 18 and the role connection to partition 2 closes under the Recall"
)]
fn a_recall_reaching_no_answer_stays_in_doubt(w: &mut Steps) -> TestResult {
    let options = Options {
        partitions: 2,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let _one = stage.partition(1).await?;
        let two = stage.partition_with(2, |_| Answer::Open).await?;
        let mut client = stage.ann().await?;
        let session = client.register().await?;
        let retried = client.send(&Commit::new(session, 1, 0)).await?;
        until("the Recall", || two.count(Command::Recall) == 1).await?;
        two.close();
        assert_eq!(client.refused(retried).await?.0, ErrorCode::Unavailable);
        Ok(())
    })
}

#[then("the frontend answers code 17, and the client resends after a delay")]
fn a_recall_reaching_no_answer_stays_in_doubt_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a retried Commit is recalled over two partitions, partition 1 answers 18 and partition 2's address refuses the Recall with Error 1, serving another partition"
)]
fn any_other_refusal_of_a_recall_stays_in_doubt(w: &mut Steps) -> TestResult {
    let options = Options {
        partitions: 2,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let _one = stage.partition(1).await?;
        let _two = stage
            .partition_with(2, |_| {
                Answer::Refuse(ErrorReply::new(
                    ErrorCode::Protocol,
                    "this address hosts another",
                ))
            })
            .await?;
        let mut client = stage.ann().await?;
        let session = client.register().await?;
        let retried = client.send(&Commit::new(session, 1, 0)).await?;
        assert_eq!(client.refused(retried).await?.0, ErrorCode::Unavailable);
        Ok(())
    })
}

#[when(
    "a retried Commit is recalled over three partitions, one answers the stored reply, one 14, and one's connection closes"
)]
fn a_stored_reply_outranks_every_other_recall_answer(w: &mut Steps) -> TestResult {
    let options = Options {
        partitions: 3,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let stored = stage
            .partition_with(1, |frame| {
                let commit = Commit::new([3; 16], 1, BIND_VERSION);
                last_frame(CLUSTER, frame.stream(), &commit).map_or(Answer::Done, Answer::Reply)
            })
            .await?;
        let _busy = stage
            .partition_with(2, |_| {
                Answer::Refuse(ErrorReply::new(
                    ErrorCode::SessionBusy,
                    "in its commit step",
                ))
            })
            .await?;
        let closing = stage.partition_with(3, |_| Answer::Open).await?;
        let mut client = stage.ann().await?;
        let session = client.register().await?;
        let retried = client.send(&Commit::new(session, 1, 0)).await?;
        until("the Recall", || closing.count(Command::Recall) == 1).await?;
        closing.close();
        assert_eq!(client.got(retried).await?.command, Command::Commit);
        assert_eq!(stored.count(Command::Recall), 1);
        Ok(())
    })
}

#[then("the client receives the stored reply")]
fn a_stored_reply_outranks_every_other_recall_answer_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a retried Commit is recalled, every partition answers 18, and the registry has evicted the session meanwhile"
)]
fn a_retry_of_a_session_evicted_meanwhile_is_unknown(w: &mut Steps) -> TestResult {
    let options = Options {
        sessions: 1,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let partition = stage.partition_with(1, |_| Answer::Open).await?;
        let mut client = stage.ann().await?;
        let session = client.register().await?;
        let retried = client.send(&Commit::new(session, 1, 0)).await?;
        until("the Recall", || partition.count(Command::Recall) == 1).await?;
        let _next = client.register().await?;
        let held = partition.frames(Command::Recall);
        let held = held.first().context("the Recall")?;
        let answer = ErrorReply::new(ErrorCode::NotCommitted, "not held").with_detail(1);
        held.answer_with(last_frame(CLUSTER, held.stream(), &answer)?);
        assert_eq!(client.refused(retried).await?.0, ErrorCode::SessionEvicted);
        Ok(())
    })
}

#[then("the frontend answers code 12, and the client reports the outcome as unknown")]
fn a_retry_of_a_session_evicted_meanwhile_is_unknown_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a DirectWrite through a session is answered code 17, the partition had not committed it, and the client sends it again"
)]
fn a_one_frame_write_s_resend_is_decided_by_the_table(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let answered = Rc::new(Cell::new(0));
        let count = Rc::clone(&answered);
        let partition = stage
            .partition_with(1, move |frame| {
                count.set(count.get() + 1);
                if count.get() == 1 {
                    return Answer::Refuse(ErrorReply::new(
                        ErrorCode::Unavailable,
                        "the write is in doubt",
                    ));
                }
                partition_answer(frame, CLUSTER, BIND_VERSION)
            })
            .await?;
        let mut client = stage.ann().await?;
        let session = client.register().await?;
        let mut terms = TermTable::new();
        let change = insert_into(&mut terms, &stage.on(1)?);
        let write = DirectWrite::new(0, terms, vec![change]).with_session(session, 1);
        let first = client.send(&write).await?;
        assert_eq!(client.refused(first).await?.0, ErrorCode::Unavailable);
        let resent = client.send(&write).await?;
        let got = client.got(resent).await?;
        let view = view_frame(&got.bytes)?;
        assert_eq!(
            CommitView::from_frame(&view)?.commit_version(),
            BIND_VERSION
        );
        assert_eq!(partition.count(Command::DirectWrite), 2, "relayed");
        assert_eq!(partition.count(Command::Recall), 0, "not recalled");
        Ok(())
    })
}

#[then(
    "the frontend relays it, not recalls it, the partition commits it, and the reply carries its commit version"
)]
fn a_one_frame_write_s_resend_is_decided_by_the_table_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "the sequencer's open epoch is 30, and a Begin, a DirectWrite and a ConditionalWrite each carry a token of epoch 31"
)]
fn a_token_above_the_open_epoch_is_refused(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let facts = stage
            .owner
            .tenant_facts(0)
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        let above = facts.open_epoch() + 1_000_000;
        let token = SessionToken::new(1, 1, Epoch::new(above)?);
        let named = above.to_string();
        let mut client = stage.ann().await?;
        let begin = client
            .send(&Begin::new(0, 0, false).with_token(token.clone()))
            .await?;
        let mut terms = TermTable::new();
        let change = insert_into(&mut terms, OBJECT);
        let direct = client
            .send(
                &DirectWrite::new(0, terms.clone(), vec![change.clone()]).with_token(token.clone()),
            )
            .await?;
        let operation = CommitOperation::CounterAdd {
            graph: None,
            subject: 0,
            predicate: 1,
            delta: 1,
        };
        let conditional = client
            .send(&ConditionalWrite::new(0, terms, vec![change], vec![operation]).with_token(token))
            .await?;
        for stream in [begin, direct, conditional] {
            let got = client.got(stream).await?;
            assert_eq!(got.code, Some(ErrorCode::Protocol));
            assert!(got.message.contains(&named), "{}", got.message);
        }
        Ok(())
    })
}

#[then("each is refused with Error 1 on its stream, the message naming epoch 31")]
fn a_token_above_the_open_epoch_is_refused_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a DirectWrite writes subjects of partitions 1 and 2")]
fn a_write_reaching_two_partitions_is_refused(w: &mut Steps) -> TestResult {
    let options = Options {
        partitions: 2,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let (one, two) = (stage.partition(1).await?, stage.partition(2).await?);
        let mut terms = TermTable::new();
        let changes = vec![
            insert_into(&mut terms, &stage.on(1)?),
            insert_into(&mut terms, &stage.on(2)?),
        ];
        let mut client = stage.ann().await?;
        let write = client.send(&DirectWrite::new(0, terms, changes)).await?;
        assert_eq!(
            client.refused(write).await?,
            (ErrorCode::CrossPartition, 1 | (2 << 32))
        );
        assert_eq!(
            one.count(Command::DirectWrite) + two.count(Command::DirectWrite),
            0
        );
        Ok(())
    })
}

#[then("the frontend refuses it with code 23 before any relay, and neither is held")]
fn a_write_reaching_two_partitions_is_refused_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a DirectWrite holds a declaration of a predicate")]
fn a_declaration_in_a_write_is_refused(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let before = stage
            .owner
            .schema_id(0)
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        let mut client = stage.ann().await?;
        let declared = Declared::Predicate(DeclaredPredicate::new(PREDICATE));
        let write = client
            .send(&DirectWrite::new(
                0,
                TermTable::new(),
                vec![WriteChange::Declare(declared)],
            ))
            .await?;
        assert_eq!(client.refused(write).await?.0, ErrorCode::Protocol);
        let after = stage
            .owner
            .schema_id(0)
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        assert_eq!(before, after, "the catalog is unchanged");
        Ok(())
    })
}

#[then("it is refused with a protocol error, and the catalog is unchanged")]
fn a_declaration_in_a_write_is_refused_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a ConditionalWrite's changes name only new blank nodes as subjects, its operation is an insert-if-absent of `epi.id` `\"x\"`, and partition 2 owns the bucket of `\"x\"`"
)]
fn a_conditionalwrite_claiming_a_value_goes_to_its_claim(w: &mut Steps) -> TestResult {
    let options = Options {
        partitions: 2,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let predicate = "http://ex.org/epi.id";
        let id = SystemId::new(TypePrefix::RESOURCE, Bucket::new(1)?, Sequence::new(7)?);
        let (one, two) = (
            stage
                .partition_with(1, move |frame| answer_claims(frame, id))
                .await?,
            stage
                .partition_with(2, move |frame| answer_claims(frame, id))
                .await?,
        );
        let value = (0..1 << 16)
            .map(|at| format!("x{at}"))
            .find(|value| {
                let literal = Literal::new_simple_literal(value.as_str());
                let encoded = EncodedTerm::literal::<TermError>(literal.as_ref(), |_| {
                    Err(TermError::malformed(Vec::new(), "no ID"))
                });
                encoded.is_ok_and(|encoded| {
                    let bucket = Bucket::of_value(Tenant::new(0), id, encoded.term().as_bytes());
                    stage
                        .map
                        .partition_of(bucket, stage.map.latest_change())
                        .get()
                        == 2
                })
            })
            .context("a value partition 2 owns")?;
        let mut terms = TermTable::new();
        let operation = CommitOperation::InsertIfAbsent {
            graph: None,
            predicate: terms.iri(predicate),
            value: terms.intern(RequestTerm::Literal {
                lexical: value,
                datatype: None,
            })?,
            properties: Vec::new(),
        };
        let mut client = stage.ann().await?;
        let write = client
            .send(&ConditionalWrite::new(
                0,
                terms,
                Vec::new(),
                vec![operation],
            ))
            .await?;
        let got = client.got(write).await?;
        assert_eq!(got.command, Command::Commit, "{}", got.message);
        assert_eq!(
            two.count(Command::ConditionalWrite),
            1,
            "relayed to its claim's partition"
        );
        assert_eq!(one.count(Command::ConditionalWrite), 0);
        Ok(())
    })
}

#[then("the frontend relays it to partition 2, and its reply names the node")]
fn a_conditionalwrite_claiming_a_value_goes_to_its_claim_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a DirectWrite through a session names only new blank nodes as subjects, its reply is lost, and the client sends it again"
)]
fn a_resent_blank_node_write_reaches_one_session_table(w: &mut Steps) -> TestResult {
    let options = Options {
        partitions: 2,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let (one, two) = (stage.partition(1).await?, stage.partition(2).await?);
        let mut client = stage.ann().await?;
        let session = client.register().await?;
        let write = blank_write(session, 1)?;
        let first = client.send(&write).await?;
        assert_eq!(client.got(first).await?.command, Command::Commit);
        let resent = client.send(&write).await?;
        assert_eq!(client.got(resent).await?.command, Command::Commit);
        let mut placed = session.to_vec();
        placed.extend_from_slice(&1_u64.to_le_bytes());
        let at = xxh3_64(&placed) % 2;
        let (hit, missed) = if at == 0 { (&one, &two) } else { (&two, &one) };
        assert_eq!(hit.count(Command::DirectWrite), 2, "both at position {at}");
        assert_eq!(missed.count(Command::DirectWrite), 0);
        Ok(())
    })
}

#[then(
    "both go to the partition at the position XXH3-64 of its `session` and `request` bytes gives among the partitions holding the tenant's run, and the second is answered the stored reply"
)]
fn a_resent_blank_node_write_reaches_one_session_table_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a read-write transaction's first request is a Read binding its subject to an IRI under tenant 7's skolem base in tenant 0, and its next request is a Write of `<a>`"
)]
fn a_skolem_iri_nobody_can_hold_binds_nothing(w: &mut Steps) -> TestResult {
    let options = Options {
        partitions: 2,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let (one, two) = (stage.partition(1).await?, stage.partition(2).await?);
        let compute = stage.compute().await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        let foreign = format!(
            "{SYSTEM_VOCABULARY}tenant/7/.well-known/genid/000000-00000000000000000000000000"
        );
        let read = client.read(transaction, &foreign).await?;
        let got = client.got(read).await?;
        let view = view_frame(&got.bytes)?;
        assert_eq!(ResultView::from_frame(&view)?.rows(), 0);
        assert!(view.header().flags() & FLAG_LAST != 0, "flagged LAST");
        assert_eq!(compute.count(Command::ComputeRead), 0, "no role sees it");
        assert_eq!(one.count(Command::Bind) + two.count(Command::Bind), 0);
        client.write(transaction, &stage.on(2)?).await?;
        until("the Write's Bind", || two.count(Command::Bind) == 1).await?;
        assert_eq!(one.count(Command::Bind), 0, "the Write is its first");
        Ok(())
    })
}

#[then(
    "the Read is answered one batch flagged `LAST`, `rows` 0, no role sees it, and the transaction binds to `<a>`'s partition"
)]
fn a_skolem_iri_nobody_can_hold_binds_nothing_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a Write names as a subject an IRI under the tenant's skolem base whose tail states no bucket, and another under tenant 7's"
)]
fn a_skolem_iri_nobody_can_hold_is_refused_in_a_change(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let partition = stage.partition(1).await?;
        let mut client = stage.ann().await?;
        let no_bucket = format!("{SYSTEM_VOCABULARY}tenant/0/.well-known/genid/nothing");
        let foreign = format!(
            "{SYSTEM_VOCABULARY}tenant/7/.well-known/genid/000000-00000000000000000000000000"
        );
        for subject in [no_bucket, foreign] {
            let transaction = client.begin(0).await?;
            client.write(transaction, &subject).await?;
            assert_eq!(client.refused(transaction).await?, (ErrorCode::Refused, 0));
        }
        assert!(partition.commands(&[]).is_empty(), "no partition is asked");
        Ok(())
    })
}

#[then("each is refused with Error 7, `detail` the term's index, and no partition is asked")]
fn a_skolem_iri_nobody_can_hold_is_refused_in_a_change_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a Write names `https://example.org/tenant/0/.well-known/genid/00abcd-0123456789abcdef0123456789`"
)]
fn the_same_path_under_another_base_is_an_ordinary_iri(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let partition = stage.partition(1).await?;
        let iri =
            "https://example.org/tenant/0/.well-known/genid/00abcd-0123456789abcdef0123456789";
        assert!(
            matches!(iri_place(0, iri), IriPlace::Ordinary(_)),
            "routed by its text's hash"
        );
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        client.write(transaction, iri).await?;
        until("the Write", || partition.count(Command::Write) == 1).await?;
        Ok(())
    })
}

#[then("it is routed by its text's hash and stored as any IRI")]
fn the_same_path_under_another_base_is_an_ordinary_iri_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a Read's subject slot holds a literal, and another's graph slot a blank node's label")]
fn a_literal_at_a_pattern_s_subject_is_refused(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let _compute = stage.compute().await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin_read_only(ReadLevel::Snapshot).await?;
        let mut terms = TermTable::new();
        let literal = terms.intern(RequestTerm::Literal {
            lexical: "x".to_string(),
            datatype: None,
        })?;
        let pattern = QuadPattern::new(
            PatternSlot::Unbound,
            PatternSlot::Bound(literal),
            PatternSlot::Unbound,
            PatternSlot::Unbound,
        );
        let read = client
            .send(&Read::new(transaction, 0, terms, pattern, Vec::new()))
            .await?;
        let got = client.got(read).await?;
        assert_eq!(
            (got.code, got.detail),
            (Some(ErrorCode::Refused), u64::from(literal))
        );
        assert!(got.message.contains("subject"), "{}", got.message);
        let mut terms_2 = TermTable::new();
        let label = terms_2.intern(RequestTerm::BlankNode("b".to_string()))?;
        let pattern_2 = QuadPattern::new(
            PatternSlot::Bound(label),
            PatternSlot::Unbound,
            PatternSlot::Unbound,
            PatternSlot::Unbound,
        );
        let read_2 = client
            .send(&Read::new(transaction, 0, terms_2, pattern_2, Vec::new()))
            .await?;
        let got_2 = client.got(read_2).await?;
        assert_eq!(
            (got_2.code, got_2.detail),
            (Some(ErrorCode::Refused), u64::from(label))
        );
        assert!(got_2.message.contains("graph"), "{}", got_2.message);
        Ok(())
    })
}

#[then("each is refused with Error 7, `detail` the term's index, the message naming the rule")]
fn a_literal_at_a_pattern_s_subject_is_refused_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a Read's object slot holds a blank node's label, and another's a triple term")]
fn a_label_or_a_triple_term_at_a_pattern_s_object_is_refused(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let _compute = stage.compute().await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin_read_only(ReadLevel::Snapshot).await?;
        let object = |terms: &mut TermTable, term: RequestTerm| -> TestResult<(u32, QuadPattern)> {
            let index = terms.intern(term)?;
            Ok((
                index,
                QuadPattern::new(
                    PatternSlot::Unbound,
                    PatternSlot::Unbound,
                    PatternSlot::Unbound,
                    PatternSlot::Bound(index),
                ),
            ))
        };
        let mut terms = TermTable::new();
        let (label, pattern) = object(&mut terms, RequestTerm::BlankNode("b".to_string()))?;
        let read = client
            .send(&Read::new(transaction, 0, terms, pattern, Vec::new()))
            .await?;
        assert_eq!(
            client.refused(read).await?,
            (ErrorCode::Refused, u64::from(label))
        );
        let mut terms_2 = TermTable::new();
        let parts = (
            terms_2.iri("http://ex.org/a"),
            terms_2.iri(PREDICATE),
            terms_2.iri(OBJECT),
        );
        let (triple, pattern_2) = object(
            &mut terms_2,
            RequestTerm::Triple {
                subject: parts.0,
                predicate: parts.1,
                object: parts.2,
            },
        )?;
        let read_2 = client
            .send(&Read::new(transaction, 0, terms_2, pattern_2, Vec::new()))
            .await?;
        assert_eq!(
            client.refused(read_2).await?,
            (ErrorCode::Refused, u64::from(triple))
        );
        Ok(())
    })
}

#[when("a Write's object is a triple term")]
fn a_triple_term_in_a_change_is_refused(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let partition = stage.partition(1).await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        let mut terms = TermTable::new();
        let subject = terms.iri("http://ex.org/a");
        let predicate = terms.iri(PREDICATE);
        let inner = terms.iri(OBJECT);
        let object = terms.intern(RequestTerm::Triple {
            subject,
            predicate,
            object: inner,
        })?;
        let change = WriteChange::Insert {
            graph: None,
            subject,
            predicate,
            object,
        };
        client
            .on(transaction, &Write::new(terms, vec![change]))
            .await?;
        assert_eq!(
            client.refused(transaction).await?,
            (ErrorCode::Refused, u64::from(object))
        );
        assert!(
            partition.commands(&[]).is_empty(),
            "the transaction ended here"
        );
        Ok(())
    })
}

#[then("the Write is refused with Error 7, `detail` the term's index, and the transaction ends")]
fn a_triple_term_in_a_change_is_refused_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a Read names as its subject a skolem IRI no dictionary holds")]
fn a_forged_skolem_iri_in_a_pattern_matches_nothing(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let _partition = stage.partition(1).await?;
        let compute = stage.compute().await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin_read_only(ReadLevel::Snapshot).await?;
        let run = u32::from(Tenant::new(0).bucket_bits()) << 16;
        let forged = format!(
            "{SYSTEM_VOCABULARY}tenant/0/.well-known/genid/{run:06x}-00000000000000000000000000"
        );
        let read = client.read(transaction, &forged).await?;
        let got = client.got(read).await?;
        let view = view_frame(&got.bytes)?;
        assert_eq!(ResultView::from_frame(&view)?.rows(), 0);
        assert!(
            view.header().flags() & FLAG_LAST != 0,
            "one batch, flagged LAST"
        );
        assert_eq!(
            compute.count(Command::ComputeRead),
            1,
            "the partition owning its bucket says"
        );
        let _next = client.read(transaction, OBJECT).await?;
        until("the next read", || compute.count(Command::ComputeRead) == 2).await?;
        Ok(())
    })
}

#[then("the read answers one batch flagged `LAST` with no row, and its transaction goes on")]
fn a_forged_skolem_iri_in_a_pattern_matches_nothing_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a statement is prepared, the catalog of its tenant changes, and the statement is executed")]
fn a_stale_statement_is_refused(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let _compute = stage.compute().await?;
        let mut client = stage.ann().await?;
        let prepare = client.send(&prepared_in(0, OBJECT)).await?;
        let got = client.got(prepare).await?;
        let view = view_frame(&got.bytes)?;
        let statement = PrepareView::from_frame(&view)?.statement();
        declare(&mut client, DeclaredPredicate::new(PREDICATE)).await?;
        let transaction = client.begin_read_only(ReadLevel::Snapshot).await?;
        let mut terms = TermTable::new();
        let parameter = terms.iri(OBJECT);
        let execute = client
            .send(&Execute::new(
                statement,
                transaction,
                0,
                terms,
                vec![parameter],
                Vec::new(),
            ))
            .await?;
        assert_eq!(
            client.refused(execute).await?,
            (ErrorCode::StaleStatement, statement)
        );
        let again = client.send(&prepared_in(0, OBJECT)).await?;
        assert_eq!(
            client.got(again).await?.command,
            Command::Prepare,
            "preparing it again succeeds"
        );
        Ok(())
    })
}

#[then("the execution is refused as stale, and preparing it again succeeds")]
fn a_stale_statement_is_refused_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "`name` is declared a property with a `term` index, `score` holds a default entry, and a client sends Describe for the tenant"
)]
fn describe_answers_from_the_metadata_group(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let partition = stage.partition(1).await?;
        let (name, score) = ("http://ex.org/name", "http://ex.org/score");
        let mut client = stage.ann().await?;
        declare(
            &mut client,
            DeclaredPredicate::new(name)
                .with_kind(DeclaredKind::Property)
                .with_tokenizers(vec!["term".to_string()]),
        )
        .await?;
        record_default(&stage, score).await?;
        let (keys, types, declarations, read_version) =
            described_after(&stage, &mut client, name).await?;
        assert!(
            keys.iter().any(|key| key == name) && keys.iter().any(|key| key == score),
            "{keys:?}"
        );
        assert!(types.iter().any(|kind| kind == score), "{types:?}");
        assert_eq!(declarations, 1, "the declarations list name");
        let closed = stage
            .owner
            .tenant_facts(0)
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?
            .last_closed();
        assert!(
            read_version <= Version::bound_of(Epoch::new(closed)?),
            "the bound of a closed epoch"
        );
        assert!(
            partition.commands(&[]).is_empty(),
            "no other role was asked"
        );
        Ok(())
    })
}

#[then(
    "the frontend answers asking no other role: `read_version` is the version bound of the latest closed epoch, the declarations list `name` with its `term` index, the property keys hold `name` and `score`, and the relationship types hold `score`"
)]
fn describe_answers_from_the_metadata_group_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a statement's subject parameter is bound to a literal")]
fn a_literal_at_a_node_slot_is_refused(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let _compute = stage.compute().await?;
        let mut client = stage.ann().await?;
        let parameter = QuadPattern::new(
            PatternSlot::Unbound,
            PatternSlot::Parameter(0),
            PatternSlot::Unbound,
            PatternSlot::Unbound,
        );
        let body = Prepare::new(
            0,
            0,
            0,
            TermTable::new(),
            parameter,
            vec![ColumnType::Node],
            Vec::new(),
        );
        let prepare = client.send(&body).await?;
        let got = client.got(prepare).await?;
        let view = view_frame(&got.bytes)?;
        let statement = PrepareView::from_frame(&view)?.statement();
        let transaction = client.begin_read_only(ReadLevel::Snapshot).await?;
        let mut terms = TermTable::new();
        let literal = terms.intern(RequestTerm::Literal {
            lexical: "x".to_string(),
            datatype: None,
        })?;
        let execute = client
            .send(&Execute::new(
                statement,
                transaction,
                0,
                terms,
                vec![literal],
                Vec::new(),
            ))
            .await?;
        assert_eq!(
            client.refused(execute).await?,
            (ErrorCode::Parameters, statement)
        );
        Ok(())
    })
}

#[then("the execution is refused with code 11, `detail` the statement")]
fn a_literal_at_a_node_slot_is_refused_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a tenant's Describe answer does not fit one frame within the client's `max_frame`")]
fn a_describe_past_the_client_s_frame_is_refused(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        for at in 0..140 {
            let long = format!("http://ex.org/{at}/{}", "a".repeat(3_900));
            record_default(&stage, &long).await?;
        }
        let inner =
            RoleClient::dial_taking(&stage.client, CLUSTER, "ann", "pw", MAX_FRAME_FLOOR as u32)
                .await?;
        let mut client = Client::over(inner);
        let describe = client.send(&Describe::request(0)).await?;
        assert_eq!(
            client.refused(describe).await?,
            (ErrorCode::TooLarge, MAX_FRAME_FLOOR as u64)
        );
        Ok(())
    })
}

#[then("it is refused with code 6, `detail` that `max_frame`")]
fn a_describe_past_the_client_s_frame_is_refused_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a connection prepares a statement in tenant 1 and executes it in a transaction of tenant 0"
)]
fn a_statement_prepared_in_another_tenant_is_refused(w: &mut Steps) -> TestResult {
    let options = Options {
        tenants: vec![1],
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let _compute = stage.compute().await?;
        let mut client = stage.ann().await?;
        let prepare = client.send(&prepared_in(1, OBJECT)).await?;
        let got = client.got(prepare).await?;
        let view = view_frame(&got.bytes)?;
        let statement = PrepareView::from_frame(&view)?.statement();
        let transaction = client.begin_read_only(ReadLevel::Snapshot).await?;
        let mut terms = TermTable::new();
        let parameter = terms.iri(OBJECT);
        let execute = client
            .send(&Execute::new(
                statement,
                transaction,
                0,
                terms,
                vec![parameter],
                Vec::new(),
            ))
            .await?;
        assert_eq!(
            client.refused(execute).await?,
            (ErrorCode::UnknownStatement, statement)
        );
        let _next = client.read(transaction, OBJECT).await?;
        Ok(())
    })
}

#[then(
    "the execution is refused with code 10, `detail` the statement, and the transaction goes on"
)]
fn a_statement_prepared_in_another_tenant_is_refused_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a connection begins one transaction in tenant 0 and another in tenant 1, and each writes and reads `<a> <p> <b>`"
)]
fn one_connection_s_transactions_act_in_two_tenants(w: &mut Steps) -> TestResult {
    let options = Options {
        tenants: vec![1],
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let partition = stage.partition(1).await?;
        let compute = stage.compute().await?;
        let mut client = stage.ann().await?;
        let (subject, mut tenants) = ("http://ex.org/a", Vec::new());
        for tenant in [0, 1] {
            let transaction = client.begin(tenant).await?;
            client.write(transaction, subject).await?;
            let read = client.read(transaction, subject).await?;
            assert_eq!(client.got(read).await?.command, Command::Result);
            tenants.push(tenant);
        }
        let binds = partition.frames(Command::Bind);
        let bound: Vec<u64> = binds
            .iter()
            .map(|bind| -> TestResult<u64> {
                let view = view_frame(bind.bytes())?;
                Ok(BindView::from_frame(&view)?.tenant())
            })
            .collect::<TestResult<_>>()?;
        assert_eq!(bound, tenants, "each bound in its tenant");
        let reads = compute.frames(Command::ComputeRead);
        let read: Vec<u64> = reads
            .iter()
            .map(|read| -> TestResult<u64> {
                let view = view_frame(read.bytes())?;
                Ok(ComputeReadView::from_frame(&view)?.tenant())
            })
            .collect::<TestResult<_>>()?;
        assert_eq!(read, tenants, "each read in its tenant");
        Ok(())
    })
}

#[then("each reads its own statement, and neither reads the other's")]
fn one_connection_s_transactions_act_in_two_tenants_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a client sends a Write, an Operation, an Abort, and a Commit through no session, each on a new stream"
)]
fn a_write_on_a_new_stream_is_refused(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let mut client = stage.ann().await?;
        let write = client.send(&write_of(OBJECT)).await?;
        let operation = client
            .send(&Operation::new(
                counter_terms(),
                CommitOperation::CounterAdd {
                    graph: None,
                    subject: 0,
                    predicate: 1,
                    delta: 1,
                },
            ))
            .await?;
        let abort = client.send(&Abort).await?;
        let commit = client.send(&Commit::new([0; 16], 0, 0)).await?;
        for (stream, named) in [
            (write, "Write"),
            (operation, "Operation"),
            (abort, "Abort"),
            (commit, "Commit"),
        ] {
            let got = client.got(stream).await?;
            assert_eq!(got.code, Some(ErrorCode::Protocol));
            assert!(got.message.contains(named), "{}", got.message);
        }
        Ok(())
    })
}

#[then("each is refused with Error 1 on that stream, the message naming the command")]
fn a_write_on_a_new_stream_is_refused_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a client sends a Write, an Operation and a Commit, each on the stream of its own read-only transaction"
)]
fn a_commit_in_a_read_only_transaction_is_refused(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let mut client = stage.ann().await?;
        let first = client.begin_read_only(ReadLevel::Snapshot).await?;
        client.write(first, OBJECT).await?;
        let second = client.begin_read_only(ReadLevel::Snapshot).await?;
        client
            .on(
                second,
                &Operation::new(
                    counter_terms(),
                    CommitOperation::CounterAdd {
                        graph: None,
                        subject: 0,
                        predicate: 1,
                        delta: 1,
                    },
                ),
            )
            .await?;
        let third = client.begin_read_only(ReadLevel::Snapshot).await?;
        client.commit(third).await?;
        for (stream, named) in [(first, "Write"), (second, "Operation"), (third, "Commit")] {
            let got = client.got(stream).await?;
            assert_eq!(got.code, Some(ErrorCode::Protocol));
            assert!(got.message.contains(named), "{}", got.message);
            client.silent(stream, 5).await?;
        }
        Ok(())
    })
}

#[then("each is refused with Error 1, the message naming the command, and the transaction ends")]
fn a_commit_in_a_read_only_transaction_is_refused_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a client's Commit crosses the Error that ended its transaction's stream")]
fn a_commit_crossing_its_stream_s_error_is_late(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let _partition = stage
            .partition_with(1, |frame| {
                if frame.command() == Command::Write {
                    return Answer::Refuse(ErrorReply::new(
                        ErrorCode::Refused,
                        "the write is refused",
                    ));
                }
                partition_answer(frame, CLUSTER, BIND_VERSION)
            })
            .await?;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        client.write(transaction, &stage.on(1)?).await?;
        assert_eq!(client.refused(transaction).await?.0, ErrorCode::Refused);
        client.commit(transaction).await?;
        client.silent(transaction, 20).await?;
        let _open = client.begin(0).await?;
        Ok(())
    })
}

#[then("the server drops the Commit, and the connection stays open")]
fn a_commit_crossing_its_stream_s_error_is_late_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("a frontend starts on a metadata group holding no tenant")]
fn the_first_start_creates_tenant_0(w: &mut Steps) -> TestResult {
    let options = Options {
        directory: Some(fresh()?),
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let directory = held_directory(&stage)?;
        let placed = stage
            .owner
            .placement(0)
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        assert_eq!(placed.runs().len(), 1);
        assert_eq!(placed.runs().first().map(TenantRun::tenant), Some(0));
        stage.stop().await?;
        let stage_2 = Stage::start(Options {
            directory: Some(directory),
            ..Options::default()
        })
        .await?;
        let placed_2 = stage_2
            .owner
            .placement(0)
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        assert_eq!(placed_2.runs().len(), 1, "a restart creates none");
        Ok(())
    })
}

#[then(
    "the group holds tenant 0 durably before the frontend serves, Placement's runs list it, and a restart creates none"
)]
fn the_first_start_creates_tenant_0_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "the metadata group holds tenants 7 and 8, whose hashes share their top 8 bits, and a role reads Placement"
)]
fn two_tenants_share_a_run(w: &mut Steps) -> TestResult {
    let options = Options {
        tenants: pair_sharing_a_run()?,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let pair = pair_sharing_a_run()?;
        let placed = stage
            .owner
            .placement(0)
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        let runs: Vec<_> = placed
            .runs()
            .iter()
            .filter(|run| pair.contains(&run.tenant()))
            .collect();
        assert_eq!(runs.len(), 2, "each tenant once");
        assert_eq!(
            runs.first().map(|run| run.run()),
            runs.get(1).map(|run| run.run()),
            "the one run"
        );
        assert_eq!(placed.ranges().len(), 1, "the ranges unchanged by either");
        Ok(())
    })
}

#[then(
    "Placement's runs list each tenant once, both with the one `run`, and the bucket ranges are unchanged by either"
)]
fn two_tenants_share_a_run_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a transaction's declarations build a catalog version of 523,001 bytes, and the client commits it"
)]
fn a_catalog_version_past_its_cap_is_refused_at_its_commit(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let mut client = stage.ann().await?;
        let transaction = client.begin(0).await?;
        for frame in 0..4 {
            let declared = (0..64)
                .map(|at| {
                    let long = format!("http://ex.org/{frame}/{at}/{}", "a".repeat(3_000));
                    WriteChange::Declare(Declared::Predicate(DeclaredPredicate::new(long)))
                })
                .collect();
            client
                .on(transaction, &Write::new(TermTable::new(), declared))
                .await?;
        }
        client.commit(transaction).await?;
        let got = client.got(transaction).await?;
        assert_eq!((got.code, got.detail), (Some(ErrorCode::Refused), 523_000));
        assert!(got.message.contains("bytes"), "{}", got.message);
        let facts = stage
            .owner
            .tenant_facts(0)
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        assert_eq!(facts.current_version(), 0, "the current version stays");
        Ok(())
    })
}

#[then(
    "the Commit is refused with code 7, `detail` 523,000, the message naming the size, and the current version stays"
)]
fn a_catalog_version_past_its_cap_is_refused_at_its_commit_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a Drop of `p` through a session commits at its partition, the frontend stops before it commits the catalog version removing `p`'s declaration, and the client resends the Drop after the frontend restarts"
)]
fn a_resent_drop_builds_the_version_a_stopped_frontend_left(w: &mut Steps) -> TestResult {
    let options = Options {
        directory: Some(fresh()?),
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let directory = held_directory(&stage)?;
        let dropped = "http://ex.org/dropped";
        let holding = Rc::new(Cell::new(true));
        let hold = Rc::clone(&holding);
        let partition = stage
            .partition_with(1, move |frame| {
                if frame.command() == Command::Drop && hold.get() {
                    return Answer::Open;
                }
                partition_answer(frame, CLUSTER, BIND_VERSION)
            })
            .await?;
        let mut client = stage.ann().await?;
        declare(&mut client, DeclaredPredicate::new(dropped)).await?;
        let session = client.register().await?;
        let drop_of = DropCommand::predicate(0, dropped).with_session(session, 1);
        let _sent = client.send(&drop_of).await?;
        until("the Drop", || partition.count(Command::Drop) == 1).await?;
        let declared = stage
            .owner
            .schema_id(0)
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        drop(client);
        stage.stop().await?;
        let stage_2 = Stage::start(Options {
            directory: Some(directory),
            ..Options::default()
        })
        .await?;
        partition
            .join(
                &stage_2.role,
                &stage_2.cluster,
                "storage",
                &Join::storage(partition.address(), vec![1]),
            )
            .await
            .map_err(TestError::new)?;
        holding.set(false);
        let mut client_2 = stage_2.ann().await?;
        let resent = client_2.send(&drop_of).await?;
        assert_eq!(
            client_2.got(resent).await?.command,
            Command::Drop,
            "the stored reply"
        );
        let after = stage_2
            .owner
            .schema_id(0)
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        assert_ne!(
            after, declared,
            "the version commits before the reply is relayed"
        );
        Ok(())
    })
}

#[then(
    "the partition's table answers the stored reply, the frontend commits that version as pending before it relays the reply, and `p` is dropped once"
)]
fn a_resent_drop_builds_the_version_a_stopped_frontend_left_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "a Drop of `p` through no session commits at its partition, its reply is lost, and the client sends the Drop again"
)]
fn a_drop_through_no_session_runs_again(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let dropped = "http://ex.org/dropped";
        let partition = stage.partition(1).await?;
        let mut client = stage.ann().await?;
        declare(&mut client, DeclaredPredicate::new(dropped)).await?;
        let declared = stage
            .owner
            .schema_id(0)
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        let first = client.send(&DropCommand::predicate(0, dropped)).await?;
        assert_eq!(client.got(first).await?.command, Command::Drop);
        let removed = stage
            .owner
            .schema_id(0)
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        assert_ne!(removed, declared);
        let again = client.send(&DropCommand::predicate(0, dropped)).await?;
        assert_eq!(client.got(again).await?.command, Command::Drop);
        let facts = stage
            .owner
            .tenant_facts(0)
            .await
            .map_err(|refusal| TestError::new(refusal.message().to_string()))?;
        assert_eq!(facts.current_version(), 2, "the second builds no version");
        assert_eq!(partition.count(Command::Drop), 2);
        Ok(())
    })
}

#[then(
    "the second Drop removes nothing more, and the frontend builds the version removing `p`'s declaration before it relays the reply"
)]
fn a_drop_through_no_session_runs_again_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when("`score` holds a default-entry record, and a Drop of `score` commits")]
fn a_default_entry_outlives_a_drop(w: &mut Steps) -> TestResult {
    let options = Options::default();
    run(w, options, |stage| async move {
        let _whole = &stage;
        let score = "http://ex.org/score";
        let _partition = stage.partition(1).await?;
        record_default(&stage, score).await?;
        let mut client = stage.ann().await?;
        let dropping = client.send(&DropCommand::predicate(0, score)).await?;
        assert_eq!(client.got(dropping).await?.command, Command::Drop);
        let (keys, ..) = described(&mut client).await?;
        assert!(keys.iter().any(|key| key == score), "{keys:?}");
        Ok(())
    })
}

#[then("the metadata group still holds the record, and Describe names `score` as recorded")]
fn a_default_entry_outlives_a_drop_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}

#[when(
    "three transactions whose first writes name only new blank nodes arrive over three partitions"
)]
fn transactions_naming_only_new_blank_nodes_bind_in_turn(w: &mut Steps) -> TestResult {
    let options = Options {
        partitions: 3,
        ..Options::default()
    };
    run(w, options, |stage| async move {
        let _whole = &stage;
        let parts = [
            stage.partition(1).await?,
            stage.partition(2).await?,
            stage.partition(3).await?,
        ];
        let mut client = stage.ann().await?;
        for (at, part) in parts.iter().enumerate() {
            let transaction = client.begin(0).await?;
            let mut terms = TermTable::new();
            let change = WriteChange::Insert {
                graph: None,
                subject: terms.intern(RequestTerm::BlankNode("b".to_string()))?,
                predicate: terms.iri(PREDICATE),
                object: terms.iri(OBJECT),
            };
            client
                .on(transaction, &Write::new(terms, vec![change]))
                .await?;
            until("its Bind", || part.count(Command::Bind) == 1).await?;
            assert_eq!(
                parts
                    .iter()
                    .map(|one| one.count(Command::Bind))
                    .sum::<usize>(),
                at + 1,
                "one Bind each, in turn"
            );
        }
        Ok(())
    })
}

#[then(
    "the frontend binds them to partitions 1, 2 and 3 in turn, and each blank node's bucket is one its partition owns"
)]
fn transactions_naming_only_new_blank_nodes_bind_in_turn_held(w: &mut Steps) {
    assert!(w.ran, "the scenario ran");
}
