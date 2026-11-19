/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The system group's books in the storage process hosting it, as processes: what the group holds across a kill, what its
//! closes wait for, what a partition of the hosting process reads at its recovery, a failed write to the metadata log, and
//! where each runtime of a pinned role runs. Bound to `features/system/books.feature` and
//! `features/system/pinned.feature`.

use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use compio::runtime::Runtime;
use cucumber::{then, when};
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_core_ids::Version;
use ultragraphdb_protocol_transport::{Address, ReceivedFrame};
use ultragraphdb_protocol_wire::{
    BodyView, BucketRange, Catalog, CatalogView, Challenge, ChallengeView, CloseRequest, Command,
    CommitView, ConflictKind, Declare, DeclareView, Declared, DeclaredPredicate, EpochCloseView,
    ErrorCode, ErrorReplyView, Lease, LeaseView, Placement, PlacementView, Subscribe, TenantRun,
};
use ultragraphdb_tests::{CLUSTER, SECRET, START_SECONDS, exit_is, fresh_data, scratch, with};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult, loopback};

use crate::{
    FIRST_OFFSET, SECOND_OFFSET, Steps, TENANT, commit_answer, edge_write, first_observed, hosting,
    instance, refused_at, rounds_replayed, second_observed, services, started, subject_of,
    system_address,
};

/// The closes the scenario's group holds before its host is killed.
const CLOSES: u64 = 12;

/// The longest an epoch stays open with no close asked for: the most the sequencer allows (`SequencerConfig`'s
/// `EPOCH_MS_MAX_MAX`), so in the books' scenario every close but those it asks for lies at least this far apart.
const EPOCH_MS_MAX_LONGEST: &str = "10000";

/// The bootstrap administrator the hosting process creates at its first start, and the password it reads.
const ADMIN: &str = "admin";
const ADMIN_PASSWORD: &str = "the administrator's password";

/// The type prefix and size of the scenario's block leases.
const LEASED_PREFIX: u16 = 1;
const LEASED_SIZE: u64 = 16;

/// The catalog versions declared before the hosting process restarts: version 3 is the last.
const VERSIONS: u64 = 3;

/// `epoch_ms_max` in the delayed partition's scenario, and the window after its process is stopped, by the sequencer's
/// clock, that it stays stopped through: ten such epochs, past `epoch_ms_max` as the scenario asks.
const EPOCH_MS_MAX: u64 = 50;
const STOPPED_MS: u64 = 10 * EPOCH_MS_MAX;

/// The partitions of the pinned role, and the CPUs it leaves free from the lowest.
const PINNED_PARTITIONS: u32 = 5;
const PIN_SKIP: usize = 1;

/// A role connection to the system group as `role`.
async fn group_as(role: &str) -> TestResult<RoleClient> {
    let target = Address::parse(&format!("tcp://{}", system_address()?))?;
    Ok(RoleClient::dial(&target, CLUSTER, role, SECRET).await?)
}

/// The epoch of `frame`, a close.
fn close_epoch(frame: &ReceivedFrame) -> TestResult<u64> {
    Ok(EpochCloseView::from_frame(&frame.view()?)?.epoch())
}

/// Fails on `frame` when it is an Error, naming what it answers.
fn answered(what: &str, frame: &ReceivedFrame) -> TestResult {
    let view = frame.view()?;
    if view.command() == Command::Error {
        let refusal = ErrorReplyView::from_frame(&view)?;
        return Err(TestError::new(format!(
            "{what} refused: {}",
            refusal.message()
        )));
    }
    Ok(())
}

/// Whether `frame` aborts a transaction that executed against another catalog version than the one in effect at its
/// commit: code 2 (Conflict), kind 5 (Catalog), retryable (`PROTOCOL.md` §9.4, §11.13).
fn catalog_aborted(frame: &ReceivedFrame) -> TestResult<bool> {
    let view = frame.view()?;
    if view.command() != Command::Error {
        return Ok(false);
    }
    let refusal = ErrorReplyView::from_frame(&view)?;
    Ok(refusal.code() == ErrorCode::Conflict
        && ConflictKind::from_number(refusal.detail()) == Some(ConflictKind::Catalog))
}

