/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Whole clusters as processes, every role started through the config service, bound to `features/`, with no container. One
//! scenario runs at a time: the binaries listen at their declared ports. Every scenario runs through the compio shell over
//! TCP; those tagged `both_carriers` again with every address a Unix socket; those tagged `core` again over TLS, and through
//! the tokio shell over TCP and over TLS; those tagged `both_shells`, which lose frames on a link between the client and the
//! frontend, through the tokio shell over TCP; and those tagged `over_tls`, of a TLS endpoint, over TLS alone.

mod client_reads;
mod cluster;
mod conformance_steps;
mod endpoint_steps;
mod failure_steps;
mod group_link;
mod kubernetes_steps;
mod limit_steps;
mod lossy_link;
mod operation_steps;
mod own_steps;
mod read_steps;
mod request_steps;
mod resume_steps;
mod session_steps;
mod shells;
mod spread_steps;
mod tightening_steps;
mod transaction_steps;
mod write_read_steps;

use cucumber::{World, then, when};
use service_utils::ServiceUtil;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use ultragraphdb_protocol_wire::RoleKind;
use ultragraphdb_tests::{
    COMPUTE_BINARY, CONFIG_BINARY, STORAGE_BINARY, answering, compute_holding,
    compute_observability, config_address, http_status, members_at, role_of_service, scratch,
    start_config_service, storage_holding, storage_observability, storage_role_address,
    system_role_address, with,
};
use ultragraphdb_tool_bdd::TestResult;

/// Where the sandbox holds the binaries, relative to the test's working directory: `copy_file` writes them to this package.
const ROOT_PATH: &str = "ultragraphdb/ultragraphdb_tests";

pub(crate) fn services() -> TestResult<ServiceUtil> {
    Ok(ServiceUtil::new(
        ROOT_PATH,
        vec![CONFIG_BINARY, COMPUTE_BINARY, STORAGE_BINARY],
    )?)
}

/// Whether this run's clusters take Unix sockets, as the second run's do.
static OVER_UNIX: AtomicBool = AtomicBool::new(false);

/// Whether this run's clusters take TLS on their client endpoints, as the TLS runs' do.
static OVER_TLS: AtomicBool = AtomicBool::new(false);

/// The scenarios begun so far, which number each scenario's IRIs in the cluster its run shares.
static SCENARIOS: AtomicU32 = AtomicU32::new(0);

/// The carrier of the run under way.
pub(crate) fn current_carrier() -> cluster::Carriers {
    match (
        OVER_UNIX.load(Ordering::Relaxed),
        OVER_TLS.load(Ordering::Relaxed),
    ) {
        (true, _) => cluster::Carriers::Unix,
        (false, true) => cluster::Carriers::Tls,
        (false, false) => cluster::Carriers::Tcp,
    }
}

#[derive(Debug, Default, World)]
#[world(init = Self::new)]
struct Steps {
    answers: Vec<client_reads::Answer>,
    carrier: cluster::Carriers,
    cluster: Option<cluster::Cluster>,
    committed: bool,
    conversation: Option<conformance_steps::Conversation>,
    described: Option<ultragraphdb_protocol_wire::Describe>,
    failures: failure_steps::Seen,
    /// The system group's role address, in the storage process hosting it.
    system: String,
    kind: Option<u64>,
    link: Option<lossy_link::LossyLink>,
    named: Option<String>,
    roles: Option<ServiceUtil>,
    routed: Vec<write_read_steps::Routed>,
    scenario: u32,
    seen: client_reads::Seen,
    sockets: Vec<PathBuf>,
    storage: String,
    tightening: tightening_steps::Seen,
}

impl Steps {
    /// A scenario's world over this run's carrier, its IRIs numbered apart from every other scenario's.
    fn new() -> Self {
        Self {
            carrier: current_carrier(),
            scenario: SCENARIOS.fetch_add(1, Ordering::Relaxed),
            ..Self::default()
        }
    }

    /// The IRI `http://ex.org/<scenario>/<name>` of this scenario alone.
    fn iri(&self, name: &str) -> String {
        format!("http://ex.org/{}/{name}", self.scenario)
    }

    /// The carrier a failure happened over.
    fn over(&self) -> String {
        format!(
            "over {:?} through {:?}",
            self.carrier,
            shells::Shell::current()
        )
    }
}

/// Starts the config service, then storage, then compute, each once the one before is ready, as the supervisor orders them,
/// on the settings given; and holds whether storage's `/ready` answered before compute started and compute's while storage's
/// did, at once on its start.
fn start_in_order(
    w: &mut Steps,
    storage_settings: &[(String, String)],
    compute_settings: &[(String, String)],
) -> TestResult {
    let util = services()?;
    let _service: u32 = start_config_service(
        &util,
        &[("storage", storage_settings), ("compute", compute_settings)],
    )?;
    let config_first = http_status(&config_address()?, "/ready")? == 200;
    let storage = storage_observability()?;
    let storage_ready = answering(&storage, "/ready");
    let started = role_of_service(
        STORAGE_BINARY,
        "storage",
        &scratch(STORAGE_BINARY),
        storage_ready,
    )?;
    let _storage: u32 = util.start_service_from_config(started)?;
    let storage_first = http_status(&storage, "/ready")? == 200;
    let compute_ready = answering(&compute_observability()?, "/ready");
    let started = role_of_service(
        COMPUTE_BINARY,
        "compute",
        &scratch(COMPUTE_BINARY),
        compute_ready,
    )?;
    let _compute: u32 = util.start_service_from_config(started)?;
    w.committed = config_first && storage_first;
    w.roles = Some(util);
    Ok(())
}

