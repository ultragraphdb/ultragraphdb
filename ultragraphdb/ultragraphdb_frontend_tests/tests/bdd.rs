/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The frontend binary as a process, bound to `features/`, with no container. One scenario runs at a time: it listens at its
//! declared ports.

mod endpoint_steps;
mod role_read_steps;

use compio::runtime::Runtime;
use cucumber::{World, then, when};
use futures::FutureExt;
use service_utils::{ServiceUtil, WaitStrategy};
use std::path::Path;
use std::process::ExitStatus;
use std::time::Duration;
use ultragraphdb_client_compio::{Client, ClientConfig, TcpConnector};
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_common_service::BIND_RETRIES;
use ultragraphdb_protocol_scram::{ScramLogin, system_nonce};
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{
    BodyView, EpochCloseView, Lease, LeaseView, Placement, PlacementView, Subscribe,
};
use ultragraphdb_tests::{
    CLIENT_ADDRESS, CLUSTER, COMPUTE_BINARY, DOUBLE_ADDRESS, FRONTEND_BINARY,
    FRONTEND_OBSERVABILITY, FRONTEND_ROLE_ADDRESS, PARENT_PID, SECRET, START_SECONDS,
    STORAGE_BINARY, answering, bootstrap, exit_is, frontend_holding, http_status, process,
    render_settings, role, scratch, served_by_double, start_double, until_answering,
    until_ports_free, with,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

/// Where the sandbox holds the binaries, relative to the test's working directory: `copy_file` writes them to this package.
const ROOT_PATH: &str = "ultragraphdb/ultragraphdb_frontend_tests";

/// An address in TEST-NET-3, which no host's interface holds (RFC 5737).
const NO_INTERFACE: &str = "203.0.113.1:18099";

/// The addresses a role of these scenarios holds beyond those every suite declares: none.
const OWN_ADDRESSES: [(&str, &str); 0] = [];

pub(crate) fn services() -> TestResult<ServiceUtil> {
    Ok(ServiceUtil::new(
        ROOT_PATH,
        vec![FRONTEND_BINARY, COMPUTE_BINARY, STORAGE_BINARY],
    )?)
}

#[derive(Debug, Default, World)]
pub(crate) struct Steps {
    epochs: Vec<u64>,
    pub(crate) exited: Option<ExitStatus>,
    graph: String,
    leased: Option<(u64, u64)>,
    pub(crate) log: String,
    ranges: usize,
    pub(crate) roles: Option<ServiceUtil>,
    pub(crate) refused: Option<String>,
    pub(crate) answered: role_read_steps::Answered,
    pub(crate) ids: Vec<Vec<u8>>,
}

/// A client's refusal as it reads it, logging in as `name` with `password`.
fn client_refusal(name: &str, password: &str) -> TestResult<String> {
    Runtime::new()?.block_on(async {
        let login = ScramLogin::new(name, password, system_nonce)?;
        let address = CLIENT_ADDRESS.parse()?;
        let config = ClientConfig::new(CLUSTER, "ultragraphdb-test");
        let mut client = Client::new(TcpConnector::new(address), config, login, 16);
        let refused = client.connect().await.err().context("a refusal")?;
        Ok(refused.to_string())
    })
}

#[when("a client authenticates on the client endpoint with the role secret")]
fn client_with_secret(w: &mut Steps) -> TestResult {
    let util = services()?;
    let log = scratch(FRONTEND_BINARY);
    let wait = answering(FRONTEND_OBSERVABILITY, "/health");
    let settings = frontend_holding()?;
    let _pid: u32 = util.start_service_from_config(role(FRONTEND_BINARY, &settings, &log, wait))?;
    w.log = client_refusal("storage", SECRET)?;
    w.roles = Some(util);
    Ok(())
}

#[then("authentication is refused with the same error as a wrong password")]
fn same_as_wrong_password(w: &mut Steps) -> TestResult {
    assert!(w.log.contains("authentication failed"), "{}", w.log);
    assert_eq!(w.log, client_refusal("storage", "a wrong password")?);
    w.roles = None;
    Ok(())
}

/// Starts the frontend alone, waiting on its `/health`.
fn frontend_alone(w: &mut Steps) -> TestResult {
    let util = services()?;
    let log = scratch(FRONTEND_BINARY);
    let wait = answering(FRONTEND_OBSERVABILITY, "/health");
    let _pid: u32 =
        util.start_service_from_config(role(FRONTEND_BINARY, &frontend_holding()?, &log, wait))?;
    w.roles = Some(util);
    Ok(())
}

/// The frontend's role address, dialled as storage holding the cluster's secret.
async fn as_storage() -> TestResult<RoleClient> {
    let target = Address::parse(&format!("tcp://{}", FRONTEND_ROLE_ADDRESS))?;
    Ok(RoleClient::dial(&target, CLUSTER, "storage", SECRET).await?)
}

#[when("the frontend's sequencer closes epochs 1 to 5 and storage is connected")]
fn subscribed(w: &mut Steps) -> TestResult {
    frontend_alone(w)?;
    Runtime::new()?.block_on(async {
        let mut client = as_storage().await?;
        let first = client.request(&Subscribe::new(0)).await?;
        let mut note = |frame: &ultragraphdb_protocol_transport::ReceivedFrame| -> TestResult {
            let view = frame.view()?;
            w.epochs.push(EpochCloseView::from_frame(&view)?.epoch());
            Ok(())
        };
        note(&first)?;
        drop(first);
        for _ in 1..5 {
            let next = client.next_frame().await?;
            note(&next)?;
        }
        Ok(())
    })
}

#[then("storage receives five epoch-close commands, in order")]
fn in_order(w: &mut Steps) {
    assert_eq!(w.epochs, [1, 2, 3, 4, 5]);
    w.roles = None;
}

#[when("a partition asks the frontend for a block and the placement")]
fn lease_and_placement(w: &mut Steps) -> TestResult {
    frontend_alone(w)?;
    Runtime::new()?.block_on(async {
        let mut client = as_storage().await?;
        let leased = client.request(&Lease::request(1, 1, 16)).await?;
        let view = leased.view()?;
        let block = LeaseView::from_frame(&view)?;
        w.leased = Some((block.start(), block.end()));
        drop(leased);
        let placed = client.request(&Placement::request(0)).await?;
        w.ranges = PlacementView::from_frame(&placed.view()?)?.range_count();
        Ok(())
    })
}

#[then("the frontend answers both over the role connection")]
fn answered_both(w: &mut Steps) -> TestResult {
    let (start, end) = w.leased.context("a block")?;
    assert_eq!(end - start, 16);
    assert_eq!(w.ranges, 1, "one partition holds every bucket");
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

#[when("the frontend binary's dependency graph is listed")]
fn frontend_graph(w: &mut Steps) -> TestResult {
    w.graph = std::fs::read_to_string(Path::new(ROOT_PATH).join("frontend_deps"))?;
    Ok(())
}

#[when("the frontend binary receives settings with no `frontend.role_address`")]
fn no_frontend_address(w: &mut Steps) -> TestResult {
    let mut settings = frontend_holding()?;
    settings.retain(|(name, _)| name != "frontend.role_address");
    let log = scratch("frontend-no-address.log");
    let util = services()?;
    let pid = util.start_service_from_config(role(
        FRONTEND_BINARY,
        &settings,
        &log,
        WaitStrategy::NoWait,
    ))?;
    w.exited = Some(util.wait_for_exit(pid, Duration::from_secs(START_SECONDS))?);
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

#[then("it exits with code 78 and an error naming `frontend.role_address`")]
fn names_frontend_address(w: &mut Steps) -> TestResult {
    exit_is(w.exited, &w.log, 78)?;
    assert!(w.log.contains("frontend.role_address"), "{}", w.log);
    Ok(())
}

#[when("a role starts with `ULTRAGRAPHDB_PARENT_PID` naming a process that is not its parent")]
fn parent_gone(w: &mut Steps) -> TestResult {
    let log = scratch("frontend-parent.log");
    let instance = served_by_double(FRONTEND_BINARY, &render_settings(&frontend_holding()?));
    // No process holds the largest identity, so it is no parent of the role's.
    let named = with(
        bootstrap(DOUBLE_ADDRESS, &instance),
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
    let pid =
        util.start_service_from_config(role(FRONTEND_BINARY, settings, log, WaitStrategy::NoWait))?;
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
    let held = std::net::TcpListener::bind(FRONTEND_OBSERVABILITY)?;
    let _pid = start_frontend(w, &frontend_holding()?, &log)?;
    let mut retried = false;
    for _ in 0..START_SECONDS * 100 {
        w.log = std::fs::read_to_string(&log).unwrap_or_default();
        retried = retries_of(&w.log, FRONTEND_OBSERVABILITY) > 0;
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
    until_answering(FRONTEND_OBSERVABILITY, "/health")?;
    w.log = std::fs::read_to_string(&log)?;
    Ok(())
}

#[then("the role logs each retry naming the address, binds it, and answers `/health`")]
fn took_the_port(w: &mut Steps) -> TestResult {
    assert_eq!(http_status(FRONTEND_OBSERVABILITY, "/health")?, 200);
    assert!(retries_of(&w.log, FRONTEND_OBSERVABILITY) > 0, "{}", w.log);
    Ok(())
}

#[when("a role starts at a TCP address another process holds past the bound")]
fn port_held_past_the_bound(w: &mut Steps) -> TestResult {
    let log = scratch("frontend-port-held.log");
    let held = std::net::TcpListener::bind(FRONTEND_ROLE_ADDRESS)?;
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
        retries_of(&w.log, FRONTEND_ROLE_ADDRESS),
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
                    let freed = until_ports_free(&OWN_ADDRESSES);
                    assert!(freed.is_ok(), "{freed:?}");
                }
                .boxed_local()
            })
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
