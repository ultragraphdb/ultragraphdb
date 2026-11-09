/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A tap between a core's sessions and its inbox, for the tests of what a role connection hands its core while the core takes
//! nothing, as while its turn waits on a log sync. The tap counts each request a session hands on and each close it reports;
//! while held it keeps every message, in order, and released it hands them all to the core at once and lets the rest through.
//! The tests serve the sessions, the tap and the core on one thread, so its state is `Rc`'s, and a release runs to its end with
//! nothing between.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::Duration;

use compio::runtime::spawn;
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use ultragraphdb_common_storage::CoreMessage;
use ultragraphdb_tool_bdd::{TestError, TestResult};

/// How long [`Tap::until_requests`] waits for its count.
const DUE: Duration = Duration::from_secs(10);
/// How often [`Tap::until_requests`] looks at the count.
const LOOK: Duration = Duration::from_millis(1);

/// The state the tap's task and the test share.
#[derive(Debug, Default)]
struct Shared {
    held: Cell<bool>,
    requests: Cell<u64>,
    closed: Cell<u64>,
    kept: RefCell<VecDeque<CoreMessage>>,
}

/// The test's side of the tap.
#[derive(Debug, Clone)]
pub(crate) struct Tap {
    shared: Rc<Shared>,
    inbox: UnboundedSender<CoreMessage>,
}

impl Tap {
    /// Keeps every message from now on, until [`Tap::release`].
    pub(crate) fn hold(&self) {
        self.shared.held.set(true);
    }

    /// Hands the core every message kept, in order, and lets every later one through.
    pub(crate) fn release(&self) -> TestResult {
        let kept: Vec<CoreMessage> = self.shared.kept.borrow_mut().drain(..).collect();
        for message in kept {
            self.inbox.unbounded_send(message)?;
        }
        self.shared.held.set(false);
        Ok(())
    }

    /// The requests the sessions have handed on so far.
    pub(crate) fn requests(&self) -> u64 {
        self.shared.requests.get()
    }

    /// The connections whose sessions have said they closed so far.
    pub(crate) fn closed(&self) -> u64 {
        self.shared.closed.get()
    }

    /// Waits until the sessions have handed on `count` requests, within [`DUE`].
    pub(crate) async fn until_requests(&self, count: u64) -> TestResult {
        until(count, "requests", || self.requests()).await
    }

    /// Waits until `count` connections' sessions have said they closed, within [`DUE`].
    pub(crate) async fn until_closed(&self, count: u64) -> TestResult {
        until(count, "connection closes", || self.closed()).await
    }
}

/// Waits until `counted` reaches `count`, within [`DUE`], naming `what` it counts should it not.
async fn until(count: u64, what: &str, counted: impl Fn() -> u64) -> TestResult {
    let mut waited = Duration::ZERO;
    while counted() < count {
        if waited >= DUE {
            return Err(TestError::new(format!(
                "{} of {count} {what} reached the core's inbox within {DUE:?}",
                counted()
            )));
        }
        compio::time::sleep(LOOK).await;
        waited += LOOK;
    }
    Ok(())
}

/// A tap in front of `inbox`: the sender the sessions hand their messages to, and the test's side.
pub(crate) fn tap(inbox: UnboundedSender<CoreMessage>) -> (UnboundedSender<CoreMessage>, Tap) {
    let (sessions, mut handed) = unbounded::<CoreMessage>();
    let shared = Rc::new(Shared::default());
    let tap = Tap {
        shared: Rc::clone(&shared),
        inbox: inbox.clone(),
    };
    spawn(async move {
        while let Some(message) = handed.next().await {
            if matches!(message, CoreMessage::Request { .. }) {
                shared.requests.set(shared.requests.get() + 1);
            }
            if matches!(message, CoreMessage::Closed { .. }) {
                shared.closed.set(shared.closed.get() + 1);
            }
            if shared.held.get() {
                shared.kept.borrow_mut().push_back(message);
            } else if inbox.unbounded_send(message).is_err() {
                // The core has stopped: nothing more reaches it.
                break;
            }
        }
    })
    .detach();
    (sessions, tap)
}