#[when("the supervisor starts the config service and the two roles")]
fn start_all(w: &mut Steps) -> TestResult {
    w.system = format!("tcp://{}", system_role_address()?);
    w.storage = format!("tcp://{}", storage_role_address()?);
    start_in_order(w, &storage_holding()?, &compute_holding()?)
}

#[when("the storage process starts, then compute, each at a Unix socket")]
fn start_all_unix(w: &mut Steps) -> TestResult {
    // A socket's path has a short limit, which the sandbox's own directory passes.
    let socket =
        |name: &str| PathBuf::from(format!("/tmp/ugdb-{name}-{}.sock", std::process::id()));
    let paths = [socket("g"), socket("s")];
    let [g, st] = paths
        .each_ref()
        .map(|path| format!("unix://{}", path.display()));
    let partition_socket = PathBuf::from(format!("/tmp/ugdb-s-{}-p1.sock", std::process::id()));
    let dialled = "cluster.system_address";
    let storage_settings = with(
        with(
            with(storage_holding()?, dialled, &g),
            "system.role_address",
            &g,
        ),
        "storage.role_address",
        &st,
    );
    let compute_settings = with(compute_holding()?, dialled, &g);
    (w.system, w.storage) = (g, format!("unix://{}", partition_socket.display()));
    let [g_path, _] = paths;
    w.sockets = vec![g_path, partition_socket];
    start_in_order(w, &storage_settings, &compute_settings)
}

#[then(
    "the config service answers first, storage's `/ready` answers before compute is started, compute's answers last, and no role waited on one that was waiting on it"
)]
fn all_ready(w: &mut Steps) -> TestResult {
    assert!(
        w.committed,
        "the config service, then storage, were ready first"
    );
    assert_eq!(http_status(&storage_observability()?, "/ready")?, 200);
    assert_eq!(http_status(&compute_observability()?, "/ready")?, 200);
    Ok(())
}

#[then(
    "every role's `/ready` answers, compute's last, and the system group's members name storage alone"
)]
fn all_ready_over_unix(w: &mut Steps) -> TestResult {
    all_ready(w)?;
    assert_eq!(http_status(&compute_observability()?, "/other")?, 404);
    // Compute sends the group no Join until the node list is built (C6.28): Members names the storage process alone.
    let expected = vec![(RoleKind::Storage, 1, w.storage.clone())];
    assert_eq!(members_at(&w.system)?, expected);
    Ok(())
}

#[then("each role's socket has mode 0600")]
fn private_sockets(w: &mut Steps) -> TestResult {
    for socket in &w.sockets {
        let mode = std::fs::metadata(socket)?.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "{}", socket.display());
    }
    w.roles = None;
    for socket in &w.sockets {
        std::fs::remove_file(socket)?;
    }
    Ok(())
}

/// One run of the features: its shell, its carrier, and the tags that select a scenario, every scenario of the features when
/// it names none, one tagged `over_tls` only over TLS.
struct Run {
    shell: shells::Shell,
    carrier: cluster::Carriers,
    tags: &'static [&'static str],
}

/// The runs in their order.
const RUNS: [Run; 5] = [
    Run {
        shell: shells::Shell::Compio,
        carrier: cluster::Carriers::Tcp,
        tags: &[],
    },
    Run {
        shell: shells::Shell::Compio,
        carrier: cluster::Carriers::Unix,
        tags: &["both_carriers"],
    },
    Run {
        shell: shells::Shell::Compio,
        carrier: cluster::Carriers::Tls,
        tags: &["core", "over_tls"],
    },
    Run {
        shell: shells::Shell::Tokio,
        carrier: cluster::Carriers::Tcp,
        tags: &["core", "both_shells"],
    },
    Run {
        shell: shells::Shell::Tokio,
        carrier: cluster::Carriers::Tls,
        tags: &["core", "over_tls"],
    },
];

/// Runs the features as `run` says, beside the clusters the run shares: whether any failed.
fn run_over(features: &Path, run: &Run) -> TestResult<bool> {
    println!("Through {:?} over {:?}:", run.shell, run.carrier);
    run.shell.select();
    OVER_UNIX.store(run.carrier == cluster::Carriers::Unix, Ordering::Relaxed);
    OVER_TLS.store(run.carrier == cluster::Carriers::Tls, Ordering::Relaxed);
    read_steps::forget_big();
    let (tags, carrier) = (run.tags, run.carrier);
    let summary = futures::executor::block_on(
        Steps::cucumber()
            .max_concurrent_scenarios(1)
            .fail_on_skipped()
            .filter_run(features, move |feature, _, scenario| {
                let tagged = |tag: &str| {
                    feature
                        .tags
                        .iter()
                        .chain(&scenario.tags)
                        .any(|held| held == tag)
                };
                if tagged("over_tls") {
                    return carrier == cluster::Carriers::Tls;
                }
                tags.is_empty() || tags.iter().any(|tag| tagged(tag))
            }),
    );
    shells::forget_clients()?;
    cluster::stop_shared();
    Ok(cucumber::writer::Stats::execution_has_failed(&summary)
        || cucumber::writer::Stats::hook_errors(&summary) > 0)
}

fn main() -> TestResult {
    let features = ultragraphdb_tool_bdd::features_dir()?;
    let mut failed = false;
    // `--test_env=ULTRAGRAPHDB_TESTS_RUNS=1`, with `--test_arg=--name`, runs a scenario through the first run alone: `--name` takes
    // the place of the tags that select a run's scenarios.
    let runs: usize = std::env::var("ULTRAGRAPHDB_TESTS_RUNS")
        .ok()
        .and_then(|runs| runs.parse().ok())
        .unwrap_or(RUNS.len());
    for run in RUNS.iter().take(runs) {
        failed |= run_over(&features, run)?;
    }
    if failed {
        std::process::exit(1);
    }
    Ok(())
}
