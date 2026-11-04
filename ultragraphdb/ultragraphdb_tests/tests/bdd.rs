/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The role binaries' scenarios, bound to `features/role_binaries/`. One scenario runs at a time: the binaries listen at
//! their declared ports.

use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::time::Duration;

use cucumber::{World, then, when};
use service_utils::{ServiceStartConfig, ServiceUtil, WaitStrategy};
use ultragraphdb_compute_specs as compute;
use ultragraphdb_frontend_specs as frontend;
use ultragraphdb_storage_specs as storage;
use ultragraphdb_tests::http_status;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

/// Where the sandbox holds the binaries, relative to the test's working directory: `copy_file` writes them to this package.
const ROOT_PATH: &str = "ultragraphdb/ultragraphdb_tests";

/// How long a role has to answer its first probe, or to exit. A start takes milliseconds; the margin is for a loaded machine.
const START_SECONDS: u64 = 20;

const GRAPHS: [&str; 2] = [
    "ultragraphdb/ultragraphdb_tests/frontend_deps",
    "ultragraphdb/ultragraphdb_tests/compute_deps",
];

#[derive(Debug, Default, World)]
struct Steps {
    roles: Option<ServiceUtil>,
    exited: Option<ExitStatus>,
    log: String,
    graph: String,
}

/// The file in the test's own directory named `name`.
fn scratch(name: &str) -> PathBuf {
    std::env::var_os("TEST_TMPDIR")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join(format!("{name}-{}", std::process::id()))
}

fn services() -> TestResult<ServiceUtil> {
    Ok(ServiceUtil::new(
        ROOT_PATH,
        vec![frontend::BINARY, compute::BINARY, storage::BINARY],
    )?)
}

/// What starts the binary `program` with `variables` alone in its environment, so nothing of the test's own reaches its
/// settings, its log in the file `log`, waiting as `wait_strategy` says.
fn role(
    program: &'static str,
    variables: Vec<(String, String)>,
    log: &Path,
    wait_strategy: WaitStrategy,
) -> ServiceStartConfig {
    ServiceStartConfig::builder()
        .program(program)
        .env_vars(variables)
        .clear_env(true)
        .stderr_file(log)
        .wait_strategy(wait_strategy)
        .build()
}

/// A wait for `address` to answer `path` with success.
fn answering(address: &str, path: &str) -> WaitStrategy {
    WaitStrategy::WaitForHttpHealthCheck(format!("http://{address}{path}"), START_SECONDS)
}

/// Runs the storage binary on `text` to its exit.
fn storage_on(w: &mut Steps, text: &str) -> TestResult {
    let file = scratch("settings.toml");
    std::fs::write(&file, text)?;
    // The arguments live as long as the test, as `ServiceStartConfig` asks: a test process is short.
    let path: &'static str = file.display().to_string().leak();
    let log = scratch("storage-refused.log");
    let config = ServiceStartConfig::builder()
        .program(storage::BINARY)
        .program_args(vec!["--config", path])
        .env_vars(storage::settings())
        .clear_env(true)
        .stderr_file(&log)
        .wait_strategy(WaitStrategy::NoWait)
        .build();
    let util = services()?;
    let pid = util.start_service_from_config(config)?;
    w.exited = Some(util.wait_for_exit(pid, Duration::from_secs(START_SECONDS))?);
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

fn exited_with(w: &Steps, code: i32) -> TestResult {
    let status = w.exited.context("an exit")?;
    assert_eq!(status.code(), Some(code), "{}", w.log);
    Ok(())
}

#[when("the frontend starts, then storage, then compute")]
fn start_all(w: &mut Steps) -> TestResult {
    let util = services()?;
    let starts = [
        (
            frontend::BINARY,
            frontend::settings(),
            answering(frontend::OBSERVABILITY, "/health"),
        ),
        (
            storage::BINARY,
            storage::settings(),
            answering(storage::OBSERVABILITY, "/ready"),
        ),
        (
            compute::BINARY,
            compute::settings(),
            answering(compute::OBSERVABILITY, "/health"),
        ),
    ];
    for (program, variables, wait) in starts {
        let log = scratch(program);
        let _pid: u32 = util.start_service_from_config(role(program, variables, &log, wait))?;
    }
    w.roles = Some(util);
    Ok(())
}

#[then(
    "each answers `/health`, storage answers `/ready`, and the frontend and compute answer `/ready` with 503"
)]
fn answers(w: &mut Steps) -> TestResult {
    for address in [
        frontend::OBSERVABILITY,
        compute::OBSERVABILITY,
        storage::OBSERVABILITY,
    ] {
        assert_eq!(http_status(address, "/health")?, 200, "{address}");
    }
    assert_eq!(http_status(storage::OBSERVABILITY, "/ready")?, 200);
    assert_eq!(http_status(frontend::OBSERVABILITY, "/ready")?, 503);
    assert_eq!(http_status(compute::OBSERVABILITY, "/ready")?, 503);
    assert_eq!(http_status(frontend::OBSERVABILITY, "/other")?, 404);
    w.roles = None;
    Ok(())
}

