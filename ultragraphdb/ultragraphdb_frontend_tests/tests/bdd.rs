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
use service_utils::{ServiceUtil, WaitStrategy};
use std::path::Path;
use std::process::ExitStatus;
use std::time::Duration;
use ultragraphdb_client_compio::{Client, ClientConfig, TcpConnector};
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_protocol_scram::{ScramLogin, system_nonce};
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{
    BodyView, EpochCloseView, Lease, LeaseView, Placement, PlacementView, Subscribe,
};
use ultragraphdb_tests::{
    CLIENT_ADDRESS, CLUSTER, COMPUTE_BINARY, FRONTEND_BINARY, FRONTEND_OBSERVABILITY,
    FRONTEND_ROLE_ADDRESS, SECRET, START_SECONDS, STORAGE_BINARY, answering, exit_is,
    frontend_holding, role, scratch, start_double,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

/// Where the sandbox holds the binaries, relative to the test's working directory: `copy_file` writes them to this package.
const ROOT_PATH: &str = "ultragraphdb/ultragraphdb_frontend_tests";

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
