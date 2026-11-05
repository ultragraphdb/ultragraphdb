/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The compute binary as a process, bound to `features/`, with no container. One scenario runs at a time: the binaries listen
//! at their declared ports.

use compio::runtime::Runtime;
use cucumber::{World, then, when};
use service_utils::{ServiceUtil, WaitStrategy};
use std::path::Path;
use std::process::ExitStatus;
use std::time::Duration;
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{ComputeRead, PatternSlot, QuadPattern, ReadLevel, TermTable};
use ultragraphdb_tests::{
    CLUSTER, COMPUTE_BINARY, COMPUTE_OBSERVABILITY, COMPUTE_ROLE_ADDRESS, DOUBLE_ADDRESS,
    FRONTEND_BINARY, FRONTEND_OBSERVABILITY, SECRET, START_SECONDS, STORAGE_BINARY,
    STORAGE_OBSERVABILITY, answering, bootstrap, compute_on_one_host, exit_is, frontend_holding,
    holding, http_status, process, render_settings, role, scratch, served_by_double, start_double,
    storage_holding, until_answering,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

/// Where the sandbox holds the binaries, relative to the test's working directory: `copy_file` writes them to this package.
const ROOT_PATH: &str = "ultragraphdb/ultragraphdb_compute_tests";

fn services() -> TestResult<ServiceUtil> {
    Ok(ServiceUtil::new(
        ROOT_PATH,
        vec![FRONTEND_BINARY, COMPUTE_BINARY, STORAGE_BINARY],
    )?)
}

#[derive(Debug, Default, World)]
struct Steps {
    exited: Option<ExitStatus>,
    graph: String,
    log: String,
    roles: Option<ServiceUtil>,
}

#[when("a role is running and receives SIGTERM")]
fn terminate(w: &mut Steps) -> TestResult {
    let util = services()?;
    let log = scratch("compute-stopped.log");
    let wait = answering(COMPUTE_OBSERVABILITY, "/health");
    let settings = holding(compute_on_one_host(), SECRET)?;
    let pid = util.start_service_from_config(role(COMPUTE_BINARY, &settings, &log, wait))?;
    w.exited = Some(util.stop_service(pid)?);
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

#[then("it exits with code 0")]
fn exits_clean(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 0)
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

#[when("the compute binary receives settings with no `compute.role_address`")]
fn no_compute_address(w: &mut Steps) -> TestResult {
    let mut settings = holding(compute_on_one_host(), SECRET)?;
    settings.retain(|(name, _)| name != "compute.role_address");
    let log = scratch("compute-no-address.log");
    let util = services()?;
    let instance = served_by_double(COMPUTE_BINARY, &render_settings(&settings));
    let pid = util.start_service_from_config(process(
        COMPUTE_BINARY,
        bootstrap(DOUBLE_ADDRESS, &instance),
        &log,
        WaitStrategy::NoWait,
    ))?;
    w.exited = Some(util.wait_for_exit(pid, Duration::from_secs(START_SECONDS))?);
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

#[then("it exits with code 78 and an error naming `compute.role_address`")]
fn names_compute_address(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 78)?;
    assert!(w.log.contains("compute.role_address"), "{}", w.log);
    Ok(())
}

#[when("the compute binary's dependency graph is listed")]
fn compute_graph(w: &mut Steps) -> TestResult {
    w.graph = std::fs::read_to_string(Path::new(ROOT_PATH).join("compute_deps"))?;
    Ok(())
}

/// Starts compute as a process joining a frontend, served by the double.
fn compute_joining(util: &ServiceUtil) -> TestResult {
    let _frontend: u32 = util.start_service_from_config(role(
        FRONTEND_BINARY,
        &frontend_holding()?,
        &scratch(FRONTEND_BINARY),
        answering(FRONTEND_OBSERVABILITY, "/health"),
    ))?;
    let _compute: u32 = util.start_service_from_config(role(
        COMPUTE_BINARY,
        &holding(compute_on_one_host(), SECRET)?,
        &scratch(COMPUTE_BINARY),
        answering(COMPUTE_OBSERVABILITY, "/health"),
    ))?;
    Ok(())
}

#[when("compute has joined a frontend no partition has joined")]
fn compute_alone(w: &mut Steps) -> TestResult {
    let util = services()?;
    compute_joining(&util)?;
    w.roles = Some(util);
    Ok(())
}

#[then("its `/health` answers, its `/ready` does not, and it turns ready once a partition joins")]
fn ready_after_a_partition(w: &mut Steps) -> TestResult {
    assert_eq!(http_status(COMPUTE_OBSERVABILITY, "/health")?, 200);
    assert_eq!(http_status(COMPUTE_OBSERVABILITY, "/ready")?, 503);
    let util = w.roles.as_ref().context("the roles")?;
    let _storage: u32 = util.start_service_from_config(role(
        STORAGE_BINARY,
        &storage_holding()?,
        &scratch(STORAGE_BINARY),
        answering(STORAGE_OBSERVABILITY, "/ready"),
    ))?;
    until_answering(COMPUTE_OBSERVABILITY, "/ready")?;
    w.roles = None;
    Ok(())
}

#[when("a role asks compute for a command it does not host")]
fn asks_compute(w: &mut Steps) -> TestResult {
    let util = services()?;
    compute_joining(&util)?;
    w.log = Runtime::new()?.block_on(async {
        let target = Address::parse(&format!("tcp://{COMPUTE_ROLE_ADDRESS}"))?;
        let mut client = RoleClient::dial(&target, CLUSTER, "frontend", SECRET).await?;
        let mut terms = TermTable::new();
        let predicate = terms.iri("http://ex.org/p");
        let pattern = QuadPattern::new(
            PatternSlot::Unbound,
            PatternSlot::Unbound,
            PatternSlot::Bound(predicate),
            PatternSlot::Unbound,
        );
        let read = ComputeRead::new(
            1,
            true,
            ReadLevel::MonotonicFast,
            terms,
            pattern,
            Vec::new(),
        );
        let refused = client.request(&read).await.err();
        let refused = refused.context("a refusal")?;
        Ok::<String, TestError>(refused.to_string())
    })?;
    w.roles = Some(util);
    Ok(())
}

#[then("the connection authenticates and the command is refused as not hosted")]
fn not_hosted(w: &mut Steps) {
    assert!(w.log.contains("not hosted"), "{}", w.log);
    w.roles = None;
}

#[when("a role asks compute with a different role secret")]
fn wrong_secret_at_compute(w: &mut Steps) -> TestResult {
    let util = services()?;
    compute_joining(&util)?;
    w.log = Runtime::new()?.block_on(async {
        let target = Address::parse(&format!("tcp://{COMPUTE_ROLE_ADDRESS}"))?;
        let refused = RoleClient::dial(&target, CLUSTER, "frontend", "another secret")
            .await
            .err()
            .context("a refusal")?;
        Ok::<String, TestError>(refused.to_string())
    })?;
    w.roles = Some(util);
    Ok(())
}

#[then("its authentication is refused")]
fn refused_at_compute(w: &mut Steps) {
    assert!(w.log.contains("authentication failed"), "{}", w.log);
    w.roles = None;
}

fn main() -> TestResult {
    start_double()?;
    futures::executor::block_on(
        Steps::cucumber()
            .max_concurrent_scenarios(1)
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