/// Sends `pid` the signal `signal`, by the host's `kill`.
fn signal(pid: u32, signal: &str) -> TestResult {
    let sent = std::process::Command::new("kill")
        .args([signal, &pid.to_string()])
        .status()?;
    if sent.success() {
        Ok(())
    } else {
        Err(TestError::new(format!("kill {signal} {pid}: {sent}")))
    }
}

/// Kills `pid` as a host lost: SIGKILL, waited for.
fn killed(w: &Steps, pid: u32) -> TestResult {
    signal(pid, "-9")?;
    let util = w.roles.as_ref().context("the processes")?;
    let _status = util.wait_for_exit(pid, Duration::from_secs(START_SECONDS))?;
    Ok(())
}

/// The hosting process's settings over one partition, bootstrapping [`ADMIN`] from a file of its own.
fn bootstrapping(system: &str) -> TestResult<Vec<(String, String)>> {
    let file = scratch("books-bootstrap-secret");
    std::fs::write(&file, ADMIN_PASSWORD)?;
    Ok(with(
        hosting((1, 1, FIRST_OFFSET, &first_observed()?), 1, system)?,
        "metadata.bootstrap_secret_file",
        &file.display().to_string(),
    ))
}

/// What the group holds, as a role reads it: the tenant's current catalog version (its number, epoch and bytes), the
/// administrator's salt and iteration count, and the placement (its epochs, ranges and runs); and the log position each
/// was read at.
#[derive(Debug, PartialEq, Eq)]
struct Books {
    catalog: (u64, u64, Vec<u8>),
    principal: (u32, Vec<u8>),
    placement: (u64, u64, u64, Vec<BucketRange>, Vec<TenantRun>),
    positions: (u64, u64),
}

async fn books() -> TestResult<Books> {
    let mut storage = group_as("storage").await?;
    let reply = storage.request(&Catalog::request(TENANT, 0)).await?;
    let view = reply.view()?;
    let held = CatalogView::from_frame(&view)?;
    let catalog = (held.version(), held.epoch(), held.bytes().to_vec());
    let catalog_position = held.position();
    drop(reply);
    let placed_reply = storage.request(&Placement::request(0)).await?;
    let placed_view = placed_reply.view()?;
    let placed = PlacementView::from_frame(&placed_view)?;
    let placement_position = placed.position();
    let placement = (
        placed.epoch(),
        placed.from(),
        placed.until(),
        (0..placed.range_count())
            .filter_map(|index| placed.range(index))
            .collect(),
        (0..placed.run_count())
            .filter_map(|index| placed.run(index))
            .collect(),
    );
    drop(placed_reply);
    let mut frontend = group_as("compute").await?;
    let challenge_reply = frontend.request(&Challenge::request(ADMIN)).await?;
    let challenge_view = challenge_reply.view()?;
    let challenged = ChallengeView::from_frame(&challenge_view)?;
    Ok(Books {
        catalog,
        principal: (challenged.iterations(), challenged.salt().to_vec()),
        placement,
        positions: (catalog_position, placement_position),
    })
}

/// A block lease at the group: its start and end.
async fn leased(storage: &mut RoleClient) -> TestResult<(u64, u64)> {
    let reply = storage
        .request(&Lease::request(LEASED_PREFIX, 1, LEASED_SIZE))
        .await?;
    let view = reply.view()?;
    let block = LeaseView::from_frame(&view)?;
    Ok((block.start(), block.end()))
}

/// The latest close the group holds, as the first close of a subscription at the latest.
async fn latest_close() -> TestResult<u64> {
    let mut closes = group_as("compute").await?;
    closes.send_only(&Subscribe::at_latest()).await?;
    close_epoch(&closes.next_frame().await?)
}

