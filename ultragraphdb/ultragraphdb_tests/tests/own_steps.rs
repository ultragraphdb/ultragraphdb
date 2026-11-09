/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Scenarios of a cluster of their own: its settings differ, or a role of it misbehaves. A role that misbehaves is the real
//! process under a signal: `SIGKILL` for a crash, `SIGSTOP` for one that hangs, its socket still accepting.

use std::time::{Duration, Instant};

use cucumber::{then, when};
use ultragraphdb_client_compio::{ClientError, ClientErrorEnum, Login};
use ultragraphdb_protocol_scram::{ScramError, ScramLogin, system_nonce};
use ultragraphdb_protocol_transport::{Address, ClientMechanism, TransportError};
use ultragraphdb_protocol_wire::{Begin, Command, ErrorCode};
use ultragraphdb_tests::CLUSTER;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::Steps;
use crate::client_reads::{
    Answer, Pattern, TENANT, batch_of, closing_code, direct_of, drain, iri, read, read_whole,
    refusal_of, string, write_of,
};
use crate::cluster::{
    Carriers, Cluster, Signal, Spec, client, hand_back, iri_of_partition, placement_ranges, taken,
};
use crate::lossy_link::LossyLink;
use crate::read_steps::{big_pattern, until_rows, write_big};
use crate::request_steps::latest_read;
use crate::shells::{CompioDial, Knobs, TestClient, pause, run, with_client};
use crate::transaction_steps::{committed, nothing};

/// The time, to measure a wait a scenario names.
#[expect(
    clippy::disallowed_methods,
    reason = "a test measures a wait its scenario names; no simulation replays it"
)]
fn clock() -> Instant {
    Instant::now()
}

/// The least between two closes of a slow cluster, the most `epoch_ms_min` takes: however a read asks, a close comes this long
/// after the one before it.
const SLOW_EPOCH_MS: u64 = 1_000;

/// A cluster whose epochs close every `SLOW_EPOCH_MS`, the frontend's sequencer and its close bound at it.
fn slow_epochs() -> Spec {
    Spec::one_partition()
        .frontend("storage.scan_stall_ms", STALL_MS)
        .frontend("sequencer.epoch_ms_min", SLOW_EPOCH_MS)
        .frontend("sequencer.epoch_ms_max", SLOW_EPOCH_MS)
        .storage("storage.close_request_ms", 4 * SLOW_EPOCH_MS)
}

/// A commit through a new session at the start of an epoch of a slow cluster, whose token's read waits nearly the whole epoch:
/// a read through a first commit's token ends just after a close, and the session's next commit lands in the epoch it opened.
async fn commit_early_in_an_epoch(
    client: &mut TestClient,
    (a, p, b): (&str, &str, &str),
) -> TestResult<ultragraphdb_client_compio::Session> {
    let session = client.register().await?;
    let first = direct_of(a, p, &string("first"))?;
    let _first = client.direct_write(first, session).await?;
    let reader = client
        .begin_with(Begin::new(TENANT, 0, true), session)
        .await?;
    let _closed = read_whole(client, &reader, &Pattern::of(Some(a), Some(p))).await?;
    client.abort(reader).await?;
    let _written = client
        .direct_write(direct_of(a, p, &iri(b))?, session)
        .await?;
    Ok(session)
}

/// A client timeout a login's PBKDF2 stays within on a loaded machine: the client's own work counts against it.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(2);

/// A frontend that takes a compute process for hung past 100 ms of silence.
const STALL_MS: u64 = 100;

/// A cluster of two compute processes behind that frontend.
fn two_computes() -> Spec {
    Spec::one_partition()
        .second_compute()
        .frontend("storage.scan_stall_ms", STALL_MS)
}

/// A cluster of one compute process behind that frontend.
fn stalling() -> Spec {
    Spec::one_partition().frontend("storage.scan_stall_ms", STALL_MS)
}

/// Takes `spec`'s cluster for `w`, over TCP in clear text: this run holds one of each spec, and the scenarios of a spec take it
/// in turn, each in IRIs of its own, leaving no role stopped.
fn own(w: &mut Steps, spec: &Spec) -> TestResult {
    let (name, cluster) = taken(Carriers::Tcp, spec)?;
    w.named = Some(name);
    w.cluster = Some(cluster);
    Ok(())
}

/// Hands the scenario's cluster back for the next scenario of its spec.
fn release(w: &mut Steps) -> TestResult {
    if let (Some(name), Some(cluster)) = (w.named.take(), w.cluster.take()) {
        hand_back(name, cluster);
    }
    Ok(())
}

fn cluster(w: &Steps) -> TestResult<&Cluster> {
    w.cluster.as_ref().context("the scenario's cluster")
}

/// The refusals seen, which were the code `code`.
fn refused_all(w: &Steps, code: ErrorCode, count: usize) -> TestResult {
    assert_eq!(
        w.seen.refusals.len(),
        count,
        "{:?} {}",
        w.seen.refusals,
        w.over()
    );
    for refused in &w.seen.refusals {
        assert_eq!(refusal_of(refused)?.0, code, "{refused:?}");
    }
    Ok(())
}

#[when("the frontend allows 2 connections and a third client connects")]
fn third_connection(w: &mut Steps) -> TestResult {
    own(
        w,
        &Spec::one_partition().frontend("endpoint.connections_max", 2),
    )?;
    let at = cluster(w)?.client.clone();
    let (refused, begun) = run(async {
        let (mut one, mut two) = (client(&at).await?, client(&at).await?);
        let mut third = TestClient::new(&at, &Knobs::default())?;
        let refused = third.connect().await.err().context("the third refused")?;
        let (first, second) = (
            one.begin(TENANT, true).await?,
            two.begin(TENANT, true).await?,
        );
        Ok((refused, (first.tenant(), second.tenant())))
    })?;
    w.seen.facts = vec![format!("{begun:?}")];
    w.seen.refusals = vec![refused];
    Ok(())
}

#[then("the third connection is refused with the limit error, and the first two keep working")]
fn third_refused(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    assert_eq!(
        closing_code(refused),
        Some(ErrorCode::StreamLimit),
        "{refused:?}"
    );
    assert_eq!(
        w.seen.facts,
        [format!("{:?}", (TENANT, TENANT))],
        "{}",
        w.over()
    );
    release(w)
}

/// A registry of one session, held by a client whose session the next Register evicts: the refusal its commit takes, and what
/// it left held.
fn evicted_commit(w: &mut Steps) -> TestResult {
    own(
        w,
        &Spec::one_partition().frontend("metadata.sessions_max", 1),
    )?;
    let at = cluster(w)?.client.clone();
    let [a, p, b] = ["a", "p", "b"].map(|name| w.iri(name));
    let (refused, held) = run(async {
        let mut first = client(&at).await?;
        let session = first.register().await?;
        let mut second = client(&at).await?;
        let _evicting = second.register().await?;
        let own = first.begin(TENANT, false).await?;
        first.write(&own, &write_of(&a, &p, &iri(&b))?).await?;
        let refused = first
            .commit(own, session)
            .await
            .err()
            .context("a refusal")?;
        let held = latest_read(&mut second, &Pattern::of(Some(&a), Some(&p))).await?;
        Ok((refused, held))
    })?;
    w.seen.refusals = vec![refused];
    w.seen.answers = vec![held];
    Ok(())
}

/// That the session's request was refused as evicted, nothing of it held.
fn refused_as_evicted(w: &mut Steps) -> TestResult {
    refused_all(w, ErrorCode::SessionEvicted, 1)?;
    nothing(w.seen.answers.first().context("what is held")?)?;
    release(w)
}

/// The clients of the many-cores scenario, each over a connection of its own.
const CORE_CLIENTS: usize = 32;

/// How long the frontend has to log the phases of the writes once they end: a window is due a second after its last write.
const PHASES_DUE: Duration = Duration::from_secs(10);

/// The values of `key=` on each line of `log` holding `marker`, as numbers.
fn logged(log: &str, marker: &str, key: &str) -> Vec<u64> {
    log.lines()
        .filter(|line| line.contains(marker))
        .filter_map(|line| {
            line.split_whitespace()
                .find_map(|field| field.strip_prefix(key))
                .and_then(|value| value.parse().ok())
        })
        .collect()
}

