/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The role binaries' scenarios, bound to `features/role_binaries/`. One scenario runs at a time: the binaries listen at
//! their declared ports.

use std::process::ExitStatus;

use cucumber::{World, then, when};
use ultragraphdb_compute_specs as compute;
use ultragraphdb_frontend_specs as frontend;
use ultragraphdb_storage_specs as storage;
use ultragraphdb_tests::{RoleProcess, http_status};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

const GRAPHS: [&str; 2] = [
    "ultragraphdb/ultragraphdb_tests/frontend_deps",
    "ultragraphdb/ultragraphdb_tests/compute_deps",
];

#[derive(Debug, Default, World)]
struct Steps {
    roles: Vec<RoleProcess>,
    exited: Option<ExitStatus>,
    log: String,
    graph: String,
}

/// The settings file of `text`, in the test's own directory.
fn settings_file(text: &str) -> TestResult<String> {
    let root =
        std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, std::path::PathBuf::from);
    let file = root.join(format!("settings-{}.toml", std::process::id()));
    std::fs::write(&file, text)?;
    Ok(file.display().to_string())
}

/// Runs the storage binary on `text` to its exit.
fn storage_on(w: &mut Steps, text: &str) -> TestResult {
    let file = settings_file(text)?;
    let mut role = RoleProcess::start(storage::BINARY, &["--config", &file], &storage::settings())?;
    w.exited = Some(role.wait_exit()?);
    w.log = role.log()?;
    Ok(())
}

fn exited_with(w: &Steps, code: i32) -> TestResult {
    let status = w.exited.context("an exit")?;
    assert_eq!(status.code(), Some(code), "{}", w.log);
    Ok(())
}

#[when("the frontend starts, then storage, then compute")]
fn start_all(w: &mut Steps) -> TestResult {
    let mut first = RoleProcess::start(frontend::BINARY, &[], &frontend::settings())?;
    first.wait_for(frontend::OBSERVABILITY, "/health")?;
    let mut second = RoleProcess::start(storage::BINARY, &[], &storage::settings())?;
    second.wait_for(storage::OBSERVABILITY, "/ready")?;
    let mut third = RoleProcess::start(compute::BINARY, &[], &compute::settings())?;
    third.wait_for(compute::OBSERVABILITY, "/health")?;
    w.roles = vec![first, second, third];
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
    w.roles.clear();
    Ok(())
}

#[when("a role is running and receives SIGTERM")]
fn terminate(w: &mut Steps) -> TestResult {
    let mut role = RoleProcess::start(compute::BINARY, &[], &compute::settings())?;
    role.wait_for(compute::OBSERVABILITY, "/health")?;
    w.exited = Some(role.terminate()?);
    w.log = role.log()?;
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
    let mut role = RoleProcess::start(storage::BINARY, &[], &variables)?;
    role.wait_for(storage::OBSERVABILITY, "/health")?;
    w.log = role.log()?;
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
