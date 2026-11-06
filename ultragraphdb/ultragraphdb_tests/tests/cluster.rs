/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Clusters of the three binaries, every address over one carrier, started through the config service, and where a client
//! dials them. A role a scenario makes misbehave is a real process: stopped by a signal, killed, or started again.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use compio::runtime::Runtime;
use service_utils::{ServiceStartConfig, ServiceUtil, WaitStrategy};
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_protocol_transport::{Address, Scheme};
use ultragraphdb_protocol_wire::{BodyView, Placement, PlacementView};
use ultragraphdb_store_quads::iri_bucket;
use ultragraphdb_tests::{
    CLIENT_ADDRESS, CLUSTER, COMPUTE_BINARY, COMPUTE_OBSERVABILITY, COMPUTE_ROLE_ADDRESS,
    CONFIG_ADDRESS, FRONTEND_BINARY, FRONTEND_OBSERVABILITY, FRONTEND_ROLE_ADDRESS, SECRET,
    START_SECONDS, STORAGE_BINARY, STORAGE_OBSERVABILITY, STORAGE_ROLE_ADDRESS, answering,
    bootstrap, compute_on_one_host, fresh_data, frontend_holding, holding, process, scratch,
    start_config_service_at, storage_holding, until_answering, with,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_transport_tls::TestPki;

use crate::shells::{ClientAt, PASSWORD, TestClient};

/// How far the shared cluster's ports lie above the declared ones, so a scenario's own cluster runs beside it.
const SHARED_SHIFT: u16 = 100;

/// How far the shared cluster of several partitions lies above the declared ports.
const SPREAD_SHIFT: u16 = 200;

/// How far the first cluster of a spec of its own lies above the declared ports.
const SPECS_SHIFT: u16 = 300;

/// How many ports each such cluster takes.
const SPEC_PORTS: u16 = 20;

/// The partitions of the shared cluster that spreads its tenant.
pub(crate) const SPREAD_PARTITIONS: u32 = 3;

/// How far a second compute process's and a second storage process's addresses lie above the first's. A first storage process
/// of up to 4 partitions leaves them free.
const SECOND_PROCESS: u16 = 5;

/// How many IRIs a scenario tries for one of a partition's buckets, each bucket as likely as another.
const IRIS_TRIED: u32 = 10_000;

/// The carrier every address of a run names: the roles' and the client endpoint's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Carriers {
    /// `tcp://` on the loopback, the client in clear text.
    #[default]
    Tcp,
    /// `unix://` sockets under `/tmp`, whose short path a socket's address needs.
    Unix,
    /// `tcp://` on the loopback, the client endpoint speaking TLS 1.3.
    Tls,
}

/// The authority of this process's TLS endpoints.
fn pki() -> TestResult<&'static TestPki> {
    static PKI: OnceLock<TestPki> = OnceLock::new();
    if let Some(pki) = PKI.get() {
        return Ok(pki);
    }
    let made = TestPki::new()?;
    Ok(PKI.get_or_init(|| made))
}

/// `address`, a host and a port, with its port `shift` higher.
fn shifted(address: &str, shift: u16) -> TestResult<String> {
    let (host, port) = address
        .rsplit_once(':')
        .ok_or_else(|| TestError::new(format!("a port in {address}")))?;
    let port: u16 = port.parse()?;
    Ok(format!("{host}:{}", port + shift))
}

