/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The processes a scenario over the system group starts, kills and starts again: the storage process hosting the group, a
//! second storage process hosting none, and compute, each on settings served by the config double; and the administrator's
//! client.

use std::path::PathBuf;
use std::process::ExitStatus;
use std::time::Duration;

use compio::runtime::Runtime;
use service_utils::{ServiceUtil, WaitStrategy};
use ultragraphdb_client_compio::{Client, ClientConfig, TcpConnector};
use ultragraphdb_common_role::RoleClient;
use ultragraphdb_protocol_scram::{ScramError, ScramLogin, system_nonce};
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{
    BodyView, DirectWrite, Placement, PlacementView, TermTable, WriteChange,
};
use ultragraphdb_store_quads::iri_bucket;
use ultragraphdb_tests::{
    CLUSTER, COMPUTE_BINARY, SECRET, START_SECONDS, STORAGE_BINARY, answering, client_address,
    compute_observability, not_hosting, role, scratch, storage_holding, storage_observability,
    system_role_address, until_answering, with,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult, loopback};

use crate::endpoint_steps::{ADMIN, PASSWORD, bootstrapping, until_listening};

/// The second storage process's partition, 2 of two, listens here, and its `/health` and `/ready` at
/// [`second_observability`]: it hosts no system group. Offset 5 above the target's port base, one the harness leaves free.
pub(crate) fn second_role_address() -> TestResult<String> {
    loopback(5)
}

/// Where the second storage process's `/health` and `/ready` listen: offset 16 above the target's port base, one the harness
/// leaves free.
pub(crate) fn second_observability() -> TestResult<String> {
    loopback(16)
}

/// The tenant every write of these scenarios names.
pub(crate) const TENANT: u64 = 0;

/// How long a step lets the client endpoint send the Ended of a commit it answered before a process is killed, so the session holds
/// no mark of a connection that closes with it (`PROTOCOL.md` §11.5): far above a turn.
pub(crate) const ENDED: Duration = Duration::from_millis(300);

/// A client authenticated as the administrator the hosting storage process bootstraps.
pub(crate) type Admin = Client<TcpConnector, ScramLogin<fn() -> Result<String, ScramError>>>;

/// The processes of a scenario and the settings each started on, so a killed one starts again as the same instance would.
#[derive(Debug, Default)]
pub(crate) struct Roles {
    pub(crate) util: Option<ServiceUtil>,
    pub(crate) host: Option<(u32, Vec<(String, String)>)>,
    pub(crate) second: Option<(u32, Vec<(String, String)>)>,
    pub(crate) compute: Option<(u32, Vec<(String, String)>)>,
    /// The log of each start, by the order of the starts.
    pub(crate) logs: Vec<PathBuf>,
}

impl Roles {
    pub(crate) fn util(&self) -> TestResult<&ServiceUtil> {
        self.util.as_ref().context("the scenario's processes")
    }

    /// Starts `program` on `settings`, its log named for `name` and the start, waiting as `wait` says: its pid.
    fn start(
        &mut self,
        program: &'static str,
        settings: &[(String, String)],
        name: &str,
        wait: WaitStrategy,
    ) -> TestResult<u32> {
        if self.util.is_none() {
            self.util = Some(crate::services()?);
        }
        let log = scratch(&format!("{name}-{}", self.logs.len()));
        let pid = self
            .util()?
            .start_service_from_config(role(program, settings, &log, wait)?)?;
        self.logs.push(log);
        Ok(pid)
    }

    /// Starts the storage process hosting the system group on `settings`, waiting for its `/ready` when `ready`.
    pub(crate) fn start_host(
        &mut self,
        settings: Vec<(String, String)>,
        ready: bool,
    ) -> TestResult {
        let wait = if ready {
            answering(&storage_observability()?, "/ready")
        } else {
            WaitStrategy::NoWait
        };
        let pid = self.start(STORAGE_BINARY, &settings, "storage-host", wait)?;
        self.host = Some((pid, settings));
        Ok(())
    }

    /// Starts the second storage process on `settings`, once its `/ready` answers.
    pub(crate) fn start_second(&mut self, settings: Vec<(String, String)>) -> TestResult {
        let wait = answering(&second_observability()?, "/ready");
        let pid = self.start(STORAGE_BINARY, &settings, "storage-second", wait)?;
        self.second = Some((pid, settings));
        Ok(())
    }

    /// Starts compute on `settings`, once its `/health` answers, and when `ready` until its client endpoint listens and its
    /// `/ready` answers.
    pub(crate) fn start_compute(
        &mut self,
        settings: Vec<(String, String)>,
        ready: bool,
    ) -> TestResult {
        let wait = answering(&compute_observability()?, "/health");
        let pid = self.start(COMPUTE_BINARY, &settings, "compute", wait)?;
        self.compute = Some((pid, settings));
        if ready {
            until_listening()?;
            until_answering(&compute_observability()?, "/ready")?;
        }
        Ok(())
    }

    /// Starts compute on `settings` and waits for nothing.
    pub(crate) fn start_compute_unwaited(&mut self, settings: Vec<(String, String)>) -> TestResult {
        let pid = self.start(COMPUTE_BINARY, &settings, "compute", WaitStrategy::NoWait)?;
        self.compute = Some((pid, settings));
        Ok(())
    }