#[when(
    "32 clients each commit a write over a connection of their own to a frontend of two partitions"
)]
fn commits_on_every_core(w: &mut Steps) -> TestResult {
    own(w, &Spec::over(2))?;
    let at = cluster(w)?.client.clone();
    let before = cluster(w)?.frontend_log()?.len();
    let writes = (0..CORE_CLIENTS)
        .map(|n| direct_of(&w.iri(&format!("a{n}")), &w.iri("p"), &iri(&w.iri("b"))))
        .collect::<TestResult<Vec<_>>>()?;
    w.seen.commits = run(async {
        // Every connection open before the first write, so each is accepted while the others are, by whichever core the kernel
        // hands it to.
        let mut clients = Vec::new();
        for _ in 0..CORE_CLIENTS {
            clients.push(client(&at).await?);
        }
        let mut commits = Vec::new();
        for (client, write) in clients.iter_mut().zip(writes) {
            commits.push(client.direct_write(write, None).await?);
        }
        Ok(commits)
    })?;
    let started = clock();
    loop {
        let log = cluster(w)?.frontend_log()?;
        let since = log.get(before..).unwrap_or_default();
        let relayed: u64 = logged(since, "one-frame write phases", "writes=")
            .iter()
            .sum();
        if relayed >= CORE_CLIENTS as u64 || started.elapsed() >= PHASES_DUE {
            w.seen.facts = vec![log.clone(), since.to_string()];
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[then(
    "every write is committed, the frontend logs a runtime per CPU, and more than one of its cores relays the writes"
)]
fn served_on_every_core(w: &mut Steps) -> TestResult {
    assert_eq!(w.seen.commits.len(), CORE_CLIENTS, "{}", w.over());
    let [log, since] = [0, 1].map(|at| w.seen.facts.get(at).cloned().unwrap_or_default());
    let mut runtimes = logged(&log, "a runtime on the", "core=");
    runtimes.sort_unstable();
    runtimes.dedup();
    let cpus = std::thread::available_parallelism().map_or(1, usize::from);
    assert_eq!(runtimes.len(), cpus, "a runtime per CPU: {runtimes:?}");
    let writing = logged(&since, "one-frame write phases", "core=");
    let relayed: u64 = logged(&since, "one-frame write phases", "writes=")
        .iter()
        .sum();
    assert_eq!(
        relayed, CORE_CLIENTS as u64,
        "every write relayed once:\n{since}"
    );
    let mut cores = writing.clone();
    cores.sort_unstable();
    cores.dedup();
    assert!(
        cores.len() > 1 || cpus == 1,
        "relayed by cores {cores:?}:\n{since}"
    );
    release(w)
}

#[when(
    "a session is evicted while a connection on another core holds it, and that connection commits next"
)]
fn evicted_on_another_core(w: &mut Steps) -> TestResult {
    evicted_commit(w)
}

#[then("the commit is refused with the eviction's error, and no partition sees it")]
fn evicted_refused(w: &mut Steps) -> TestResult {
    refused_as_evicted(w)
}

#[when("a session is evicted from the full registry and its client sends its next request")]
fn evicted_next_request(w: &mut Steps) -> TestResult {
    evicted_commit(w)
}

#[then(
    "the frontend refuses the request with an error saying the session was evicted, and no partition sees it"
)]
fn evicted_told(w: &mut Steps) -> TestResult {
    refused_as_evicted(w)
}

#[when(
    "a retried Commit is recalled, every partition answers 18, and the registry has evicted the session meanwhile"
)]
fn evicted_meanwhile(w: &mut Steps) -> TestResult {
    own(
        w,
        &Spec::one_partition().frontend("metadata.sessions_max", 1),
    )?;
    let at = cluster(w)?.client.clone();
    let [a, p, b] = ["a", "p", "b"].map(|name| w.iri(name));
    let link = LossyLink::to(&at)?;
    let resolved = run(async {
        let mut first = TestClient::new(&link.client_at(), &Knobs::one_attempt())?;
        first.connect().await?;
        let session = first.register().await?;
        let own = first.begin(TENANT, false).await?;
        first.write(&own, &write_of(&a, &p, &iri(&b))?).await?;
        link.lose_request(Command::Commit);
        let _doubt = first.commit(own, session).await.err().context("in doubt")?;
        let mut second = client(&at).await?;
        let _evicting = second.register().await?;
        Ok(first.resolve(session).await)
    })?;
    w.seen.facts = vec![format!("{resolved:?}")];
    w.committed = matches!(
        resolved,
        Err(ClientError(ClientErrorEnum::OutcomeUnknown { .. }))
    );
    Ok(())
}

#[then("the frontend answers code 12, and the client reports the outcome as unknown")]
fn evicted_unknown(w: &mut Steps) -> TestResult {
    assert!(w.committed, "{:?} {}", w.seen.facts, w.over());
    release(w)
}

#[when(
    "the registry holds `sessions_max` sessions, each with a commit in flight, and a client registers another"
)]
fn registry_all_in_flight(w: &mut Steps) -> TestResult {
    own(
        w,
        &Spec::one_partition().frontend("metadata.sessions_max", 1),
    )?;
    let at = cluster(w)?.client.clone();
    let [a, p, b] = ["a", "p", "b"].map(|name| w.iri(name));
    let (mut first, mut second, session) = run(async {
        let mut first = client(&at).await?;
        let mut second = TestClient::new(&at, &Knobs::one_attempt())?;
        second.connect().await?;
        let session = first.register().await?;
        Ok((first, second, session))
    })?;
    // A stopped partition takes the commit into its socket and answers nothing, so the commit stays in flight.
    cluster(w)?.signal("storage", Signal::Stop)?;
    let running = cluster(w)?;
    let (busy, written, again) = run(async {
        let register = async {
            pause(Duration::from_millis(200)).await;
            let busy = second.register().await;
            running.signal("storage", Signal::Continue)?;
            TestResult::Ok(busy)
        };
        let (written, busy) = futures::join!(
            first.direct_write(direct_of(&a, &p, &iri(&b))?, session),
            register
        );
        let written = written?;
        let again = second.register().await;
        Ok((busy?, written, again))
    })?;
    w.seen.refusals = busy.err().into_iter().collect();
    w.seen.commits = vec![written];
    w.committed = again.is_ok();
    Ok(())
}

#[then(
    "the Register is refused with code 14, nothing is evicted, and a Register after a commit is answered is served"
)]
fn register_busy(w: &mut Steps) -> TestResult {
    refused_all(w, ErrorCode::SessionBusy, 1)?;
    assert!(w.committed, "a Register after the commit {}", w.over());
    release(w)
}

#[when(
    "the registry holds `sessions_max` sessions, the oldest has a Commit at its partition unanswered, its client's connection closes, and another session is registered"
)]
fn oldest_in_flight(w: &mut Steps) -> TestResult {
    own(
        w,
        &Spec::one_partition().frontend("metadata.sessions_max", 2),
    )?;
    let at = cluster(w)?.client.clone();
    let [a, p, b, c] = ["a", "p", "b", "c"].map(|name| w.iri(name));
    let running = cluster(w)?;
    let (newer, oldest, resumed) = run(async {
        let mut oldest = client(&at).await?;
        let session = oldest.register().await?;
        let mut newer = client(&at).await?;
        let newer_session = newer.register().await?;
        // A stopped partition answers nothing, so the Commit stays at it while its client's connection closes.
        running.signal("storage", Signal::Stop)?;
        let write = direct_of(&a, &p, &iri(&b))?;
        let waited = within_or_gone(
            oldest.direct_write(write, session),
            Duration::from_millis(200),
        )
        .await;
        drop(oldest);
        waited?;
        let mut third = client(&at).await?;
        let _third = third.register().await?;
        running.signal("storage", Signal::Continue)?;
        let newer_write = newer
            .direct_write(direct_of(&a, &p, &iri(&c))?, newer_session)
            .await;
        let mut again = client(&at).await?;
        let resumed = again.resume_session(session.number(), 1)?;
        let oldest_write = again
            .direct_write(direct_of(&a, &p, &iri(&b))?, resumed)
            .await;
        Ok((newer_write, oldest_write.is_ok(), session.number()))
    })?;
    w.seen.facts = vec![
        format!("{newer:?}"),
        oldest.to_string(),
        resumed.to_string(),
    ];
    w.committed = oldest
        && matches!(&newer, Err(error) if refusal_of(error).is_ok_and(|(code, _, _)| code == ErrorCode::SessionEvicted));
    Ok(())
}

/// Waits `bound` on `work`, which must not have answered by then; it is then dropped, as a client gone from it.
async fn within_or_gone<T: std::fmt::Debug>(
    work: impl Future<Output = T>,
    bound: Duration,
) -> TestResult {
    let work = std::pin::pin!(work);
    let timer = std::pin::pin!(pause(bound));
    match futures::future::select(work, timer).await {
        futures::future::Either::Left((answered, _)) => Err(TestError::new(format!(
            "answered while its role stood stopped: {answered:?}"
        ))),
        futures::future::Either::Right(_) => Ok(()),
    }
}