#[when("a role is running and receives SIGTERM")]
fn terminate(w: &mut Steps) -> TestResult {
    let util = services()?;
    let log = scratch("compute-stopped.log");
    let wait = answering(compute::OBSERVABILITY, "/health");
    let pid =
        util.start_service_from_config(role(compute::BINARY, compute::settings(), &log, wait))?;
    w.exited = Some(util.stop_service(pid)?);
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

#[then("it exits with code 0")]
fn exits_clean(w: &mut Steps) -> TestResult {
    exited_with(w, 0)
}

#[when(
    "the storage binary starts with a TOML file whose `[storage]` table holds the key `partiton_count`"
)]
fn unknown_setting(w: &mut Steps) -> TestResult {
    storage_on(w, "[storage]\npartiton_count = 4\n")
}

#[then("it exits with code 78 and an error naming `partiton_count`")]
fn names_the_key(w: &mut Steps) -> TestResult {
    exited_with(w, 78)?;
    assert!(w.log.contains("partiton_count"), "{}", w.log);
    Ok(())
}

#[when("the storage binary starts with a setting out of its bounds")]
fn out_of_bounds(w: &mut Steps) -> TestResult {
    storage_on(w, "[storage]\npartition_count = -1\n")
}

#[then("it exits with code 78 and an error naming the setting and the bound")]
fn names_the_bound(w: &mut Steps) -> TestResult {
    exited_with(w, 78)?;
    assert!(w.log.contains("storage.partition_count"), "{}", w.log);
    assert!(w.log.contains(&u32::MAX.to_string()), "{}", w.log);
    Ok(())
}

#[when("a role starts with `pin_cores = true`")]
fn pinned(w: &mut Steps) -> TestResult {
    let mut variables = storage::settings();
    variables.push((
        "ULTRAGRAPHDB_RUNTIME_PIN_CORES".to_string(),
        "true".to_string(),
    ));
    let util = services()?;
    let log = scratch("storage-pinned.log");
    let wait = answering(storage::OBSERVABILITY, "/health");
    let _pid: u32 = util.start_service_from_config(role(storage::BINARY, variables, &log, wait))?;
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

/// Only Linux can read a binding back here: macOS binds nothing, and a container may refuse. Each says so, and serves.
#[then("it logs what became of its binding, and serves requests")]
fn logs_the_binding(w: &mut Steps) {
    let expected: &[&str] = if cfg!(target_os = "linux") {
        &["pinned to CPU", "unpinned: asked for CPU"]
    } else {
        &["unpinned: this platform cannot bind"]
    };
    assert!(
        expected.iter().any(|line| w.log.contains(line)),
        "{}",
        w.log
    );
}

#[when("the frontend binary's dependency graph is listed")]
fn frontend_graph(w: &mut Steps) -> TestResult {
    let graph = GRAPHS.first().context("the frontend's graph")?;
    w.graph = std::fs::read_to_string(graph)?;
    Ok(())
}

#[when("the compute binary's dependency graph is listed")]
fn compute_graph(w: &mut Steps) -> TestResult {
    let graph = GRAPHS.get(1).context("compute's graph")?;
    w.graph = std::fs::read_to_string(graph)?;
    Ok(())
}

#[then("it names neither `ultragraphdb_store_kv` nor fjall")]
fn no_engine(w: &mut Steps) -> TestResult {
    if !w.graph.contains("ultragraphdb_common_service") {
        return Err(TestError::new(format!(
            "the graph is not the binary's: {}",
            w.graph
        )));
    }
    for engine in ["ultragraphdb_store_kv", "fjall"] {
        assert!(!w.graph.contains(engine), "{engine} is linked");
    }
    Ok(())
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .max_concurrent_scenarios(1)
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
