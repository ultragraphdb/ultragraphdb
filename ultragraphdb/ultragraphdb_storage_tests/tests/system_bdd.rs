/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The storage binary hosting the system group, as processes, bound to the features tagged `@system`: an instance whose
//! settings hold `[system]` hosts the group, every partition dials `cluster.system_address`, and no frontend runs. The
//! settings are served by the config double, which serves them as they stand, so these scenarios do not wait for the
//! config service's checks of group 7. One scenario runs at a time: the binaries listen at their declared ports.

use std::path::Path;
use std::process::ExitStatus;
use std::time::Duration;

use compio::runtime::Runtime;
use cucumber::{World, then, when};
use futures::FutureExt;
use service_utils::{ServiceUtil, WaitStrategy};
use ultragraphdb_cluster_placement::{NodeId, PlacementMap};
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_protocol_transport::{Address, ReceivedFrame};
use ultragraphdb_protocol_wire::{
    Bind, BindKind, BodyView, Command, Commit, CommitView, ErrorReplyView, Placement,
    PlacementView, RoleKind, TermTable, Write, WriteChange,
};
use ultragraphdb_store_quads::iri_bucket;
use ultragraphdb_tests::{
    CLUSTER, CONFIG_BINARY, SECRET, START_SECONDS, STORAGE_BINARY, answering, exit_is, fresh_data,
    frontend_holding, http_status, members_at, not_hosting, role, role_of_service, scratch,
    start_config_service, start_double, storage_holding, until_answering, until_ports_free, with,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

/// Where the sandbox holds the binaries, relative to the test's working directory: `copy_file` writes them to this package.
const ROOT_PATH: &str = "ultragraphdb/ultragraphdb_storage_tests";

/// The system group's role address, which the hosting instance listens at and every instance dials.
const SYSTEM_ADDRESS: &str = "127.0.0.1:17400";

/// The first instance's partitions listen from this port, and its `/health` and `/ready` at [`FIRST_OBSERVED`].
const FIRST_PORT: u32 = 17_200;
const FIRST_OBSERVED: &str = "127.0.0.1:18101";

/// The second instance's partitions listen from this port, and its `/health` and `/ready` at [`SECOND_OBSERVED`].
const SECOND_PORT: u32 = 17_300;
const SECOND_OBSERVED: &str = "127.0.0.1:18103";

/// How long a refused instance retries, in a scenario that waits it out.
const SHORT_DEADLINE_MS: &str = "800";

/// The rounds a restart replays: 10,000 in the specification's scenario; this many here, so the target ends within the
/// repository's ten seconds (`notes/group-4.md`, *Found wanting*). The law, `/ready` only after the replay, is the same.
const ROUNDS: u32 = 200;

/// The addresses these scenarios hold, and what holds each.
const OWN_ADDRESSES: [(&str, &str); 9] = [
    (SYSTEM_ADDRESS, "the system group's role address"),
    (
        "127.0.0.1:17200",
        "the first instance's partition 1 or the claim's",
    ),
    ("127.0.0.1:17201", "the first instance's second partition"),
    ("127.0.0.1:17202", "the first instance's third partition"),
    ("127.0.0.1:17203", "the first instance's fourth partition"),
    ("127.0.0.1:17300", "the second instance's first partition"),
    ("127.0.0.1:17301", "the second instance's second partition"),
    (FIRST_OBSERVED, "the first instance's observability"),
    (SECOND_OBSERVED, "the second instance's observability"),
];

const TENANT: u64 = 0;

fn services() -> TestResult<ServiceUtil> {
    Ok(ServiceUtil::new(
        ROOT_PATH,
        vec![CONFIG_BINARY, STORAGE_BINARY],
    )?)
}

#[derive(Debug, Default, World)]
struct Steps {
    roles: Option<ServiceUtil>,
    exited: Option<ExitStatus>,
    logs: Vec<String>,
    data: String,
    placements: Vec<Vec<(u32, u32, u32)>>,
    status: u16,
}

/// The settings of an instance of `count` partitions from `first`, at ports from `port`, observed at `observed`, dialling
/// the system group at [`SYSTEM_ADDRESS`] and hosting none.
fn instance(
    first: u32,
    count: u32,
    port: u32,
    observed: &str,
) -> TestResult<Vec<(String, String)>> {
    let settings = with(
        not_hosting(storage_holding()?),
        "observability.address",
        &format!("tcp://{observed}"),
    );
    let settings = with(settings, "storage.partition_first", &first.to_string());
    let settings = with(settings, "storage.partition_count", &count.to_string());
    let settings = with(
        settings,
        "storage.role_address",
        &format!("tcp://127.0.0.1:{port}"),
    );
    Ok(with(
        settings,
        "cluster.system_address",
        &format!("tcp://{SYSTEM_ADDRESS}"),
    ))
}

/// As [`instance`], hosting the system group over `partitions` partitions, its logs under `system`.
fn hosting(
    (first, count, port, observed): (u32, u32, u32, &str),
    partitions: u32,
    system: &str,
) -> TestResult<Vec<(String, String)>> {
    let settings = instance(first, count, port, observed)?;
    let settings = with(
        settings,
        "system.role_address",
        &format!("tcp://{SYSTEM_ADDRESS}"),
    );
    let settings = with(settings, "system.data_dir", system);
    let settings = with(settings, "cluster.partition_count", &partitions.to_string());
    let settings = with(settings, "sequencer.epoch_ms_min", "1");
    // The `[metadata]` table the config service requires of a host, at the bound `PROTOCOL.md` §11.9 gives when unnamed.
    let settings = with(settings, "metadata.sessions_max", "4096");
    Ok(with(settings, "sequencer.epoch_ms_max", "20"))
}

/// Starts the storage binary on `settings`, its log in `log`, waiting on `observed`'s `/ready`: its pid.
fn started(
    util: &ServiceUtil,
    settings: &[(String, String)],
    log: &Path,
    observed: &str,
) -> TestResult<u32> {
    let ready = answering(observed, "/ready");
    Ok(util.start_service_from_config(role(STORAGE_BINARY, settings, log, ready))?)
}

/// Starts the storage binary on `settings` and waits for its exit, its log read.
fn to_exit(
    w: &mut Steps,
    util: &ServiceUtil,
    settings: &[(String, String)],
    name: &str,
) -> TestResult {
    let log = scratch(name);
    let pid =
        util.start_service_from_config(role(STORAGE_BINARY, settings, &log, WaitStrategy::NoWait))?;
    w.exited = Some(util.wait_for_exit(pid, Duration::from_secs(START_SECONDS))?);
    w.logs.push(std::fs::read_to_string(&log)?);
    Ok(())
}

/// The placement the system group answers at [`SYSTEM_ADDRESS`]: each range's start, end and partition.
fn placement() -> TestResult<Vec<(u32, u32, u32)>> {
    Runtime::new()?.block_on(async {
        let target = Address::parse(&format!("tcp://{SYSTEM_ADDRESS}"))?;
        let mut client = RoleClient::dial(&target, CLUSTER, "storage", SECRET).await?;
        let reply = client.request(&Placement::request(0)).await?;
        let view = reply.view()?;
        let placed = PlacementView::from_frame(&view)?;
        Ok((0..placed.range_count())
            .filter_map(|index| placed.range(index))
            .map(|range| (range.start(), range.end(), range.partition()))
            .collect())
    })
}

/// A subject of `partitions` partitions that `partition` homes.
fn subject_of(partition: u32, partitions: u32) -> TestResult<String> {
    let map = PlacementMap::even(partitions, NodeId::new(1))?;
    (0..100_000)
        .map(|at| format!("http://ex.org/s{at}"))
        .find(|iri| {
            iri_bucket(TENANT, iri)
                .is_some_and(|bucket| map.partition_of(bucket, Epoch::ZERO).get() == partition)
        })
        .context("a subject of the partition")
}

/// A Write of `subject` `<p>` `object`, the object an IRI.
fn edge_write(subject: &str, object: &str) -> Write {
    let mut table = TermTable::new();
    let s = table.iri(subject);
    let p = table.iri("http://ex.org/p");
    let o = table.iri(object);
    Write::new(
        table,
        vec![WriteChange::Insert {
            graph: None,
            subject: s,
            predicate: p,
            object: o,
        }],
    )
}

/// Commits `write` in a transaction bound to `partition` on `client`, through no session: its commit version.
async fn commit_on(client: &mut RoleClient, partition: u32, write: &Write) -> TestResult<u64> {
    let bind = Bind::new(partition, BindKind::Transaction, TENANT, 0, 1, [7; 16]);
    drop(client.request(&bind).await?);
    let stream = client.last_stream();
    client.send_on(stream, write).await?;
    let held = client.next_frame().await?;
    refused_at(partition, &held)?;
    drop(held);
    client.send_on(stream, &Commit::new([0; 16], 0, 0)).await?;
    let answer = client.next_frame().await?;
    refused_at(partition, &answer)?;
    Ok(CommitView::from_frame(&answer.view()?)?.commit_version())
}

/// Commits `count` writes of distinct objects to `partition` at `address`, each in a round of its own: the last version.
fn commits(address: &str, partition: u32, partitions: u32, count: u32) -> TestResult<u64> {
    let subject = subject_of(partition, partitions)?;
    Runtime::new()?.block_on(async {
        let target = Address::parse(address)?;
        let mut client = RoleClient::dial(&target, CLUSTER, "frontend", SECRET).await?;
        let mut version = 0;
        for at in 0..count {
            let write = edge_write(&subject, &format!("http://ex.org/o{at}"));
            version = commit_on(&mut client, partition, &write).await?;
        }
        Ok(version)
    })
}

/// Fails on `frame` when it is an Error of `partition`.
fn refused_at(partition: u32, frame: &ReceivedFrame) -> TestResult {
    let view = frame.view()?;
    if view.command() == Command::Error {
        let refusal = ErrorReplyView::from_frame(&view)?;
        return Err(TestError::new(format!(
            "partition {partition} refused: {}",
            refusal.message()
        )));
    }
    Ok(())
}

/// The rounds the last recovery in `log` says its partition replayed.
fn rounds_replayed(log: &str) -> TestResult<u64> {
    log.lines()
        .rev()
        .filter(|line| line.contains("recovered") && line.contains("partition="))
        .filter_map(|line| line.split("rounds=").nth(1))
        .filter_map(|rest| rest.split_whitespace().next())
        .find_map(|count| count.parse().ok())
        .context("a partition's recovery in the log")
}

#[when("`storage_1` holds a `[system]` table and `storage_2` none, and both start")]
fn two_instances(w: &mut Steps) -> TestResult {
    let util = services()?;
    let first = hosting((1, 2, FIRST_PORT, FIRST_OBSERVED), 4, &fresh_data())?;
    let second = instance(3, 2, SECOND_PORT, SECOND_OBSERVED)?;
    let (one, two) = (scratch("storage-1-hosting.log"), scratch("storage-2.log"));
    let _first: u32 = started(&util, &first, &one, FIRST_OBSERVED)?;
    let _second: u32 = started(&util, &second, &two, SECOND_OBSERVED)?;
    w.logs = vec![
        std::fs::read_to_string(&one)?,
        std::fs::read_to_string(&two)?,
    ];
    w.placements.push(placement()?);
    w.roles = Some(util);
    Ok(())
}

#[then(
    "`storage_1` listens at `system.role_address` and answers a Placement request there, and `storage_2` listens at its partitions' addresses alone"
)]
fn hosting_answers(w: &mut Steps) -> TestResult {
    let answered = w.placements.first().context("a Placement answer")?;
    let partitions: std::collections::BTreeSet<u32> = answered
        .iter()
        .map(|(_, _, partition)| *partition)
        .collect();
    assert_eq!(partitions, (1..=4).collect(), "{answered:?}");
    let (one, two) = (
        w.logs.first().context("a log")?,
        w.logs.get(1).context("a log")?,
    );
    assert!(one.contains("the system group listens"), "{one}");
    assert!(!two.contains("the system group listens"), "{two}");
    Runtime::new()?.block_on(async {
        for port in [SECOND_PORT, SECOND_PORT + 1] {
            let target = Address::parse(&format!("tcp://127.0.0.1:{port}"))?;
            drop(RoleClient::dial(&target, CLUSTER, "frontend", SECRET).await?);
        }
        Ok::<(), TestError>(())
    })?;
    w.roles = None;
    Ok(())
}

