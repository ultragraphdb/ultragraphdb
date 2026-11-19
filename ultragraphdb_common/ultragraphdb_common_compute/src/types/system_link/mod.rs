/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod system_link_asks;
mod system_link_reads;
mod system_link_tasks;

use std::cell::{Ref, RefCell};
use std::collections::{HashMap, VecDeque};
use std::future::poll_fn;
use std::rc::Rc;
use std::task::{Poll, Waker};
use std::time::Instant;

use futures::channel::oneshot;
use ultragraphdb_common_role::RoleCluster;
use ultragraphdb_common_service::Readiness;
use ultragraphdb_protocol_transport::Address;
use ultragraphdb_protocol_wire::{
    BodyBuilder, CheckKind, CheckOutcome, Command, ErrorCode, ErrorReply, Header,
    MAX_FRAME_FLOOR_U32, SessionCheck,
};

use crate::types::check_round_trips::CheckRoundTrips;
use crate::types::client_session::now;
use crate::types::endpoint_view::EndpointView;
use crate::types::hold_outcome::HoldOutcome;
use crate::types::role_links::RoleLinks;
use crate::types::system_turns::SystemTurns;
use crate::utils::recall_order::check_refusal;

/// An ask on its way: the command it asks by, the frames of its answer read so far, and where the answer goes.
#[derive(Debug)]
struct Ask {
    command: Command,
    frames: Vec<Vec<u8>>,
    reply: oneshot::Sender<Result<Vec<Vec<u8>>, ErrorReply>>,
}

/// One frame of a close continuing past one: its epoch, its placement version, and the `(tenant, version)` it announces.
type ClosePart = (u64, u64, Vec<(u64, u64)>);

/// What a link holds, shared by its handles and its tasks on the core's one thread. Every borrow lasts one call, never
/// across a wait (Ultra rule 9).
#[derive(Debug)]
struct LinkState {
    cluster: u64,
    view: EndpointView,
    turns: SystemTurns,
    /// The outcome each check queued or sent waits for, by its ticket: one for each request waiting on its check, so
    /// the core's connections and their streams bound it (Ultra rule 2).
    waiting: HashMap<u64, oneshot::Sender<Result<CheckOutcome, ErrorReply>>>,
    /// The Check frames on their way, by stream: their checks in frame order, and when each left. A turn sends one, so
    /// the turns run between a frame and its reply bound it.
    checks_out: HashMap<u64, (Vec<(u64, SessionCheck)>, Instant)>,
    /// The asks on their way, by stream: one for each request or task of the core waiting on its answer, so the core's
    /// connections and their streams bound it.
    asks: HashMap<u64, Ask>,
    /// The subscription's stream on the open connection, and the frames of a close continuing past one.
    subscription: Option<u64>,
    close_parts: Vec<ClosePart>,
    /// Whether a connection is open, the frames waiting to leave on it, and its sending task's waker.
    open: bool,
    /// The number of the connection open or last open, counted from 1, and whether an answer read on it was refused, which
    /// closes it (`PROTOCOL.md` §2.6).
    connection: u64,
    refused: bool,
    /// A frame for each ask on its way, the Subscribe, a CloseRequest for each Begin that asked for one, a Credit for
    /// each Catalog or Defaults frame read and a turn's two: each is queued for a request of the core's connections or
    /// for a frame read, and the sending task takes them all at each run, so those connections' bounds bound it.
    outgoing: VecDeque<Vec<u8>>,
    sender: Option<Waker>,
    next_stream: u64,
    max_frame: u32,
    checks_max: usize,
    ended_max: usize,
    lost_since: Option<Instant>,
    /// Bumped by every change the view's waiters wait for, which wakes them.
    changes: u64,
    /// The waiters' wakers, taken whole at each change: a waiter adds its own when it is polled with no change and the link
    /// holds none that wakes the same task, so it holds one for each waiter.
    watchers: Vec<Waker>,
    /// The Begins a token holds, decided and not yet taken by their waiter: one for each [`SystemLink::token`] waiting, since
    /// a token dropped takes its own away.
    decided: HashMap<u64, HoldOutcome>,
    next_hold: u64,
    /// The placement version the latest close assigned, which the next Placement read holds.
    placement_asked: u64,
    refreshing: bool,
    /// Whether the link was stopped, which dials no more.
    stopped: bool,
    round_trips: CheckRoundTrips,
    /// The core's links, which Members is read into.
    links: RoleLinks,
    /// The role's readiness, withdrawn when the connection closes and set again by the Members task (design D10).
    readiness: Readiness,
}

