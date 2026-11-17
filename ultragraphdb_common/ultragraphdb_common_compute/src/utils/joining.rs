/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compio::runtime::spawn;
use ultragraphdb_common_role::{RoleClient, RoleCluster, retry_until};
use ultragraphdb_common_service::{RoleKind, Stopping};
use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_error_role::{RoleError, RoleErrorEnum};
use ultragraphdb_protocol_transport::{Address, ReceivedFrame, TransportError};
use ultragraphdb_protocol_wire::{BodyView, Join, Members, MembersView, RoleKind as MemberKind};

/// How often a peer's task looks again: between the dials to a peer that did not answer, and at least between two asks
/// of one that did; and how often the holder looks at what its tasks found.
const POLL: Duration = Duration::from_millis(100);

/// How many times per `cluster.join_deadline_ms` compute asks each peer whether it still answers.
const ASKS_PER_DEADLINE: u32 = 2;

/// Joins the system group at `group` with `join` until it answers and its Members lists a partition that answers in turn,
/// retrying until the cluster's deadline (`PROTOCOL.md` §11.12, *Joining*): where the partitions listen, or `None` once
/// the service stops.
///
/// A future dropped at an await drops the connection it dialled; a Join the group took stays taken, and the next attempt
/// joins again.
///
/// # Errors
///
/// [`RoleError::unavailable`] naming `group`, the attempts made and the last refusal, past the deadline.
pub async fn join_group(
    group: &Address,
    join: &Join,
    cluster: &RoleCluster,
    stopping: &Stopping,
) -> Result<Option<Vec<Address>>, RoleError> {
    let name = RoleKind::Compute.to_string();
    retry_until(cluster, &name, stopping, || attempt(group, cluster, join))
        .await
        .map_err(|error| naming(group, error))
}

/// `error`, an attempt's past the deadline, naming the system group at `group` it was made at.
fn naming(group: &Address, error: RoleError) -> RoleError {
    let at =
        |message: String| format!("compute did not join the system group at {group}: {message}");
    match error.0 {
        RoleErrorEnum::Unavailable { message } => RoleError::unavailable(at(message)),
        RoleErrorEnum::Configuration { message } => RoleError::configuration(at(message)),
        RoleErrorEnum::Io { message } => RoleError::io(at(message)),
        RoleErrorEnum::Software { message } => RoleError::software(at(message)),
    }
}

/// One attempt: dial, authenticate, Join, read the Members until a partition has joined, and dial the first: where those
/// partitions listen.
///
/// A future dropped at an await drops the connection it dialled.
async fn attempt(
    group: &Address,
    cluster: &RoleCluster,
    join: &Join,
) -> Result<Vec<Address>, TransportError> {
    let name = RoleKind::Compute.to_string();
    let mut client =
        RoleClient::dial(group, cluster.id(), &name, cluster.secret().password()).await?;
    drop(client.request(join).await?);
    let answer = client.request(&Members::new()).await?;
    let view = answer.view().map_err(TransportError::protocol)?;
    let members = MembersView::from_frame(&view).map_err(TransportError::protocol)?;
    let partitions = (0..members.len())
        .filter_map(|index| members.member(index))
        .filter(|member| member.kind() == MemberKind::Storage)
        // A `u32` index into a `usize`, lossless on every target the node builds for.
        .filter_map(|member| members.address(member.address() as usize))
        .map(Address::parse)
        .collect::<Result<Vec<_>, _>>()?;
    let Some(first) = partitions.first() else {
        return Err(TransportError::authentication(
            "no partition has joined yet",
        ));
    };
    drop(RoleClient::dial(first, cluster.id(), &name, cluster.secret().password()).await?);
    Ok(partitions)
}

/// What the peers' tasks found, read by [`hold_peers`] on this core alone, each cell set by one task: shared by an `Rc`
/// with the tasks of this runtime (Ultra rule 9).
#[derive(Default)]
struct Found {
    /// The group lost past the deadline.
    group_lost: Cell<Option<RoleError>>,
    /// The group did not answer the dial under way: a partition lost meanwhile waits on the group's verdict.
    group_failing: Cell<bool>,
    /// A partition lost past the deadline.
    partition_lost: Cell<Option<RoleError>>,
}