#[when(
    "the group holds a catalog version, two block leases, a principal and twelve closes, and its host is killed and restarted"
)]
fn books_recover(w: &mut Steps) -> TestResult {
    let util = services()?;
    // Every close comes when asked for: none by the clock within the scenario's seconds.
    let settings = with(
        bootstrapping(&fresh_data())?,
        "sequencer.epoch_ms_max",
        EPOCH_MS_MAX_LONGEST,
    );
    let pid = started(
        &util,
        &settings,
        &scratch("storage-books.log"),
        &first_observed()?,
    )?;
    w.roles = Some(util);
    let (before, second) = Runtime::new()?.block_on(async {
        declared_through_closes().await?;
        let mut storage = group_as("storage").await?;
        let _first = leased(&mut storage).await?;
        let second = leased(&mut storage).await?;
        TestResult::Ok((books().await?, second))
    })?;
    killed(w, pid)?;
    let roles = w.roles.as_ref().context("the processes")?;
    let again = scratch("storage-books-again.log");
    let _restarted: u32 = started(roles, &settings, &again, &first_observed()?)?;
    let (after, latest, third) = Runtime::new()?.block_on(async {
        let after = books().await?;
        let latest = latest_close().await?;
        let mut storage = group_as("storage").await?;
        TestResult::Ok((after, latest, leased(&mut storage).await?))
    })?;
    // The positions apart: the restarted process's partition replays its log and reports on the version it installs
    // again, which the group appends after its recovery and before any read over a role connection.
    let positions = [before.positions, after.positions];
    let (before, after) = (
        Books {
            positions: (0, 0),
            ..before
        },
        Books {
            positions: (0, 0),
            ..after
        },
    );
    w.books = vec![format!("{before:?}"), format!("{after:?}")];
    w.positions = positions.to_vec();
    w.epochs = vec![latest];
    w.leases = vec![second, third];
    w.logs = vec![std::fs::read_to_string(&again)?];
    Ok(())
}

/// Declares a predicate, which builds version 1, and asks for closes through the twelfth, which assign the version's
/// epoch: the Declare is answered once they have.
async fn declared_through_closes() -> TestResult {
    let mut closes = group_as("compute").await?;
    closes.send_only(&Subscribe::at_latest()).await?;
    let mut asking = group_as("compute").await?;
    // Answered once a close has assigned the version's epoch, which the closes asked for below bring.
    let declared = vec![Declared::Predicate(DeclaredPredicate::new(
        "http://ex.org/declared",
    ))];
    asking
        .send_only(&Declare::declarations(TENANT, 0, 0, declared))
        .await?;
    // The open epoch is 1 when the group holds no close; a request for an epoch closed already changes nothing.
    asking.send_only(&CloseRequest::compute(1)).await?;
    let mut latest = close_epoch(&closes.next_frame().await?)?;
    // Each pass asks for the open epoch's close and reads it: ends at the twelfth (Ultra rule 2).
    while latest < CLOSES {
        asking.send_only(&CloseRequest::compute(latest + 1)).await?;
        latest = close_epoch(&closes.next_frame().await?)?;
    }
    let reply = asking.next_frame().await?;
    answered("the Declare", &reply)?;
    let version = DeclareView::from_frame(&reply.view()?)?.version();
    if version != 1 {
        return Err(TestError::new(format!(
            "the Declare built version {version}"
        )));
    }
    drop(reply);
    Ok(())
}

#[then(
    "the group's state after recovery equals its state before the kill, read at the same log position, and the sequencer's last closed epoch is 12"
)]
fn books_held(w: &mut Steps) -> TestResult {
    let [before, after] = w.books.as_slice() else {
        return Err(TestError::new(format!("{:?}", w.books)));
    };
    assert_eq!(after, before, "the restart: {:?}", w.logs);
    // Nothing the group made durable before the kill is lost: every read after the recovery answers at or past the
    // position of every read before it. The partition appends as it reads, so the reads after it name no one position.
    let [
        (catalog_before, placement_before),
        (catalog_after, placement_after),
    ] = w.positions.as_slice()
    else {
        return Err(TestError::new(format!("{:?}", w.positions)));
    };
    let before_at = catalog_before.max(placement_before);
    assert!(
        catalog_after.min(placement_after) >= before_at,
        "{:?}",
        w.positions
    );
    assert_eq!(w.epochs, vec![CLOSES]);
    // A lease after the recovery starts past the second before the kill: the lease mark recovered.
    let [(_, second_end), (third_start, _)] = w.leases.as_slice() else {
        return Err(TestError::new(format!("{:?}", w.leases)));
    };
    assert!(third_start >= second_end, "{:?}", w.leases);
    w.roles = None;
    Ok(())
}