impl LinkState {
    /// A stream for the next request on the open connection.
    fn stream(&mut self) -> u64 {
        let stream = self.next_stream;
        debug_assert!(stream != 0, "stream 0 is the connection's own");
        self.next_stream = stream.saturating_add(1);
        stream
    }

    /// `frame` waits to leave; the waker of the sending task, for after the borrow.
    fn queue(&mut self, frame: Vec<u8>) -> Option<Waker> {
        self.outgoing.push_back(frame);
        self.sender.take()
    }

    /// The waiter polled with `waker` is told of the next change: its waker is kept once, however often it is polled before
    /// that change (Ultra rule 2).
    fn watch(&mut self, waker: &Waker) {
        if !self.watchers.iter().any(|held| held.will_wake(waker)) {
            self.watchers.push(waker.clone());
        }
    }

    /// The view's waiters are told something changed; their wakers, for after the borrow.
    fn changed(&mut self) -> Vec<Waker> {
        self.changes = self.changes.wrapping_add(1);
        std::mem::take(&mut self.watchers)
    }
}

/// One endpoint core's role connection to the system group at `cluster.system_address` (design D5): dialled as compute,
/// dialled again when it closes, subscribed with `latest` on each connection, its closes kept in the core's
/// [`EndpointView`]. The core's requests ask on it: Session, Challenge, Verify, Declare, Defaults, Catalog, Placement,
/// Members and CloseRequest each on a stream of its own, and the Checks and Endeds of a turn ([`crate::SystemTurns`]),
/// one run of its sending task each, which waits for no reply (`PROTOCOL.md` §11.5, *Turns*). Each Check frame's round
/// trip is counted in its [`CheckRoundTrips`]. While it is closed, what needs the system group is refused with code 17,
/// and the Endeds of requests that end meanwhile leave on the next connection (`PROTOCOL.md` §11.12, *While the system
/// group cannot be reached*). Shared by an `Rc`, on its one core.
///
/// Cancellation: a check or an ask whose future is dropped before its answer stays with the link until the answer
/// arrives or the connection closes, and the answer then finds no one to tell; a check that marked its session in
/// flight leaves it so, as [`crate::ClientSession`] says. A [`SystemLink::token`] dropped takes its hold from the view and
/// any outcome decided for it from the link. [`SystemLink::answering`] dropped leaves its waker, which the next change
/// wakes for nothing.
///
/// Data plane (Ultra rule 3) for the Checks and Endeds of a commit's path, each frame the `Vec` the wire's builders return
/// and each check's answer a channel of its own; control plane for the rest.
#[derive(Debug, Clone)]
pub struct SystemLink {
    state: Rc<RefCell<LinkState>>,
    round_trips: CheckRoundTrips,
}

impl SystemLink {
    /// The link to the system group at `address`, dialled on a task of this core as compute of `cluster` within
    /// `dial_ms` milliseconds, reading Members into `links` (design D9) and setting `readiness` once the view holds a close
    /// and a placement, Members lists every partition of it and a partition answers (design D10); readiness is withdrawn
    /// while its connection is closed, and set again once all of that holds again.
    #[must_use]
    pub fn dial(
        address: Address,
        cluster: RoleCluster,
        links: RoleLinks,
        dial_ms: u32,
        readiness: Readiness,
    ) -> Self {
        let round_trips = CheckRoundTrips::new(now());
        let state = LinkState {
            cluster: cluster.id(),
            view: EndpointView::new(),
            turns: SystemTurns::new(),
            waiting: HashMap::new(),
            checks_out: HashMap::new(),
            asks: HashMap::new(),
            subscription: None,
            close_parts: Vec::new(),
            open: false,
            connection: 0,
            refused: false,
            outgoing: VecDeque::new(),
            sender: None,
            next_stream: 1,
            max_frame: MAX_FRAME_FLOOR_U32,
            checks_max: SystemTurns::checks_max(MAX_FRAME_FLOOR_U32),
            ended_max: SystemTurns::ended_max(MAX_FRAME_FLOOR_U32),
            lost_since: Some(now()),
            changes: 0,
            watchers: Vec::new(),
            decided: HashMap::new(),
            next_hold: 0,
            placement_asked: 0,
            refreshing: false,
            stopped: false,
            round_trips: round_trips.clone(),
            links: links.clone(),
            readiness: readiness.clone(),
        };
        let link = Self {
            state: Rc::new(RefCell::new(state)),
            round_trips,
        };
        system_link_tasks::spawn_connection(link.clone(), address, cluster, dial_ms);
        system_link_reads::spawn_members(link.clone(), links, readiness);
        link
    }