#[then(
    "the registry evicts the next oldest session instead, and the oldest only once its partition has answered the Commit"
)]
fn next_oldest(w: &mut Steps) -> TestResult {
    assert!(w.committed, "{:?} {}", w.seen.facts, w.over());
    release(w)
}

/// A read of the fixture, its first batch taken, in a read-only transaction, its stream's credit spent.
async fn open_big_read(
    client: &mut TestClient,
    deadline_ms: u32,
) -> TestResult<(
    ultragraphdb_client_compio::Transaction,
    ultragraphdb_client_compio::ResultStream,
    usize,
)> {
    let begin = Begin::new(TENANT, 0, true).with_deadline_ms(deadline_ms);
    let reader = client.begin_with(begin, None).await?;
    let stream = client
        .read(&reader, &big_pattern().read(&reader, 0, Vec::new())?)
        .await?;
    let (rows, _) = client
        .next_batch(&stream, batch_of)
        .await?
        .context("the read's first batch")?;
    Ok((reader, stream, rows.len()))
}

/// The fixture written, its every row closed for a snapshot's read.
async fn big_held(client: &mut TestClient) -> TestResult {
    write_big(client).await?;
    let _closed = until_rows(client, &big_pattern(), 14).await?;
    Ok(())
}

#[when("two compute processes have joined and a connection sends two reads")]
fn two_reads_two_computes(w: &mut Steps) -> TestResult {
    own(w, &two_computes())?;
    let at = cluster(w)?.client.clone();
    let [a, p, b] = ["a", "p", "b"].map(|name| w.iri(name));
    // One of the two is stopped: the read it receives goes unanswered, so each read tells which process it reached.
    cluster(w)?.signal("compute_2", Signal::Stop)?;
    let outcomes = with_client(&at, async |client| {
        committed(client, &a, &p, &iri(&b)).await?;
        let mut outcomes = Vec::new();
        for _ in 0..2 {
            let begin = Begin::new(TENANT, 0, true).with_deadline_ms(50);
            let reader = client.begin_with(begin, None).await?;
            let pattern = Pattern::of(Some(&a), Some(&p));
            let stream = client
                .read(&reader, &pattern.read(&reader, 0, Vec::new())?)
                .await?;
            outcomes.push(
                drain(client, &stream)
                    .await
                    .map_err(|error| error.to_string()),
            );
        }
        Ok(outcomes)
    })?;
    cluster(w)?.signal("compute_2", Signal::Continue)?;
    w.seen.facts = outcomes
        .iter()
        .map(|outcome| format!("{outcome:?}"))
        .collect();
    let answered = outcomes.iter().filter(|outcome| outcome.is_ok()).count();
    w.committed = answered == 1;
    Ok(())
}

#[then("each process receives one")]
fn each_receives_one(w: &mut Steps) -> TestResult {
    assert!(
        w.committed,
        "one read answered, one hung: {:?} {}",
        w.seen.facts,
        w.over()
    );
    release(w)
}

#[when(
    "the connection to one of two compute processes closes under a read before any frame of it reached the client"
)]
fn closed_compute(w: &mut Steps) -> TestResult {
    own(w, &two_computes())?;
    let at = cluster(w)?.client.clone();
    let [a, p, b] = ["a", "p", "b"].map(|name| w.iri(name));
    let pattern = Pattern::of(Some(&a), Some(&p));
    let held = with_client(&at, async |client| {
        committed(client, &a, &p, &iri(&b)).await?;
        until_rows(client, &pattern, 1).await
    })?;
    cluster(w)?.signal("compute_2", Signal::Kill)?;
    let read = with_client(&at, async |client| {
        let mut answers = vec![held];
        for _ in 0..4 {
            let reader = client.begin(TENANT, true).await?;
            answers.push(read_whole(client, &reader, &pattern).await?);
            client.abort(reader).await?;
        }
        Ok(answers)
    })?;
    w.seen.answers = read;
    Ok(())
}

#[then("the frontend sends the read to the other, and the client receives its rows")]
fn other_compute_answers(w: &mut Steps) -> TestResult {
    assert_eq!(w.seen.answers.len(), 5, "{}", w.over());
    for answer in &w.seen.answers {
        assert_eq!(answer.column(0), [w.iri("b")], "{}", w.over());
    }
    release(w)
}

#[when("no connection to a compute process can be opened for a read")]
fn no_compute(w: &mut Steps) -> TestResult {
    own(w, &stalling())?;
    let at = cluster(w)?.client.clone();
    let [a, p] = ["a", "p"].map(|name| w.iri(name));
    cluster(w)?.signal("compute", Signal::Kill)?;
    let (refused, open) = with_client(&at, async |client| {
        let reader = client.begin(TENANT, true).await?;
        let pattern = Pattern::of(Some(&a), Some(&p));
        let stream = client
            .read(&reader, &pattern.read(&reader, 0, Vec::new())?)
            .await?;
        let refused = client
            .next_batch(&stream, batch_of)
            .await
            .err()
            .context("a refusal")?;
        Ok((refused, client.begin(TENANT, true).await.is_ok()))
    })?;
    w.seen.refusals = vec![refused];
    w.committed = open;
    Ok(())
}

#[then("the client receives code 17, and its connection stays open")]
fn seventeen_open(w: &mut Steps) -> TestResult {
    refused_all(w, ErrorCode::Unavailable, 1)?;
    assert!(
        w.committed,
        "the connection served the next request {}",
        w.over()
    );
    release(w)
}

#[when("a compute process's connection closes after one frame of a read reached the client")]
fn compute_closes_after_a_frame(w: &mut Steps) -> TestResult {
    own(w, &two_computes())?;
    let at = cluster(w)?.client.clone();
    let mut held = run(async {
        let mut held = client(&at).await?;
        big_held(&mut held).await?;
        Ok(held)
    })?;
    let opened = run(async {
        let first = open_big_read(&mut held, 0).await?;
        let second = open_big_read(&mut held, 0).await?;
        Ok((first, second))
    })?;
    cluster(w)?.signal("compute_2", Signal::Kill)?;
    let outcomes = run(async {
        let ((_, one, _), (_, two, _)) = opened;
        Ok(vec![
            drain(&mut held, &one)
                .await
                .map_err(|error| error.to_string()),
            drain(&mut held, &two)
                .await
                .map_err(|error| error.to_string()),
            Ok(Answer {
                rows: vec![vec![held.begin(TENANT, true).await.is_ok().to_string()]],
                resume: Vec::new(),
            }),
        ])
    })?;
    w.seen.facts = outcomes
        .iter()
        .map(|outcome| match outcome {
            Ok(answer) => format!("rows {}", answer.rows.len()),
            Err(refused) => refused.clone(),
        })
        .collect();
    Ok(())
}

#[then(
    "the read is refused with code 17, no other compute process receives it, and the client's connection stays open"
)]
fn never_resent(w: &mut Steps) -> TestResult {
    let unavailable = format!("{:?}", ErrorCode::Unavailable);
    let refused = w
        .seen
        .facts
        .iter()
        .filter(|fact| fact.contains(&unavailable))
        .count();
    assert_eq!(
        refused,
        1,
        "one read refused: {:?} {}",
        w.seen.facts,
        w.over()
    );
    assert_eq!(
        w.seen.facts.last().map(String::as_str),
        Some("rows 1"),
        "{:?}",
        w.seen.facts
    );
    release(w)
}

#[when(
    "a compute process accepts a read of `deadline_ms` 50 and sends nothing, `scan_stall_ms` is 100, and the read's stream holds credit"
)]
fn hung_compute(w: &mut Steps) -> TestResult {
    own(w, &stalling())?;
    let at = cluster(w)?.client.clone();
    let [a, p] = ["a", "p"].map(|name| w.iri(name));
    let mut held = run(async { client(&at).await })?;
    cluster(w)?.signal("compute", Signal::Stop)?;
    let (refused, waited) = run(async {
        let begin = Begin::new(TENANT, 0, true).with_deadline_ms(50);
        let reader = held.begin_with(begin, None).await?;
        let pattern = Pattern::of(Some(&a), Some(&p));
        let started = clock();
        let stream = held
            .read(&reader, &pattern.read(&reader, 0, Vec::new())?)
            .await?;
        let refused = held
            .next_batch(&stream, batch_of)
            .await
            .err()
            .context("a refusal")?;
        Ok((refused, clock().duration_since(started)))
    })?;
    cluster(w)?.signal("compute", Signal::Continue)?;
    w.seen.refusals = vec![refused];
    w.seen.facts = vec![waited.as_millis().to_string()];
    Ok(())
}