/// What differs in a scenario's own cluster: where its processes stand and the settings each role holds beyond the harness's.
#[derive(Debug, Clone)]
pub(crate) struct Spec {
    pub(crate) partitions: u32,
    /// Whether two storage processes share the partitions, the second holding them from partition 2.
    pub(crate) two_storages: bool,
    /// Whether two compute processes join.
    pub(crate) two_computes: bool,
    pub(crate) frontend: Vec<(&'static str, String)>,
    pub(crate) storage: Vec<(&'static str, String)>,
}

impl Spec {
    /// One partition, one process of each role, the settings the harness gives.
    pub(crate) fn one_partition() -> Self {
        Self {
            partitions: 1,
            two_storages: false,
            two_computes: false,
            frontend: Vec::new(),
            storage: Vec::new(),
        }
    }

    /// `partitions` partitions of one storage process.
    pub(crate) fn over(partitions: u32) -> Self {
        Self {
            partitions,
            ..Self::one_partition()
        }
    }

    /// With the frontend's setting `name` holding `value`.
    pub(crate) fn frontend(mut self, name: &'static str, value: u64) -> Self {
        self.frontend.push((name, value.to_string()));
        self
    }

    /// With every storage process's setting `name` holding `value`.
    pub(crate) fn storage(mut self, name: &'static str, value: u64) -> Self {
        self.storage.push((name, value.to_string()));
        self
    }

    /// With a second compute process.
    pub(crate) fn second_compute(mut self) -> Self {
        self.two_computes = true;
        self
    }

    /// Two partitions, each on a storage process of its own.
    pub(crate) fn second_storage(mut self) -> Self {
        self.partitions = 2;
        self.two_storages = true;
        self
    }
}

/// Where one cluster's processes listen: the config service, each role's `/health` and `/ready` (frontend, storage, compute),
/// each role's address in that order, and the client endpoint's.
#[derive(Debug, Clone)]
struct Layout {
    carrier: Carriers,
    shift: u16,
    tag: String,
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
            Carriers::Tcp | Carriers::Tls => (
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
            carrier,
            shift,
            tag: tag.to_string(),
            config: shifted(CONFIG_ADDRESS, shift)?,
            observability,
            roles,
            client,
        })
    }

    /// The address of partition `partition` of the storage process, as storage derives it from its role address.
    fn partition(&self, partition: u32, spec: &Spec) -> TestResult<String> {
        let [_, storage, _] = &self.roles;
        if let Some(stem) = storage.strip_suffix(".sock") {
            return Ok(format!("{stem}-p{partition}.sock"));
        }
        let target = storage.trim_start_matches("tcp://");
        if spec.two_storages && partition > 1 {
            let shift = SECOND_PROCESS + u16::try_from(partition - 2)?;
            return Ok(format!("tcp://{}", shifted(target, shift)?));
        }
        let shift = u16::try_from(partition - 1)?;
        Ok(format!("tcp://{}", shifted(target, shift)?))
    }

    /// Where a client of this cluster dials.
    fn client_at(&self) -> TestResult<ClientAt> {
        let tls = match self.carrier {
            Carriers::Tls => Some(pki()?.connector()?),
            Carriers::Tcp | Carriers::Unix => None,
        };
        Ok(ClientAt {
            address: self.client.clone(),
            tls,
        })
    }

    /// The frontend's settings of `spec`, its endpoint over this layout's carrier.
    fn frontend_settings(&self, spec: &Spec) -> TestResult<Vec<(String, String)>> {
        let secret = scratch("bootstrap-secret");
        std::fs::write(&secret, PASSWORD)?;
        let [f_obs, _, _] = &self.observability;
        let [f, _, _] = &self.roles;
        let mut settings = frontend_holding()?;
        for (name, value) in [
            ("observability.address", format!("tcp://{f_obs}")),
            (
                "metadata.bootstrap_secret_file",
                secret.display().to_string(),
            ),
            ("cluster.partition_count", spec.partitions.to_string()),
            ("frontend.role_address", f.clone()),
            ("endpoint.address", self.client.clone()),
        ] {
            settings = with(settings, name, &value);
        }
        if self.carrier == Carriers::Tls {
            let pki = pki()?;
            let (chain, key) = (scratch("server-chain.pem"), scratch("server-key.pem"));
            std::fs::write(&chain, pki.server_chain_pem())?;
            std::fs::write(&key, pki.server_key_pem())?;
            settings.retain(|(name, _)| name != "endpoint.plaintext");
            settings = with(
                settings,
                "endpoint.tls_cert_file",
                &chain.display().to_string(),
            );
            settings = with(
                settings,
                "endpoint.tls_key_file",
                &key.display().to_string(),
            );
        }
        for (name, value) in &spec.frontend {
            settings = with(settings, name, value);
        }
        Ok(settings)
    }

