/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Exactly-once through the cluster's failures (design D12, *Failures*): a real process killed, started late, or cut from the
//! system group by a [`GroupLink`], while sessions' Commits are in flight. A Commit writes `<subject> <predicate> _:b`, a new
//! blank node, with a predicate of its own homed in the subject's partition, so it needs no other partition and a Commit
//! applied twice holds two statements of its predicate. After the fault each session resolves what it holds in doubt by its
//! resend (`PROTOCOL.md` §11.5), and the subjects are read back, each predicate counted.
//!
//! The scenarios are the compio shell's over TCP alone, the run their untagged feature takes: a role client asks the group on
//! the step's own runtime, and the link reads frames in clear text.

use std::future::Future;
use std::time::{Duration, Instant};

use cucumber::{then, when};
use ultragraphdb_client_compio::{ClientError, ClientErrorEnum, Committed, Session, Transaction};
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_core_ids::Version;
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{
    BodyView, Catalog, CatalogView, ConflictKind, DEFAULT_TRANSACTION_MS, Declare, DeclareView,
    Declared, DeclaredPredicate, DropCommand, ErrorCode, RequestTerm, TermTable, Write,
    WriteChange,
};
use ultragraphdb_tests::{
    CLUSTER, SECRET, START_SECONDS, http_status, system_role_address, until_answering,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::Steps;
use crate::client_reads::{Pattern, TENANT, direct_of, iri, refusal_of, string};
use crate::cluster::{
    Signal, Spec, after_quiet_time, client, iri_of_partition, placement_ranges, start_own_as,
};
use crate::group_link::GroupLink;
use crate::request_steps::latest_read;
use crate::shells::{ClientAt, Knobs, TestClient, pause, run};

/// The sessions whose Commits a fault lands on.
const WRITERS: usize = 3;

/// How long a scene lets Commits sent reach their partition, or a role see a peer gone: far above a turn and a close.
const SETTLE: Duration = Duration::from_millis(300);

/// How long a session waits before it sends again a request still in doubt (`PROTOCOL.md` §11.5: code 14 or 17, "the client
/// resends after a delay").
const RESEND_DELAY: Duration = Duration::from_millis(250);

/// How often a step looks again at what a role does on its own.
const LOOK: Duration = Duration::from_millis(10);

/// The longest a resend stays in doubt: twice `transaction_ms`, the longest the group stays quiet after it begins to listen and
/// a mark outlives its connection (§11.5), and a role's start beside it.
fn resend_bound() -> Duration {
    Duration::from_millis(2 * u64::from(DEFAULT_TRANSACTION_MS))
        + Duration::from_secs(START_SECONDS)
}

/// How a Commit through a session ended.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    /// Its reply, or its stored reply: committed as request `request`.
    Committed { request: u64 },
    /// Sent, unanswered or answered code 14 or 17: the session resolves it by its resend.
    InDoubt,
    /// A refusal saying it wrote nothing, or a request never sent, as the client reports it.
    NotCommitted(String),
}

/// One Commit through a session: the writer it is of, the predicate of what it wrote, its first answer, and its resend's when
/// it was in doubt, with the resends that took.
#[derive(Debug, Clone)]
struct Sent {
    writer: usize,
    predicate: String,
    first: Outcome,
    resolved: Option<(Outcome, u32)>,
}

impl Sent {
    /// How it ended once its session resolved it.
    fn last(&self) -> &Outcome {
        self.resolved
            .as_ref()
            .map_or(&self.first, |(outcome, _)| outcome)
    }

    /// Its request, committed.
    fn committed(&self) -> Option<u64> {
        match self.last() {
            Outcome::Committed { request } => Some(*request),
            Outcome::InDoubt | Outcome::NotCommitted(_) => None,
        }
    }
}

