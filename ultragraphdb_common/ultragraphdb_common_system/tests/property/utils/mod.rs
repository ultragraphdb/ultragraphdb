/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the property tests share: the group over the simulated storage by a clock the test moves, and served at a role address.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compio::runtime::{JoinHandle, spawn};
use futures::channel::mpsc::unbounded;
use ultragraphdb_cluster_metadata::MetadataConfig;
use ultragraphdb_cluster_sequencer::{ClockReading, SequencerConfig};
use ultragraphdb_common_role::{
    ROLE_ITERATIONS, RoleClient, RoleCluster, RoleSecret, Server, listen, serve_connection,
};
use ultragraphdb_common_system::{
    SystemGroup, SystemHandle, SystemSession, SystemShared, SystemStart,
};
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_protocol_transport::{Address, Endpoint};
use ultragraphdb_transactions_log::{SimStorage, Wal, WalConfig};

pub(crate) const CLUSTER: u64 = 7;

/// The group over the simulated storage.
pub(crate) type SimGroup = SystemGroup<Wal<SimStorage>, Wal<SimStorage>>;

/// A clock the test moves: the group reads it, and only the test changes it.
#[derive(Debug, Clone)]
pub(crate) struct Clock(Rc<Cell<u64>>);

impl Clock {
    pub(crate) fn new() -> Self {
        Self(Rc::new(Cell::new(1)))
    }

    pub(crate) fn reading(&self) -> impl Fn() -> ClockReading + 'static {
        let held = self.0.clone();
        move || ClockReading::new(held.get(), held.get())
    }

    pub(crate) fn now(&self) -> ClockReading {
        ClockReading::new(self.0.get(), self.0.get())
    }

    pub(crate) fn now_us(&self) -> u64 {
        self.0.get()
    }

    pub(crate) fn advance(&self, us: u64) {
        self.0.set(self.0.get().saturating_add(us));
    }
}

/// A group of one partition, epochs of 1 to 5 ms, a registry of `sessions_max` and `transaction_ms`.
pub(crate) fn start_of(sessions_max: u32, transaction_ms: u64) -> Option<SystemStart> {
    Some(
        SystemStart::new(
            MetadataConfig::new().with_sessions_max(sessions_max).ok()?,
            SequencerConfig::new(1, 5).ok()?,
            1,
        )
        .with_transaction_ms(transaction_ms),
    )
}

/// The group recovered over `storage` with `start` at `clock`'s reading, holding `alice` and `bob`, each with the password `pw`.
pub(crate) async fn recovered(
    storage: &SimStorage,
    start: &SystemStart,
    clock: &Clock,
) -> Option<SimGroup> {
    let config = WalConfig::new(CLUSTER, false);
    let (metadata, _) = Wal::open(storage.clone(), "/meta", config).await.ok()?;
    let (sequencer, _) = Wal::open(storage.clone(), "/seq", config).await.ok()?;
    let mut group = SystemGroup::recover(metadata, sequencer, start, None, clock.now())
        .await
        .ok()?;
    for name in ["alice", "bob"] {
        let verifier =
            ScramVerifier::derive("pw", format!("a salt of {name}").as_bytes(), 4_096).ok()?;
        group.create_principal_once(name, verifier).await.ok()?;
    }
    Some(group)
}

std::thread_local! {
    /// How many groups this thread has served, which names each one's role secret file apart from another test's.
    static SERVED: Cell<u64> = const { Cell::new(0) };
}

/// A number no other group served by this process takes: the thread's and its count of groups served.
fn served_number() -> String {
    let count = SERVED.with(|served| {
        served.set(served.get() + 1);
        served.get()
    });
    format!("{:?}-{count}", std::thread::current().id())
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect()
}

/// `handle` served at a role address on this runtime: the address, the role password, and the task accepting connections, which
/// holds a way to the group until it is dropped.
pub(crate) async fn served(handle: &SystemHandle) -> Option<(Address, String, JoinHandle<()>)> {
    let root =
        std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, std::path::PathBuf::from);
    let file = root.join(format!(
        "system-property-secret-{}-{}",
        std::process::id(),
        served_number()
    ));
    std::fs::write(&file, "a role secret").ok()?;
    let secret = RoleSecret::read(&file.display().to_string()).ok()?;
    let password = secret.password().to_string();
    let cluster = RoleCluster::new(CLUSTER, secret, Duration::from_secs(1));
    let (joins, _joined) = unbounded();
    let shared = SystemShared::new(cluster.clone(), handle.clone(), joins);
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .ok()?
        .local_addr()
        .ok()?
        .port();
    let listener = listen(&format!("tcp://127.0.0.1:{port}")).await.ok()?;
    let address = listener.local_address().ok()?;
    let server = Server::new(CLUSTER, "frontend", Endpoint::Frontend)
        .ok()?
        .with_iterations(ROLE_ITERATIONS);
    let credentials = cluster.secret().credentials().ok()?;
    let accepting = spawn(async move {
        while let Ok(carrier) = listener.accept().await {
            let (server, credentials) = (server.clone(), credentials.clone());
            let session = SystemSession::new(shared.clone());
            spawn(async move { serve_connection(carrier, &server, credentials, session).await })
                .detach();
        }
    });
    Some((address, password, accepting))
}

/// A role connection to `address` as `role`, stating `max_frame` in its Hello.
pub(crate) async fn dialled(
    address: &Address,
    password: &str,
    role: &str,
    max_frame: u32,
) -> Option<RoleClient> {
    RoleClient::dial_taking(address, CLUSTER, role, password, max_frame)
        .await
        .ok()
}