    /// The storage process of the partitions from `first`, `count` of them, at `address`, its observability at `observability`.
    fn storage_settings(
        &self,
        spec: &Spec,
        (first, count): (u32, u32),
        address: &str,
        observability: &str,
    ) -> TestResult<Vec<(String, String)>> {
        let [f, _, _] = &self.roles;
        let mut settings = storage_holding()?;
        for (name, value) in [
            ("observability.address", format!("tcp://{observability}")),
            ("storage.partition_first", first.to_string()),
            ("storage.partition_count", count.to_string()),
            ("storage.role_address", address.to_string()),
            ("storage.data_dir", fresh_data()),
            ("cluster.frontend_address", f.clone()),
        ] {
            settings = with(settings, name, &value);
        }
        for (name, value) in &spec.storage {
            settings = with(settings, name, value);
        }
        Ok(settings)
    }

    /// The compute process at `address`, its observability at `observability`.
    fn compute_settings(
        &self,
        address: &str,
        observability: &str,
    ) -> TestResult<Vec<(String, String)>> {
        let [f, _, _] = &self.roles;
        let mut settings = holding(compute_on_one_host(), SECRET)?;
        for (name, value) in [
            ("observability.address", format!("tcp://{observability}")),
            ("compute.role_address", address.to_string()),
            ("cluster.frontend_address", f.clone()),
        ] {
            settings = with(settings, name, &value);
        }
        Ok(settings)
    }

    /// `address` of the second process of a role: `SECOND_PROCESS` ports above it, or a socket of its own name.
    fn second(&self, address: &str) -> TestResult<String> {
        if let Some(stem) = address.strip_suffix(".sock") {
            return Ok(format!("{stem}2.sock"));
        }
        let target = address.trim_start_matches("tcp://");
        Ok(format!("tcp://{}", shifted(target, SECOND_PROCESS)?))
    }
}

/// One process of a cluster: what starts it again, and its pid while it runs.
#[derive(Debug)]
struct Member {
    program: &'static str,
    instance: String,
    log: PathBuf,
    observability: String,
    pid: u32,
    /// Whether a scenario killed it.
    killed: Cell<bool>,
}

/// A running cluster: its processes, which stop when it is dropped, and where it answers.
#[derive(Debug)]
pub(crate) struct Cluster {
    util: Option<ServiceUtil>,
    /// Where a client dials.
    pub(crate) client: ClientAt,
    /// The frontend's role address.
    pub(crate) frontend: String,
    /// Each partition's role address, partition 1 first.
    pub(crate) partitions: Vec<String>,
    config: String,
    members: Vec<Member>,
}

impl Drop for Cluster {
    /// Stops the processes, then removes their sockets, which a later start could not bind over.
    fn drop(&mut self) {
        // A client kept for this cluster dials nothing any more.
        crate::shells::forget_clients_of(&self.client.address);
        self.util = None;
        let sockets = [&self.client.address, &self.frontend]
            .into_iter()
            .chain(&self.partitions)
            .filter_map(|address| address.strip_prefix("unix://"));
        for socket in sockets {
            // A socket the process never bound is not there to remove.
            drop(std::fs::remove_file(socket));
        }
    }
}

/// The signal a scenario sends a role.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Signal {
    /// `SIGKILL`: the process is gone, as a crash leaves it.
    Kill,
    /// `SIGSTOP`: the process is hung: its sockets accept, and it answers nothing.
    Stop,
    /// `SIGCONT`: a stopped process goes on.
    Continue,
}

impl Cluster {
    fn member(&self, instance: &str) -> TestResult<&Member> {
        self.members
            .iter()
            .find(|member| member.instance == instance)
            .context("a member of the cluster")
    }