/// What this file's steps saw.
#[derive(Debug, Default)]
pub(crate) struct Seen {
    /// The Commits before the fault, those it landed on and those sent meanwhile, in the order sent.
    sent: Vec<Sent>,
    /// Each session's next Commit once it resolved what it held in doubt.
    next: Vec<Sent>,
    /// The predicate of each statement the subjects hold, read back after every next Commit.
    held: Vec<String>,
    /// What else a scene saw, for its `then` step to judge, by name.
    facts: Vec<(&'static str, String)>,
    /// The link between compute and the system group, kept to the scenario's end.
    link: Option<GroupLink>,
}

impl Seen {
    fn fact(&self, name: &str) -> TestResult<&str> {
        self.facts
            .iter()
            .find(|(held, _)| *held == name)
            .map(|(_, value)| value.as_str())
            .ok_or_else(|| TestError::new(format!("the fact {name}: {:?}", self.facts)))
    }
}

/// A client of a connection of its own with a session, taking one attempt at each commit, so a fault's answer is the step's to
/// see.
struct Writer {
    client: TestClient,
    session: Session,
}

/// `count` writers of the endpoint at `at`.
async fn writers(at: &ClientAt, count: usize) -> TestResult<Vec<Writer>> {
    let mut writers = Vec::with_capacity(count);
    for _ in 0..count {
        let mut client = TestClient::new(at, &Knobs::one_attempt())?;
        client.connect().await?;
        let session = client.register().await?;
        writers.push(Writer { client, session });
    }
    Ok(writers)
}

/// A Write inserting `subject` `predicate` and a new blank node.
fn blank_write(subject: &str, predicate: &str) -> TestResult<Write> {
    let mut terms = TermTable::new();
    let change = WriteChange::Insert {
        graph: None,
        subject: terms.iri(subject),
        predicate: terms.iri(predicate),
        object: terms.intern(RequestTerm::BlankNode("b".to_string()))?,
    };
    Ok(Write::new(terms, vec![change]))
}

/// A transaction of `writer` holding the blank write of `subject` `predicate`, its Commit not sent.
async fn begun(writer: &mut Writer, subject: &str, predicate: &str) -> TestResult<Transaction> {
    let transaction = writer.client.begin(TENANT, false).await?;
    writer
        .client
        .write(&transaction, &blank_write(subject, predicate)?)
        .await?;
    Ok(transaction)
}

/// How `answer`, a Commit's, ended; a failure for an answer exactly-once does not name.
fn outcome_of(answer: Result<Committed, ClientError>) -> TestResult<Outcome> {
    match answer {
        Ok(committed) => Ok(Outcome::Committed {
            request: committed.request(),
        }),
        Err(ClientError(ClientErrorEnum::InDoubt { .. })) => Ok(Outcome::InDoubt),
        Err(ClientError(ClientErrorEnum::ConnectionLost { .. })) => {
            Ok(Outcome::NotCommitted("never sent".to_string()))
        }
        Err(error) => match refusal_of(&error) {
            Ok((code, detail, message)) => Ok(Outcome::NotCommitted(format!(
                "{code:?} {detail}: {message}"
            ))),
            Err(_) => Err(TestError::new(format!(
                "a Commit's answer that names no outcome: {error:?}"
            ))),
        },
    }
}

/// A Commit of writer `at`'s blank write of `subject` `predicate`, sent and answered.
async fn commit_one(
    writers: &mut [Writer],
    at: usize,
    (subject, predicate): (&str, &str),
) -> TestResult<Sent> {
    let writer = writers.get_mut(at).context("the writer")?;
    let transaction = begun(writer, subject, predicate).await?;
    let answer = writer.client.commit(transaction, writer.session).await;
    Ok(Sent {
        writer: at,
        predicate: predicate.to_string(),
        first: outcome_of(answer)?,
        resolved: None,
    })
}

/// Each writer's Commit at `subject`, one predicate of `predicates` each, sent and answered one after another.
async fn commit_each(
    writers: &mut [Writer],
    subject: &str,
    predicates: &[String],
) -> TestResult<Vec<Sent>> {
    let mut sent = Vec::with_capacity(predicates.len());
    for (at, predicate) in predicates.iter().enumerate() {
        sent.push(commit_one(writers, at, (subject, predicate)).await?);
    }
    Ok(sent)
}

/// Resolves `writer`'s request in doubt by its resend, sent again after [`RESEND_DELAY`] while it stays in doubt, within
/// [`resend_bound`]: its outcome and the resends it took.
#[expect(
    clippy::disallowed_methods,
    reason = "a test measures a wait its scenario names; no simulation replays it"
)]
async fn resolved(writer: &mut Writer) -> TestResult<(Outcome, u32)> {
    let begun = Instant::now();
    let mut resends = 0;
    // Bounded by `resend_bound` (Ultra rule 2).
    loop {
        resends += 1;
        let answer = writer.client.resolve(writer.session).await;
        let outcome = outcome_of(answer)?;
        if outcome != Outcome::InDoubt {
            return Ok((outcome, resends));
        }
        if begun.elapsed() > resend_bound() {
            return Err(TestError::new(format!(
                "still in doubt after {resends} resends over {:?}",
                begun.elapsed()
            )));
        }
        pause(RESEND_DELAY).await;
    }
}

/// Resolves every Commit of `sent` left in doubt by its writer's resend.
async fn resolve_all(writers: &mut [Writer], sent: &mut [Sent]) -> TestResult {
    for sent in sent
        .iter_mut()
        .filter(|sent| sent.first == Outcome::InDoubt)
    {
        let writer = writers.get_mut(sent.writer).context("the writer")?;
        sent.resolved = Some(resolved(writer).await?);
    }
    Ok(())
}

/// The Commits of `transactions`, the `i`th of writer `i`, sent together while `fault` runs beside them: their answers, as sent.
async fn in_flight(
    writers: &mut [Writer],
    transactions: Vec<(Transaction, String)>,
    fault: impl Future<Output = TestResult>,
) -> TestResult<Vec<Sent>> {
    let predicates: Vec<String> = transactions
        .iter()
        .map(|(_, predicate)| predicate.clone())
        .collect();
    let commits = futures::future::join_all(writers.iter_mut().zip(transactions).map(
        |(writer, (transaction, _))| {
            let session = writer.session;
            writer.client.commit(transaction, session)
        },
    ));
    let (answers, faulted) = futures::join!(commits, fault);
    faulted?;
    predicates
        .into_iter()
        .zip(answers)
        .enumerate()
        .map(|(writer, (predicate, answer))| {
            Ok(Sent {
                writer,
                predicate,
                first: outcome_of(answer)?,
                resolved: None,
            })
        })
        .collect()
}