/// Commits writes of distinct objects to `partition` at `address` until one commits at `epoch` or later, its epoch
/// returned: what the partition's open epoch, one past the last close delivered to it, has reached. A transaction the
/// partition aborts for a catalog version taking effect ([`catalog_aborted`]) runs again. Gives up past
/// [`START_SECONDS`].
#[expect(
    clippy::disallowed_methods,
    reason = "a test measures a wait its scenario names; no simulation replays it"
)]
fn committed_at_or_after(
    address: &str,
    (partition, partitions): (u32, u32),
    epoch: u64,
) -> TestResult<u64> {
    let subject = subject_of(partition, partitions)?;
    let began = Instant::now();
    Runtime::new()?.block_on(async {
        let target = Address::parse(address)?;
        let mut client = RoleClient::dial(&target, CLUSTER, "compute", SECRET).await?;
        // Ends at a commit of `epoch` or later, or past the bound (Ultra rule 2).
        for at in 0u32.. {
            let write = edge_write(&subject, &format!("http://ex.org/late{at}"));
            let answer = commit_answer(&mut client, partition, &write).await?;
            // A transaction that executed against another catalog version than the one in effect at its commit epoch is
            // aborted, code 2 kind 5, and runs again on a new Bind against the version in effect (`PROTOCOL.md` §11.13,
            // *Transactions against a version*; §9.4, code 2: "the whole transaction on a new Begin"). The partition takes
            // the closes at its own pace, behind the group's under load, so one bound before the close opening a
            // version's epoch may commit after it. Any other refusal fails.
            let committed = if catalog_aborted(&answer)? {
                None
            } else {
                refused_at(partition, &answer)?;
                let version = CommitView::from_frame(&answer.view()?)?.commit_version();
                Some(Version::from_u64(version).epoch().get())
            };
            drop(answer);
            if let Some(committed) = committed.filter(|committed| *committed >= epoch) {
                return Ok(committed);
            }
            if began.elapsed() > Duration::from_secs(START_SECONDS) {
                break;
            }
            compio::time::sleep(Duration::from_millis(10)).await;
        }
        Err(TestError::new(format!(
            "partition {partition} committed below epoch {epoch} for {START_SECONDS} s"
        )))
    })
}

#[when("a partition's log sync is delayed past `epoch_ms_max` while the group's logs sync at once")]
#[expect(
    clippy::disallowed_methods,
    reason = "a test measures a wait its scenario names; no simulation replays it"
)]
fn partition_delayed(w: &mut Steps) -> TestResult {
    let util = services()?;
    let host = with(
        hosting((1, 1, FIRST_OFFSET, &first_observed()?), 2, &fresh_data())?,
        "sequencer.epoch_ms_max",
        &EPOCH_MS_MAX.to_string(),
    );
    let delayed = instance(2, 1, SECOND_OFFSET, &second_observed()?)?;
    let _host: u32 = started(
        &util,
        &host,
        &scratch("storage-delay-host.log"),
        &first_observed()?,
    )?;
    let pid = started(
        &util,
        &delayed,
        &scratch("storage-delayed.log"),
        &second_observed()?,
    )?;
    w.roles = Some(util);
    // Partition 2's process stopped whole: whatever sync of its log is in flight, and every read of its subscription,
    // waits until it runs again.
    let during = Runtime::new()?.block_on(async {
        let mut closes = group_as("compute").await?;
        closes.send_only(&Subscribe::at_latest()).await?;
        let mut during = vec![timed_close(&closes.next_frame().await?)?];
        signal(pid, "-STOP")?;
        let stopped = Instant::now();
        let stopped_at = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_micros())?;
        // Read until a close the sequencer made past the window, by the time it recorded on it: a close a loaded machine
        // hands this reader late is still counted in the window it was made in. Ends at that close, or past
        // START_SECONDS (Ultra rule 2).
        let window_end = stopped_at + STOPPED_MS * 1_000;
        while during.last().is_some_and(|(_, time)| *time <= window_end) {
            let left = Duration::from_secs(START_SECONDS)
                .checked_sub(stopped.elapsed())
                .context("a close made past the window while the partition is stopped")?;
            let frame = compio::time::timeout(left, closes.next_frame())
                .await
                .map_err(|_| {
                    TestError::new(format!(
                        "no close made past {STOPPED_MS} ms after the stop read in {START_SECONDS} s"
                    ))
                })??;
            during.push(timed_close(&frame)?);
        }
        TestResult::Ok((stopped_at, during))
    });
    // Run again whatever the closes read, so the scenario's end stops it as any other.
    signal(pid, "-CONT")?;
    let (stopped_at, during) = during?;
    let last = during.last().context("a close")?.0;
    // The partition's open epoch passes the last close the group sent while it was stopped once it has read them all.
    let reached = committed_at_or_after(
        &format!("tcp://{}", loopback(SECOND_OFFSET)?),
        (2, 2),
        last + 1,
    )?;
    w.stopped_at = stopped_at;
    w.close_times = during.iter().map(|(_, time)| *time).collect();
    w.epochs = during.iter().map(|(epoch, _)| *epoch).collect();
    w.epochs.push(reached);
    Ok(())
}

