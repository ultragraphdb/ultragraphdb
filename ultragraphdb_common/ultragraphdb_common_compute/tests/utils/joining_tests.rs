/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Compute joining the system group and holding it and the partitions (`PROTOCOL.md` §11.12, *Joining* and *While the system
//! group cannot be reached*; system-group, "A lost system group is waited for until the deadline"; role-binaries, "Exit
//! codes name the cause"): corner cases C6.1 to C6.5. Time is the test's: [`hold_peers`] reads it from a clock the test
//! moves, so whether a peer is gone past the deadline is decided by the clock alone.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use compio::runtime::spawn;
use ultragraphdb_common_compute::{hold_peers, join_group};
use ultragraphdb_common_role::{
    Answer, Push, ROLE_ITERATIONS, Server, Session, serve_connection, system_address,
};
use ultragraphdb_common_service::{RoleIdentity, RoleStart, Stopping};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_transport::{Address, Endpoint, ReceivedFrame};
use ultragraphdb_protocol_transport_compio::Listener;
use ultragraphdb_protocol_wire::{
    BodyBuilder, Command, ErrorCode, ErrorReply, FLAG_LAST, Header, Join, Members,
};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::played::played_cluster::{CLUSTER, SECRET, role_cluster, run};
use crate::played::played_frontend::{PlayedFrontend, even_ranges};

/// Compute's own role address, which its Join names.
const OWN: &str = "tcp://127.0.0.1:7082";

/// The deadline of the holding tests, which their clock passes: a peer gone that long is lost.
const DEADLINE: Duration = Duration::from_millis(200);

/// One tick of the clock: a microsecond.
const TICK: Duration = Duration::from_micros(1);

/// The settings of a compute process whose `[cluster]` table holds `cluster`.
fn start(cluster: &str) -> TestResult<RoleStart> {
    let text = format!(
        "[observability]\naddress = \"tcp://127.0.0.1:1\"\n[compute]\nrole_address = \"{OWN}\"\n[cluster]\n{cluster}"
    );
    Ok(RoleStart::from_settings(
        text.as_bytes(),
        RoleIdentity::new("ci", "compute", 1),
    )?)
}

/// C6.4: compute dials the system group at `cluster.system_address`, the one address its settings name for it (design D2);
/// settings naming the old `cluster.frontend_address` are refused at the start, naming it.
#[test]
fn test_the_group_is_the_system_address() -> TestResult {
    let named = start("system_address = \"unix:///run/ultragraph/system.sock\"\n")?;
    assert_eq!(
        system_address(&named)?.to_string(),
        "unix:///run/ultragraph/system.sock"
    );
    let old = start(
        "frontend_address = \"tcp://frontend-0:7081\"\nsystem_address = \"tcp://storage-0:7100\"\n",
    )
    .err()
    .context("a refusal")?;
    assert!(old.to_string().contains("frontend_address"), "{old}");
    Ok(())
}

/// No address, or a `cluster.system_address` that is no address, is a configuration error naming `cluster.system_address`.
#[test]
fn test_no_group_address_is_refused_naming_the_system_address() -> TestResult {
    let refusals = [start("")?, start("system_address = \"not an address\"\n")?];
    for settings in refusals {
        let refused = system_address(&settings).err().context("a refusal")?;
        assert_eq!(refused.exit_code(), RoleError::EX_CONFIG);
        assert!(
            refused.to_string().contains("cluster.system_address"),
            "{refused}"
        );
    }
    Ok(())
}

/// Polls `holds` until it is true, failing past 10 s, a bound no step here comes near: the step's outcome is what the test
/// asserts after it, never how long it took.
async fn until(what: &str, holds: impl Fn() -> bool) -> TestResult {
    for _ in 0..2_000 {
        if holds() {
            return Ok(());
        }
        compio::time::sleep(Duration::from_millis(5)).await;
    }
    Err(TestError::new(format!("{what} did not happen")))
}

/// What a task's future gave, once it ended.
type Ended<T> = Rc<RefCell<Option<T>>>;

/// C6.1: compute dials a group that does not answer until it answers, joins, and goes on until its Members lists a partition
/// that answers in turn: where the partitions listen, and the group took its Join at the group's address (C6.4).
#[test]
fn test_compute_dials_until_the_group_answers_and_a_partition_joined() -> TestResult {
    run(async {
        let cluster = role_cluster(SECRET, Duration::from_secs(30))?;
        let group = PlayedFrontend::start(&cluster, even_ranges(1)?).await?;
        let partition = PlayedFrontend::start(&cluster, even_ranges(1)?).await?;
        group.answer(false);
        let joined: Ended<Result<Option<Vec<Address>>, RoleError>> = Rc::default();
        let address = Address::parse(group.address())?;
        let (told, stopping) = (Rc::clone(&joined), Stopping::default());
        let task = {
            let cluster = cluster.clone();
            spawn(async move {
                let outcome = join_group(&address, &Join::compute(OWN), &cluster, &stopping).await;
                *told.borrow_mut() = Some(outcome);
            })
        };
        until("a dial turned away", || group.turned_away() > 0).await?;
        group.answer(true);
        until("a Members asked", || group.members_asked() > 0).await?;
        assert!(joined.borrow().is_none(), "no partition has joined yet");
        group.list(1, partition.address());
        until("the join", || joined.borrow().is_some()).await?;
        drop(task);
        let partitions = joined
            .borrow_mut()
            .take()
            .context("an outcome")?
            .map_err(|error| TestError::new(error.to_string()))?
            .context("partitions")?;
        assert_eq!(partitions, vec![Address::parse(partition.address())?]);
        assert!(group.joins().iter().all(|join| join == OWN));
        assert!(!group.joins().is_empty());
        assert!(partition.dials() > 0, "the partition was asked to answer");
        Ok(())
    })
}

/// C6.5: a group holding another role secret refuses each dial with code 19 (AuthenticationFailed); compute asks it again
/// until its deadline, then gives up with code 69 naming the group's address and the refusal.
#[test]
fn test_a_group_refusing_the_secret_is_asked_until_the_deadline() -> TestResult {
    run(async {
        let other = role_cluster("another secret", Duration::from_secs(5))?;
        let group = PlayedFrontend::start(&other, even_ranges(1)?).await?;
        let cluster = role_cluster(SECRET, Duration::from_millis(300))?;
        let address = Address::parse(group.address())?;
        let refused = join_group(
            &address,
            &Join::compute(OWN),
            &cluster,
            &Stopping::default(),
        )
        .await
        .err()
        .context("a refusal")?;
        assert_eq!(refused.exit_code(), RoleError::EX_UNAVAILABLE);
        let message = refused.to_string();
        assert!(message.contains(group.address()), "{message}");
        assert!(message.contains("AuthenticationFailed"), "{message}");
        assert!(group.dials() > 1, "asked again: {}", group.dials());
        assert!(group.joins().is_empty());
        Ok(())
    })
}

/// A service stopping ends a join under way with nothing.
#[test]
fn test_a_stop_ends_a_join_with_nothing() -> TestResult {
    run(async {
        let cluster = role_cluster(SECRET, Duration::from_secs(30))?;
        let group = PlayedFrontend::start(&cluster, even_ranges(1)?).await?;
        group.answer(false);
        let stopping = Stopping::default();
        stopping.set();
        let address = Address::parse(group.address())?;
        let joined = join_group(&address, &Join::compute(OWN), &cluster, &stopping)
            .await
            .map_err(|error| TestError::new(error.to_string()))?;
        assert_eq!(joined, None);
        Ok(())
    })
}

/// The group and a peer standing in for a partition, held by compute on a clock the test moves.
struct Held {
    group: PlayedFrontend,
    partition: PlayedFrontend,
    clock: Rc<Cell<Duration>>,
    stopping: Stopping,
    ended: Ended<Result<(), RoleError>>,
}

impl Held {
    /// Holds a group listing compute already, as one it joined does, and a partition, the clock at 0, the group of a cluster
    /// whose roles hold `group_secret`, compute's deadline `deadline`.
    async fn start(group_secret: &str, deadline: Duration) -> TestResult<Self> {
        let cluster = role_cluster(SECRET, deadline)?;
        let group =
            PlayedFrontend::start(&role_cluster(group_secret, deadline)?, even_ranges(1)?).await?;
        let partition = PlayedFrontend::start(&cluster, even_ranges(1)?).await?;
        group.list_compute(OWN);
        let held = Self {
            group,
            partition,
            clock: Rc::default(),
            stopping: Stopping::default(),
            ended: Rc::default(),
        };
        let (address, partitions) = (
            Address::parse(held.group.address())?,
            vec![Address::parse(held.partition.address())?],
        );
        let (clock, stopping, told) = (
            Rc::clone(&held.clock),
            held.stopping.clone(),
            Rc::clone(&held.ended),
        );
        spawn(async move {
            let join = Join::compute(OWN);
            let outcome = hold_peers(
                &address,
                &join,
                partitions,
                &cluster,
                &stopping,
                move || clock.get(),
            )
            .await;
            *told.borrow_mut() = Some(outcome);
        })
        .detach();
        Ok(held)
    }

    /// Whether holding has ended.
    fn has_ended(&self) -> bool {
        self.ended.borrow().is_some()
    }

    /// How holding ended, once it has.
    async fn outcome(&self) -> TestResult<Result<(), RoleError>> {
        until("holding's end", || self.has_ended()).await?;
        self.ended.borrow_mut().take().context("an outcome")
    }
}

/// C6.2, C6.3: a group whose connections close is dialled again; back one tick before compute's deadline, and holding no
/// compute as a restarted group does, it is sent compute's Join again and compute goes on. A group listing compute is sent
/// none. A stop ends holding cleanly.
#[test]
fn test_a_group_back_before_the_deadline_is_joined_again() -> TestResult {
    run(async {
        let held = Held::start(SECRET, DEADLINE).await?;
        let group = &held.group;
        // The second ask begins once the first has ended, so a Join the first sent has arrived by then.
        until("two Members asked", || group.members_asked() > 1).await?;
        assert!(
            group.joins().is_empty(),
            "a group listing compute takes no Join"
        );
        group.answer(false);
        until("a dial turned away", || group.turned_away() > 0).await?;
        held.clock.set(DEADLINE - TICK);
        group.forget_compute();
        group.answer(true);
        until("a Join", || !group.joins().is_empty()).await?;
        assert_eq!(group.joins(), vec![OWN.to_string()]);
        assert!(!held.has_ended(), "compute goes on");
        held.stopping.set();
        assert!(held.outcome().await?.is_ok());
        Ok(())
    })
}

/// The user's decision of 2026-11-17 (`notes/group-7.md`, *Found wanting* 1): compute joins a group again as soon as its
/// connection to it comes back. The group restarts once compute's first ask is answered on the wire, closing compute's
/// connection and holding no Join from before; compute's deadline of 60 s puts its next ask 30 s away, past the 10 s `until`
/// waits, and the clock stands at 0, so no deadline passes: the Join comes on the connection compute dials as the old one
/// closes.
#[test]
fn test_a_restarted_group_is_joined_again_at_once() -> TestResult {
    run(async {
        let held = Held::start(SECRET, Duration::from_secs(60)).await?;
        let group = &held.group;
        until("the first ask answered", || {
            group.members_asked() > 0 && group.requests_open() == 0
        })
        .await?;
        assert!(
            group.joins().is_empty(),
            "a group listing compute takes no Join"
        );
        let dialled = group.dials();
        group.restart();
        until("a Join", || !group.joins().is_empty()).await?;
        assert_eq!(group.joins(), vec![OWN.to_string()]);
        assert!(group.dials() > dialled, "the Join came on a new connection");
        assert!(!held.has_ended(), "compute goes on");
        held.stopping.set();
        assert!(held.outcome().await?.is_ok());
        Ok(())
    })
}

/// C6.3: a group gone is waited for until the deadline, not a tick less, and then ends compute with code 69 naming the group
/// and its address.
#[test]
fn test_a_group_gone_past_the_deadline_ends_compute_naming_its_address() -> TestResult {
    run(async {
        let held = Held::start(SECRET, DEADLINE).await?;
        let group = &held.group;
        until("a Members asked", || group.members_asked() > 0).await?;
        group.answer(false);
        until("a dial turned away", || group.turned_away() > 0).await?;
        held.clock.set(DEADLINE - TICK);
        let seen = group.turned_away();
        until("another dial turned away", || group.turned_away() > seen).await?;
        assert!(
            !held.has_ended(),
            "one tick before the deadline compute waits"
        );
        held.clock.set(DEADLINE);
        let lost = held.outcome().await?.err().context("a loss")?;
        assert_eq!(lost.exit_code(), RoleError::EX_UNAVAILABLE);
        let message = lost.to_string();
        assert!(message.contains("system group"), "{message}");
        assert!(message.contains(group.address()), "{message}");
        Ok(())
    })
}

/// C6.5: a group refusing compute's secret is a group that does not answer: past the deadline compute ends with code 69
/// naming its address and the refusal, code 19.
#[test]
fn test_a_group_refusing_the_secret_ends_compute_at_the_deadline() -> TestResult {
    run(async {
        let held = Held::start("another secret", DEADLINE).await?;
        let group = &held.group;
        until("a dial", || group.dials() > 0).await?;
        held.clock.set(DEADLINE);
        let lost = held.outcome().await?.err().context("a loss")?;
        assert_eq!(lost.exit_code(), RoleError::EX_UNAVAILABLE);
        let message = lost.to_string();
        assert!(message.contains(group.address()), "{message}");
        assert!(message.contains("AuthenticationFailed"), "{message}");
        assert_eq!(group.members_asked(), 0);
        Ok(())
    })
}

/// A partition gone past the deadline ends compute with code 69 naming its address, as the group does.
#[test]
fn test_a_partition_gone_past_the_deadline_ends_compute_naming_it() -> TestResult {
    run(async {
        let held = Held::start(SECRET, DEADLINE).await?;
        let partition = &held.partition;
        until("the partition dialled", || partition.dials() > 0).await?;
        partition.answer(false);
        until("a dial turned away", || partition.turned_away() > 0).await?;
        held.clock.set(DEADLINE);
        let lost = held.outcome().await?.err().context("a loss")?;
        assert_eq!(lost.exit_code(), RoleError::EX_UNAVAILABLE);
        let message = lost.to_string();
        assert!(message.contains(partition.address()), "{message}");
        assert!(!message.contains("system group"), "{message}");
        Ok(())
    })
}

/// A partition lost while the group is gone too, as both are when the process hosting them is killed, is reported as the
/// group lost: compute names the system group's address (system-group, "A group gone past the deadline ends its
/// dependants").
#[test]
fn test_a_partition_lost_with_the_group_is_reported_as_the_group() -> TestResult {
    run(async {
        let held = Held::start(SECRET, DEADLINE).await?;
        let (group, partition) = (&held.group, &held.partition);
        until("a Members asked", || group.members_asked() > 0).await?;
        partition.answer(false);
        until("a partition's dial turned away", || {
            partition.turned_away() > 0
        })
        .await?;
        group.answer(false);
        until("a group's dial turned away", || group.turned_away() > 0).await?;
        held.clock.set(DEADLINE);
        let lost = held.outcome().await?.err().context("a loss")?;
        let message = lost.to_string();
        assert!(message.contains("system group"), "{message}");
        assert!(message.contains(group.address()), "{message}");
        Ok(())
    })
}

/// What a [`StrayGroup`] sends unasked once it has answered a connection's Members.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stray {
    /// An Error on a stream compute has not opened.
    Ahead,
    /// An Error on the Members' stream, which compute has ended, its body failing its checksum when `corrupted`.
    Late { corrupted: bool },
}