#[when("a storage instance's `[system]` table names `role_address` and no `data_dir`")]
fn no_system_dir(w: &mut Steps) -> TestResult {
    let util = services()?;
    let settings = hosting((1, 1, FIRST_PORT, FIRST_OBSERVED), 1, &fresh_data())?;
    let settings: Vec<(String, String)> = settings
        .into_iter()
        .filter(|(name, _)| name != "system.data_dir")
        .collect();
    to_exit(w, &util, &settings, "storage-no-system-dir.log")
}

#[then("the process exits with code 78 and an error naming `system.data_dir`")]
fn names_system_dir(w: &mut Steps) -> TestResult {
    let log = w.logs.first().context("a log")?;
    exit_is(w.exited, log, 78)?;
    assert!(log.contains("system.data_dir"), "{log}");
    Ok(())
}

#[when(r#"the development container has started with `system.data_dir = "/data/system"`"#)]
fn data_layout(w: &mut Steps) -> TestResult {
    let util = services()?;
    let root = fresh_data();
    let settings = hosting(
        (1, 1, FIRST_PORT, FIRST_OBSERVED),
        1,
        &format!("{root}/system"),
    )?;
    let settings = with(settings, "storage.data_dir", &format!("{root}/storage"));
    let _pid: u32 = started(
        &util,
        &settings,
        &scratch("storage-layout.log"),
        FIRST_OBSERVED,
    )?;
    w.data = root;
    w.roles = Some(util);
    Ok(())
}