    /// Sends the process of `instance` a signal.
    pub(crate) fn signal(&self, instance: &str, signal: Signal) -> TestResult {
        let flag = match signal {
            Signal::Kill => "-KILL",
            Signal::Stop => "-STOP",
            Signal::Continue => "-CONT",
        };
        let member = self.member(instance)?;
        member.killed.set(matches!(signal, Signal::Kill));
        let pid = member.pid;
        let status = Command::new("kill")
            .arg(flag)
            .arg(pid.to_string())
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(TestError::new(format!("kill {flag} {pid}: {status}")))
        }
    }

    /// Starts `instance` again on the settings the config service holds, after a signal killed it, and waits until it answers
    /// its readiness.
    pub(crate) fn restart(&mut self, instance: &str) -> TestResult {
        let config = self.config.clone();
        let member = self
            .members
            .iter_mut()
            .find(|member| member.instance == instance)
            .context("a member of the cluster")?;
        let util = self.util.as_ref().context("a running cluster")?;
        let wait = answering(&member.observability, "/health");
        let started = started_process(member.program, &config, instance, &member.log, wait);
        member.pid = util.start_service_from_config(started)?;
        member.killed.set(false);
        Ok(())
    }

    /// Starts again every role a scenario killed, waiting until each and the frontend are ready; a killed frontend's cluster is
    /// not started again this way, since its roles went with it.
    fn revive(&mut self) -> TestResult {
        let killed: Vec<(String, String)> = self
            .members
            .iter()
            .filter(|member| member.killed.get())
            .map(|member| (member.instance.clone(), member.observability.clone()))
            .collect();
        for (instance, observability) in &killed {
            if instance == "frontend" {
                return Err(TestError::new("the frontend was killed"));
            }
            self.restart(instance)?;
            until_answering(observability, "/ready")?;
        }
        until_answering(&self.frontend_ready_address()?, "/ready")
    }

    /// Stops `instance` by SIGTERM, as an operator does, and waits for its exit: what it holds it syncs first.
    pub(crate) fn stop(&self, instance: &str) -> TestResult {
        let pid = self.member(instance)?.pid;
        let util = self.util.as_ref().context("a running cluster")?;
        let _exited = util.stop_service(pid)?;
        Ok(())
    }

    /// What the frontend has logged so far.
    pub(crate) fn frontend_log(&self) -> TestResult<String> {
        Ok(std::fs::read_to_string(&self.member("frontend")?.log)?)
    }

    /// Where `/ready` of the frontend answers.
    pub(crate) fn frontend_ready_address(&self) -> TestResult<String> {
        Ok(self.member("frontend")?.observability.clone())
    }
}

fn started_process(
    program: &'static str,
    config: &str,
    instance: &str,
    log: &std::path::Path,
    wait: WaitStrategy,
) -> ServiceStartConfig {
    process(program, bootstrap(config, instance), log, wait)
}

/// One process a cluster starts: its program, its instance's name, its settings and where it answers.
struct Planned {
    program: &'static str,
    instance: &'static str,
    settings: Vec<(String, String)>,
    observability: String,
    wait_path: &'static str,
}

/// The processes of `spec` at `layout`, in the order their readiness needs: frontend, storage, compute. A role's bare name is
/// the file its labelled instances share, so the first of two takes a label too.
fn plan(layout: &Layout, spec: &Spec) -> TestResult<Vec<Planned>> {
    let [f_obs, s_obs, c_obs] = &layout.observability;
    let [_, storage_role, compute_role] = &layout.roles;
    let first_count = if spec.two_storages {
        1
    } else {
        spec.partitions
    };
    let mut planned = vec![Planned {
        program: FRONTEND_BINARY,
        instance: "frontend",
        settings: layout.frontend_settings(spec)?,
        observability: f_obs.clone(),
        wait_path: "/health",
    }];
    planned.push(Planned {
        program: STORAGE_BINARY,
        instance: if spec.two_storages {
            "storage_1"
        } else {
            "storage"
        },
        settings: layout.storage_settings(spec, (1, first_count), storage_role, s_obs)?,
        observability: s_obs.clone(),
        wait_path: "/ready",
    });
    if spec.two_storages {
        let observability = shifted(s_obs, SECOND_PROCESS)?;
        planned.push(Planned {
            program: STORAGE_BINARY,
            instance: "storage_2",
            settings: layout.storage_settings(
                spec,
                (2, spec.partitions.saturating_sub(1).max(1)),
                &layout.second(storage_role)?,
                &observability,
            )?,
            observability,
            wait_path: "/ready",
        });
    }
    planned.push(Planned {
        program: COMPUTE_BINARY,
        instance: if spec.two_computes {
            "compute_1"
        } else {
            "compute"
        },
        settings: layout.compute_settings(compute_role, c_obs)?,
        observability: c_obs.clone(),
        wait_path: "/ready",
    });
    if spec.two_computes {
        let observability = shifted(c_obs, SECOND_PROCESS)?;
        planned.push(Planned {
            program: COMPUTE_BINARY,
            instance: "compute_2",
            settings: layout.compute_settings(&layout.second(compute_role)?, &observability)?,
            observability,
            wait_path: "/ready",
        });
    }
    Ok(planned)
}