    /// The wakers the link holds for the waiters of its view, its own tasks' among them: one for each waiter at most, however
    /// often a waiter is polled between two changes (Ultra rule 2).
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn watchers_held(&self) -> usize {
        self.state.borrow().watchers.len()
    }

    /// The outcomes of [`SystemLink::token`]'s holds the link has decided and no waiter has taken: one for each token still
    /// waiting at most, a token dropped leaving none.
    #[cfg(feature = "test-utils")]
    #[must_use]
    pub fn outcomes_held(&self) -> usize {
        self.state.borrow().decided.len()
    }

    /// The link counting its Check frames' round trips as endpoint core `core`'s.
    #[must_use]
    pub fn with_core(mut self, core: usize) -> Self {
        self.round_trips = self.round_trips.with_core(core);
        self
    }

    /// Whether its connection is open and subscribed.
    #[must_use]
    pub fn is_up(&self) -> bool {
        self.state.borrow().open
    }

    /// The largest frame the system group takes, by its Welcome once a connection opened: what one Declare frame may hold
    /// (`PROTOCOL.md` §11.13).
    #[must_use]
    pub fn max_frame(&self) -> u32 {
        self.state.borrow().max_frame
    }

    /// Stops the link: its connection closes, and it is dialled no more. What waits on it is refused with code 17.
    pub fn stop(&self) {
        let sender = {
            let mut state = self.state.borrow_mut();
            state.stopped = true;
            state.open = false;
            state.sender.take()
        };
        if let Some(sender) = sender {
            sender.wake();
        }
        self.notify();
    }

    /// Since when its connection has been closed, `None` while it is open.
    #[must_use]
    pub fn lost_since(&self) -> Option<Instant> {
        self.state.borrow().lost_since
    }

