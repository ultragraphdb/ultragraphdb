/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Clusters of the three binaries, every address over one carrier, started through the config service, and the client of
//! their endpoint: the client crates' own [`Client`], dialling through the transport's [`Carrier`].

use std::io;
use std::time::Duration;

use service_utils::ServiceUtil;
use ultragraphdb_client_compio::{Client, ClientConfig, Connect};
use ultragraphdb_protocol_scram::{ScramError, ScramLogin, system_nonce};
use ultragraphdb_protocol_transport::{Address, Scheme};
use ultragraphdb_protocol_transport_compio::Carrier;
use ultragraphdb_tests::{
    CLIENT_ADDRESS, CLUSTER, COMPUTE_BINARY, COMPUTE_OBSERVABILITY, COMPUTE_ROLE_ADDRESS,
    CONFIG_ADDRESS, FRONTEND_BINARY, FRONTEND_OBSERVABILITY, FRONTEND_ROLE_ADDRESS, SECRET,
    START_SECONDS, STORAGE_BINARY, STORAGE_OBSERVABILITY, STORAGE_ROLE_ADDRESS, answering,
    bootstrap, compute_on_one_host, frontend_holding, holding, process, scratch,
    start_config_service_at, storage_holding, until_answering, with,
};
use ultragraphdb_tool_bdd::{TestError, TestResult};

/// The administrator the frontend's first start creates, whom every client of these scenarios logs in as.
const ADMIN: &str = "admin";

/// The administrator's password, which the frontend's bootstrap secret holds.
const PASSWORD: &str = "the administrator's password";

/// How far the shared cluster's ports lie above the declared ones, so a scenario's own cluster runs beside it.
const SHARED_SHIFT: u16 = 100;

/// The carrier every address of a run names: the roles' and the client endpoint's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Carriers {
    /// `tcp://` on the loopback.
    #[default]
    Tcp,
    /// `unix://` sockets under `/tmp`, whose short path a socket's address needs.
    Unix,
}

/// `address`, a host and a port, with its port `shift` higher.
fn shifted(address: &str, shift: u16) -> TestResult<String> {
    let (host, port) = address
        .rsplit_once(':')
        .ok_or_else(|| TestError::new(format!("a port in {address}")))?;
    let port: u16 = port.parse()?;
    Ok(format!("{host}:{}", port + shift))
}

/// Where one cluster's processes listen: the config service, each role's `/health` and `/ready` (frontend, storage,
/// compute), each role's address in that order, and the client endpoint's.
#[derive(Debug, Clone)]
struct Layout {
    config: String,
    observability: [String; 3],
    roles: [String; 3],
    client: String,
}

impl Layout {
    /// The declared addresses of one host's cluster, every port `shift` higher, its roles and client endpoint over
    /// `carrier` at sockets named with `tag`.
    fn new(carrier: Carriers, shift: u16, tag: &str) -> TestResult<Self> {
        let observability = [
            shifted(FRONTEND_OBSERVABILITY, shift)?,
            shifted(STORAGE_OBSERVABILITY, shift)?,
            shifted(COMPUTE_OBSERVABILITY, shift)?,
        ];
        let socket =
            |name: &str| format!("unix:///tmp/ugdb-{tag}{name}-{}.sock", std::process::id());
        let (roles, client) = match carrier {
            Carriers::Tcp => (
                [
                    format!("tcp://{}", shifted(FRONTEND_ROLE_ADDRESS, shift)?),
                    format!("tcp://{}", shifted(STORAGE_ROLE_ADDRESS, shift)?),
                    format!("tcp://{}", shifted(COMPUTE_ROLE_ADDRESS, shift)?),
                ],
                format!("tcp://{}", shifted(CLIENT_ADDRESS, shift)?),
            ),
            Carriers::Unix => (["f", "s", "c"].map(socket), socket("e")),
        };
        Ok(Self {
            config: shifted(CONFIG_ADDRESS, shift)?,
            observability,
            roles,
            client,
        })
    }

    /// The address of partition `partition` of the storage process, as storage derives it from its role address.
    fn partition(&self, partition: u32) -> TestResult<String> {
        let [_, storage, _] = &self.roles;
        if let Some(stem) = storage.strip_suffix(".sock") {
            return Ok(format!("{stem}-p{partition}.sock"));
        }
        let target = storage.trim_start_matches("tcp://");
        let shift = u16::try_from(partition - 1)?;
        Ok(format!("tcp://{}", shifted(target, shift)?))
    }

    /// The three roles' settings, frontend, storage and compute, the placement over `partitions` partitions of one storage
    /// process.
    fn settings(&self, partitions: u32) -> TestResult<[Vec<(String, String)>; 3]> {
        let count = partitions.to_string();
        let secret = scratch("bootstrap-secret");
        std::fs::write(&secret, PASSWORD)?;
        let secret = secret.display().to_string();
        let [f_obs, s_obs, c_obs] = self
            .observability
            .each_ref()
            .map(|at| format!("tcp://{at}"));
        let [f, s, c] = &self.roles;
        let joining = "cluster.frontend_address";
        let frontend = [
            ("observability.address", &f_obs),
            ("metadata.bootstrap_secret_file", &secret),
            ("cluster.partition_count", &count),
            ("frontend.role_address", f),
            ("endpoint.address", &self.client),
        ];
        let storage = [
            ("observability.address", &s_obs),
            ("storage.partition_count", &count),
            ("storage.role_address", s),
            (joining, f),
        ];
        let compute = [
            ("observability.address", &c_obs),
            ("compute.role_address", c),
            (joining, f),
        ];
        let held = |mut settings: Vec<(String, String)>, changes: &[(&str, &String)]| {
            for (name, value) in changes {
                settings = with(settings, name, value);
            }
            settings
        };
        Ok([
            held(frontend_holding()?, &frontend),
            held(storage_holding()?, &storage),
            held(holding(compute_on_one_host(), SECRET)?, &compute),
        ])
    }
}