#[then(
    "the frontend cancels the ComputeRead and answers the read with code 17 once 150 ms have passed, and no other compute process receives it"
)]
fn hung_answered(w: &mut Steps) -> TestResult {
    refused_all(w, ErrorCode::Unavailable, 1)?;
    let waited: u64 = w.seen.facts.first().context("how long")?.parse()?;
    assert!(waited >= 150, "answered after {waited} ms {}", w.over());
    release(w)
}

#[when(
    "a read of `deadline_ms` 40,000 waits 35 s for a close, and compute sends its first frame once the close arrives"
)]
fn waiting_within_deadline(w: &mut Steps) -> TestResult {
    own(w, &slow_epochs())?;
    let at = cluster(w)?.client.clone();
    let [a, p, b] = ["a", "p", "b"].map(|name| w.iri(name));
    w.seen.answers = with_client(&at, async |client| {
        let session = commit_early_in_an_epoch(client, (&a, &p, &b)).await?;
        let begin = Begin::new(TENANT, 0, true).with_deadline_ms(3 * SLOW_EPOCH_MS as u32);
        let reader = client.begin_with(begin, session).await?;
        let started = clock();
        let answer = read_whole(client, &reader, &Pattern::of(Some(&a), Some(&p))).await?;
        let waited = clock().duration_since(started).as_millis();
        if waited <= u128::from(STALL_MS) {
            return Err(TestError::new(format!(
                "the read waited {waited} ms, no longer than the stall"
            )));
        }
        Ok(vec![answer])
    })?;
    Ok(())
}

#[then("the client receives the rows")]
fn rows_after_the_wait(w: &mut Steps) -> TestResult {
    let answer = w.seen.answers.first().context("the answer")?;
    assert!(
        answer.column(0).contains(&w.iri("b")),
        "{answer:?} {}",
        w.over()
    );
    release(w)
}

#[when(
    "a read's stream has spent its credit, and its client takes no batch for longer than `scan_stall_ms`"
)]
fn no_credit_past_stall(w: &mut Steps) -> TestResult {
    own(w, &stalling())?;
    let at = cluster(w)?.client.clone();
    let (rows, rest) = with_client(&at, async |client| {
        big_held(client).await?;
        let (_, stream, rows) = open_big_read(client, 0).await?;
        pause(Duration::from_millis(4 * STALL_MS)).await;
        Ok((rows, drain(client, &stream).await?))
    })?;
    w.seen.facts = vec![(rows + rest.rows.len()).to_string()];
    Ok(())
}

#[then("the frontend does not answer it with code 17 on that account")]
fn no_seventeen(w: &mut Steps) -> TestResult {
    assert_eq!(w.seen.facts, ["14"], "every row arrived {}", w.over());
    release(w)
}

#[when(
    "`held_bytes` is 1,024, a read of a transaction is open, and the client sends Writes that the read holds back past 1,024 bytes"
)]
fn past_held_bytes(w: &mut Steps) -> TestResult {
    own(
        w,
        &Spec::one_partition().frontend("endpoint.held_bytes", 1024),
    )?;
    let at = cluster(w)?.client.clone();
    let [a, p] = ["a", "p"].map(|name| w.iri(name));
    let (refused, next) = with_client(&at, async |client| {
        big_held(client).await?;
        let own = client.begin(TENANT, false).await?;
        let stream = client
            .read(&own, &big_pattern().read(&own, 0, Vec::new())?)
            .await?;
        let _first = client
            .next_batch(&stream, batch_of)
            .await?
            .context("a batch")?;
        for number in 0..8 {
            let long = "y".repeat(400);
            client
                .write(
                    &own,
                    &write_of(&format!("{a}{number}"), &p, &string(&long))?,
                )
                .await?;
        }
        let refused = client.commit(own, None).await.err().context("a refusal")?;
        Ok((refused, client.commit(own, None).await.err()))
    })?;
    w.seen.refusals = vec![refused];
    w.seen.facts = vec![format!("{next:?}")];
    Ok(())
}

#[then(
    "the Write past the bound is refused with code 15, `detail` 1,024, on the transaction's stream, and the transaction ends"
)]
fn past_held_refused(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    let (code, detail, _) = refusal_of(refused)?;
    assert_eq!(code, ErrorCode::StreamLimit, "{refused:?}");
    assert_eq!(detail, 1024, "{refused:?}");
    let next = w.seen.facts.first().context("the next request")?;
    assert!(
        next.starts_with("Some"),
        "the transaction ended: {next} {}",
        w.over()
    );
    release(w)
}

#[when("a read holds Writes back, and the client grants the read credit")]
fn credit_drains(w: &mut Steps) -> TestResult {
    let [a, p] = ["a", "p"].map(|name| w.iri(name));
    let (rows, seen) = crate::transaction_steps::in_shared(w, async |client| {
        crate::read_steps::holding_big(client).await?;
        let own = client.begin(TENANT, false).await?;
        let stream = client
            .read(&own, &big_pattern().read(&own, 0, Vec::new())?)
            .await?;
        client
            .write(&own, &write_of(&a, &p, &string("held"))?)
            .await?;
        let drained = drain(client, &stream).await?;
        let seen = read_whole(client, &own, &Pattern::of(Some(&a), Some(&p))).await?;
        client.abort(own).await?;
        Ok((drained.rows.len(), seen))
    })?;
    w.seen.facts = vec![rows.to_string()];
    w.seen.answers = vec![seen];
    Ok(())
}

#[then(
    "the frontend reads the Credit and relays it, the read reaches its `LAST`, and the held Writes are then sent"
)]
fn credit_drained(w: &mut Steps) -> TestResult {
    let rows: usize = w.seen.facts.first().context("the rows")?.parse()?;
    assert!(rows >= 14, "{rows} {}", w.over());
    let seen = w.seen.answers.first().context("the write")?;
    assert_eq!(seen.column(0), ["held"], "{}", w.over());
    Ok(())
}

/// The cluster of two storage processes, and the subject in each's partition.
fn two_storages(w: &mut Steps, spec: Spec) -> TestResult<(String, String)> {
    own(w, &spec.second_storage())?;
    let ranges = placement_ranges(&cluster(w)?.frontend)?;
    Ok((
        iri_of_partition(&ranges, &w.iri("first/"), 1)?,
        iri_of_partition(&ranges, &w.iri("second/"), 2)?,
    ))
}

/// That `answer` left its request in doubt with code 17 as its cause.
fn in_doubt_by_seventeen<T: std::fmt::Debug>(answer: &Result<T, ClientError>) -> bool {
    matches!(answer, Err(ClientError(ClientErrorEnum::InDoubt { cause: Some(cause), .. }))
        if refusal_of(cause).is_ok_and(|(code, _, _)| code == ErrorCode::Unavailable))
}

#[when(
    "the connection between the frontend and a storage process closes under a Write and a Commit"
)]
fn storage_closes(w: &mut Steps) -> TestResult {
    let (first, second) = two_storages(w, Spec::one_partition())?;
    let at = cluster(w)?.client.clone();
    let p = iri_of_partition(&placement_ranges(&cluster(w)?.frontend)?, &w.iri("p/"), 1)?;
    let mut held = run(async { client(&at).await })?;
    let (own, session) = run(async {
        // A partition's first write of a tenant gives it its initial catalog, which needs `rdf:type` from that term's home, so
        // this one is written while every home answers.
        let _first = held
            .direct_write(direct_of(&first, &p, &iri(&first))?, None)
            .await?;
        let session = held.register().await?;
        let own = held.begin(TENANT, false).await?;
        held.write(&own, &write_of(&second, &p, &string("x"))?)
            .await?;
        // A read of the transaction answered says the Write has reached its partition.
        let _bound = read_whole(&mut held, &own, &Pattern::of(Some(&second), Some(&p))).await?;
        Ok((own, session))
    })?;
    cluster(w)?.signal("storage_2", Signal::Kill)?;
    let (commit, other) = run(async {
        let commit = held.commit(own, session).await;
        let other = held
            // Every term of this partition's: a term homed in the partition gone would wait for it.
            .direct_write(direct_of(&first, &p, &iri(&first))?, None)
            .await;
        Ok((commit, other))
    })?;
    w.committed = in_doubt_by_seventeen(&commit);
    w.seen.facts = vec![format!("{commit:?}"), format!("{other:?}")];
    w.seen.commits = other.into_iter().collect();
    Ok(())
}