/// The epoch of `frame`, a close, and the sequencer's wall-clock time recorded on it, in microseconds.
fn timed_close(frame: &ReceivedFrame) -> TestResult<(u64, u64)> {
    let close = EpochCloseView::from_frame(&frame.view()?)?;
    Ok((close.epoch(), close.time()))
}

#[then(
    "the sequencer closes its epochs on time, and the delayed partition receives them once it reads again"
)]
fn closes_on_time(w: &mut Steps) -> TestResult {
    let (reached, during) = w.epochs.split_last().context("the epochs")?;
    let [first, closed @ ..] = during else {
        return Err(TestError::new("a close before the stop"));
    };
    // In order, one epoch after another, while the partition was stopped.
    let mut next = *first;
    for epoch in closed {
        next += 1;
        assert_eq!(*epoch, next, "{:?}", w.epochs);
    }
    // On time: a close per `epoch_ms_max` by the times the sequencer recorded on them (sequencer, "The sequencer closes
    // epochs at its own wall-clock time"), at least half of those the window after the stop holds made in it, where a group
    // waiting on the stopped partition would have made none, and handed none over before it ran again.
    let window = w.stopped_at..=w.stopped_at + STOPPED_MS * 1_000;
    let made = w
        .close_times
        .iter()
        .filter(|time| window.contains(time))
        .count();
    let on_time = usize::try_from(STOPPED_MS / EPOCH_MS_MAX / 2)?;
    assert!(
        made >= on_time,
        "{made} closes made in the {STOPPED_MS} ms after the stop at {}: {:?} at {:?}",
        w.stopped_at,
        w.epochs,
        w.close_times
    );
    let last = closed.last().unwrap_or(first);
    assert!(reached > last, "{:?}", w.epochs);
    w.roles = None;
    Ok(())
}

#[when("the hosting process restarts with a partition whose log names catalog version 3")]
fn restarts_with_version(w: &mut Steps) -> TestResult {
    let util = services()?;
    let settings = hosting((1, 1, FIRST_OFFSET, &first_observed()?), 1, &fresh_data())?;
    let pid = started(
        &util,
        &settings,
        &scratch("storage-versions.log"),
        &first_observed()?,
    )?;
    w.roles = Some(util);
    let effective = Runtime::new()?.block_on(async {
        let mut asking = group_as("compute").await?;
        let mut effective = 0;
        for built_on in 0..VERSIONS {
            let declared = vec![Declared::Predicate(DeclaredPredicate::new(format!(
                "http://ex.org/declared{built_on}"
            )))];
            let reply = asking
                .request(&Declare::declarations(TENANT, built_on, 0, declared))
                .await?;
            let view = reply.view()?;
            let built = DeclareView::from_frame(&view)?;
            if built.version() != built_on + 1 {
                return Err(TestError::new(format!(
                    "a Declare on version {built_on} built version {}",
                    built.version()
                )));
            }
            effective = Version::from_u64(built.commit_version()).epoch().get();
        }
        TestResult::Ok(effective)
    })?;
    // A round at the epoch version 3 takes effect or later: its records name version 3. The partition takes the closes at
    // its own pace, so a transaction bound before the close opening that epoch and committed after it is aborted and runs
    // again, as `committed_at_or_after` does.
    let _epoch = committed_at_or_after(
        &format!("tcp://{}", loopback(FIRST_OFFSET)?),
        (1, 1),
        effective,
    )?;
    killed(w, pid)?;
    let again = scratch("storage-versions-again.log");
    let roles = w.roles.as_ref().context("the processes")?;
    let _restarted: u32 = started(roles, &settings, &again, &first_observed()?)?;
    w.logs = vec![std::fs::read_to_string(&again)?];
    Ok(())
}

