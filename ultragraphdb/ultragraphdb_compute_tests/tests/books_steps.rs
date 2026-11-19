/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the system group decides apart from compute: a tightening's check, which outlives the compute node that asked for it,
//! and an eviction, which reaches a partition's session table with the close carrying it.

use std::time::{Duration, Instant};

use compio::runtime::Runtime;
use cucumber::{then, when};
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_protocol_transport::{Address, ReceivedFrame};
use ultragraphdb_protocol_wire::{
    BodyView, Catalog, CatalogView, CloseRequest, Command, Declared, DeclaredPredicate,
    EpochCloseView, ErrorCode, ErrorReplyView, Recall, Subscribe, TermTable, Write, WriteChange,
};
use ultragraphdb_tests::{
    CLUSTER, SECRET, START_SECONDS, compute_holding, storage_role_address, system_role_address,
    with,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::Steps;
use crate::processes::{
    ENDED, Roles, TENANT, admin, host_of, second_of, signal, subject_of, write_of,
};

/// The predicate the tightening declares single-valued.
const NICK: &str = "http://ex.org/nick";

/// How often a step looks again at what it waits for.
const LOOK: Duration = Duration::from_millis(10);

/// What the system group logs of a partition's report on a tightening's check (`common_system`).
const REPORTED: &str = "a partition reported on a catalog version";

/// `transaction_ms` as the storage binary starts the system group with it, the protocol's default (`PROTOCOL.md` §11.9): for
/// twice it after the group begins to listen, the group evicts no session (§11.5).
const TRANSACTION_MS: u64 = 10_000;

/// The latest catalog version of the tenant at the system group, 0 for none, as a partition asks for it.
fn latest_version() -> TestResult<u64> {
    Runtime::new()?.block_on(async {
        let address = Address::parse(&format!("tcp://{}", system_role_address()?))?;
        let mut role = RoleClient::dial(&address, CLUSTER, "storage", SECRET).await?;
        let Ok(reply) = role.request(&Catalog::request(TENANT, 0)).await else {
            return Ok(0);
        };
        let view = reply.view()?;
        Ok(CatalogView::from_frame(&view)?.version())
    })
}

/// Waits until the tenant's latest catalog version is `version`, within `within`: whether it came.
#[expect(
    clippy::disallowed_methods,
    reason = "a test measures a wait its scenario names; no simulation replays it"
)]
fn until_version(version: u64, within: Duration) -> TestResult<bool> {
    let begun = Instant::now();
    while begun.elapsed() < within {
        if latest_version()? == version {
            return Ok(true);
        }
        std::thread::sleep(LOOK);
    }
    Ok(false)
}

/// Writes `a` holding two objects of `nick`, each in a write of its own.
fn two_nicks(runtime: &Runtime, a: &str) -> TestResult {
    runtime.block_on(async {
        let mut client = admin(1).await?;
        for object in ["http://ex.org/x", "http://ex.org/y"] {
            let mut terms = TermTable::new();
            let change = WriteChange::Insert {
                graph: None,
                subject: terms.iri(a),
                predicate: terms.iri(NICK),
                object: terms.iri(object),
            };
            let write = ultragraphdb_protocol_wire::DirectWrite::new(TENANT, terms, vec![change]);
            let _committed = client.direct_write(write, None).await?;
        }
        TestResult::Ok(())
    })
}