/// Each writer's transaction at `subject`, one predicate of `predicates` each, its Commit not sent.
async fn begun_each(
    writers: &mut [Writer],
    subject: &str,
    predicates: &[String],
) -> TestResult<Vec<(Transaction, String)>> {
    let mut transactions = Vec::with_capacity(predicates.len());
    for (writer, predicate) in writers.iter_mut().zip(predicates) {
        transactions.push((begun(writer, subject, predicate).await?, predicate.clone()));
    }
    Ok(transactions)
}

/// The predicate of every statement `subjects` hold, read in read-write transactions, which read their partitions' latest state.
async fn held_by(client: &mut TestClient, subjects: &[impl AsRef<str>]) -> TestResult<Vec<String>> {
    let mut held = Vec::new();
    for subject in subjects {
        let answer = latest_read(client, &Pattern::of(Some(subject.as_ref()), None)).await?;
        held.extend(answer.column(0));
    }
    Ok(held)
}

/// The tenant's latest catalog version at the system group at `system`, 0 for none, as a partition asks for it.
async fn catalog_version(system: &str) -> TestResult<u64> {
    let address = Address::parse(system)?;
    let mut role = RoleClient::dial(&address, CLUSTER, "storage", SECRET).await?;
    let Ok(reply) = role.request(&Catalog::request(TENANT, 0)).await else {
        return Ok(0);
    };
    let view = reply.view()?;
    Ok(CatalogView::from_frame(&view)?.version())
}

/// Waits until the tenant's latest catalog version at `system` is `version`, within [`START_SECONDS`].
#[expect(
    clippy::disallowed_methods,
    reason = "a test measures a wait its scenario names; no simulation replays it"
)]
async fn until_version(system: &str, version: u64) -> TestResult {
    let begun = Instant::now();
    // Bounded by `START_SECONDS` (Ultra rule 2).
    while catalog_version(system).await? != version {
        if begun.elapsed() > Duration::from_secs(START_SECONDS) {
            return Err(TestError::new(format!(
                "catalog version {version} never held"
            )));
        }
        pause(LOOK).await;
    }
    Ok(())
}

/// The names of a scene's Commits, `w`'s own and each homed in one partition: the subject; a predicate for each writer's
/// Commit before the fault, for each Commit the fault lands on, for the Commit sent meanwhile, and for each writer's next.
struct Names {
    subject: String,
    first: Vec<String>,
    flight: Vec<String>,
    outage: String,
    next: Vec<String>,
}

/// The names of `w`'s Commits in `partition` of `ranges`, for `writers` writers.
fn names_in(
    w: &Steps,
    ranges: &[(u32, u32, u32)],
    partition: u32,
    writers: usize,
) -> TestResult<Names> {
    let homed =
        |name: &str| iri_of_partition(ranges, &w.iri(&format!("{name}-{partition}/")), partition);
    let each = |stem: &str| -> TestResult<Vec<String>> {
        (0..writers)
            .map(|at| homed(&format!("{stem}{at}")))
            .collect()
    };
    Ok(Names {
        subject: homed("s")?,
        first: each("first")?,
        flight: each("flight")?,
        outage: homed("outage")?,
        next: each("next")?,
    })
}

#[when(
    "three sessions each have a Commit at partition 2, held by the second storage process, as the storage process hosting the system group is killed, a fourth session commits while the group is gone, and the host is started again"
)]
fn host_killed(w: &mut Steps) -> TestResult {
    let mut cluster = start_own_as(w.carrier, &Spec::one_partition().second_storage())?;
    let names = names_in(w, &placement_ranges(&cluster.system)?, 2, WRITERS + 1)?;
    let at = cluster.client.clone();
    let mut writers = run(writers(&at, WRITERS + 1))?;
    let (mut sent, transactions) = run(async {
        let sent = commit_each(&mut writers, &names.subject, &names.first).await?;
        let transactions =
            begun_each(&mut writers, &names.subject, &names.flight[..WRITERS]).await?;
        Ok((sent, transactions))
    })?;
    // A stopped partition takes the Commits into its socket and answers nothing: they stay in flight there.
    cluster.signal("storage_2", Signal::Stop)?;
    let mut faulted = run(async {
        let mut outage = Vec::new();
        let running = &mut cluster;
        let (flight, rest) = writers.split_at_mut(WRITERS);
        let fault = async {
            pause(SETTLE).await;
            running.signal("storage_1", Signal::Kill)?;
            pause(SETTLE).await;
            // The group is gone: this Commit's check cannot be asked.
            let mut sent = commit_one(rest, 0, (&names.subject, &names.outage)).await?;
            sent.writer = WRITERS;
            outage.push(sent);
            running.restart("storage_1")?;
            running.host_ready()?;
            running.signal("storage_2", Signal::Continue)?;
            until_answering(&running.compute_ready_address()?, "/ready")
        };
        let mut faulted = in_flight(flight, transactions, fault).await?;
        faulted.extend(outage);
        Ok(faulted)
    })?;
    // The host started again: the group answers a resend's check 14 while it is quiet.
    after_quiet_time(&at);
    let (next, held) = run(async {
        resolve_all(&mut writers, &mut faulted).await?;
        let next = commit_each(&mut writers, &names.subject, &names.next).await?;
        let mut reader = client(&at).await?;
        Ok((next, held_by(&mut reader, &[&names.subject]).await?))
    })?;
    sent.extend(faulted);
    w.failures = Seen {
        sent,
        next,
        held,
        ..Seen::default()
    };
    w.cluster = Some(cluster);
    Ok(())
}