#[then("the group recovers first, the partition reads version 3 with Catalog and then replays")]
fn group_first(w: &mut Steps) -> TestResult {
    let log = w.logs.first().context("the restart's log")?;
    let listens = line_of(log, "the group's listen", |line| {
        line.contains("the system group listens")
    })?;
    let version = format!("version={VERSIONS}");
    let read = line_of(log, "version 3 read", |line| {
        line.contains("a catalog version read")
            && line.split_whitespace().any(|word| word == version)
    })?;
    let recovered = line_of(log, "the partition's recovery", |line| {
        line.contains("recovered") && line.contains("partition=")
    })?;
    assert!(listens < read && read < recovered, "{log}");
    assert!(rounds_replayed(log)? > 0, "{log}");
    w.roles = None;
    Ok(())
}

/// The number of the first line of `log` that `holds`, or a failure naming `named`.
fn line_of(log: &str, named: &str, holds: impl Fn(&str) -> bool) -> TestResult<usize> {
    log.lines()
        .position(holds)
        .ok_or_else(|| TestError::new(format!("{named} in the log: {log}")))
}

#[when("a write to the metadata log fails")]
fn metadata_write_fails(w: &mut Steps) -> TestResult {
    let util = services()?;
    let system = fresh_data();
    // A snapshot after every round of the metadata group, each trimming its log through it: the trim writes the log's
    // superblock, a file created in `meta/` and renamed over its copy.
    let settings = with(
        hosting((1, 1, FIRST_OFFSET, &first_observed()?), 1, &system)?,
        "metadata.snapshot_rounds",
        "1",
    );
    let log = scratch("storage-failed-log.log");
    let pid = started(&util, &settings, &log, &first_observed()?)?;
    let meta = Path::new(&system).join("meta");
    // The log writes a superblock copy through a temporary file beside it, created and then renamed over the copy: a
    // directory standing at both copies' temporary paths makes the log's next superblock write fail at its create, the
    // first of its operations, while every other operation of the log goes on.
    occupied(&meta.join("superblock.a.tmp"))?;
    occupied(&meta.join("superblock.b.tmp"))?;
    // A lease is a round of the metadata group, so a snapshot falls due at the next close.
    // The group may stop before it answers, at a snapshot that fell due as the paths were taken: the failure the scenario
    // asks for, so the lease's own outcome tells nothing.
    let asked = Runtime::new()?.block_on(async {
        let mut storage = group_as("storage").await?;
        drop(leased(&mut storage).await);
        TestResult::Ok(())
    });
    let exited = util.wait_for_exit(pid, Duration::from_secs(START_SECONDS));
    asked?;
    w.exited = Some(exited?);
    w.logs = vec![std::fs::read_to_string(&log)?];
    w.roles = Some(util);
    Ok(())
}

/// A directory made at `path`, waiting while a file stands there: a superblock write past its create, whose rename takes
/// the file away, so no write is left between its create and its rename once the directory stands (Ultra rule 2: at most
/// `START_SECONDS`).
fn occupied(path: &Path) -> TestResult {
    for _ in 0..START_SECONDS * 1_000 {
        match std::fs::create_dir(path) {
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                std::thread::sleep(Duration::from_millis(1));
            }
            made => return Ok(made?),
        }
    }
    Err(TestError::new(format!(
        "{} was never free of the log's write",
        path.display()
    )))
}

#[then("the hosting process shuts down through its ordinary path and exits with code 74")]
fn stops_with_74(w: &mut Steps) -> TestResult {
    let log = w.logs.first().context("the log")?;
    exit_is(w.exited, log, 74)?;
    assert!(
        log.contains("a partition or the system group stopped; stopping the rest"),
        "{log}"
    );
    assert!(log.contains("stopping role=storage"), "{log}");
    assert!(
        log.contains("failure=io: ordered log: storage create failed"),
        "the log's refused write: {log}"
    );
    w.roles = None;
    Ok(())
}

