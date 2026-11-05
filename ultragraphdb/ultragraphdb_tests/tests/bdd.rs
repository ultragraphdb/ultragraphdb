/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Whole clusters as processes, every role started through the config service, bound to `features/`, with no container. One
//! scenario runs at a time: the binaries listen at their declared ports.

use cucumber::{World, then, when};
use service_utils::ServiceUtil;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use ultragraphdb_protocol_wire::RoleKind;
use ultragraphdb_tests::{
    COMPUTE_BINARY, COMPUTE_OBSERVABILITY, COMPUTE_ROLE_ADDRESS, CONFIG_BINARY, FRONTEND_BINARY,
    FRONTEND_OBSERVABILITY, FRONTEND_ROLE_ADDRESS, SECRET, STORAGE_BINARY, STORAGE_OBSERVABILITY,
    STORAGE_ROLE_ADDRESS, answering, compute_on_one_host, frontend_holding, holding, http_status,
    members_at, role_of_service, scratch, start_config_service, storage_holding, until_answering,
    with,
};
use ultragraphdb_tool_bdd::TestResult;

/// Where the sandbox holds the binaries, relative to the test's working directory: `copy_file` writes them to this package.
const ROOT_PATH: &str = "ultragraphdb/ultragraphdb_tests";

fn services() -> TestResult<ServiceUtil> {
    Ok(ServiceUtil::new(
        ROOT_PATH,
        vec![
            CONFIG_BINARY,
            FRONTEND_BINARY,
            COMPUTE_BINARY,
            STORAGE_BINARY,
        ],
    )?)
}

#[derive(Debug, Default, World)]
struct Steps {
    compute: String,
    frontend: String,
    roles: Option<ServiceUtil>,
    sockets: Vec<PathBuf>,
    storage: String,
}

/// Starts the frontend, storage and compute in the order their readiness needs, on the settings given.
fn start_in_order(
    w: &mut Steps,
    frontend_settings: &[(String, String)],
    storage_settings: &[(String, String)],
    compute_settings: &[(String, String)],
) -> TestResult {
    let util = services()?;
    let _service: u32 = start_config_service(
        &util,
        &[
            ("frontend", frontend_settings),
            ("storage", storage_settings),
            ("compute", compute_settings),
        ],
    )?;
    let log = scratch(FRONTEND_BINARY);
    let wait = answering(FRONTEND_OBSERVABILITY, "/health");
    let _pid: u32 =
        util.start_service_from_config(role_of_service(FRONTEND_BINARY, "frontend", &log, wait))?;
    assert_eq!(
        http_status(FRONTEND_OBSERVABILITY, "/ready")?,
        503,
        "the frontend is ready only once a partition answers it"
    );
    for (program, instance, address) in [
        (STORAGE_BINARY, "storage", STORAGE_OBSERVABILITY),
        (COMPUTE_BINARY, "compute", COMPUTE_OBSERVABILITY),
    ] {
        let ready = answering(address, "/ready");
        let joined = role_of_service(program, instance, &scratch(program), ready);
        let _joined: u32 = util.start_service_from_config(joined)?;
    }
    w.roles = Some(util);
    Ok(())
}

#[when("the frontend starts, then storage, then compute")]
fn start_all(w: &mut Steps) -> TestResult {
    w.frontend = format!("tcp://{}", FRONTEND_ROLE_ADDRESS);
    w.storage = format!("tcp://{}", STORAGE_ROLE_ADDRESS);
    w.compute = format!("tcp://{}", COMPUTE_ROLE_ADDRESS);
    start_in_order(
        w,
        &frontend_holding()?,
        &storage_holding()?,
        &holding(compute_on_one_host(), SECRET)?,
    )
}

#[when("the frontend starts, then storage, then compute, each at a Unix socket")]
fn start_all_unix(w: &mut Steps) -> TestResult {
    // A socket's path has a short limit, which the sandbox's own directory passes.
    let socket =
        |name: &str| PathBuf::from(format!("/tmp/ugdb-{name}-{}.sock", std::process::id()));
    let paths = [socket("f"), socket("s"), socket("c")];
    let [f, st, c] = paths
        .each_ref()
        .map(|path| format!("unix://{}", path.display()));
    let partition_socket = PathBuf::from(format!("/tmp/ugdb-s-{}-p1.sock", std::process::id()));
    let joining = "cluster.frontend_address";
    let frontend_settings = with(frontend_holding()?, "frontend.role_address", &f);
    let storage_settings = with(
        with(storage_holding()?, joining, &f),
        "storage.role_address",
        &st,
    );
    let compute_settings = with(
        with(compute_on_one_host(), joining, &f),
        "compute.role_address",
        &c,
    );
    (w.frontend, w.storage, w.compute) = (f, format!("unix://{}", partition_socket.display()), c);
    let [f_path, _, c_path] = paths;
    w.sockets = vec![f_path, partition_socket, c_path];
    start_in_order(
        w,
        &frontend_settings,
        &storage_settings,
        &holding(compute_settings, SECRET)?,
    )
}

#[then(
    "every role's `/ready` answers, the frontend's last, and the frontend's members name storage and compute"
)]
fn all_ready(w: &mut Steps) -> TestResult {
    assert_eq!(http_status(STORAGE_OBSERVABILITY, "/ready")?, 200);
    assert_eq!(http_status(COMPUTE_OBSERVABILITY, "/ready")?, 200);
    until_answering(FRONTEND_OBSERVABILITY, "/ready")?;
    assert_eq!(http_status(FRONTEND_OBSERVABILITY, "/other")?, 404);
    let expected = vec![
        (RoleKind::Compute, 0, w.compute.clone()),
        (RoleKind::Storage, 1, w.storage.clone()),
    ];
    assert_eq!(members_at(&w.frontend)?, expected);
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

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .max_concurrent_scenarios(1)
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
