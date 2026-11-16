/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Endpoint cores whose system group a test plays (`SystemPlay` at a `RoleDouble`'s address), each core's link, links and
//! client listener on the test's thread: what the group answers, the scenario decides.

use std::time::{Duration, Instant};

use compio::runtime::{JoinHandle, spawn};
use ultragraphdb_common_frontend::{
    AuthThrottle, ClientSession, DoubleFrame, EndpointCore, EndpointLimits, EveryAuthenticated,
    LoginLimits, RelayAuthenticator, RoleDouble, RoleLinks, SystemLink, SystemPlay, TenantGate,
};
use ultragraphdb_common_role::{Answer, RoleClient, RoleCluster, Server, serve_connection_with};
use ultragraphdb_common_service::Readiness;
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_protocol_transport::{Address, Endpoint};
use ultragraphdb_protocol_transport_compio::Listener;
use ultragraphdb_tool_bdd::{TestError, TestResult};

use crate::Steps;
use crate::endpoint_stage::{CLUSTER, Client, fresh};

/// The principal the played group holds: `ann`, password `pw`.
pub(crate) const ANN: u64 = 41;
const WAIT: Duration = Duration::from_secs(5);

/// Now, from the runtime's timer, whose first tick is now.
pub(crate) async fn now() -> Instant {
    compio::time::interval(Duration::from_secs(1)).tick().await
}

/// One endpoint core of a played stage: its link and where its clients connect.
pub(crate) struct Core {
    pub system: SystemLink,
    pub client: Address,
    _accepting: JoinHandle<()>,
}

/// Endpoint cores over a played system group.
pub(crate) struct Played {
    pub play: SystemPlay,
    pub group: RoleDouble,
    pub cluster: RoleCluster,
    pub cores: Vec<Core>,
}

impl Played {
    /// `cores` endpoint cores over `play`, holding `ann`, each core's view answering.
    pub(crate) async fn start(play: SystemPlay, cores: usize) -> TestResult<Self> {
        let directory = fresh()?;
        let file = directory.join("role-secret");
        std::fs::write(&file, "the role secret of a played stage")?;
        let secret = ultragraphdb_common_role::RoleSecret::read(&file.display().to_string())?;
        let cluster = RoleCluster::new(CLUSTER, secret, Duration::from_secs(2));
        let play = play.with_principal(
            "ann",
            ANN,
            ScramVerifier::derive("pw", b"ann's salt", 4_096)?,
        );
        let script = play.clone();
        let group = RoleDouble::start(&cluster, Endpoint::Frontend, move |frame| {
            script.answer(frame)
        })
        .await?;
        let mut played = Self {
            play,
            group,
            cluster,
            cores: Vec::new(),
        };
        for _ in 0..cores {
            let _core = played.add_core().await?;
        }
        Ok(played)
    }

    /// One more endpoint core, its view answering.
    pub(crate) async fn add_core(&mut self) -> TestResult<usize> {
        let links = RoleLinks::new(self.cluster.clone(), "frontend".to_string());
        let address = Address::parse(self.group.address())?;
        let system = SystemLink::dial(
            address,
            self.cluster.clone(),
            links.clone(),
            5_000,
            Readiness::default(),
        );
        system
            .answering(now().await + WAIT)
            .await
            .map_err(|refusal| TestError::new(format!("{refusal:?}")))?;
        let (throttle, throttling) = AuthThrottle::new(LoginLimits::default()).serve();
        spawn(throttling).detach();
        let core = EndpointCore::new(
            system.clone(),
            throttle,
            links.clone(),
            TenantGate::new(EveryAuthenticated),
            EndpointLimits::new(1 << 22, Duration::from_secs(30)),
        );
        let listener = Listener::bind(&Address::any_port("127.0.0.1")?).await?;
        let client = listener.local_address()?;
        let server = Server::new(CLUSTER, "frontend", Endpoint::Client)?;
        let accepting = spawn(async move {
            while let Ok(carrier) = listener.accept().await {
                let relay = RelayAuthenticator::default();
                let session = ClientSession::new(core.clone(), relay.clone(), carrier.peer_ip());
                let server = server.clone();
                spawn(async move { serve_connection_with(carrier, &server, relay, session).await })
                    .detach();
            }
        });
        self.cores.push(Core {
            system,
            client,
            _accepting: accepting,
        });
        Ok(self.cores.len() - 1)
    }

    /// `ann`'s connection to core `at`.
    pub(crate) async fn ann(&self, at: usize) -> TestResult<Client> {
        let core = self
            .cores
            .get(at)
            .ok_or_else(|| TestError::new("no such core"))?;
        Ok(Client::over(
            RoleClient::dial(&core.client, CLUSTER, "ann", "pw").await?,
        ))
    }

    /// A partition played by `script`, which Members lists as `partition` and every core has read.
    pub(crate) async fn partition<F>(&self, partition: u32, script: F) -> TestResult<RoleDouble>
    where
        F: Fn(&DoubleFrame) -> Answer + 'static,
    {
        let double = RoleDouble::start(&self.cluster, Endpoint::Partition, script).await?;
        self.play.storage(partition, double.address());
        self.read_members().await?;
        Ok(double)
    }

    /// A compute process played by `script`, which Members lists and every core has read.
    pub(crate) async fn compute<F>(&self, script: F) -> TestResult<RoleDouble>
    where
        F: Fn(&DoubleFrame) -> Answer + 'static,
    {
        let double = RoleDouble::start(&self.cluster, Endpoint::Compute, script).await?;
        self.play.compute(double.address());
        self.read_members().await?;
        Ok(double)
    }

    /// Every core reads Members.
    pub(crate) async fn read_members(&self) -> TestResult {
        for core in &self.cores {
            core.system
                .members()
                .await
                .map_err(|refusal| TestError::new(format!("{refusal:?}")))?;
        }
        Ok(())
    }

    /// Core `at`'s link.
    pub(crate) fn system(&self, at: usize) -> TestResult<&SystemLink> {
        self.cores
            .get(at)
            .map(|core| &core.system)
            .ok_or_else(|| TestError::new("no such core"))
    }
}

/// Runs `scenario` over a fresh runtime, marking `w` as run once it passed.
pub(crate) fn played_run<F, Fut>(w: &mut Steps, scenario: F) -> TestResult
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = TestResult>,
{
    compio::runtime::Runtime::new()?.block_on(scenario())?;
    w.ran = true;
    Ok(())
}