#[when(
    "a client commits a declaration of `nick` single-valued, compute stops before any partition reports, and a partition then reports that the data refuses it"
)]
fn check_outlives_compute(w: &mut Steps) -> TestResult {
    let mut roles = Roles::default();
    roles.start_host(host_of(2)?, true)?;
    roles.start_second(second_of()?)?;
    roles.start_compute(compute_holding()?, true)?;
    // `a` in partition 2, the second process's, holding two objects of `nick`.
    let a = subject_of("http://ex.org/a", 2)?;
    let runtime = Runtime::new()?;
    two_nicks(&runtime, &a)?;
    let before = latest_version()?;
    let second = roles
        .second
        .as_ref()
        .map(|(pid, _)| *pid)
        .context("partition 2's process")?;
    // Partition 2 reports nothing while its process is stopped.
    signal(second, "STOP")?;
    let declaring = Write::new(
        TermTable::new(),
        vec![WriteChange::Declare(Declared::Predicate(
            DeclaredPredicate::new(NICK).with_single(true),
        ))],
    );
    let left = runtime.block_on(async {
        let mut client = admin(1).await?;
        let transaction = client.begin(TENANT, false).await?;
        client.write(&transaction, &declaring).await?;
        // Sent, the Commit goes on to its answer without this client, which stops waiting for it here.
        let waited =
            compio::time::timeout(Duration::from_millis(300), client.commit(transaction, None))
                .await;
        TestResult::Ok(waited.is_err())
    });
    // The Declare reached the group once the version it asks for is held, being checked.
    let asked = until_version(before + 1, Duration::from_secs(START_SECONDS));
    let compute = roles.compute_pid()?;
    let stopped = roles.kill(compute);
    // Whatever the steps above came to, a process left stopped would hold the scenario's end up.
    signal(second, "CONT")?;
    let (left, asked, _stopped) = (left?, asked?, stopped?);
    let restored = until_version(before + 2, Duration::from_secs(START_SECONDS))?;
    let host_log = roles.last_log("storage-host")?;
    let compute_settings = roles.compute.take().context("compute")?.1;
    roles.start_compute(compute_settings, true)?;
    let single = runtime.block_on(async {
        let mut client = admin(1).await?;
        let described = client.describe(TENANT).await?;
        TestResult::Ok(
            described
                .predicates()
                .iter()
                .any(|held| held.predicate() == NICK && held.single()),
        )
    })?;
    w.seen = vec![
        left.to_string(),
        asked.to_string(),
        restored.to_string(),
        single.to_string(),
        before.to_string(),
        latest_version()?.to_string(),
        host_log,
    ];
    w.processes = roles;
    Ok(())
}

#[then(
    "the metadata group holds a version restoring the previous declaration, and no Describe reports `nick` single-valued"
)]
fn check_restored(w: &mut Steps) -> TestResult {
    let [left, asked, restored, single, before, latest, log] = w.seen.as_slice() else {
        return Err(TestError::new(format!("{:?}", w.seen)));
    };
    assert_eq!(
        left, "true",
        "the Commit was still unanswered as compute stopped"
    );
    assert_eq!(asked, "true", "the version being checked was held: {log}");
    // The data refuses it: partition 2's report, which it could send only after compute was gone.
    let refusing = log.lines().any(|line| {
        line.contains(REPORTED) && line.contains("partition=2") && line.contains("refused=true")
    });
    assert!(refusing, "partition 2 reported a refusal: {log}");
    assert_eq!(restored, "true", "a restoring version: {log}");
    let before: u64 = before.parse()?;
    assert_eq!(
        latest,
        &(before + 2).to_string(),
        "the version and its restoring"
    );
    assert_eq!(single, "false");
    w.processes = Roles::default();
    Ok(())
}

/// What partition 1 answers a Recall of `request` of `session` with: its command, and an Error's code.
async fn recalled(session: u128, request: u64) -> TestResult<(Command, Option<ErrorCode>)> {
    let address = Address::parse(&format!("tcp://{}", storage_role_address()?))?;
    let mut role = RoleClient::dial(&address, CLUSTER, "compute", SECRET).await?;
    role.send_only(&Recall::new(1, session.to_be_bytes(), request))
        .await?;
    let answer = role.next_frame().await?;
    let view = answer.view()?;
    let code = (view.command() == Command::Error)
        .then(|| ErrorReplyView::from_frame(&view).map(|refusal| refusal.code()))
        .transpose()?;
    Ok((view.command(), code))
}

/// The epoch of `frame`, a close on a subscription.
fn closed_epoch(frame: &ReceivedFrame) -> TestResult<u64> {
    Ok(EpochCloseView::from_frame(&frame.view()?)?.epoch())
}

