/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::future::Future;
use std::net::IpAddr;
use std::time::Instant;

use futures::StreamExt;
use futures::channel::mpsc;

use crate::types::auth_throttle::AuthThrottle;
use crate::types::login_asked::LoginAsked;
use crate::types::throttle_handle::ThrottleHandle;

impl AuthThrottle {
    /// The throttle served by message (design D15): the handle every endpoint core asks it through, and its run, which the
    /// first endpoint core spawns and which ends once every handle is gone.
    ///
    /// A dropped run drops the throttle: what it counted is forgotten, and every ask is answered as the throttle stopped.
    pub fn serve(self) -> (ThrottleHandle, impl Future<Output = ()>) {
        // Bounded as `ThrottleHandle` says: each connection waits for its answer before it asks again.
        let (inbox, received) = mpsc::unbounded();
        (ThrottleHandle::new(inbox), self.run(received))
    }

    async fn run(mut self, mut inbox: mpsc::UnboundedReceiver<LoginAsked>) {
        // An event loop: it ends once every handle is gone, as compute stops (Ultra rule 2).
        while let Some(asked) = inbox.next().await {
            self.take(asked);
        }
    }

    /// Answers a login's ask from the failed authentications every core counts here, or counts what it tells.
    /// An asker gone has no one to tell, so every send's failure is dropped.
    fn take(&mut self, asked: LoginAsked) {
        match asked {
            LoginAsked::Delay { source, reply } => drop(reply.send(self.delay(source))),
            LoginAsked::Admit { source, at, reply } => {
                let _unheard: Result<(), bool> = reply.send(self.admits(source, at));
            }
            LoginAsked::Outcome {
                source,
                at,
                succeeded,
            } => self.outcome(source, at, succeeded),
        }
    }

    /// Counts an authentication's outcome from `source`, logging the ban a failure begins.
    fn outcome(&mut self, source: IpAddr, at: Instant, succeeded: bool) {
        if succeeded {
            return self.succeeded(source);
        }
        if self.failed(source, at) {
            tracing::warn!(
                target: "audit",
                event = "ban",
                source = %AuthThrottle::source_of(source),
                "a source past its failed authentications is banned"
            );
        }
    }
}