#[then(
    "the client receives code 17 for each, the Commit is in doubt, and requests to other partitions still succeed"
)]
fn seventeen_in_doubt(w: &mut Steps) -> TestResult {
    assert!(w.committed, "{:?} {}", w.seen.facts, w.over());
    assert_eq!(w.seen.commits.len(), 1, "{:?} {}", w.seen.facts, w.over());
    release(w)
}

#[when("a client's Commit through a session is answered code 17")]
fn commit_seventeen(w: &mut Steps) -> TestResult {
    let (_, second) = two_storages(w, Spec::one_partition())?;
    let at = cluster(w)?.client.clone();
    let p = w.iri("p");
    let mut held = run(async { client(&at).await })?;
    let (own, session) = run(async {
        let session = held.register().await?;
        let own = held.begin(TENANT, false).await?;
        held.write(&own, &write_of(&second, &p, &string("x"))?)
            .await?;
        let _bound = read_whole(&mut held, &own, &Pattern::of(Some(&second), Some(&p))).await?;
        Ok((own, session))
    })?;
    cluster(w)?.signal("storage_2", Signal::Kill)?;
    let (commit, next) = run(async {
        let commit = held.commit(own, session).await;
        let another = held.begin(TENANT, false).await?;
        held.write(&another, &write_of(&second, &p, &string("y"))?)
            .await?;
        let next = held.commit(another, session).await;
        Ok((commit, next))
    })?;
    w.committed = in_doubt_by_seventeen(&commit)
        && matches!(
            &next,
            Err(ClientError(
                ClientErrorEnum::InDoubt { .. } | ClientErrorEnum::SessionBusy { .. }
            ))
        );
    w.seen.facts = vec![format!("{commit:?}"), format!("{next:?}")];
    Ok(())
}

#[then(
    "the client answers `InDoubt`, keeps the request number, and sends no other commit through the session until a resend resolves it"
)]
fn doubt_holds_the_session(w: &mut Steps) -> TestResult {
    assert!(w.committed, "{:?} {}", w.seen.facts, w.over());
    release(w)
}

#[when("a read is refused with code 17, and a Commit with code 17")]
fn both_seventeen(w: &mut Steps) -> TestResult {
    let (_, second) = two_storages(w, Spec::one_partition())?;
    let at = cluster(w)?.client.clone();
    let p = w.iri("p");
    let mut held = run(async { client(&at).await })?;
    let (own, session) = run(async {
        let session = held.register().await?;
        let own = held.begin(TENANT, false).await?;
        held.write(&own, &write_of(&second, &p, &string("x"))?)
            .await?;
        let _bound = read_whole(&mut held, &own, &Pattern::of(Some(&second), Some(&p))).await?;
        Ok((own, session))
    })?;
    cluster(w)?.signal("compute", Signal::Kill)?;
    let read_refused = run(async {
        let reader = held.begin(TENANT, true).await?;
        let pattern = Pattern::of(Some(&second), Some(&p));
        let stream = held
            .read(&reader, &pattern.read(&reader, 0, Vec::new())?)
            .await?;
        held.next_batch(&stream, batch_of)
            .await
            .err()
            .context("a refusal")
    })?;
    cluster(w)?.signal("storage_2", Signal::Kill)?;
    let commit_refused = run(async { held.commit(own, session).await.err().context("a refusal") })?;
    w.seen.refusals = vec![read_refused, commit_refused];
    Ok(())
}

#[then(
    "the client's error says of the first that the read runs again, and of the second that it is in doubt"
)]
fn says_what_runs_again(w: &mut Steps) -> TestResult {
    let [read, commit] = w.seen.refusals.as_slice() else {
        return Err(TestError::new("two refusals"));
    };
    assert_eq!(
        read.runs_again(),
        Some(ultragraphdb_client_compio::RunsAgain::Read),
        "{read:?}"
    );
    assert!(
        matches!(&commit.0, ClientErrorEnum::InDoubt { .. }),
        "{commit:?}"
    );
    release(w)
}

#[when(
    "a DirectWrite through a session is answered code 17, the partition had not committed it, and the client sends it again"
)]
fn direct_resent(w: &mut Steps) -> TestResult {
    let (_, second) = two_storages(w, Spec::one_partition())?;
    let at = cluster(w)?.client.clone();
    let p = w.iri("p");
    let mut held = run(async { client(&at).await })?;
    let session = run(async { Ok(held.register().await?) })?;
    cluster(w)?.signal("storage_2", Signal::Kill)?;
    let answered = run(async {
        Ok(held
            .direct_write(direct_of(&second, &p, &string("x"))?, session)
            .await)
    })?;
    w.seen.facts = vec![format!("{answered:?}")];
    let in_doubt = in_doubt_by_seventeen(&answered);
    if let Some(running) = w.cluster.as_mut() {
        running.restart("storage_2")?;
    }
    let (resolved, seen) = run(async {
        let resolved = resolve_within(&mut held, session).await?;
        Ok((
            resolved,
            latest_read(&mut held, &Pattern::of(Some(&second), Some(&p))).await?,
        ))
    })?;
    w.committed = in_doubt;
    w.seen.commits = vec![resolved];
    w.seen.answers = vec![seen];
    Ok(())
}

/// Resolves `session`'s request in doubt, sending it again while the partition is coming back, within what a restart takes.
async fn resolve_within(
    client: &mut TestClient,
    session: ultragraphdb_client_compio::Session,
) -> TestResult<ultragraphdb_client_compio::Committed> {
    let mut last = None;
    for _ in 0..200 {
        match client.resolve(session).await {
            Ok(committed) => return Ok(committed),
            Err(error) => {
                if !matches!(&error.0, ClientErrorEnum::InDoubt { .. }) {
                    return Err(TestError::new(format!("the resend: {error:?}")));
                }
                last = Some(error);
            }
        }
        pause(Duration::from_millis(50)).await;
    }
    Err(TestError::new(format!(
        "the resend stayed in doubt for 10 s: {last:?}"
    )))
}

#[then(
    "the frontend relays it, not recalls it, the partition commits it, and the reply carries its commit version"
)]
fn direct_resend_committed(w: &mut Steps) -> TestResult {
    assert!(w.committed, "{:?} {}", w.seen.facts, w.over());
    let resolved = w.seen.commits.first().context("the reply")?;
    assert!(resolved.version() > 0, "{resolved:?}");
    let seen = w.seen.answers.first().context("what is held")?;
    assert_eq!(seen.column(0), ["x"], "{}", w.over());
    release(w)
}

#[when(
    "a retried Commit is recalled over two partitions, partition 1 answers 18 and the role connection to partition 2 closes under the Recall"
)]
fn recall_reaching_no_answer(w: &mut Steps) -> TestResult {
    let (first, _) = two_storages(w, Spec::one_partition())?;
    let at = cluster(w)?.client.clone();
    let p = w.iri("p");
    let link = LossyLink::to(&at)?;
    let mut held = run(async {
        let mut client = TestClient::new(&link.client_at(), &Knobs::one_attempt())?;
        client.connect().await?;
        Ok(client)
    })?;
    let session = run(async {
        let session = held.register().await?;
        let own = held.begin(TENANT, false).await?;
        held.write(&own, &write_of(&first, &p, &string("x"))?)
            .await?;
        link.lose_request(Command::Commit);
        let _doubt = held.commit(own, session).await.err().context("in doubt")?;
        Ok(session)
    })?;
    cluster(w)?.signal("storage_2", Signal::Kill)?;
    let resolved = run(async { Ok(held.resolve(session).await) })?;
    w.committed = in_doubt_by_seventeen(&resolved);
    w.seen.facts = vec![format!("{resolved:?}")];
    Ok(())
}

#[then("the frontend answers code 17, and the client resends after a delay")]
fn recall_in_doubt(w: &mut Steps) -> TestResult {
    assert!(w.committed, "{:?} {}", w.seen.facts, w.over());
    release(w)
}

#[when(
    "a read-only transaction begins with a token of epoch 30 while epoch 29 is the latest closed, reads, and epoch 30 closes"
)]
fn future_token(w: &mut Steps) -> TestResult {
    own(w, &slow_epochs())?;
    let at = cluster(w)?.client.clone();
    let [a, p, b] = ["a", "p", "b"].map(|name| w.iri(name));
    w.seen.answers = with_client(&at, async |client| {
        let session = commit_early_in_an_epoch(client, (&a, &p, &b)).await?;
        let reader = client
            .begin_with(Begin::new(TENANT, 0, true), session)
            .await?;
        Ok(vec![
            read_whole(client, &reader, &Pattern::of(Some(&a), Some(&p))).await?,
        ])
    })?;
    Ok(())
}