/// That every Commit `seen` holds ended committed or not, its statement held exactly once when committed and nowhere when not,
/// that the subjects hold no other statement, and that each session's next Commit took the request above its last committed.
fn each_once(w: &Steps) -> TestResult {
    let seen = &w.failures;
    let all = || seen.sent.iter().chain(&seen.next);
    for sent in all() {
        let held = seen
            .held
            .iter()
            .filter(|held| **held == sent.predicate)
            .count();
        match sent.last() {
            Outcome::Committed { .. } => assert_eq!(held, 1, "{sent:?} held once: {seen:#?}"),
            Outcome::NotCommitted(_) => assert_eq!(held, 0, "{sent:?} held nowhere: {seen:#?}"),
            Outcome::InDoubt => {
                return Err(TestError::new(format!("{sent:?} left in doubt: {seen:#?}")));
            }
        }
    }
    let committed = all().filter(|sent| sent.committed().is_some()).count();
    assert_eq!(seen.held.len(), committed, "no other statement: {seen:#?}");
    for next in &seen.next {
        let before = seen
            .sent
            .iter()
            .filter(|sent| sent.writer == next.writer && sent.committed().is_some())
            .count();
        let expected = u64::try_from(before)? + 1;
        assert_eq!(
            next.committed(),
            Some(expected),
            "the session's next Commit, one above its committed requests: {seen:#?} {}",
            w.over()
        );
    }
    Ok(())
}

/// The Commits a session's resend resolved.
fn resent(w: &Steps) -> usize {
    w.failures
        .sent
        .iter()
        .filter(|sent| sent.resolved.is_some())
        .count()
}

#[then(
    "every Commit in doubt is resolved by its session's resend, every Commit acknowledged or resolved committed is held exactly once and no other, and each session's next Commit commits"
)]
fn resolved_each_once(w: &mut Steps) -> TestResult {
    each_once(w)?;
    // The Commit sent while the group was gone is answered code 17 at least.
    assert!(resent(w) >= 1, "{:#?}", w.failures);
    w.cluster = None;
    Ok(())
}

/// A Write declaring `predicate` single-valued.
fn single_valued(predicate: &str) -> Write {
    Write::new(
        TermTable::new(),
        vec![WriteChange::Declare(Declared::Predicate(
            DeclaredPredicate::new(predicate).with_single(true),
        ))],
    )
}

/// Whether Describe declares `predicate` single-valued, looked at until it does within [`START_SECONDS`].
#[expect(
    clippy::disallowed_methods,
    reason = "a test measures a wait its scenario names; no simulation replays it"
)]
async fn until_single(client: &mut TestClient, predicate: &str) -> TestResult<bool> {
    let begun = Instant::now();
    // Bounded by `START_SECONDS` (Ultra rule 2).
    loop {
        let described = client.describe(TENANT).await?;
        let single = described
            .predicates()
            .iter()
            .any(|held| held.predicate() == predicate && held.single());
        if single || begun.elapsed() > Duration::from_secs(START_SECONDS) {
            return Ok(single);
        }
        pause(LOOK).await;
    }
}