/// A group listing compute in its Members, as one compute joined does, and sending `stray` after each Members it answers.
#[derive(Debug)]
struct StrayGroup {
    stray: Stray,
    members: Rc<Cell<u32>>,
    then: Option<(u64, Push)>,
}

/// An Error frame on `stream`, flagged last, its last byte changed when `corrupted`: its header holds, its body's checksum
/// fails.
fn stray_error(stream: u64, corrupted: bool) -> TestResult<Vec<u8>> {
    let mut frame = ErrorReply::new(ErrorCode::Refused, "a stray refusal").frame(
        Header::new(CLUSTER, Command::Error)
            .with_stream(stream)
            .with_flags(FLAG_LAST),
    )?;
    if corrupted && let Some(last) = frame.last_mut() {
        *last ^= 0xFF;
    }
    Ok(frame)
}

impl StrayGroup {
    /// The Members of `stream`, compute listed, and what follows it: a late frame on its stream, which it leaves open for that
    /// frame, or an Error on a stream compute has not opened.
    fn members(&mut self, stream: u64) -> TestResult<Vec<u8>> {
        self.members.set(self.members.get() + 1);
        let mut members = Members::new();
        members.push_compute(OWN);
        let header = Header::new(CLUSTER, Command::Members).with_stream(stream);
        let (header, then) = match self.stray {
            Stray::Ahead => (
                header.with_flags(FLAG_LAST),
                (
                    stream + 100,
                    Push::Refuse(ErrorReply::new(ErrorCode::Refused, "a stray refusal")),
                ),
            ),
            Stray::Late { corrupted } => (
                header,
                (stream, Push::Frame(stray_error(stream, corrupted)?)),
            ),
        };
        self.then = Some(then);
        Ok(members.frame(header)?)
    }
}