#[then("`/data` holds `system/meta/`, `system/seq/` and `storage/p1/r1/`")]
fn data_holds(w: &mut Steps) -> TestResult {
    for held in ["system/meta", "system/seq", "storage/p1/r1"] {
        let path = Path::new(&w.data).join(held);
        assert!(path.is_dir(), "{} is no directory", path.display());
    }
    w.roles = None;
    Ok(())
}

#[when("the config service and the hosting storage process start, and no other role does")]
fn storage_alone(w: &mut Steps) -> TestResult {
    let util = services()?;
    let settings = hosting((1, 1, FIRST_PORT, FIRST_OBSERVED), 1, &fresh_data())?;
    // The target declares the frontend, as every target does (config-service, Roles); it is not started.
    let _service: u32 = start_config_service(
        &util,
        &[
            (
                "frontend",
                &with(
                    frontend_holding()?,
                    "cluster.system_address",
                    &format!("tcp://{SYSTEM_ADDRESS}"),
                ),
            ),
            ("storage_1", &settings),
        ],
    )?;
    let health = answering(FIRST_OBSERVED, "/health");
    let log = scratch("storage-alone.log");
    let _pid: u32 =
        util.start_service_from_config(role_of_service(STORAGE_BINARY, "storage_1", &log, health))?;
    until_answering(FIRST_OBSERVED, "/ready")?;
    w.status = http_status(FIRST_OBSERVED, "/ready")?;
    w.roles = Some(util);
    Ok(())
}