#[when(
    "`nick` is being checked single-valued against partition 2, three sessions each have a Commit at partition 2, and compute is killed and started again"
)]
fn compute_killed(w: &mut Steps) -> TestResult {
    let mut cluster = start_own_as(w.carrier, &Spec::one_partition().second_storage())?;
    let ranges = placement_ranges(&cluster.system)?;
    let names = names_in(w, &ranges, 2, WRITERS)?;
    let (a, nick) = (iri_of_partition(&ranges, &w.iri("a/"), 2)?, w.iri("nick"));
    let (at, system) = (cluster.client.clone(), cluster.system.clone());
    let mut declaring = run(async { client(&at).await })?;
    let mut writers = run(writers(&at, WRITERS))?;
    let (mut sent, transactions, before, tightening) = run(async {
        // `a`, in partition 2, holds one object of `nick`: the check passes once partition 2 reports.
        let _held = declaring
            .direct_write(direct_of(&a, &nick, &string("x"))?, None)
            .await?;
        let sent = commit_each(&mut writers, &names.subject, &names.first).await?;
        let transactions = begun_each(&mut writers, &names.subject, &names.flight).await?;
        let before = catalog_version(&system).await?;
        let tightening = declaring.begin(TENANT, false).await?;
        declaring.write(&tightening, &single_valued(&nick)).await?;
        Ok((sent, transactions, before, tightening))
    })?;
    // Partition 2 reports nothing, and answers no Commit, while its process is stopped.
    cluster.signal("storage_2", Signal::Stop)?;
    let (faulted, declared) = run(async {
        let running = &mut cluster;
        let fault = async {
            // The version is held being checked once the Declare reached the group.
            until_version(&system, before + 1).await?;
            pause(SETTLE).await;
            running.signal("compute", Signal::Kill)?;
            running.signal("storage_2", Signal::Continue)?;
            running.restart("compute")?;
            until_answering(&running.compute_ready_address()?, "/ready")
        };
        let (declared, faulted) = futures::join!(
            declaring.commit(tightening, None),
            in_flight(&mut writers, transactions, fault)
        );
        Ok((faulted?, declared))
    })?;
    let mut faulted = faulted;
    let (next, held, single, after) = run(async {
        // The marks of the killed compute's connection outlive it by twice `transaction_ms`: a resend meanwhile is told
        // 14.
        resolve_all(&mut writers, &mut faulted).await?;
        let next = commit_each(&mut writers, &names.subject, &names.next).await?;
        let mut reader = client(&at).await?;
        let held = held_by(&mut reader, &[&names.subject]).await?;
        let single = until_single(&mut reader, &nick).await?;
        Ok((next, held, single, catalog_version(&system).await?))
    })?;
    sent.extend(faulted);
    w.failures = Seen {
        sent,
        next,
        held,
        facts: vec![
            ("declared", format!("{declared:?}")),
            ("single", single.to_string()),
            ("versions", format!("{before} {after}")),
        ],
        ..Seen::default()
    };
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "every Commit in doubt is resolved by its session's resend through the new compute node, every Commit acknowledged or resolved committed is held exactly once and no other, each session's next Commit commits, and the tightening takes effect once"
)]
fn compute_killed_once(w: &mut Steps) -> TestResult {
    each_once(w)?;
    // Each Commit at the stopped partition was sent when its connection went with compute.
    assert_eq!(resent(w), WRITERS, "{:#?}", w.failures);
    let seen = &w.failures;
    assert_eq!(seen.fact("single")?, "true", "{seen:#?}");
    let versions: Vec<u64> = seen
        .fact("versions")?
        .split(' ')
        .map(str::parse)
        .collect::<Result<_, _>>()?;
    let [before, after] = versions.as_slice() else {
        return Err(TestError::new(format!("{versions:?}")));
    };
    assert_eq!(
        *after,
        before + 1,
        "the tightening's version, and no other: {seen:#?}"
    );
    w.cluster = None;
    Ok(())
}

/// The system group's role address of a scenario's own cluster over TCP, which a [`GroupLink`] carries compute to.
fn own_system() -> TestResult<String> {
    Ok(format!("tcp://{}", system_role_address()?))
}

#[when(
    "three sessions each have a Commit at partition 2 as compute's connection to the system group closes, a fourth session commits before compute has dialled again, and compute dials again"
)]
fn link_closed(w: &mut Steps) -> TestResult {
    let link = GroupLink::to(&own_system()?)?;
    let spec = Spec::one_partition()
        .second_storage()
        .compute_dialling(&link.address());
    let cluster = start_own_as(w.carrier, &spec)?;
    let names = names_in(w, &placement_ranges(&cluster.system)?, 2, WRITERS + 1)?;
    let at = cluster.client.clone();
    let mut writers = run(writers(&at, WRITERS + 1))?;
    let (mut sent, transactions) = run(async {
        let sent = commit_each(&mut writers, &names.subject, &names.first).await?;
        let transactions =
            begun_each(&mut writers, &names.subject, &names.flight[..WRITERS]).await?;
        Ok((sent, transactions))
    })?;
    // The Commits stay at the stopped partition while the connection closes and is dialled again.
    cluster.signal("storage_2", Signal::Stop)?;
    let dialled_before = link.connections();
    let (mut faulted, next) = run(async {
        let mut outage = Vec::new();
        let (flight, rest) = writers.split_at_mut(WRITERS);
        let fault = async {
            pause(SETTLE).await;
            link.sever();
            pause(SETTLE).await;
            // The core's connection is closed: this Commit's check cannot be asked.
            let mut sent = commit_one(rest, 0, (&names.subject, &names.outage)).await?;
            sent.writer = WRITERS;
            outage.push(sent);
            link.restore();
            until_answering(&cluster.compute_ready_address()?, "/ready")?;
            cluster.signal("storage_2", Signal::Continue)
        };
        let mut faulted = in_flight(flight, transactions, fault).await?;
        // Each of these sessions' marks ended by an Ended on the new connection: the next Commit's check passes at once.
        let next = commit_each(flight, &names.subject, &names.next[..WRITERS]).await?;
        faulted.extend(outage);
        Ok((faulted, next))
    })?;
    let dialled_after = link.connections();
    // The group began to listen as the cluster started: it answers a resend's check 14 while it is quiet.
    after_quiet_time(&at);
    let (outage_next, held) = run(async {
        resolve_all(&mut writers, &mut faulted).await?;
        let next = commit_one(
            &mut writers,
            WRITERS,
            (
                &names.subject,
                names.next.get(WRITERS).context("a predicate")?,
            ),
        )
        .await?;
        let mut reader = client(&at).await?;
        Ok((next, held_by(&mut reader, &[&names.subject]).await?))
    })?;
    sent.extend(faulted);
    let mut next = next;
    next.push(outage_next);
    w.failures = Seen {
        sent,
        next,
        held,
        facts: vec![("dialled", format!("{dialled_before} {dialled_after}"))],
        link: Some(link),
    };
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "each Commit at partition 2 is acknowledged, every Commit in doubt is resolved by its session's resend, every Commit acknowledged or resolved committed is held exactly once and no other, and each session's next Commit passes its check at once"
)]
fn link_closed_once(w: &mut Steps) -> TestResult {
    each_once(w)?;
    let seen = &w.failures;
    let flight = seen
        .sent
        .get(WRITERS + 1..2 * WRITERS + 1)
        .context("the Commits at partition 2")?;
    for sent in flight {
        assert!(
            matches!(sent.first, Outcome::Committed { .. }),
            "acknowledged through the closed connection: {seen:#?}"
        );
    }
    // The Commit sent while the connection was closed was answered code 17.
    assert_eq!(resent(w), 1, "{seen:#?}");
    let dialled: Vec<u32> = seen
        .fact("dialled")?
        .split(' ')
        .map(str::parse)
        .collect::<Result<_, _>>()?;
    let [before, after] = dialled.as_slice() else {
        return Err(TestError::new(format!("{dialled:?}")));
    };
    assert!(after > before, "compute dialled the group again: {seen:#?}");
    w.cluster = None;
    w.failures.link = None;
    Ok(())
}