/// A running cluster: its processes, which stop when it is dropped, and where it answers.
#[derive(Debug)]
pub(crate) struct Cluster {
    util: Option<ServiceUtil>,
    /// The client endpoint's address.
    pub(crate) client: String,
    /// The frontend's role address.
    pub(crate) frontend: String,
    /// Each partition's role address, partition 1 first.
    pub(crate) partitions: Vec<String>,
}

impl Drop for Cluster {
    /// Stops the processes, then removes their sockets, which a later start could not bind over.
    fn drop(&mut self) {
        self.util = None;
        let sockets = [&self.client, &self.frontend]
            .into_iter()
            .chain(&self.partitions)
            .filter_map(|address| address.strip_prefix("unix://"));
        for socket in sockets {
            // A socket the process never bound is not there to remove.
            drop(std::fs::remove_file(socket));
        }
    }
}

/// Starts the config service holding `layout`'s three roles, then the frontend, storage and compute in the order their
/// readiness needs, and waits until the frontend is ready and its client endpoint accepts.
fn start(layout: &Layout, partitions: u32, util: ServiceUtil) -> TestResult<Cluster> {
    let [frontend, storage, compute] = layout.settings(partitions)?;
    let [frontend_role, _, _] = &layout.roles;
    let mut cluster = Cluster {
        util: None,
        client: layout.client.clone(),
        frontend: frontend_role.clone(),
        partitions: (1..=partitions)
            .map(|partition| layout.partition(partition))
            .collect::<TestResult<_>>()?,
    };
    let instances: [(&str, &[(String, String)]); 3] = [
        ("frontend", &frontend),
        ("storage", &storage),
        ("compute", &compute),
    ];
    let _service: u32 = start_config_service_at(&util, &layout.config, &instances)?;
    let [f_obs, s_obs, c_obs] = &layout.observability;
    for (program, instance, wait) in [
        (FRONTEND_BINARY, "frontend", answering(f_obs, "/health")),
        (STORAGE_BINARY, "storage", answering(s_obs, "/ready")),
        (COMPUTE_BINARY, "compute", answering(c_obs, "/ready")),
    ] {
        let variables = bootstrap(&layout.config, instance);
        let started = process(program, variables, &scratch(program), wait);
        let _pid: u32 = util.start_service_from_config(started)?;
    }
    cluster.util = Some(util);
    until_answering(f_obs, "/ready")?;
    until_listening(&cluster.client)?;
    Ok(cluster)
}

/// Starts a scenario's own cluster over `carrier` at the declared ports, the placement over `partitions` partitions.
pub(crate) fn start_own(carrier: Carriers, partitions: u32) -> TestResult<Cluster> {
    start(
        &Layout::new(carrier, 0, "own-")?,
        partitions,
        crate::services()?,
    )
}

/// Starts the cluster of one partition the scenarios of a run share over `carrier`, each in IRIs of its own.
pub(crate) fn start_shared(carrier: Carriers) -> TestResult<Cluster> {
    start(
        &Layout::new(carrier, SHARED_SHIFT, "shared-")?,
        1,
        crate::services()?,
    )
}

/// The client endpoint of the shared cluster over `carrier`.
pub(crate) fn shared_client(carrier: Carriers) -> TestResult<String> {
    Ok(Layout::new(carrier, SHARED_SHIFT, "shared-")?.client)
}

/// Waits for the client endpoint at `address` to accept, which it does once the first start has derived the administrator's
/// verifier.
fn until_listening(address: &str) -> TestResult {
    let parsed = Address::parse(address)?;
    for _ in 0..START_SECONDS * 20 {
        let accepted = match parsed.scheme() {
            Scheme::Tcp => std::net::TcpStream::connect(parsed.target()).is_ok(),
            Scheme::Unix => std::os::unix::net::UnixStream::connect(parsed.target()).is_ok(),
        };
        if accepted {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(TestError::new(format!(
        "the client endpoint {address} never listened"
    )))
}

/// Connections to one address over the carrier its scheme names.
#[derive(Debug, Clone)]
pub(crate) struct Dial(Address);

impl Connect for Dial {
    type Stream = Carrier;

    async fn connect(&mut self) -> io::Result<Carrier> {
        Carrier::connect(&self.0)
            .await
            .map_err(|error| io::Error::other(error.to_string()))
    }
}

/// The client of these scenarios.
pub(crate) type TestClient = Client<Dial, ScramLogin<fn() -> Result<String, ScramError>>>;

/// A client of the endpoint at `address`, connected and authenticated as the administrator.
pub(crate) async fn client(address: &str) -> TestResult<TestClient> {
    let nonces: fn() -> Result<String, ScramError> = system_nonce;
    let login = ScramLogin::new(ADMIN, PASSWORD, nonces)?;
    let config = ClientConfig::new(CLUSTER, "ultragraphdb-tests");
    let mut client = Client::new(Dial(Address::parse(address)?), config, login, 16);
    client.connect().await?;
    Ok(client)
}