#[then("the read answers after epoch 30 closes, at that epoch or later")]
fn future_token_answered(w: &mut Steps) -> TestResult {
    let answer = w.seen.answers.first().context("the answer")?;
    assert!(
        answer.column(0).contains(&w.iri("b")),
        "{answer:?} {}",
        w.over()
    );
    release(w)
}

#[when(
    "a read-only transaction begins with a token of epoch 30 and a deadline of 50 ms, and epoch 30 does not close within it"
)]
fn token_past_deadline(w: &mut Steps) -> TestResult {
    own(w, &slow_epochs())?;
    let at = cluster(w)?.client.clone();
    let [a, p, b] = ["a", "p", "b"].map(|name| w.iri(name));
    let refused = with_client(&at, async |client| {
        let session = commit_early_in_an_epoch(client, (&a, &p, &b)).await?;
        let begin = Begin::new(TENANT, 0, true).with_deadline_ms(50);
        let reader = client.begin_with(begin, session).await?;
        let pattern = Pattern::of(Some(&a), Some(&p));
        let stream = client
            .read(&reader, &pattern.read(&reader, 0, Vec::new())?)
            .await?;
        client
            .next_batch(&stream, batch_of)
            .await
            .err()
            .context("a refusal")
    })?;
    w.seen.refusals = vec![refused];
    Ok(())
}

#[then("its read is refused as timed out")]
fn deadline_passed(w: &mut Steps) -> TestResult {
    refused_all(w, ErrorCode::TimedOut, 1)?;
    release(w)
}

#[when(
    "a read-only transaction's deadline is 40 s, and its read waits 35 s for a close, the client's timeout set at 30 s"
)]
fn long_deadline(w: &mut Steps) -> TestResult {
    own(w, &slow_epochs())?;
    let running = cluster(w)?;
    let at = running.client.clone();
    let [a, p, b] = ["a", "p", "b"].map(|name| w.iri(name));
    w.seen.answers = run(async {
        let token = with_committed(&at, (&a, &p, &b)).await?;
        let mut client = TestClient::new(&at, &Knobs::timing_out(LOGIN_TIMEOUT))?;
        client.connect().await?;
        let begin = Begin::new(TENANT, 0, true)
            .with_deadline_ms(4 * LOGIN_TIMEOUT.as_millis() as u32)
            .with_token(token);
        let reader = client.begin_with(begin, None).await?;
        // A stopped partition closes nothing, so the read waits past the client's timeout on a close.
        running.signal("storage", Signal::Stop)?;
        let pattern = Pattern::of(Some(&a), Some(&p));
        let close_later = async {
            pause(LOGIN_TIMEOUT + Duration::from_millis(500)).await;
            running.signal("storage", Signal::Continue)
        };
        let (answer, gone_on) =
            futures::join!(read_whole(&mut client, &reader, &pattern), close_later);
        gone_on?;
        let answer = answer?;
        let _still = client.begin(TENANT, true).await?;
        Ok(vec![answer])
    })?;
    Ok(())
}

/// A commit early in an epoch of a slow cluster, by a client of its own: its token.
async fn with_committed(
    at: &crate::shells::ClientAt,
    terms: (&str, &str, &str),
) -> TestResult<ultragraphdb_core_ids::SessionToken> {
    let mut client = client(at).await?;
    let session = commit_early_in_an_epoch(&mut client, terms).await?;
    client
        .session_token(session)
        .cloned()
        .context("the session's token")
}

#[then("the connection stays open, and the read answers once the close arrives")]
fn survived_timeout(w: &mut Steps) -> TestResult {
    let answer = w.seen.answers.first().context("the answer")?;
    assert!(
        answer.column(0).contains(&w.iri("b")),
        "{answer:?} {}",
        w.over()
    );
    release(w)
}

#[when(
    "a read's stream has spent its credit, and its caller takes no batch for longer than the timeout"
)]
fn no_credit_past_timeout(w: &mut Steps) -> TestResult {
    own(w, &Spec::one_partition())?;
    let at = cluster(w)?.client.clone();
    let (rows, rest) = with_client(&at, async |held| {
        big_held(held).await?;
        let mut client = TestClient::new(&at, &Knobs::timing_out(LOGIN_TIMEOUT))?;
        client.connect().await?;
        let (_, stream, rows) = open_big_read(&mut client, 0).await?;
        pause(LOGIN_TIMEOUT + Duration::from_millis(500)).await;
        Ok((rows, drain(&mut client, &stream).await?))
    })?;
    w.seen.facts = vec![(rows + rest.rows.len()).to_string()];
    Ok(())
}

#[then("the connection stays open")]
fn connection_open(w: &mut Steps) -> TestResult {
    assert_eq!(w.seen.facts, ["14"], "every row arrived {}", w.over());
    release(w)
}

/// A cluster retaining a closed epoch for `read_retain_ms`.
fn retaining(read_retain_ms: u64) -> Spec {
    Spec::one_partition().storage("storage.read_retain_ms", read_retain_ms)
}

/// A read-only read of `<a> <p> ?o` at snapshot level, then after `wait` and a commit, the same transaction's next read.
async fn read_again_after(
    client: &mut TestClient,
    (a, p): (&str, &str),
    wait: Duration,
) -> TestResult<(Answer, Result<Answer, TestError>)> {
    let pattern = Pattern::of(Some(a), Some(p));
    committed(client, a, p, &string("before")).await?;
    let _closed = until_rows(client, &pattern, 1).await?;
    let reader = client.begin(TENANT, true).await?;
    let first = read_whole(client, &reader, &pattern).await?;
    committed(client, a, p, &string("after")).await?;
    pause(wait).await;
    Ok((first, read_whole(client, &reader, &pattern).await))
}

#[when(
    "a read-only transaction at snapshot level reads at epoch 20, epochs close for 30 s with `read_retain_ms` 60,000, and it reads again"
)]
fn back_within_retention(w: &mut Steps) -> TestResult {
    own(w, &Spec::one_partition())?;
    let at = cluster(w)?.client.clone();
    let [a, p] = ["a", "p"].map(|name| w.iri(name));
    let (first, second) = with_client(&at, async |client| {
        read_again_after(client, (&a, &p), Duration::from_millis(200)).await
    })?;
    w.seen.answers = vec![first, second?];
    Ok(())
}

#[then("the second read answers at epoch 20's version bound")]
fn same_version(w: &mut Steps) -> TestResult {
    let [first, second] = w.seen.answers.as_slice() else {
        return Err(TestError::new("two answers"));
    };
    assert_eq!(first.rows, second.rows, "{}", w.over());
    release(w)
}

#[when(
    "a read-only transaction at snapshot level reads at epoch 20, the partition raises its discard version past that version bound once a later epoch has been closed for more than `read_retain_ms`, and it reads again"
)]
fn back_past_retention(w: &mut Steps) -> TestResult {
    own(w, &retaining(100))?;
    let at = cluster(w)?.client.clone();
    let [a, p] = ["a", "p"].map(|name| w.iri(name));
    let second = with_client(&at, async |client| {
        let (_, second) = read_again_after(client, (&a, &p), Duration::from_millis(1_500)).await?;
        Ok(second)
    })?;
    w.seen.facts = vec![format!("{second:?}")];
    w.committed = second.is_err_and(|error| error.to_string().contains("TooOld"));
    Ok(())
}

#[then("the read is refused with code 3")]
fn too_old(w: &mut Steps) -> TestResult {
    assert!(w.committed, "{:?} {}", w.seen.facts, w.over());
    release(w)
}

#[when("a reader resumes with a key at a named version the partition has discarded")]
fn resumed_past_retention(w: &mut Steps) -> TestResult {
    own(w, &retaining(100))?;
    let at = cluster(w)?.client.clone();
    let [a, p] = ["a", "p"].map(|name| w.iri(name));
    let resumed = with_client(&at, async |client| {
        for number in 0..3 {
            committed(client, &a, &p, &iri(&format!("{a}/{number}"))).await?;
        }
        let pattern = Pattern::of(Some(&a), Some(&p));
        let _closed = until_rows(client, &pattern, 3).await?;
        let reader = client.begin(TENANT, true).await?;
        let page = read(client, &reader, &pattern, (1, Vec::new())).await?;
        committed(client, &a, &p, &iri(&format!("{a}/later"))).await?;
        pause(Duration::from_millis(1_500)).await;
        let again = client
            .begin_with(Begin::new(TENANT, reader.read_version(), true), None)
            .await?;
        Ok(read(client, &again, &pattern, (0, page.resume)).await)
    })?;
    w.seen.facts = vec![format!("{resumed:?}")];
    w.committed = resumed.is_err_and(|error| error.to_string().contains("TooOld"));
    Ok(())
}