/// A writer of `at` once an endpoint core there relays its authentication to the system group, within [`START_SECONDS`]: a
/// core answers Error 17 until its connection to the group has opened (`PROTOCOL.md` §11.12).
#[expect(
    clippy::disallowed_methods,
    reason = "a test measures a wait its scenario names; no simulation replays it"
)]
async fn writer_once_relayed(at: &ClientAt) -> TestResult<Writer> {
    let begun = Instant::now();
    // Bounded by `START_SECONDS` (Ultra rule 2).
    loop {
        let mut client = TestClient::new(at, &Knobs::one_attempt())?;
        let connected = client.connect().await;
        if connected.is_ok() {
            let session = client.register().await?;
            return Ok(Writer { client, session });
        }
        if begun.elapsed() > Duration::from_secs(START_SECONDS) {
            return Err(TestError::new(format!(
                "no core relayed an authentication: {connected:?}"
            )));
        }
        pause(LOOK).await;
    }
}

/// A Commit of writer `at`'s blank write of `subject` `predicate`, its transaction begun and written as it is sent: a Begin or
/// Write refused leaves it never sent.
async fn begun_and_committed(
    writers: &mut [Writer],
    at: usize,
    (subject, predicate): (&str, &str),
) -> TestResult<Sent> {
    let writer = writers.get_mut(at).context("the writer")?;
    let answer = match begun(writer, subject, predicate).await {
        Ok(transaction) => outcome_of(writer.client.commit(transaction, writer.session).await)?,
        Err(refused) => Outcome::NotCommitted(format!("never sent: {refused}")),
    };
    Ok(Sent {
        writer: at,
        predicate: predicate.to_string(),
        first: answer,
        resolved: None,
    })
}

#[when(
    "compute starts before any storage process, the storage process hosting the system group starts, and sessions commit at partitions 1 and 2 as the second storage process starts"
)]
fn compute_first(w: &mut Steps) -> TestResult {
    let spec = Spec::one_partition().second_storage().compute_first();
    let mut cluster = start_own_as(w.carrier, &spec)?;
    let unready = http_status(&cluster.compute_ready_address()?, "/ready")?;
    cluster.restart("storage_1")?;
    cluster.host_ready()?;
    let ranges = placement_ranges(&cluster.system)?;
    // Writer `i` commits at partition 1 when `i` is even, at partition 2 when odd.
    let partition_of = |writer: usize| if writer.is_multiple_of(2) { 1 } else { 2 };
    let first = names_in(w, &ranges, 1, WRITERS)?;
    let second = names_in(w, &ranges, 2, WRITERS)?;
    let named = |writer: usize| {
        if partition_of(writer) == 1 {
            &first
        } else {
            &second
        }
    };
    let at = cluster.client.clone();
    let mut writers = run(async {
        let mut writers = Vec::with_capacity(WRITERS);
        for _ in 0..WRITERS {
            writers.push(writer_once_relayed(&at).await?);
        }
        Ok(writers)
    })?;
    let mut sent = run(async {
        let running = &mut cluster;
        let fault = async {
            pause(SETTLE).await;
            running.restart("storage_2")?;
            until_answering(&running.observability("storage_2")?, "/ready")?;
            running.restart("compute")?;
            until_answering(&running.observability("compute")?, "/ready")?;
            until_answering(&running.compute_ready_address()?, "/ready")
        };
        let commits = async {
            let mut sent = Vec::with_capacity(WRITERS);
            for writer in 0..WRITERS {
                let names = named(writer);
                let predicate = names.flight.get(writer).context("a predicate")?;
                sent.push(
                    begun_and_committed(&mut writers, writer, (&names.subject, predicate)).await?,
                );
            }
            TestResult::Ok(sent)
        };
        let (sent, faulted) = futures::join!(commits, fault);
        faulted?;
        sent
    })?;
    // The host began to listen as it started: it answers a resend's check 14 while it is quiet, and a session holding nothing
    // in doubt sends no resend.
    if sent.iter().any(|sent| sent.first == Outcome::InDoubt) {
        after_quiet_time(&at);
    }
    let (next, held) = run(async {
        resolve_all(&mut writers, &mut sent).await?;
        let mut next = Vec::with_capacity(WRITERS);
        for writer in 0..WRITERS {
            let names = named(writer);
            let predicate = names.next.get(writer).context("a predicate")?;
            next.push(commit_one(&mut writers, writer, (&names.subject, predicate)).await?);
        }
        let mut reader = client(&at).await?;
        let held = held_by(&mut reader, &[&first.subject, &second.subject]).await?;
        Ok((next, held))
    })?;
    w.failures = Seen {
        sent,
        next,
        held,
        facts: vec![("unready", unready.to_string())],
        ..Seen::default()
    };
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "compute answers not ready until storage has started, every Commit in doubt is resolved by its session's resend, every Commit acknowledged or resolved committed is held exactly once and no other, and each session's next Commit commits"
)]
fn compute_first_once(w: &mut Steps) -> TestResult {
    each_once(w)?;
    assert_eq!(
        w.failures.fact("unready")?,
        "503",
        "compute was not ready before storage: {:#?}",
        w.failures
    );
    w.cluster = None;
    Ok(())
}

