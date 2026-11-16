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
use ultragraphdb_common_compute::{group_address, hold_peers, join_group};
use ultragraphdb_common_service::{RoleIdentity, RoleStart, Stopping};
use ultragraphdb_error_role::RoleError;
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::Join;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::played::played_cluster::{SECRET, role_cluster, run};
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

/// C6.4: with `cluster.system_address` named, compute dials it, whatever `cluster.frontend_address` names beside it.
#[test]
fn test_the_group_is_the_system_address_where_it_is_named() -> TestResult {
    let alone = start("system_address = \"tcp://storage-0:7100\"\n")?;
    assert_eq!(group_address(&alone)?.to_string(), "tcp://storage-0:7100");
    let both = start(
        "frontend_address = \"tcp://frontend-0:7081\"\nsystem_address = \"unix:///run/ultragraph/system.sock\"\n",
    )?;
    assert_eq!(
        group_address(&both)?.to_string(),
        "unix:///run/ultragraph/system.sock"
    );
    Ok(())
}

/// Settings naming `cluster.frontend_address` alone, as every profile does until the switch (design D2), keep compute
/// dialling it.
#[test]
fn test_the_frontend_address_alone_is_dialled_as_before() -> TestResult {
    let old = start("frontend_address = \"tcp://frontend-0:7081\"\n")?;
    assert_eq!(group_address(&old)?.to_string(), "tcp://frontend-0:7081");
    Ok(())
}

/// Neither address is a configuration error naming `cluster.system_address`, and a `cluster.system_address` that is no
/// address is one naming it, though `cluster.frontend_address` is named beside it.
#[test]
fn test_no_group_address_is_refused_naming_the_system_address() -> TestResult {
    let refusals = [
        start("")?,
        start(
            "frontend_address = \"tcp://frontend-0:7081\"\nsystem_address = \"not an address\"\n",
        )?,
    ];
    for settings in refusals {
        let refused = group_address(&settings).err().context("a refusal")?;
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
    /// whose roles hold `group_secret`.
    async fn start(group_secret: &str) -> TestResult<Self> {
        let cluster = role_cluster(SECRET, DEADLINE)?;
        let group =
            PlayedFrontend::start(&role_cluster(group_secret, DEADLINE)?, even_ranges(1)?).await?;
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
        let held = Held::start(SECRET).await?;
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

/// C6.3: a group gone is waited for until the deadline, not a tick less, and then ends compute with code 69 naming the group
/// and its address.
#[test]
fn test_a_group_gone_past_the_deadline_ends_compute_naming_its_address() -> TestResult {
    run(async {
        let held = Held::start(SECRET).await?;
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
        let held = Held::start("another secret").await?;
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
        let held = Held::start(SECRET).await?;
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
        let held = Held::start(SECRET).await?;
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
