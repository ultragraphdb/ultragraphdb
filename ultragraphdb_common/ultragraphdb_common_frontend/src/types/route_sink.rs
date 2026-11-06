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

/// Where a stream's frames go: its session's events, under the name the session knows it by, and the commit its stream holds
/// in flight once that session is gone.
#[derive(Debug, Clone)]
pub struct RouteSink {
    events: UnboundedSender<SessionEvent>,
    key: RouteKey,
    hold: Option<Rc<CommitHold>>,
}

impl RouteSink {
    /// The sink of `key` in the session reading `events`.
    pub fn new(events: UnboundedSender<SessionEvent>, key: RouteKey) -> Self {
        Self {
            events,
            key,
            hold: None,
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