/// The catalog version `predicate`'s declaration builds on `built_on`, declared at the system group at `system` as
/// compute asks for it, so no endpoint core's view holds it from a Declare's reply: the version and the first version
/// of the epoch it takes effect at.
async fn declared_at_group(system: &str, built_on: u64, predicate: &str) -> TestResult<(u64, u64)> {
    let address = Address::parse(system)?;
    let mut role = RoleClient::dial(&address, CLUSTER, "compute", SECRET).await?;
    let declared = vec![Declared::Predicate(DeclaredPredicate::new(predicate))];
    let reply = role
        .request(&Declare::declarations(TENANT, built_on, 0, declared))
        .await?;
    let view = reply.view()?;
    let built = DeclareView::from_frame(&view)?;
    Ok((built.version(), built.commit_version()))
}

/// Writes `subject` `predicate` and a new object through no session until the partition stamps a commit at or past `epoch`,
/// within [`START_SECONDS`]: the epoch of the last, and the statements written.
#[expect(
    clippy::disallowed_methods,
    reason = "a test measures a wait its scenario names; no simulation replays it"
)]
async fn committed_at_or_past(
    client: &mut TestClient,
    (subject, predicate): (&str, &str),
    epoch: u64,
) -> TestResult<(u64, usize)> {
    let begun = Instant::now();
    let mut written = 0;
    // Bounded by `START_SECONDS` (Ultra rule 2).
    loop {
        written += 1;
        let object = format!("{subject}/o{written}");
        let committed = client
            .direct_write(direct_of(subject, predicate, &iri(&object))?, None)
            .await?;
        let stamped = Version::from_u64(committed.version()).epoch().get();
        if stamped >= epoch {
            return Ok((stamped, written));
        }
        if begun.elapsed() > Duration::from_secs(START_SECONDS) {
            return Err(TestError::new(format!(
                "the partition stamped epoch {stamped}, below {epoch}"
            )));
        }
        pause(LOOK).await;
    }
}

/// Describe's catalog version through `client`, looked at until it is `version` within [`START_SECONDS`].
#[expect(
    clippy::disallowed_methods,
    reason = "a test measures a wait its scenario names; no simulation replays it"
)]
async fn until_described(client: &mut TestClient, version: u64) -> TestResult<u64> {
    let begun = Instant::now();
    // Bounded by `START_SECONDS` (Ultra rule 2).
    loop {
        let described = client.describe(TENANT).await?.catalog();
        if described == version || begun.elapsed() > Duration::from_secs(START_SECONDS) {
            return Ok(described);
        }
        pause(LOOK).await;
    }
}