/// Asks the system group at `group` and each of `partitions` whether they answer, until the service stops or one is gone
/// for the cluster's deadline, reading the time by `clock`, the time since an origin of its own. A role connection has no
/// idle limit and the protocol no ping (`PROTOCOL.md` §11.4, §11.12), so a dial is the question: a peer answers by its
/// Welcome and its proof of the secret. The group is asked its Members too, and a group that no longer lists `join`'s
/// address, as one restarted, is sent `join` again (system-group, "A lost system group is waited for until the deadline").
/// The connection the group was last asked on is held, and the group is asked again as soon as it closes, as storage
/// subscribes again: a restarted group is joined again as it comes back, not at the next ask.
/// A partition lost while the group does not answer either, as when the process hosting both is killed, is reported as
/// the group lost, once the group's own deadline passes.
///
/// A future dropped at its wait leaves the peers' tasks asking until the service stops, a peer lost then reported to no
/// one.
///
/// # Errors
///
/// [`RoleError::unavailable`] naming the peer's address, the deadline and the last refusal, for the first peer gone past
/// the deadline.
pub async fn hold_peers<C>(
    group: &Address,
    join: &Join,
    partitions: Vec<Address>,
    cluster: &RoleCluster,
    stopping: &Stopping,
    clock: C,
) -> Result<(), RoleError>
where
    C: Fn() -> Duration + Clone + 'static,
{
    let found: Rc<Found> = Rc::default();
    let asker = Asker {
        cluster: cluster.clone(),
        stopping: stopping.clone(),
        clock,
    };
    spawn(hold_group(
        asker.clone(),
        group.clone(),
        join.clone(),
        Rc::clone(&found),
    ))
    .detach();
    for partition in partitions {
        spawn(hold_partition(asker.clone(), partition, Rc::clone(&found))).detach();
    }
    // An event loop: the service's stop or a peer lost ends it (Ultra rule 2).
    while !stopping.is_stopping() {
        if let Some(lost) = found.group_lost.take() {
            return Err(lost);
        }
        if !found.group_failing.get()
            && let Some(lost) = found.partition_lost.take()
        {
            return Err(lost);
        }
        compio::time::sleep(POLL).await;
    }
    Ok(())
}

/// What every peer's task asks with.
#[derive(Debug, Clone)]
struct Asker<C> {
    cluster: RoleCluster,
    stopping: Stopping,
    clock: C,
}

/// Holds the group at `group` until the service stops or it is lost, which `found` is told.
/// Spawned detached: a future dropped at the runtime's end holds nothing, the dial's half-made connection dropped with it.
async fn hold_group<C>(asker: Asker<C>, group: Address, join: Join, found: Rc<Found>)
where
    C: Fn() -> Duration,
{
    let failing = |failing: bool| found.group_failing.set(failing);
    let ask = || ask_group(&group, &join, &asker.cluster);
    if let Err(lost) = hold(&asker, &group, "the system group", ask, closes, failing).await {
        found.group_lost.set(Some(lost));
    }
}

/// Holds the partition at `partition` until the service stops or it is lost, which `found` is told.
/// Spawned detached: a future dropped at the runtime's end holds nothing, the dial's half-made connection dropped with it.
async fn hold_partition<C>(asker: Asker<C>, partition: Address, found: Rc<Found>)
where
    C: Fn() -> Duration,
{
    let ask = || dial(&partition, &asker.cluster);
    // A partition's ask holds no connection, so only the half deadline asks it again.
    let never = |()| std::future::pending::<()>();
    if let Err(lost) = hold(&asker, &partition, "the role", ask, never, |_| ()).await {
        found.partition_lost.set(Some(lost));
    }
}

/// Asks `peer`, named `what`, by `ask` every half deadline for as long as it answers, and at once when what the last ask
/// holds is `closed`; `failing` is told whether the ask under way has met a refusal.
///
/// A future dropped at an ask or a wait holds nothing: the ask's half-made connection, or the one it held, is dropped with
/// it.
async fn hold<C, A, F, H, W>(
    asker: &Asker<C>,
    peer: &Address,
    what: &str,
    ask: A,
    closed: impl Fn(H) -> W,
    failing: impl Fn(bool),
) -> Result<(), RoleError>
where
    C: Fn() -> Duration,
    A: Fn() -> F,
    F: Future<Output = Result<H, TransportError>>,
    W: Future<Output = ()>,
{
    let every = (asker.cluster.deadline() / ASKS_PER_DEADLINE).max(POLL);
    // An event loop: the service's stop or the peer lost ends it (Ultra rule 2).
    while let Some(held) = answers(asker, peer, what, &ask, &failing).await? {
        let wait = std::pin::pin!(compio::time::sleep(every));
        let closing = std::pin::pin!(closed(held));
        // Whichever comes first; the other is dropped, and with it what the last ask held.
        let _first = futures::future::select(wait, closing).await;
    }
    Ok(())
}