    /// The last log of the starts named `name`, read.
    pub(crate) fn last_log(&self, name: &str) -> TestResult<String> {
        let log = self
            .logs
            .iter()
            .rev()
            .find(|log| {
                log.file_name()
                    .is_some_and(|file| file.to_string_lossy().starts_with(name))
            })
            .context("a log of the start")?;
        Ok(std::fs::read_to_string(log)?)
    }

    /// Kills `pid` with SIGKILL, as a deleted pod or a crashed host is, and reaps it.
    pub(crate) fn kill(&self, pid: u32) -> TestResult<ExitStatus> {
        signal(pid, "KILL")?;
        Ok(self
            .util()?
            .wait_for_exit(pid, Duration::from_secs(START_SECONDS))?)
    }

    pub(crate) fn host_pid(&self) -> TestResult<u32> {
        self.host.as_ref().map(|(pid, _)| *pid).context("the host")
    }

    pub(crate) fn compute_pid(&self) -> TestResult<u32> {
        self.compute
            .as_ref()
            .map(|(pid, _)| *pid)
            .context("compute")
    }

    /// Kills the second storage process and starts it again at its address, waiting for its `/ready`.
    pub(crate) fn restart_second(&mut self) -> TestResult {
        let (pid, settings) = self.second.take().context("the second storage process")?;
        let _killed = self.kill(pid)?;
        self.start_second(settings)
    }

    /// Kills compute and starts it again on its settings, waiting for its `/ready`.
    pub(crate) fn restart_compute(&mut self) -> TestResult {
        let (pid, settings) = self.compute.take().context("compute")?;
        let _killed = self.kill(pid)?;
        self.start_compute(settings, true)
    }
}

/// Sends `signal` to `pid`, as an operator would.
pub(crate) fn signal(pid: u32, signal: &str) -> TestResult {
    let sent = std::process::Command::new("kill")
        .arg(format!("-{signal}"))
        .arg(pid.to_string())
        .output()?;
    if sent.status.success() {
        return Ok(());
    }
    Err(TestError::new(format!("kill -{signal} {pid}: {sent:?}")))
}

/// The settings of the storage process hosting the system group over `partitions` partitions, holding partition 1 and
/// bootstrapping the administrator.
pub(crate) fn host_of(partitions: u32) -> TestResult<Vec<(String, String)>> {
    Ok(with(
        bootstrapping()?,
        "cluster.partition_count",
        &partitions.to_string(),
    ))
}

/// The settings of the second storage process: partition 2, hosting no system group.
pub(crate) fn second_of() -> TestResult<Vec<(String, String)>> {
    let settings = [
        (
            "observability.address",
            format!("tcp://{}", second_observability()?),
        ),
        (
            "storage.role_address",
            format!("tcp://{}", second_role_address()?),
        ),
        ("storage.partition_first", "2".to_string()),
        ("storage.partition_count", "1".to_string()),
    ];
    Ok(settings
        .iter()
        .fold(not_hosting(storage_holding()?), |held, (name, value)| {
            with(held, name, value)
        }))
}

/// The administrator's client of the client endpoint, connected and authenticated, making each commit at most `attempts` times.
pub(crate) async fn admin(attempts: u32) -> TestResult<Admin> {
    let nonces: fn() -> Result<String, ScramError> = system_nonce;
    let login = ScramLogin::new(ADMIN, PASSWORD, nonces)?;
    let config =
        ClientConfig::new(CLUSTER, "ultragraphdb-compute-tests").with_commit_attempts(attempts);
    let mut client = Client::new(
        TcpConnector::new(client_address()?.parse()?),
        config,
        login,
        16,
    );
    client.connect().await?;
    Ok(client)
}

/// A one-frame write of `<subject> <http://ex.org/p> <object>` in [`TENANT`].
pub(crate) fn write_of(subject: &str, object: &str) -> DirectWrite {
    let mut terms = TermTable::new();
    let change = WriteChange::Insert {
        graph: None,
        subject: terms.iri(subject),
        predicate: terms.iri("http://ex.org/p"),
        object: terms.iri(object),
    };
    DirectWrite::new(TENANT, terms, vec![change])
}

/// The ranges of the placement the system group holds, as its Placement answers them: first bucket, past bucket, partition.
pub(crate) fn placement_ranges() -> TestResult<Vec<(u32, u32, u32)>> {
    Runtime::new()?.block_on(async {
        let address = Address::parse(&format!("tcp://{}", system_role_address()?))?;
        let mut client = RoleClient::dial(&address, CLUSTER, "storage", SECRET).await?;
        let reply = client.request(&Placement::request(0)).await?;
        let view = reply.view()?;
        let map = PlacementView::from_frame(&view)?;
        Ok((0..map.range_count())
            .filter_map(|index| map.range(index))
            .map(|range| (range.start(), range.end(), range.partition()))
            .collect())
    })
}

/// A subject IRI of the stem `stem` whose bucket the placement gives to `partition`.
pub(crate) fn subject_of(stem: &str, partition: u32) -> TestResult<String> {
    let ranges = placement_ranges()?;
    (0..100_000)
        .map(|at| format!("{stem}{at}"))
        .find(|iri| {
            iri_bucket(TENANT, iri).is_some_and(|bucket| {
                ranges.iter().any(|(start, end, holder)| {
                    *holder == partition && *start <= bucket.get() && bucket.get() < *end
                })
            })
        })
        .context("a subject of the partition")
}