impl Session for StrayGroup {
    async fn answer(&mut self, stream: u64, frame: ReceivedFrame) -> Answer {
        if frame.command() != Command::Members.number() {
            return Answer::Refuse(ErrorReply::new(ErrorCode::Unavailable, "not played"));
        }
        self.members(stream).map_or_else(
            |error| Answer::Refuse(ErrorReply::new(ErrorCode::Unavailable, error.to_string())),
            Answer::Reply,
        )
    }

    async fn next_push(&mut self) -> (u64, Push) {
        match self.then.take() {
            Some(pushed) => pushed,
            None => std::future::pending().await,
        }
    }
}

/// Holds a [`StrayGroup`] sending `stray` and a partition, compute's deadline 60 s, so its next ask is 30 s away and the clock
/// stands at 0: the Members asked so far, and what stops holding.
async fn holding_stray(stray: Stray) -> TestResult<(Rc<Cell<u32>>, Stopping)> {
    let cluster = role_cluster(SECRET, Duration::from_secs(60))?;
    let listener = Listener::bind(&Address::any_port("127.0.0.1")?).await?;
    let group = listener.local_address()?;
    let server =
        Server::new(CLUSTER, "frontend", Endpoint::Frontend)?.with_iterations(ROLE_ITERATIONS);
    let credentials = cluster.secret().credentials()?;
    let members = Rc::new(Cell::new(0));
    let counted = Rc::clone(&members);
    spawn(async move {
        while let Ok(carrier) = listener.accept().await {
            let session = StrayGroup {
                stray,
                members: Rc::clone(&counted),
                then: None,
            };
            let (server, credentials) = (server.clone(), credentials.clone());
            spawn(async move { serve_connection(carrier, &server, credentials, session).await })
                .detach();
        }
    })
    .detach();
    let partition = PlayedFrontend::start(&cluster, even_ranges(1)?).await?;
    let partitions = vec![Address::parse(partition.address())?];
    let stopping = Stopping::default();
    let stops = stopping.clone();
    spawn(async move {
        let _held = hold_peers(
            &group,
            &Join::compute(OWN),
            partitions,
            &cluster,
            &stops,
            || Duration::ZERO,
        )
        .await;
        drop(partition);
    })
    .detach();
    Ok((members, stopping))
}

/// §2.5 step 3, §2.6, §13: a frame on a stream compute has not opened on the connection it holds the group by, or a late one
/// whose body fails its checksum, is refused, which closes the connection: compute asks the group again at once.
#[test]
fn test_a_frame_the_held_connection_refuses_closes_it() -> TestResult {
    run(async {
        for stray in [Stray::Ahead, Stray::Late { corrupted: true }] {
            let (members, stopping) = holding_stray(stray).await?;
            until(&format!("{stray:?}: the group asked again"), || {
                members.get() > 1
            })
            .await?;
            stopping.set();
        }
        Ok(())
    })
}

/// §2.5 step 3, §11.2: a whole frame on a stream compute has ended is late and dropped: the connection is kept, and the group
/// is not asked again before the half deadline.
#[test]
fn test_a_late_frame_keeps_the_held_connection() -> TestResult {
    run(async {
        let (members, stopping) = holding_stray(Stray::Late { corrupted: false }).await?;
        until("the group asked", || members.get() > 0).await?;
        compio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(members.get(), 1, "the connection is kept");
        stopping.set();
        Ok(())
    })
}
