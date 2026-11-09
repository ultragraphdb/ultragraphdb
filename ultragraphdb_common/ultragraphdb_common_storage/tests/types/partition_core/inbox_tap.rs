/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A tap between a core's sessions and its inbox, for the tests of what a role connection hands its core while the core takes
//! nothing, as while its turn waits on a log sync: the tap counts each request a session hands on and each close it reports, and
//! hands every message on in its order. The tests serve the sessions, the tap and the core on one thread, so its counts are `Rc`'s.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use compio::runtime::spawn;
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use ultragraphdb_common_storage::CoreMessage;
use ultragraphdb_tool_bdd::{TestError, TestResult};

/// How long a wait for a count lasts.
const DUE: Duration = Duration::from_secs(10);
/// How often a wait looks at the count.
const LOOK: Duration = Duration::from_millis(1);

/// What the tap has counted.
#[derive(Debug, Default)]
struct Counts {
    requests: Cell<u64>,
    closed: Cell<u64>,
}

/// The test's side of the tap.
#[derive(Debug, Clone)]
pub(crate) struct Tap {
    counts: Rc<Counts>,
}

impl Tap {
    /// The requests the sessions have handed on so far.
    pub(crate) fn requests(&self) -> u64 {
        self.counts.requests.get()
    }

    /// The connections whose sessions have said they closed so far.
    pub(crate) fn closed(&self) -> u64 {
        self.counts.closed.get()
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
    let counts = Rc::new(Counts::default());
    let tap = Tap {
        counts: Rc::clone(&counts),
    };
    spawn(async move {
        while let Some(message) = handed.next().await {
            if matches!(message, CoreMessage::Request { .. }) {
                counts.requests.set(counts.requests.get() + 1);
            }
            if matches!(message, CoreMessage::Closed { .. }) {
                counts.closed.set(counts.closed.get() + 1);
            }
            if inbox.unbounded_send(message).is_err() {
                // The core has stopped: nothing more reaches it.
                break;
            }
        }
    })
    .detach();
    (sessions, tap)
}