/// Starts the config service holding `layout`'s roles of `spec`, then the frontend, storage and compute in the order their
/// readiness needs, and waits until the frontend is ready and its client endpoint accepts.
fn start(layout: &Layout, spec: &Spec, util: ServiceUtil) -> TestResult<Cluster> {
    let planned = plan(layout, spec)?;
    let instances: Vec<(&str, &[(String, String)])> = planned
        .iter()
        .map(|process| (process.instance, process.settings.as_slice()))
        .collect();
    let _service: u32 = start_config_service_at(&util, &layout.config, &instances)?;
    let mut members = Vec::with_capacity(planned.len());
    for process in &planned {
        let log = scratch(&format!(
            "{}-{}{}-{}",
            process.program, layout.tag, layout.shift, process.instance
        ));
        let wait = answering(&process.observability, process.wait_path);
        let started = started_process(
            process.program,
            &layout.config,
            process.instance,
            &log,
            wait,
        );
        let pid = util.start_service_from_config(started)?;
        members.push(Member {
            program: process.program,
            instance: process.instance.to_string(),
            log,
            observability: process.observability.clone(),
            pid,
            killed: Cell::new(false),
        });
    }
    let [frontend_role, _, _] = &layout.roles;
    let [f_obs, _, _] = &layout.observability;
    let cluster = Cluster {
        util: Some(util),
        client: layout.client_at()?,
        frontend: frontend_role.clone(),
        partitions: (1..=spec.partitions)
            .map(|partition| layout.partition(partition, spec))
            .collect::<TestResult<_>>()?,
        config: layout.config.clone(),
        members,
    };
    until_answering(f_obs, "/ready")?;
    until_listening(&layout.client)?;
    Ok(cluster)
}

/// Starts a scenario's own cluster over `carrier` at the declared ports, as `spec` says.
pub(crate) fn start_own_as(carrier: Carriers, spec: &Spec) -> TestResult<Cluster> {
    start(&Layout::new(carrier, 0, "own-")?, spec, crate::services()?)
}

/// Starts a scenario's own cluster over `carrier` at the declared ports, the placement over `partitions` partitions.
pub(crate) fn start_own(carrier: Carriers, partitions: u32) -> TestResult<Cluster> {
    start_own_as(carrier, &Spec::over(partitions))
}

thread_local! {
    /// The clusters a run has started and shares, by what names them: a start takes seconds, and each scenario keeps to IRIs of
    /// its own.
    static SHARED: RefCell<Vec<(String, Cluster)>> = const { RefCell::new(Vec::new()) };
    /// How many clusters of a spec of their own this run has started, each at ports of its own.
    static SPECS_STARTED: Cell<u16> = const { Cell::new(0) };
}

/// The cluster named `name` of this run, started when a scenario first asks: where a client dials it.
fn shared_at(name: &str, layout: &Layout, spec: &Spec) -> TestResult<ClientAt> {
    let started = SHARED.with_borrow(|shared| shared.iter().any(|(held, _)| held == name));
    if !started {
        let cluster = start(layout, spec, crate::services()?)?;
        SHARED.with_borrow_mut(|shared| shared.push((name.to_string(), cluster)));
    }
    layout.client_at()
}

/// Lets go of what this run started, before the next run's clusters take the ports.
pub(crate) fn stop_shared() {
    drop(SHARED.take());
    SPECS_STARTED.set(0);
}

/// The cluster of `spec`, taken from those this run holds, started when no scenario has asked for one: a scenario holds it to
/// itself, and hands it back by [`hand_back`].
pub(crate) fn taken(carrier: Carriers, spec: &Spec) -> TestResult<(String, Cluster)> {
    let name = format!("{spec:?}");
    let held = SHARED.with_borrow_mut(|shared| {
        shared
            .iter()
            .position(|(key, _)| *key == name)
            .map(|place| shared.swap_remove(place).1)
    });
    if let Some(cluster) = held {
        return Ok((name, cluster));
    }
    let at = SPECS_STARTED.get();
    SPECS_STARTED.set(at + 1);
    let shift = SPECS_SHIFT + at * SPEC_PORTS;
    let cluster = start(
        &Layout::new(carrier, shift, "spec-")?,
        spec,
        crate::services()?,
    )?;
    Ok((name, cluster))
}