#[then("the storage process's `/ready` answers")]
fn ready_alone(w: &mut Steps) -> TestResult {
    assert_eq!(w.status, 200);
    w.roles = None;
    Ok(())
}

#[when(
    "the system group's host with `partition_count` 4 starts on a metadata group holding no placement"
)]
fn forms_placement(w: &mut Steps) -> TestResult {
    let util = services()?;
    let settings = hosting((1, 4, FIRST_PORT, FIRST_OBSERVED), 4, &fresh_data())?;
    let pid = started(
        &util,
        &settings,
        &scratch("storage-forms.log"),
        FIRST_OBSERVED,
    )?;
    w.placements.push(placement()?);
    w.exited = Some(util.stop_service(pid)?);
    let _again: u32 = started(
        &util,
        &settings,
        &scratch("storage-forms-again.log"),
        FIRST_OBSERVED,
    )?;
    w.placements.push(placement()?);
    w.roles = Some(util);
    Ok(())
}

#[then("the metadata group holds an even map over 4 partitions, and a restart forms none")]
fn even_map(w: &mut Steps) -> TestResult {
    let formed = w.placements.first().context("the first placement")?;
    // Even: ranges covering the buckets from 0 without a gap, each of partitions 1 to 4 holding as many buckets as any other.
    assert_eq!(
        formed.first().map(|(start, _, _)| *start),
        Some(0),
        "{formed:?}"
    );
    let contiguous = formed.windows(2).all(|pair| match pair {
        [(_, end, _), (start, _, _)] => end == start,
        _ => false,
    });
    assert!(contiguous, "{formed:?}");
    let mut held = std::collections::BTreeMap::new();
    for (start, end, partition) in formed {
        *held.entry(*partition).or_insert(0u64) += u64::from(end - start);
    }
    let partitions: Vec<u32> = held.keys().copied().collect();
    assert_eq!(partitions, vec![1, 2, 3, 4], "{held:?}");
    let sizes: std::collections::BTreeSet<u64> = held.values().copied().collect();
    assert_eq!(
        sizes.len(),
        1,
        "every partition holds as many buckets: {held:?}"
    );
    assert_eq!(w.placements.get(1), Some(formed), "a restart forms none");
    w.roles = None;
    Ok(())
}