#[when(
    "a storage process of five partitions starts with `pin_cores = true` and `pin_skip = 1` where it is permitted six CPUs"
)]
fn five_pinned(w: &mut Steps) -> TestResult {
    let util = services()?;
    let settings = hosting(
        (1, PINNED_PARTITIONS, FIRST_OFFSET, &first_observed()?),
        PINNED_PARTITIONS,
        &fresh_data(),
    )?;
    let settings = with(settings, "runtime.pin_cores", "true");
    let settings = with(settings, "runtime.pin_skip", &PIN_SKIP.to_string());
    let log = scratch("storage-five-pinned.log");
    let _pid: u32 = started(&util, &settings, &log, &first_observed()?)?;
    w.logs = vec![std::fs::read_to_string(&log)?];
    w.roles = Some(util);
    Ok(())
}

/// The line of `log` saying where partition `partition`'s runtime runs.
fn binding_of(log: &str, partition: u32) -> TestResult<&str> {
    let named = format!("partition={partition}");
    log.lines()
        .find(|line| {
            line.split_whitespace().any(|word| word == named)
                && (line.contains("pinned") || line.contains("unpinned"))
        })
        .ok_or_else(|| TestError::new(format!("partition {partition}'s runtime in: {log}")))
}

/// Linux binds and reads each binding back: the CPUs come from the set this test process, the role's parent, is
/// permitted, taken in turn past the lowest `pin_skip` (`ultragraphdb_common_service::cpu_for`). macOS binds nothing, and
/// every runtime says so.
#[then(
    "each partition's runtime logs a CPU of its own among the five highest, read back from the operating system, and no runtime of the role runs on the lowest"
)]
fn cpus_of_their_own(w: &mut Steps) -> TestResult {
    let log = w.logs.first().context("the log")?;
    if !cfg!(target_os = "linux") {
        for partition in 1..=PINNED_PARTITIONS {
            let line = binding_of(log, partition)?;
            assert!(
                line.contains("unpinned: this platform cannot bind"),
                "{line}"
            );
        }
        w.roles = None;
        return Ok(());
    }
    let permitted = ultragraphdb_common_service::allowed_in_status(&std::fs::read_to_string(
        "/proc/self/status",
    )?)
    .context("the permitted CPUs")?;
    if permitted.len() < 6 {
        return Err(TestError::new(format!(
            "the scenario asks for six permitted CPUs, this process is permitted {permitted:?}"
        )));
    }
    let lowest = *permitted.first().context("a permitted CPU")?;
    let mut held = std::collections::BTreeSet::new();
    for (core, partition) in (1..=PINNED_PARTITIONS).enumerate() {
        let expected = ultragraphdb_common_service::cpu_for(&permitted, PIN_SKIP, core)
            .context("a CPU for the core")?;
        let line = binding_of(log, partition)?;
        assert!(
            line.contains(&format!("pinned to CPU {expected}")),
            "{line}"
        );
        assert!(held.insert(expected), "CPU {expected} twice: {log}");
    }
    let highest: std::collections::BTreeSet<usize> =
        permitted.iter().rev().take(5).copied().collect();
    if permitted.len() == 6 {
        assert_eq!(held, highest, "{log}");
    }
    // Every runtime pinned, the group's and the role's own among them, holds another CPU than the lowest.
    let pinned: Vec<usize> = log
        .split("pinned to CPU ")
        .skip(1)
        .map(|rest| {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            digits.parse::<usize>()
        })
        .collect::<Result<_, _>>()?;
    assert!(
        !pinned.contains(&lowest),
        "a runtime on CPU {lowest}: {log}"
    );
    let role_cpus = log
        .split("bound to the role's CPUs ")
        .nth(1)
        .and_then(|rest| rest.split('}').next())
        .context("the role's CPUs")?;
    assert!(
        !role_cpus
            .trim_start_matches('{')
            .split(", ")
            .any(|cpu| cpu == lowest.to_string()),
        "the role's runtime on CPU {lowest}: {log}"
    );
    w.roles = None;
    Ok(())
}