/// Ends once the group closes `client`, the connection it was last asked on, every request of which was answered, or once
/// compute refuses a frame the group sends unasked, which closes the connection (`PROTOCOL.md` §2.6, §13): a frame on a stream
/// compute has ended is late, checked whole and dropped; one on stream 0, the group's connection error, or on a stream compute
/// has not opened answers no request (§2.5, step 3).
///
/// A dropped future drops the connection.
async fn closes(mut client: RoleClient) {
    let next = client.last_stream().saturating_add(1);
    // An event loop: the connection's end or a frame refused ends it (Ultra rule 2).
    while let Ok(frame) = client.next_frame().await {
        if let Err(rule) = late(&frame, next) {
            tracing::warn!(%rule, "the system group sent a frame compute refuses; closing the connection");
            return;
        }
    }
}

/// `frame`, sent unasked on a connection whose next stream is `next`, checked as a late frame.
///
/// # Errors
///
/// The rule it breaks: a frame-level rule, or a stream of 0 or of `next` or above, which answers no request.
fn late(frame: &ReceivedFrame, next: u64) -> Result<(), ProtocolError> {
    let view = frame.view()?;
    let stream = view.header().stream();
    if stream == 0 || stream >= next {
        return Err(ProtocolError::command_not_served(view.command().number()));
    }
    Ok(())
}

/// Asks `peer` by `ask` until it answers, each ask limited to the time left before the deadline, past which it is lost:
/// what the ask holds, or `None` once the service stops.
///
/// A future dropped at an ask or a wait holds nothing: the ask's half-made connection is dropped with it.
async fn answers<C, A, F, H>(
    asker: &Asker<C>,
    peer: &Address,
    what: &str,
    ask: &A,
    failing: &impl Fn(bool),
) -> Result<Option<H>, RoleError>
where
    C: Fn() -> Duration,
    A: Fn() -> F,
    F: Future<Output = Result<H, TransportError>>,
{
    let deadline = asker.cluster.deadline();
    let due = (asker.clock)().saturating_add(deadline);
    // Each turn is one ask, limited to the time left; the deadline or the service's stop ends it (Ultra rule 2).
    while !asker.stopping.is_stopping() {
        let left = due.saturating_sub((asker.clock)()).max(POLL);
        let why = match compio::time::timeout(left, ask()).await {
            Ok(Ok(held)) => {
                failing(false);
                return Ok(Some(held));
            }
            Ok(Err(error)) => error.to_string(),
            Err(_) => format!("an ask had no answer within {} ms", left.as_millis()),
        };
        failing(true);
        if (asker.clock)() >= due {
            return Err(RoleError::unavailable(format!(
                "compute lost {what} at {peer}, which did not return within {} ms: {why}. Start that role again, then compute",
                deadline.as_millis()
            )));
        }
        tracing::warn!(%peer, %why, "a role did not answer; asking it again");
        compio::time::sleep(POLL).await;
    }
    Ok(None)
}

/// Whether `peer` answers a dial: its Welcome and its proof of the secret.
///
/// A dropped future drops the half-made connection.
async fn dial(peer: &Address, cluster: &RoleCluster) -> Result<(), TransportError> {
    let name = RoleKind::Compute.to_string();
    drop(RoleClient::dial(peer, cluster.id(), &name, cluster.secret().password()).await?);
    Ok(())
}

/// Whether the group at `group` answers its Members, sending it `join` again when they do not list `join`'s address, as a
/// group restarted holds no Join from before: the connection it was asked on.
///
/// A dropped future drops its connection; a Join the group took stays taken.
async fn ask_group(
    group: &Address,
    join: &Join,
    cluster: &RoleCluster,
) -> Result<RoleClient, TransportError> {
    let name = RoleKind::Compute.to_string();
    let mut client =
        RoleClient::dial(group, cluster.id(), &name, cluster.secret().password()).await?;
    let listed = {
        let answer = client.request(&Members::new()).await?;
        let view = answer.view().map_err(TransportError::protocol)?;
        let members = MembersView::from_frame(&view).map_err(TransportError::protocol)?;
        (0..members.len())
            .filter_map(|index| members.member(index))
            .filter(|member| member.kind() == MemberKind::Compute)
            // A `u32` index into a `usize`, lossless on every target the node builds for.
            .filter_map(|member| members.address(member.address() as usize))
            .any(|address| address == join.address())
    };
    if !listed {
        drop(client.request(join).await?);
    }
    Ok(client)
}
