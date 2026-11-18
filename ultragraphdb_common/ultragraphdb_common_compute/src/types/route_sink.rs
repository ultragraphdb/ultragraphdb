/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::rc::Rc;

use futures::channel::mpsc::UnboundedSender;

use crate::types::commit_hold::CommitHold;
use crate::types::link_event::LinkEvent;
use crate::types::route_key::RouteKey;
use crate::types::session_event::SessionEvent;

/// Where a stream's frames go: its session's events, under the name the session knows it by, the commit its stream holds
/// in flight once that session is gone, and for a read's stream the credit it holds.
#[derive(Debug, Clone)]
pub struct RouteSink {
    events: UnboundedSender<SessionEvent>,
    key: RouteKey,
    /// Shared by an `Rc` among the sink's clones on its core's one thread, so the hold ends with the last of them.
    hold: Option<Rc<CommitHold>>,
    credit: Option<u32>,
}

impl RouteSink {
    /// The sink of `key` in the session reading `events`.
    pub fn new(events: UnboundedSender<SessionEvent>, key: RouteKey) -> Self {
        Self {
            events,
            key,
            hold: None,
            credit: None,
        }
    }

    /// The sink of a read's stream, which starts with `credit`, as its ComputeRead names (`PROTOCOL.md` §11.3).
    #[must_use]
    pub fn with_credit(mut self, credit: u32) -> Self {
        self.credit = Some(credit);
        self
    }

    /// Spends one frame of the stream's credit; `false` when it holds none. A stream with no credit counted spends none.
    pub(crate) fn spend_credit(&mut self) -> bool {
        let Some(credit) = self.credit.as_mut() else {
            return true;
        };
        let Some(left) = credit.checked_sub(1) else {
            return false;
        };
        *credit = left;
        true
    }

    /// Grants the stream credit for `frames` more frames, saturating as the sender's does.
    pub(crate) fn grant_credit(&mut self, frames: u32) {
        if let Some(credit) = self.credit.as_mut() {
            *credit = credit.saturating_add(frames);
        }
    }

    /// Tells the session `event`; a session gone has no one to tell.
    pub(crate) fn tell(&self, event: LinkEvent) {
        drop(self.events.unbounded_send(SessionEvent::Role {
            key: self.key,
            event,
        }));
    }

    /// Keeps `hold` with the stream until this sink is let go, by the stream's last frame or the connection's close.
    pub(crate) fn hold(&mut self, hold: CommitHold) {
        self.hold = Some(Rc::new(hold));
    }
}