#[when("a Scan keeps taking frames for longer than `read_retain_ms` while later epochs close")]
fn open_scan_holds(w: &mut Steps) -> TestResult {
    own(w, &retaining(100))?;
    let at = cluster(w)?.client.clone();
    let (rows, rest) = with_client(&at, async |client| {
        big_held(client).await?;
        let (_, stream, rows) = open_big_read(client, 0).await?;
        pause(Duration::from_millis(1_000)).await;
        Ok((rows, drain(client, &stream).await?))
    })?;
    w.seen.facts = vec![(rows + rest.rows.len()).to_string()];
    Ok(())
}

#[then(
    "the discard version stays at or below the Scan's version, and the Scan reaches `LAST` at that version"
)]
fn scan_reached_last(w: &mut Steps) -> TestResult {
    assert_eq!(w.seen.facts, ["14"], "every row arrived {}", w.over());
    release(w)
}

/// One exchange of the login these scenarios use.
type ScramMechanism = <ScramLogin<fn() -> Result<String, ScramError>> as Login>::Mechanism;

/// A login that keeps the salt and iterations of each challenge it answers.
#[derive(Debug, Clone)]
struct KeepingChallenges {
    inner: ScramLogin<fn() -> Result<String, ScramError>>,
    challenges: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
}

impl Login for KeepingChallenges {
    type Mechanism = Keeping;

    fn mechanism(&mut self, offered: &[String]) -> Result<Option<Keeping>, TransportError> {
        Ok(self.inner.mechanism(offered)?.map(|inner| Keeping {
            inner,
            challenges: self.challenges.clone(),
        }))
    }
}

/// A SCRAM exchange that keeps what the server's first message says of the name.
#[derive(Debug)]
struct Keeping {
    inner: ScramMechanism,
    challenges: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
}

impl ClientMechanism for Keeping {
    type Error = <ScramMechanism as ClientMechanism>::Error;

    fn name(&self) -> &str {
        self.inner.name()
    }

    fn start(&mut self) -> Result<Vec<u8>, Self::Error> {
        self.inner.start()
    }

    fn answer(&mut self, challenge: &[u8]) -> Result<Vec<u8>, Self::Error> {
        // `r=<nonce>,s=<salt>,i=<iterations>`: the nonce is the client's own and changes.
        let text = String::from_utf8_lossy(challenge);
        let told: Vec<&str> = text
            .split(',')
            .filter(|part| !part.starts_with("r="))
            .collect();
        self.challenges.borrow_mut().push(told.join(","));
        self.inner.answer(challenge)
    }

    fn finish(&mut self, outcome: &[u8]) -> Result<(), Self::Error> {
        self.inner.finish(outcome)
    }
}

/// Asks the endpoint at `at` to authenticate `name`, keeping the challenge: why it was refused.
async fn challenged(
    at: &str,
    name: &str,
    challenges: &std::rc::Rc<std::cell::RefCell<Vec<String>>>,
) -> TestResult<Option<ClientError>> {
    let nonces: fn() -> Result<String, ScramError> = system_nonce;
    let login = KeepingChallenges {
        inner: ScramLogin::new(name, "a guess", nonces)?,
        challenges: challenges.clone(),
    };
    let config = ultragraphdb_client_compio::ClientConfig::new(CLUSTER, "ultragraphdb-tests");
    let mut client = ultragraphdb_client_compio::Client::new(
        CompioDial::of(Address::parse(at)?),
        config,
        login,
        4,
    );
    Ok(client.connect().await.err())
}

#[when("a client asks to authenticate as an unknown name, twice, across a restart of the frontend")]
fn unknown_across_restart(w: &mut Steps) -> TestResult {
    own(w, &Spec::one_partition())?;
    // Storage and compute go with a frontend killed, so this cluster serves no scenario after this one.
    w.named = None;
    let at = cluster(w)?.client.address.clone();
    let challenges = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let (unknown, wrong) = compio::runtime::Runtime::new()?.block_on(async {
        let first = challenged(&at, "nobody-of-this-name", &challenges).await?;
        let wrong = TestClient::new(
            &crate::shells::ClientAt::clear_text(&at),
            &Knobs::logging_in(crate::shells::ADMIN, "not the password"),
        )?
        .connect()
        .await
        .err();
        TestResult::Ok((first, wrong))
    })?;
    if let Some(running) = w.cluster.as_mut() {
        running.signal("frontend", Signal::Kill)?;
        running.restart("frontend")?;
    }
    let again = compio::runtime::Runtime::new()?.block_on(async {
        for _ in 0..100 {
            let refused = challenged(&at, "nobody-of-this-name", &challenges).await?;
            if refused
                .as_ref()
                .is_some_and(|error| closing_code(error).is_some())
            {
                return TestResult::Ok(refused);
            }
            compio::time::sleep(Duration::from_millis(50)).await;
        }
        Err(TestError::new(
            "the restarted frontend answered no exchange within 5 s",
        ))
    })?;
    let told = challenges.borrow().clone();
    w.seen.facts = told;
    w.seen.refusals = [unknown, wrong, again].into_iter().flatten().collect();
    Ok(())
}

#[then(
    "both challenges carry the same salt and iteration count, and the exchange fails with the same authentication error as a wrong password"
)]
fn one_salt(w: &mut Steps) -> TestResult {
    let (first, rest) = w.seen.facts.split_first().context("a challenge")?;
    assert!(!rest.is_empty(), "challenges: {:?}", w.seen.facts);
    for again in rest {
        assert_eq!(again, first, "{}", w.over());
    }
    let [unknown, wrong, again] = w.seen.refusals.as_slice() else {
        return Err(TestError::new(format!(
            "three refusals: {:?}",
            w.seen.refusals
        )));
    };
    assert_eq!(
        closing_code(unknown),
        Some(ErrorCode::AuthenticationFailed),
        "{unknown:?}"
    );
    assert_eq!(unknown, wrong, "{}", w.over());
    assert_eq!(unknown, again, "{}", w.over());
    release(w)
}

#[when("a client drops a tenant's data after declaring `name` a property")]
fn tenant_data_dropped(w: &mut Steps) -> TestResult {
    own(w, &Spec::one_partition())?;
    let at = cluster(w)?.client.clone();
    let [a, name] = ["a", "name"].map(|n| w.iri(n));
    let (held, declared) = with_client(&at, async |client| {
        let name_declared = ultragraphdb_protocol_wire::DeclaredPredicate::new(&name)
            .with_kind(ultragraphdb_protocol_wire::DeclaredKind::Property);
        crate::endpoint_steps::declare(client, name_declared).await?;
        committed(client, &a, &name, &string("A")).await?;
        let session = client.register().await?;
        let dropped = client
            .drop_command(
                ultragraphdb_protocol_wire::DropCommand::tenant_data(TENANT),
                session,
            )
            .await?;
        let held = latest_read(client, &Pattern::of(Some(&a), Some(&name))).await?;
        // Describe reads at the latest close, which comes after a Commit's answer, so only one past the Drop's version speaks of it.
        for _ in 0..500 {
            let described = client.describe(TENANT).await?;
            if described.read_version() >= dropped.version() {
                let listed = described
                    .predicates()
                    .iter()
                    .any(|declared| declared.predicate() == name);
                return Ok((held, listed));
            }
            pause(Duration::from_millis(10)).await;
        }
        Err(TestError::new(format!(
            "no Describe reached the Drop's version {} within 5 s",
            dropped.version()
        )))
    })?;
    w.seen.answers = vec![held];
    w.committed = declared;
    Ok(())
}

#[then("no statement of the tenant remains, and Describe still lists `name`")]
fn data_gone_catalog_kept(w: &mut Steps) -> TestResult {
    nothing(w.seen.answers.first().context("what is held")?)?;
    assert!(w.committed, "Describe lists the declaration {}", w.over());
    release(w)
}

#[when(
    "a client of 4 commit attempts registers, and a full registry answers its first two Registers code 14"
)]
fn register_retried(w: &mut Steps) -> TestResult {
    own(
        w,
        &Spec::one_partition().frontend("metadata.sessions_max", 1),
    )?;
    let at = cluster(w)?.client.clone();
    let [a, p, b] = ["a", "p", "b"].map(|n| w.iri(n));
    let (mut first, mut second, session) = run(async {
        let mut first = client(&at).await?;
        let mut second = TestClient::new(&at, &Knobs::default())?;
        second.connect().await?;
        let session = first.register().await?;
        Ok((first, second, session))
    })?;
    cluster(w)?.signal("storage", Signal::Stop)?;
    let running = cluster(w)?;
    // The commit in flight fills the registry until the partition goes on, past the client's first two attempts, 50 ms apart.
    let registered = run(async {
        let free_later = async {
            pause(Duration::from_millis(120)).await;
            running.signal("storage", Signal::Continue)
        };
        let registering = async {
            pause(Duration::from_millis(20)).await;
            second.register().await
        };
        let (written, registered, freed) = futures::join!(
            first.direct_write(direct_of(&a, &p, &iri(&b))?, session),
            registering,
            free_later
        );
        let _written = written?;
        freed?;
        Ok(registered)
    })?;
    w.seen.facts = vec![format!("{registered:?}")];
    w.committed = registered.is_ok();
    Ok(())
}