#[when("one storage process claims partitions 1 and 2 and another claims 3 and 4")]
fn claims(w: &mut Steps) -> TestResult {
    let util = services()?;
    let first = hosting((1, 2, FIRST_PORT, FIRST_OBSERVED), 4, &fresh_data())?;
    let second = instance(3, 2, SECOND_PORT, SECOND_OBSERVED)?;
    let _first: u32 = started(
        &util,
        &first,
        &scratch("storage-claims-1.log"),
        FIRST_OBSERVED,
    )?;
    let _second: u32 = started(
        &util,
        &second,
        &scratch("storage-claims-3.log"),
        SECOND_OBSERVED,
    )?;
    w.roles = Some(util);
    Ok(())
}

#[then(
    "both join, Members names each partition's address, and a write reaches its partition on either process"
)]
fn on_either(w: &mut Steps) -> TestResult {
    let members = members_at(&format!("tcp://{SYSTEM_ADDRESS}"))?;
    let expected: Vec<(RoleKind, u32, String)> =
        [(1, 17_200), (2, 17_201), (3, 17_300), (4, 17_301)]
            .map(|(partition, port)| {
                (
                    RoleKind::Storage,
                    partition,
                    format!("tcp://127.0.0.1:{port}"),
                )
            })
            .to_vec();
    assert_eq!(members, expected);
    for (_, partition, address) in &members {
        assert!(
            commits(address, *partition, 4, 1)? > 0,
            "partition {partition}"
        );
    }
    w.roles = None;
    Ok(())
}

#[when("one storage process holds partition 2 and another claims it")]
fn held_claim(w: &mut Steps) -> TestResult {
    let util = services()?;
    let first = hosting((1, 2, FIRST_PORT, FIRST_OBSERVED), 4, &fresh_data())?;
    let _first: u32 = started(
        &util,
        &first,
        &scratch("storage-holder.log"),
        FIRST_OBSERVED,
    )?;
    w.placements.push(placement()?);
    let claim = with(
        instance(2, 1, SECOND_PORT, SECOND_OBSERVED)?,
        "cluster.join_deadline_ms",
        SHORT_DEADLINE_MS,
    );
    to_exit(w, &util, &claim, "storage-claim.log")?;
    w.placements.push(placement()?);
    w.roles = Some(util);
    Ok(())
}

#[then(
    "the claim is refused with an error naming partition 2 and its holder, and the placement is unchanged"
)]
fn claim_refused(w: &mut Steps) -> TestResult {
    let log = w.logs.first().context("the claim's log")?;
    exit_is(w.exited, log, 69)?;
    assert!(log.contains("partition 2"), "{log}");
    assert!(
        log.contains("127.0.0.1:17201"),
        "the holder's address: {log}"
    );
    assert_eq!(
        w.placements.first(),
        w.placements.get(1),
        "the placement is unchanged"
    );
    w.roles = None;
    Ok(())
}

#[when("a storage process holding partitions 1 and 2 restarts at the same addresses")]
fn restarts(w: &mut Steps) -> TestResult {
    let util = services()?;
    let host = hosting((3, 2, SECOND_PORT, SECOND_OBSERVED), 4, &fresh_data())?;
    let holder = instance(1, 2, FIRST_PORT, FIRST_OBSERVED)?;
    let _host: u32 = started(
        &util,
        &host,
        &scratch("storage-restart-host.log"),
        SECOND_OBSERVED,
    )?;
    let pid = started(
        &util,
        &holder,
        &scratch("storage-restart-1.log"),
        FIRST_OBSERVED,
    )?;
    w.exited = Some(util.stop_service(pid)?);
    let _again: u32 = started(
        &util,
        &holder,
        &scratch("storage-restart-2.log"),
        FIRST_OBSERVED,
    )?;
    w.status = http_status(FIRST_OBSERVED, "/ready")?;
    w.roles = Some(util);
    Ok(())
}