#[when(
    "a catalog version of the tenant takes effect while compute's endpoint cores hold none of the closes announcing it, and a client sends a Drop of the tenant's data through its session naming the version Describe gave"
)]
fn drop_behind(w: &mut Steps) -> TestResult {
    let link = GroupLink::to(&own_system()?)?;
    let spec = Spec::one_partition().compute_dialling(&link.address());
    let cluster = start_own_as(w.carrier, &spec)?;
    let (at, system) = (cluster.client.clone(), cluster.system.clone());
    let (s, p, probe, declared) = (w.iri("s"), w.iri("p"), w.iri("probe"), w.iri("declared"));
    let mut admin = run(async { client(&at).await })?;
    let mut writers = run(writers(&at, 1))?;
    let names = [w.iri("before"), w.iri("after")];
    let (first, described, held_before) = run(async {
        for object in ["o1", "o2"] {
            let _held = admin
                .direct_write(direct_of(&s, &p, &iri(&format!("{s}/{object}")))?, None)
                .await?;
        }
        let first = commit_one(&mut writers, 0, (&s, &names[0])).await?;
        let writer = writers.first_mut().context("the writer")?;
        let described = writer.client.describe(TENANT).await?.catalog();
        let held = held_by(&mut admin, &[&s]).await?;
        Ok((first, described, held))
    })?;
    // From here compute's cores hear nothing from the group: no close, and no answer, until the link resumes.
    link.stall();
    let (version, effective, stamped, held_closes, refused, held_refused) = run(async {
        let current = catalog_version(&system).await?;
        let (version, commit_version) = declared_at_group(&system, current, &declared).await?;
        let effective = Version::from_u64(commit_version).epoch().get();
        // The partition decides at a round of the epoch the new version is in effect at.
        let (stamped, probes) = committed_at_or_past(&mut admin, (&probe, &p), effective).await?;
        let writer = writers.first_mut().context("the writer")?;
        // The core passes the Drop on its view, which holds no close announcing the new version, and asks its check: the
        // check's answer comes once the link resumes, after the closes it held.
        let dropping = writer.client.drop_command(
            DropCommand::tenant_data(TENANT).with_catalog(described),
            writer.session,
        );
        let resuming = async {
            pause(SETTLE).await;
            let held = link.held();
            link.resume();
            held
        };
        let (refused, held_closes) = futures::join!(dropping, resuming);
        let held = held_by(&mut admin, &[&s, &probe]).await?;
        Ok((
            version,
            effective,
            stamped,
            held_closes,
            refused,
            (held.len(), probes),
        ))
    })?;
    let (described_after, again, next, held_after) = run(async {
        let writer = writers.first_mut().context("the writer")?;
        let described = until_described(&mut writer.client, version).await?;
        let again = writer
            .client
            .drop_command(
                DropCommand::tenant_data(TENANT).with_catalog(described),
                writer.session,
            )
            .await;
        let next = commit_one(&mut writers, 0, (&s, &names[1])).await?;
        let held = held_by(&mut admin, &[&s, &probe]).await?;
        Ok((described, again, next, held))
    })?;
    w.failures = Seen {
        sent: vec![first],
        next: vec![next],
        held: held_after,
        facts: vec![
            ("described", described.to_string()),
            ("version", version.to_string()),
            ("effective", effective.to_string()),
            ("stamped", stamped.to_string()),
            ("held closes", held_closes.to_string()),
            (
                "refused",
                format!("{:?}", refused.map(|committed| committed.request())),
            ),
            ("held before", held_before.len().to_string()),
            ("held refused", held_refused.0.to_string()),
            ("probes", held_refused.1.to_string()),
            ("described after", described_after.to_string()),
            (
                "again",
                format!("{:?}", again.map(|committed| committed.request())),
            ),
        ],
        link: Some(link),
    };
    w.cluster = Some(cluster);
    Ok(())
}

#[then(
    "the partition refuses the Drop with code 2, kind 5, removing nothing; once the closes arrive, the Drop sent again with its request number, naming the version Describe then gives, removes the data once, and the session's next request commits and is held"
)]
fn drop_refused_then_once(w: &mut Steps) -> TestResult {
    let seen = &w.failures;
    let number = |name: &str| -> TestResult<u64> { Ok(seen.fact(name)?.parse()?) };
    // The core was behind: the Drop named the version before the new one, the partition stamped where the new one is in
    // effect, and the link held a close from the core.
    assert_eq!(number("version")?, number("described")? + 1, "{seen:#?}");
    assert!(number("stamped")? >= number("effective")?, "{seen:#?}");
    assert!(number("held closes")? >= 1, "{seen:#?}");
    let refused = seen.fact("refused")?;
    let kind = ConflictKind::Catalog.number();
    assert!(
        refused.contains(&format!("code: {:?}, detail: {kind}", ErrorCode::Conflict)),
        "{seen:#?}"
    );
    // Compute's early refusal names both versions; the partition's is its own.
    assert!(
        !refused.contains("the Drop names catalog version"),
        "{seen:#?}"
    );
    // Removing nothing: what the subject held stands, beside the probes.
    assert_eq!(
        number("held refused")?,
        number("held before")? + number("probes")?,
        "{seen:#?}"
    );
    let first = seen
        .sent
        .first()
        .and_then(Sent::committed)
        .context("the session's first Commit")?;
    assert_eq!(number("described after")?, number("version")?, "{seen:#?}");
    assert_eq!(
        seen.fact("again")?,
        format!("Ok({})", first + 1),
        "the Drop sent again with the refused one's number, committed: {seen:#?}"
    );
    let next = seen.next.first().context("the next Commit")?;
    assert_eq!(next.committed(), Some(first + 2), "{seen:#?}");
    // The Drop removed the data once: what the next request wrote stands alone.
    assert_eq!(
        seen.held,
        std::slice::from_ref(&next.predicate),
        "{seen:#?}"
    );
    w.cluster = None;
    w.failures.link = None;
    Ok(())
}