#[then("the client sends the Register three times, each on a new stream, and returns the session")]
fn register_returned(w: &mut Steps) -> TestResult {
    assert!(w.committed, "{:?} {}", w.seen.facts, w.over());
    release(w)
}

#[when(
    "a client of 3 commit attempts registers, and a full registry answers every Register code 14"
)]
fn register_exhausted(w: &mut Steps) -> TestResult {
    own(
        w,
        &Spec::one_partition().frontend("metadata.sessions_max", 1),
    )?;
    let at = cluster(w)?.client.clone();
    let [a, p, b] = ["a", "p", "b"].map(|n| w.iri(n));
    let knobs = Knobs {
        commit_attempts: Some(3),
        ..Knobs::default()
    };
    let (mut first, mut second, session) = run(async {
        let mut first = client(&at).await?;
        let mut second = TestClient::new(&at, &knobs)?;
        second.connect().await?;
        let session = first.register().await?;
        Ok((first, second, session))
    })?;
    cluster(w)?.signal("storage", Signal::Stop)?;
    let running = cluster(w)?;
    let (busy, open) = run(async {
        let registering = async {
            pause(Duration::from_millis(50)).await;
            let busy = second.register().await;
            let open = second.begin(TENANT, true).await.is_ok();
            running.signal("storage", Signal::Continue)?;
            TestResult::Ok((busy, open))
        };
        let (written, outcome) = futures::join!(
            first.direct_write(direct_of(&a, &p, &iri(&b))?, session),
            registering
        );
        let _written = written?;
        outcome
    })?;
    w.seen.refusals = busy.err().into_iter().collect();
    w.committed = open;
    Ok(())
}

#[then(
    "the client sends the Register three times and returns code 14, and the connection stays open"
)]
fn register_refused(w: &mut Steps) -> TestResult {
    refused_all(w, ErrorCode::SessionBusy, 1)?;
    assert!(
        w.committed,
        "the connection served the next request {}",
        w.over()
    );
    release(w)
}

#[when(
    "a transaction's commit-time operations leave a Commit reply whose frame would pass 524,288 bytes"
)]
fn reply_past_floor(w: &mut Steps) -> TestResult {
    own(w, &Spec::one_partition())?;
    let at = cluster(w)?.client.clone();
    let [id, s] = ["id", "s"].map(|n| w.iri(n));
    let tail = "t".repeat(60_000);
    let (refused, version_before) = with_client(&at, async |client| {
        crate::endpoint_steps::declare(client, crate::request_steps::unique(&id)).await?;
        for number in 0..10 {
            let subject = format!("{s}{number}/{tail}");
            // A statement of a unique predicate goes in a transaction: a one-frame write of it is refused.
            let held = client.begin(TENANT, false).await?;
            client
                .write(
                    &held,
                    &write_of(&subject, &id, &string(&format!("v{number}")))?,
                )
                .await?;
            let _held = client.commit(held, None).await?;
        }
        let own = client.begin(TENANT, false).await?;
        for number in 0..10 {
            let mut terms = ultragraphdb_protocol_wire::TermTable::new();
            let operation = ultragraphdb_protocol_wire::CommitOperation::InsertIfAbsent {
                graph: None,
                predicate: terms.iri(&id),
                value: terms.intern(string(&format!("v{number}")))?,
                properties: Vec::new(),
            };
            client
                .operation(
                    &own,
                    &ultragraphdb_protocol_wire::Operation::new(terms, operation),
                )
                .await?;
        }
        let refused = client.commit(own, None).await.err().context("a refusal")?;
        Ok((refused, client.begin(TENANT, true).await.is_ok()))
    })?;
    w.seen.refusals = vec![refused];
    w.committed = version_before;
    Ok(())
}

#[then(
    "the commit is refused when it is decided, with code 6, `detail` 524,288, taking no commit version and writing nothing"
)]
fn reply_refused(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    let (code, detail, _) = refusal_of(refused)?;
    assert_eq!(code, ErrorCode::TooLarge, "{refused:?}");
    assert_eq!(
        detail,
        ultragraphdb_protocol_wire::MAX_FRAME_FLOOR as u64,
        "{refused:?}"
    );
    assert!(
        w.committed,
        "the connection served the next request {}",
        w.over()
    );
    release(w)
}

#[when(
    "reads A and C of a read-write transaction are open and a Write is held behind them, compute answers A with code 23, and the client then sends read D"
)]
fn code_23_answers_all(w: &mut Steps) -> TestResult {
    own(w, &Spec::over(2))?;
    let running = cluster(w)?;
    let at = running.client.clone();
    let ranges = placement_ranges(&running.frontend)?;
    let one = iri_of_partition(&ranges, &w.iri("one/"), 1)?;
    let (blob, p) = (w.iri("blob"), w.iri("p"));
    let fill = "x".repeat(400_000);
    let (own_transaction, c, a, mut held) = run(async {
        let mut held = client(&at).await?;
        for number in 0..14 {
            committed(&mut held, &one, &blob, &string(&format!("{number}{fill}"))).await?;
        }
        let transaction = held.begin(TENANT, false).await?;
        let of_c = Pattern::of(Some(&one), Some(&blob));
        let c = held
            .read(&transaction, &of_c.read(&transaction, 0, Vec::new())?)
            .await?;
        let _first = held
            .next_batch(&c, batch_of)
            .await?
            .context("C's first batch")?;
        // A stopped compute answers nothing, so A stays open while the Write comes to be held behind it.
        running.signal("compute", Signal::Stop)?;
        // Its subject unbound, so only compute, which finds the tenant spread, can refuse it.
        let of_a = Pattern::of(None, Some(&p));
        let a = held
            .read(&transaction, &of_a.read(&transaction, 0, Vec::new())?)
            .await?;
        held.write(&transaction, &write_of(&one, &p, &string("held"))?)
            .await?;
        pause(Duration::from_millis(100)).await;
        running.signal("compute", Signal::Continue)?;
        Ok((transaction, c, a, held))
    })?;
    let (refused, kept) = run(async {
        let refused_a = held
            .next_batch(&a, batch_of)
            .await
            .err()
            .context("A refused")?;
        let of_d = Pattern::of(None, Some(&p));
        let d = held
            .read(
                &own_transaction,
                &of_d.read(&own_transaction, 0, Vec::new())?,
            )
            .await?;
        let refused_d = held
            .next_batch(&d, batch_of)
            .await
            .err()
            .context("D refused")?;
        let refused_c = loop {
            match held.next_batch(&c, batch_of).await {
                Ok(Some(_)) => {}
                Ok(None) => return Err(TestError::new("C reached its LAST")),
                Err(error) => break error,
            }
        };
        let next = held
            .commit(own_transaction, None)
            .await
            .err()
            .context("the next request refused")?;
        let kept = latest_read(&mut held, &Pattern::of(Some(&one), Some(&p))).await?;
        Ok((vec![refused_a, refused_c, refused_d, next], kept))
    })?;
    w.seen.refusals = refused;
    w.seen.answers = vec![kept];
    Ok(())
}

#[then(
    "A, C and D are each answered code 23, the `detail` of A's, the Write is sent to no partition, and the next request on the transaction's stream is answered code 23 and ends it"
)]
fn code_23_answered(w: &mut Steps) -> TestResult {
    let over = format!("{:?} {}", w.seen.refusals, w.over());
    let first = w.seen.refusals.first().context("A's refusal")?;
    let (_, detail, _) =
        refusal_of(first).map_err(|error| TestError::new(format!("{error} {over}")))?;
    assert_eq!(w.seen.refusals.len(), 4, "{over}");
    for refused in &w.seen.refusals {
        let (code, found, _) =
            refusal_of(refused).map_err(|error| TestError::new(format!("{error} {over}")))?;
        assert_eq!(code, ErrorCode::CrossPartition, "{over}");
        assert_eq!(found, detail, "{over}");
    }
    nothing(w.seen.answers.first().context("what is held")?)?;
    release(w)
}