#[then("its claim succeeds")]
fn claimed_again(w: &mut Steps) -> TestResult {
    assert_eq!(w.status, 200);
    let members = members_at(&format!("tcp://{SYSTEM_ADDRESS}"))?;
    let own: Vec<(u32, String)> = members
        .into_iter()
        .filter(|(_, partition, _)| *partition <= 2)
        .map(|(_, partition, address)| (partition, address))
        .collect();
    assert_eq!(
        own,
        vec![
            (1, "tcp://127.0.0.1:17200".to_string()),
            (2, "tcp://127.0.0.1:17201".to_string())
        ]
    );
    w.roles = None;
    Ok(())
}

#[when("a storage process restarts with 10,000 rounds to replay")]
fn restarts_with_rounds(w: &mut Steps) -> TestResult {
    let util = services()?;
    let settings = hosting((1, 1, FIRST_PORT, FIRST_OBSERVED), 1, &fresh_data())?;
    let pid = started(
        &util,
        &settings,
        &scratch("storage-rounds.log"),
        FIRST_OBSERVED,
    )?;
    let _version = commits(&format!("tcp://127.0.0.1:{FIRST_PORT}"), 1, 1, ROUNDS)?;
    let killed = std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()?;
    assert!(killed.success());
    w.exited = Some(util.wait_for_exit(pid, Duration::from_secs(START_SECONDS))?);
    let again = scratch("storage-rounds-again.log");
    let health = answering(FIRST_OBSERVED, "/health");
    let _restarted: u32 =
        util.start_service_from_config(role(STORAGE_BINARY, &settings, &again, health))?;
    until_answering(FIRST_OBSERVED, "/ready")?;
    w.logs = vec![std::fs::read_to_string(&again)?];
    w.roles = Some(util);
    Ok(())
}

#[then("`/health` answers once it holds its settings, and `/ready` answers only after the replay")]
fn ready_after_replay(w: &mut Steps) -> TestResult {
    let log = w.logs.first().context("the restart's log")?;
    assert!(rounds_replayed(log)? > 0, "{log}");
    let recovered = log.find("recovered").context("a recovery")?;
    let ready = log.find("ready partitions=").context("the role ready")?;
    assert!(recovered < ready, "{log}");
    w.roles = None;
    Ok(())
}

#[when("storage receives SIGTERM after acknowledging commits")]
fn flushing(w: &mut Steps) -> TestResult {
    let util = services()?;
    let settings = hosting((1, 1, FIRST_PORT, FIRST_OBSERVED), 1, &fresh_data())?;
    let pid = started(
        &util,
        &settings,
        &scratch("storage-flush.log"),
        FIRST_OBSERVED,
    )?;
    let _version = commits(&format!("tcp://127.0.0.1:{FIRST_PORT}"), 1, 1, 8)?;
    w.exited = Some(util.stop_service(pid)?);
    let again = scratch("storage-flush-again.log");
    let _restarted: u32 = started(&util, &settings, &again, FIRST_OBSERVED)?;
    w.logs = vec![std::fs::read_to_string(&again)?];
    w.roles = Some(util);
    Ok(())
}

#[then(
    "every acknowledged commit is applied to its store before it exits, and a restart replays nothing"
)]
fn nothing_to_replay(w: &mut Steps) -> TestResult {
    let log = w.logs.first().context("the restart's log")?;
    exit_is(w.exited, log, 0)?;
    assert_eq!(rounds_replayed(log)?, 0, "{log}");
    w.roles = None;
    Ok(())
}

fn main() -> TestResult {
    // Which of the `@system` features this target runs, by their second tag: `hosting` or `partitions` (`BUILD.bazel`).
    let group =
        std::env::var("SYSTEM_FEATURES").context("SYSTEM_FEATURES names the features' tag")?;
    start_double()?;
    futures::executor::block_on(
        Steps::cucumber()
            .max_concurrent_scenarios(1)
            .before(|_, _, _, _| {
                async {
                    let freed = until_ports_free(&OWN_ADDRESSES);
                    assert!(freed.is_ok(), "{freed:?}");
                }
                .boxed_local()
            })
            .fail_on_skipped()
            .filter_run_and_exit(
                ultragraphdb_tool_bdd::features_dir()?,
                move |feature, _, _| {
                    let tagged = |name: &str| feature.tags.iter().any(|tag| tag == name);
                    tagged("system") && tagged(&group)
                },
            ),
    );
    Ok(())
}
