/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A [`SystemSession`] per role connection over the TCP loopback, its group run on the test's runtime over the simulated
//! storage by a clock the test moves, and role clients dialling it as the roles their SCRAM usernames name.

#[cfg(test)]
mod system_session_frontend_tests;
#[cfg(test)]
mod system_session_session_tests;
#[cfg(test)]
mod system_session_tests;

use std::time::Duration;

use compio::runtime::{JoinHandle, spawn};
use futures::channel::mpsc::unbounded;
use ultragraphdb_common_role::{
    ROLE_ITERATIONS, RoleClient, RoleCluster, RoleSecret, Server, listen, serve_connection,
};
use ultragraphdb_common_system::{SystemHandle, SystemSession, SystemShared, SystemStart};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_transport::{
    Address, Endpoint, ReceivedFrame, TransportError, TransportErrorEnum,
};
use ultragraphdb_protocol_wire::{BodyView, Command, ErrorCode, ErrorReplyView};
use ultragraphdb_tool_bdd::{TestError, TestResult};
use ultragraphdb_transactions_log::SimStorage;

use crate::types::system_group::{CLUSTER, SimOwner, TestClock, recovered, running};

/// The role secret every connection here authenticates with.
const SECRET: &str = "a role secret";

/// How long a test waits for a frame it expects.
pub(crate) const WAITED: Duration = Duration::from_secs(5);

/// How long a test watches for a frame it expects not to come.
pub(crate) const WATCHED: Duration = Duration::from_millis(100);

/// A group served at a role address: the address, the cluster's role settings, the way to the group, its clock and its run.
pub(crate) struct Served {
    pub(crate) address: Address,
    pub(crate) cluster: RoleCluster,
    pub(crate) handle: SystemHandle,
    pub(crate) clock: TestClock,
    pub(crate) run: JoinHandle<Result<(), RoleError>>,
    /// The task accepting role connections, whose sessions hold a way to the group: dropped, it accepts no more.
    accepting: JoinHandle<()>,
}

impl Served {
    /// The group stopped once its connections closed: no connection accepted any more, and its run ended.
    pub(crate) async fn stop(self) -> TestResult {
        let Self {
            handle,
            run,
            accepting,
            ..
        } = self;
        drop(accepting);
        drop(handle);
        run.await
            .map_err(|error| TestError::new(format!("the group's run: {error:?}")))??;
        Ok(())
    }

    /// A connection dialled as `role`, stating `max_frame` in its Hello.
    pub(crate) async fn dial_taking(&self, role: &str, max_frame: u32) -> TestResult<RoleClient> {
        Ok(RoleClient::dial_taking(
            &self.address,
            CLUSTER,
            role,
            self.cluster.secret().password(),
            max_frame,
        )
        .await?)
    }

    /// A connection dialled as `role`.
    pub(crate) async fn dial(&self, role: &str) -> TestResult<RoleClient> {
        Ok(RoleClient::dial(
            &self.address,
            CLUSTER,
            role,
            self.cluster.secret().password(),
        )
        .await?)
    }
}

/// The role cluster of [`SECRET`], its file written under the test's temporary directory as `name`.
pub(crate) fn cluster_named(name: &str) -> TestResult<RoleCluster> {
    let root =
        std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, std::path::PathBuf::from);
    let file = root.join(format!("role-secret-{name}-{}", std::process::id()));
    std::fs::write(&file, SECRET)?;
    let secret = RoleSecret::read(&file.display().to_string())?;
    Ok(RoleCluster::new(CLUSTER, secret, Duration::from_secs(1)))
}

/// `owner`, made by `make` over `storage` at the clock's reading, run on this runtime and served at a free loopback port as
/// the system group's role address.
pub(crate) async fn serving<F, Fut>(name: &str, storage: &SimStorage, make: F) -> TestResult<Served>
where
    F: FnOnce(SimStorage, TestClock) -> Fut,
    Fut: std::future::Future<Output = TestResult<SimOwner>>,
{
    let clock = TestClock::new();
    let owner = make(storage.clone(), clock.clone()).await?;
    let (handle, run) = running(owner, &clock);
    let cluster = cluster_named(name)?;
    let (joins, _joined) = unbounded();
    let shared = SystemShared::new(cluster.clone(), handle.clone(), joins);
    let port = std::net::TcpListener::bind("127.0.0.1:0")?
        .local_addr()?
        .port();
    let listener = listen(&format!("tcp://127.0.0.1:{port}")).await?;
    let address = listener.local_address()?;
    let server =
        Server::new(CLUSTER, "frontend", Endpoint::Frontend)?.with_iterations(ROLE_ITERATIONS);
    let credentials = cluster.secret().credentials()?;
    let accepting = spawn(async move {
        while let Ok(carrier) = listener.accept().await {
            let (server, credentials) = (server.clone(), credentials.clone());
            let session = SystemSession::new(shared.clone());
            spawn(async move { serve_connection(carrier, &server, credentials, session).await })
                .detach();
        }
    });
    Ok(Served {
        address,
        cluster,
        handle,
        clock,
        run,
        accepting,
    })
}

/// A group of `start` over `storage` at `clock`'s reading, holding `alice` and `bob` with the password `pw`.
pub(crate) async fn group_with(
    storage: SimStorage,
    clock: TestClock,
    start: SystemStart,
) -> TestResult<SimOwner> {
    let mut owner = recovered(&storage, &start, &clock).await?;
    crate::types::system_group::with_principal(&mut owner, "alice").await?;
    crate::types::system_group::with_principal(&mut owner, "bob").await?;
    Ok(owner)
}

/// The code a refusal names, `None` for a failure that is no refusal.
pub(crate) fn refusal_code(refused: &TransportError) -> Option<ErrorCode> {
    if let TransportErrorEnum::Refused { code, .. } = &refused.0 {
        Some(*code)
    } else {
        None
    }
}

/// The next frame on `client`, within [`WAITED`].
pub(crate) async fn next_frame(client: &mut RoleClient) -> TestResult<ReceivedFrame> {
    compio::time::timeout(WAITED, client.next_frame())
        .await
        .map_err(|_| TestError::new("no frame came"))?
        .map_err(TestError::from)
}

/// Whether a frame comes on `client` within [`WATCHED`]: `None` for none, else its stream and command.
pub(crate) async fn watched(client: &mut RoleClient) -> TestResult<Option<(u64, Command)>> {
    match compio::time::timeout(WATCHED, client.next_frame()).await {
        Err(_) => Ok(None),
        Ok(frame) => {
            let frame = frame?;
            let view = frame.view()?;
            Ok(Some((view.header().stream(), view.command())))
        }
    }
}

/// The Error `frame` carries: its stream, code and message.
pub(crate) fn error_in(frame: &ReceivedFrame) -> TestResult<(u64, ErrorCode, String)> {
    let view = frame.view()?;
    let error = ErrorReplyView::from_frame(&view)?;
    Ok((
        view.header().stream(),
        error.code(),
        error.message().to_string(),
    ))
}
