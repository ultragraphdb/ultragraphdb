/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The frontend binary as a process, bound to `features/`, with no container. One scenario runs at a time: it listens at its
//! declared ports.

mod books_steps;
mod endpoint_steps;
mod processes;
mod role_read_steps;
mod system_steps;

use compio::runtime::Runtime;
use cucumber::{World, then, when};
use futures::FutureExt;
use service_utils::{ServiceUtil, WaitStrategy};
use std::path::Path;
use std::process::ExitStatus;
use std::time::Duration;
use ultragraphdb_client_compio::{Client, ClientConfig, TcpConnector};
use ultragraphdb_common_service::BIND_RETRIES;
use ultragraphdb_protocol_scram::{ScramLogin, system_nonce};
use ultragraphdb_tests::{
    CLUSTER, COMPUTE_BINARY, FRONTEND_BINARY, PARENT_PID, SECRET, START_SECONDS, STORAGE_BINARY,
    bootstrap, client_address, double_address, exit_is, frontend_holding, frontend_observability,
    http_status, process, render_settings, role, scratch, served_by_double, start_double,
    until_answering, until_ports_free, with,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

/// Where the sandbox holds the binaries, relative to the test's working directory: `copy_file` writes them to this package.
const ROOT_PATH: &str = "ultragraphdb/ultragraphdb_frontend_tests";

/// An address in TEST-NET-3, which no host's interface holds (RFC 5737).
const NO_INTERFACE: &str = "203.0.113.1:18099";

/// The addresses a role of these scenarios holds beyond those every suite declares: the second storage process's.
fn own_addresses() -> TestResult<[(String, &'static str); 2]> {
    Ok([
        (
            processes::second_role_address()?,
            "the second storage process's partition",
        ),
        (
            processes::second_observability()?,
            "the second storage process's observability",
        ),
    ])
}

pub(crate) fn services() -> TestResult<ServiceUtil> {
    Ok(ServiceUtil::new(
        ROOT_PATH,
        vec![FRONTEND_BINARY, COMPUTE_BINARY, STORAGE_BINARY],
    )?)
}

#[derive(Debug, Default, World)]
pub(crate) struct Steps {
    pub(crate) exited: Option<ExitStatus>,
    graph: String,
    pub(crate) log: String,
    pub(crate) roles: Option<ServiceUtil>,
    pub(crate) refused: Option<String>,
    pub(crate) answered: role_read_steps::Answered,
    pub(crate) ids: Vec<Vec<u8>>,
    status: u16,
    /// The processes of a scenario over the system group, which it starts, kills and starts again.
    pub(crate) processes: processes::Roles,
    /// What a scenario's When saw, for its Then to judge.
    pub(crate) seen: Vec<String>,
}

/// A client's refusal as it reads it, logging in as `name` with `password`.
fn client_refusal(name: &str, password: &str) -> TestResult<String> {
    Runtime::new()?.block_on(async {
        let login = ScramLogin::new(name, password, system_nonce)?;
        let address = client_address()?.parse()?;
        let config = ClientConfig::new(CLUSTER, "ultragraphdb-test");
        let mut client = Client::new(TcpConnector::new(address), config, login, 16);
        let refused = client.connect().await.err().context("a refusal")?;
        Ok(refused.to_string())
    })
}

#[when("a client authenticates on the client endpoint with the role secret")]
fn client_with_secret(w: &mut Steps) -> TestResult {
    // The frontend relays an exchange to the system group, so the storage process hosting it starts first.
    endpoint_steps::started(w, &frontend_holding()?)?;
    w.log = client_refusal("storage", SECRET)?;
    Ok(())
}

#[then("authentication is refused with the same error as a wrong password")]
fn same_as_wrong_password(w: &mut Steps) -> TestResult {
    assert!(w.log.contains("authentication failed"), "{}", w.log);
    assert_eq!(w.log, client_refusal("storage", "a wrong password")?);
    w.roles = None;
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

#[then(
    "it names neither `ultragraphdb_common_system`, `ultragraphdb_cluster_metadata`, `ultragraphdb_cluster_sequencer` nor `ultragraphdb_transactions_log`"
)]
fn no_book(w: &mut Steps) -> TestResult {
    if !w.graph.contains("ultragraphdb_common_service") {
        return Err(TestError::new(format!(
            "the graph is not the binary's: {}",
            w.graph
        )));
    }
    // A crate's target is named for it, so a label naming one ends in `:<crate>`.
    for book in [
        "ultragraphdb_common_system",
        "ultragraphdb_cluster_metadata",
        "ultragraphdb_cluster_sequencer",
        "ultragraphdb_transactions_log",
    ] {
        let named: Vec<&str> = w
            .graph
            .lines()
            .filter(|label| label.contains(&format!("/{book}:")))
            .collect();
        assert!(named.is_empty(), "{book} is in the graph: {named:?}");
    }
    Ok(())
}

#[when("the frontend binary's dependency graph is listed")]
fn frontend_graph(w: &mut Steps) -> TestResult {
    w.graph = std::fs::read_to_string(Path::new(ROOT_PATH).join("frontend_deps"))?;
    Ok(())
}

#[when("the frontend binary receives settings with no `cluster.system_address`")]
fn no_system_address(w: &mut Steps) -> TestResult {
    let mut settings = frontend_holding()?;
    let before = settings.len();
    settings.retain(|(name, _)| name != "cluster.system_address");
    assert_eq!(
        settings.len() + 1,
        before,
        "the frontend's settings named the address once"
    );
    let log = scratch("frontend-no-address.log");
    let util = services()?;
    let pid = util.start_service_from_config(role(
        FRONTEND_BINARY,
        &settings,
        &log,
        WaitStrategy::NoWait,
    )?)?;
    w.exited = Some(util.wait_for_exit(pid, Duration::from_secs(START_SECONDS))?);
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

#[then("it exits with code 78 and an error naming `cluster.system_address`")]
fn names_system_address(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 78)?;
    assert!(w.log.contains("cluster.system_address"), "{}", w.log);
    Ok(())
}

#[when("a role starts with `ULTRAGRAPHDB_PARENT_PID` naming a process that is not its parent")]
fn parent_gone(w: &mut Steps) -> TestResult {
    let log = scratch("frontend-parent.log");
    let instance = served_by_double(FRONTEND_BINARY, &render_settings(&frontend_holding()?));
    // No process holds the largest identity, so it is no parent of the role's.
    let named = with(
        bootstrap(&double_address()?, &instance),
        PARENT_PID,
        &u32::MAX.to_string(),
    );
    let util = services()?;
    let pid = util.start_service_from_config(process(
        FRONTEND_BINARY,
        named,
        &log,
        WaitStrategy::NoWait,
    ))?;
    w.exited = Some(util.wait_for_exit(pid, Duration::from_secs(START_SECONDS))?);
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

#[then("it stops as on SIGTERM and exits with code 0")]
fn stopped_with_its_parent(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 0)
}

/// Starts a frontend on `settings`, served by the double, its log in `log`.
fn start_frontend(
    w: &mut Steps,
    settings: &[(String, String)],
    log: &std::path::Path,
) -> TestResult<u32> {
    let util = services()?;
    let pid = util.start_service_from_config(role(
        FRONTEND_BINARY,
        settings,
        log,
        WaitStrategy::NoWait,
    )?)?;
    w.roles = Some(util);
    Ok(pid)
}

/// The frontend `pid` of `w`, its exit and log held once it ends.
fn until_frontend_exits(w: &mut Steps, pid: u32, log: &std::path::Path) -> TestResult {
    let util = w.roles.as_ref().context("the frontend")?;
    w.exited = Some(util.wait_for_exit(pid, Duration::from_secs(START_SECONDS))?);
    w.log = std::fs::read_to_string(log)?;
    Ok(())
}

/// How many of `log`'s lines are a retry of `address`.
fn retries_of(log: &str, address: &str) -> usize {
    log.lines()
        .filter(|line| line.contains("in use") && line.contains(address))
        .count()
}

#[when("a role starts at a TCP address another process holds, which lets it go within the bound")]
fn port_held_a_moment(w: &mut Steps) -> TestResult {
    let log = scratch("frontend-port-a-moment.log");
    let observability = frontend_observability()?;
    let held = std::net::TcpListener::bind(&observability)?;
    let _pid = start_frontend(w, &frontend_holding()?, &log)?;
    let mut retried = false;
    for _ in 0..START_SECONDS * 100 {
        w.log = std::fs::read_to_string(&log).unwrap_or_default();
        retried = retries_of(&w.log, &observability) > 0;
        if retried {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(held);
    assert!(
        retried,
        "a retry in the log, {START_SECONDS} s on: {}",
        w.log
    );
    until_answering(&observability, "/health")?;
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

#[then("the role logs each retry naming the address, binds it, and answers `/health`")]
fn took_the_port(w: &mut Steps) -> TestResult {
    let observability = frontend_observability()?;
    assert_eq!(http_status(&observability, "/health")?, 200);
    assert!(retries_of(&w.log, &observability) > 0, "{}", w.log);
    Ok(())
}

#[when(
    "the frontend starts while another process holds its client address, which lets it go within the bound"
)]
fn client_port_held_a_moment(w: &mut Steps) -> TestResult {
    let log = scratch("frontend-client-port-a-moment.log");
    // The frontend's one listener beside its observability: since the switch it listens at no role address (design D2).
    let client = client_address()?;
    let held = std::net::TcpListener::bind(&client)?;
    let _pid = start_frontend(w, &frontend_holding()?, &log)?;
    let mut retried = false;
    for _ in 0..START_SECONDS * 100 {
        w.log = std::fs::read_to_string(&log).unwrap_or_default();
        retried = retries_of(&w.log, &client) > 0;
        if retried {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        retried,
        "a retry of the client address in the log, {START_SECONDS} s on: {}",
        w.log
    );
    // The observability address is bound, the client address is not.
    let observability = frontend_observability()?;
    w.status = http_status(&observability, "/health")?;
    drop(held);
    until_answering(&observability, "/health")?;
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

#[then(
    "`/health` answers 503 while the address is held, and once it answers 200 a client connection to the address is accepted at once"
)]
fn health_after_binding(w: &mut Steps) -> TestResult {
    assert_eq!(w.status, 503, "{}", w.log);
    let _connected = std::net::TcpStream::connect(client_address()?)?;
    w.roles = None;
    Ok(())
}

#[when("a role starts at a TCP address another process holds past the bound")]
fn port_held_past_the_bound(w: &mut Steps) -> TestResult {
    let log = scratch("frontend-port-held.log");
    // The frontend's client endpoint: since the switch it listens at no role address (design D2).
    let held = std::net::TcpListener::bind(client_address()?)?;
    let pid = start_frontend(w, &frontend_holding()?, &log)?;
    until_frontend_exits(w, pid, &log)?;
    drop(held);
    Ok(())
}

#[then(
    "it exits with code 69 after a retry logged per 50 ms of the bound, the error naming the address"
)]
fn past_the_bound(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 69)?;
    assert_eq!(
        retries_of(&w.log, &client_address()?),
        BIND_RETRIES as usize + 1,
        "a line per retry, and the error: {}",
        w.log
    );
    Ok(())
}

#[when("a role starts at a TCP address no interface of its host holds")]
fn no_such_interface(w: &mut Steps) -> TestResult {
    let log = scratch("frontend-no-interface.log");
    let settings = with(
        frontend_holding()?,
        "observability.address",
        &format!("tcp://{NO_INTERFACE}"),
    );
    let pid = start_frontend(w, &settings, &log)?;
    until_frontend_exits(w, pid, &log)
}

#[then("it exits with code 69 at once, the error naming the address, with no retry logged")]
fn at_once(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 69)?;
    assert!(w.log.contains(NO_INTERFACE), "{}", w.log);
    assert_eq!(retries_of(&w.log, NO_INTERFACE), 0, "{}", w.log);
    Ok(())
}

fn main() -> TestResult {
    start_double()?;
    futures::executor::block_on(
        Steps::cucumber()
            .max_concurrent_scenarios(1)
            .before(|_, _, _, _| {
                async {
                    let freed = own_addresses().and_then(|own| until_ports_free(&own));
                    assert!(freed.is_ok(), "{freed:?}");
                }
                .boxed_local()
            })
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