/// Hands `cluster` back as what `name` names, a role a scenario killed started again and ready: one it cannot start again goes.
pub(crate) fn hand_back(name: String, mut cluster: Cluster) {
    if cluster.revive().is_ok() {
        SHARED.with_borrow_mut(|shared| shared.push((name, cluster)));
    }
}

/// The client endpoint of the shared cluster of one partition over `carrier`.
pub(crate) fn shared_client(carrier: Carriers) -> TestResult<ClientAt> {
    shared_at(
        "one",
        &Layout::new(carrier, SHARED_SHIFT, "shared-")?,
        &Spec::one_partition(),
    )
}

/// The client endpoint of the shared cluster of several partitions over `carrier`.
pub(crate) fn spread_client(carrier: Carriers) -> TestResult<ClientAt> {
    let layout = Layout::new(carrier, SPREAD_SHIFT, "spread-")?;
    let at = shared_at("spread", &layout, &Spec::over(SPREAD_PARTITIONS))?;
    hold_spread_ranges(&layout.roles[0])?;
    Ok(at)
}

/// The ranges of the shared spread cluster's placement map. They follow from the partition count alone, so one read serves
/// every run.
static SPREAD_RANGES: OnceLock<Vec<(u32, u32, u32)>> = OnceLock::new();

/// Reads the shared spread cluster's ranges from its frontend, once in a process.
fn hold_spread_ranges(frontend: &str) -> TestResult {
    if SPREAD_RANGES.get().is_none() {
        let ranges = placement_ranges(frontend)?;
        drop(SPREAD_RANGES.set(ranges));
    }
    Ok(())
}

/// The ranges of the placement map the frontend at `frontend` holds, as its own Placement answers them: first bucket, past
/// bucket and partition. It runs on a compio runtime of its own, since a role client speaks compio whichever shell a run
/// drives.
pub(crate) fn placement_ranges(frontend: &str) -> TestResult<Vec<(u32, u32, u32)>> {
    Runtime::new()?.block_on(async {
        let mut role =
            RoleClient::dial(&Address::parse(frontend)?, CLUSTER, "storage", SECRET).await?;
        let reply = role.request(&Placement::request(0)).await?;
        let view = reply.view()?;
        let map = PlacementView::from_frame(&view)?;
        Ok((0..map.range_count())
            .filter_map(|index| map.range(index))
            .map(|range| (range.start(), range.end(), range.partition()))
            .collect())
    })
}

/// The partition `ranges` give `iri`'s bucket to.
pub(crate) fn partition_of(ranges: &[(u32, u32, u32)], bucket: u32) -> Option<u32> {
    ranges
        .iter()
        .find(|(start, end, _)| *start <= bucket && bucket < *end)
        .map(|(_, _, holder)| *holder)
}

/// An IRI of the prefix `stem` whose bucket `ranges` give to `partition`.
pub(crate) fn iri_of_partition(
    ranges: &[(u32, u32, u32)],
    stem: &str,
    partition: u32,
) -> TestResult<String> {
    (0..IRIS_TRIED)
        .map(|at| format!("{stem}{at}"))
        .find(|iri| {
            iri_bucket(crate::client_reads::TENANT, iri)
                .is_some_and(|bucket| partition_of(ranges, bucket.get()) == Some(partition))
        })
        .context("an IRI of the partition")
}

/// An IRI of the prefix `stem` whose bucket the shared spread cluster gives to `partition`.
pub(crate) fn iri_of_spread(stem: &str, partition: u32) -> TestResult<String> {
    iri_of_partition(spread_ranges()?, stem, partition)
}

/// The ranges of the shared spread cluster, started if no scenario has asked for it.
pub(crate) fn spread_ranges() -> TestResult<&'static [(u32, u32, u32)]> {
    drop(spread_client(crate::current_carrier())?);
    SPREAD_RANGES
        .get()
        .map(Vec::as_slice)
        .context("the shared spread cluster's ranges")
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

/// A client of the endpoint at `at`, connected and authenticated as the administrator.
pub(crate) async fn client(at: &ClientAt) -> TestResult<TestClient> {
    let mut client = TestClient::new(at, &crate::shells::Knobs::default())?;
    client.connect().await?;
    Ok(client)
}