    /// The core's view.
    #[must_use]
    pub fn view(&self) -> Ref<'_, EndpointView> {
        Ref::map(self.state.borrow(), |state| &state.view)
    }

    /// The round trips of its Check frames.
    #[must_use]
    pub fn round_trips(&self) -> &CheckRoundTrips {
        &self.round_trips
    }

    /// Waits until the view answers ([`EndpointView::answers`]), at most until `deadline`.
    ///
    /// # Errors
    ///
    /// Code 17 when the view does not answer by `deadline`.
    pub async fn answering(&self, deadline: Instant) -> Result<(), ErrorReply> {
        // Each pass waits for a change or the deadline, which ends it (Ultra rule 2).
        loop {
            let seen = {
                let state = self.state.borrow();
                if state.view.answers() {
                    return Ok(());
                }
                state.changes
            };
            if now() >= deadline {
                return Err(unavailable("the system group's view is not yet held"));
            }
            self.changed_or(seen, deadline).await;
        }
    }

    /// The check of `session` for `principal`, of `kind`, sent in the next turn and passed (`PROTOCOL.md` §13.24).
    ///
    /// # Errors
    ///
    /// Code 12, 21 (`detail` 2) or 14 as its outcome says; code 17 when the link is closed or closes before the answer.
    pub async fn check(
        &self,
        session: [u8; 16],
        principal: u64,
        kind: CheckKind,
    ) -> Result<(), ErrorReply> {
        let (answer, sender) = {
            let mut state = self.state.borrow_mut();
            if !state.open {
                return Err(unavailable("the connection to the system group is closed"));
            }
            let ticket = state.turns.queue_check(session, principal, kind);
            let (reply, answer) = oneshot::channel();
            let _none = state.waiting.insert(ticket, reply);
            (answer, state.sender.take())
        };
        if let Some(sender) = sender {
            sender.wake();
        }
        let outcome = answer.await.map_err(|_| {
            unavailable("the connection to the system group closed under the check")
        })??;
        check_refusal(outcome)
    }

    /// The request of `session` in flight ended: its Ended leaves in the next turn, on the next connection if this one is
    /// closed.
    pub fn ended(&self, session: [u8; 16]) {
        let sender = {
            let mut state = self.state.borrow_mut();
            state.turns.queue_ended(session);
            state.sender.take()
        };
        if let Some(sender) = sender {
            sender.wake();
        }
    }

    /// Asks the sequencer for an early close of `epoch` by a CloseRequest with `partition` 0, which nothing answers but the
    /// close (`PROTOCOL.md` §13.16); a closed link asks nothing.
    pub fn close_request(&self, epoch: u64) {
        let body = ultragraphdb_protocol_wire::CloseRequest::compute(epoch);
        let sender = {
            let mut state = self.state.borrow_mut();
            if !state.open {
                return;
            }
            let stream = state.stream();
            let header = Header::new(state.cluster, Command::CloseRequest).with_stream(stream);
            match body.frame(header) {
                Ok(frame) => state.queue(frame),
                // A frame that cannot be built asks nothing, as a closed link does: the close comes at its own time.
                Err(_) => None,
            }
        };
        if let Some(sender) = sender {
            sender.wake();
        }
    }

    /// Waits for the close before `token_epoch`, a Begin's token ahead of the view, until `deadline` (`PROTOCOL.md` §4.8).
    ///
    /// # Errors
    ///
    /// Code 24, `detail` 0, when the deadline is reached with that close not arrived.
    pub async fn token(&self, token_epoch: u64, deadline: Instant) -> Result<(), ErrorReply> {
        let ticket = {
            let mut state = self.state.borrow_mut();
            let ticket = state.next_hold;
            state.next_hold = ticket.wrapping_add(1);
            if state.view.hold(ticket, token_epoch, deadline).is_some() {
                return Ok(());
            }
            ticket
        };
        // Dropped, as when the request's future is, the token takes its hold and any outcome decided for it away.
        let _waiting = Waiting { link: self, ticket };
        // Each pass waits for a close or the deadline, by which the view decides the hold (Ultra rule 2).
        loop {
            let seen = {
                let mut state = self.state.borrow_mut();
                let released = state.view.released(now());
                state.decided.extend(released);
                if let Some(outcome) = state.decided.remove(&ticket) {
                    return match outcome {
                        HoldOutcome::Answered => Ok(()),
                        HoldOutcome::TimedOut => Err(ErrorReply::new(
                            ErrorCode::TimedOut,
                            format!(
                                "the close before the token's epoch {token_epoch} did not arrive by the deadline"
                            ),
                        )),
                    };
                }
                state.changes
            };
            self.changed_or(seen, deadline).await;
        }
    }

    /// Waits until a change past `seen`, or `deadline`.
    async fn changed_or(&self, seen: u64, deadline: Instant) {
        let changed = poll_fn(|context| {
            let mut state = self.state.borrow_mut();
            if state.changes != seen {
                return Poll::Ready(());
            }
            state.watch(context.waker());
            Poll::Pending
        });
        let wait = deadline.saturating_duration_since(now());
        // A change and the deadline both end the wait: the caller reads the view and the clock again.
        let _waited: Result<(), _> = compio::time::timeout(wait, changed).await;
    }

    /// Wakes the view's waiters.
    fn notify(&self) {
        let watchers = self.state.borrow_mut().changed();
        for watcher in watchers {
            watcher.wake();
        }
    }
}

/// A [`SystemLink::token`] waiting: dropped, it lets go of its hold and of the outcome another waiter's pass decided for it.
struct Waiting<'a> {
    link: &'a SystemLink,
    ticket: u64,
}

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        let mut state = self.link.state.borrow_mut();
        state.view.unhold(self.ticket);
        let _untaken: Option<HoldOutcome> = state.decided.remove(&self.ticket);
    }
}

/// Code 17, `message` saying why (`PROTOCOL.md` §11.12, *While the system group cannot be reached*).
pub(crate) fn unavailable(message: &str) -> ErrorReply {
    ErrorReply::new(ErrorCode::Unavailable, message)
}