#[when("a session has committed in partition 1, and the registry evicts it")]
#[expect(
    clippy::disallowed_methods,
    reason = "a test measures a wait its scenario names; no simulation replays it"
)]
fn session_evicted(w: &mut Steps) -> TestResult {
    let mut roles = Roles::default();
    // A registry of one session, so the next registration evicts the first; and closes far apart, 10 s at the most, so the
    // close carrying the eviction is the one the scenario asks for (sequencer, `epoch_ms_max`).
    let host = with(
        with(host_of(1)?, "metadata.sessions_max", "1"),
        "sequencer.epoch_ms_max",
        "10000",
    );
    roles.start_host(host, true)?;
    // The group listened before the host's `/ready`: its quiet time ends at most twice `transaction_ms` from here.
    let quiet_until = Instant::now() + Duration::from_millis(2 * TRANSACTION_MS);
    roles.start_compute(compute_holding()?, true)?;
    let runtime = Runtime::new()?;
    let (mut client, first) = runtime.block_on(async {
        let mut client = admin(1).await?;
        let session = client.register().await?;
        let committed = client
            .direct_write(write_of("http://ex.org/s", "http://ex.org/o"), session)
            .await?;
        compio::time::sleep(ENDED).await;
        TestResult::Ok((client, (session.number(), committed.request())))
    })?;
    std::thread::sleep(quiet_until.saturating_duration_since(Instant::now()));
    let seen = runtime.block_on(async {
        // A role connection as compute, subscribed at the latest close, which asks for the closes itself.
        let address = Address::parse(&format!("tcp://{}", system_role_address()?))?;
        let mut role = RoleClient::dial(&address, CLUSTER, "compute", SECRET).await?;
        let latest = closed_epoch(&role.request(&Subscribe::at_latest()).await?)?;
        // A close now: the next on the sequencer's own time is then 10 s away.
        role.send_only(&CloseRequest::compute(latest + 1)).await?;
        let renewed = closed_epoch(&role.next_frame().await?)?;
        let _evicting = client.register().await?;
        let kept = recalled(first.0, first.1).await?;
        // No close came between the eviction and that answer: the subscription holds none.
        let between = compio::time::timeout(Duration::from_millis(1), role.next_frame())
            .await
            .is_ok();
        role.send_only(&CloseRequest::compute(renewed + 1)).await?;
        let carrying = closed_epoch(&role.next_frame().await?)?;
        let begun = Instant::now();
        let mut purged = recalled(first.0, first.1).await?;
        while purged == kept && begun.elapsed() < Duration::from_secs(START_SECONDS) {
            compio::time::sleep(LOOK).await;
            purged = recalled(first.0, first.1).await?;
        }
        TestResult::Ok(vec![
            format!("{kept:?}"),
            between.to_string(),
            format!("{:?}", (renewed, carrying)),
            format!("{purged:?}"),
        ])
    })?;
    w.seen = seen;
    w.seen.push(roles.last_log("storage-host")?);
    w.processes = roles;
    Ok(())
}

#[then(
    "partition 1 keeps the session's entry until the close carrying the eviction is delivered to it, and then purges it"
)]
fn purged_after_eviction(w: &mut Steps) -> TestResult {
    let [kept, between, closes, purged, log] = w.seen.as_slice() else {
        return Err(TestError::new(format!("{:?}", w.seen)));
    };
    // The stored reply of the session's last request, a Commit (`PROTOCOL.md` §13.11), while no close has carried the eviction.
    assert_eq!(
        kept,
        &format!("{:?}", (Command::Commit, None::<ErrorCode>)),
        "{log}"
    );
    assert_eq!(
        between, "false",
        "a close came before the Recall's answer: {closes}"
    );
    // Once the close carrying it is delivered, the table holds no entry of the session: 18.
    assert_eq!(
        purged,
        &format!("{:?}", (Command::Error, Some(ErrorCode::NotCommitted))),
        "{log}"
    );
    w.processes = Roles::default();
    Ok(())
}
