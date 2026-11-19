/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::net::IpAddr;
use std::time::Duration;

use futures::channel::{mpsc, oneshot};

use crate::types::login_asked::LoginAsked;

/// An endpoint core's way to the [`crate::AuthThrottle`] the first endpoint core holds: every question goes by message, and
/// nothing the throttle counts is kept here (design D15).
///
/// The channel is unbounded because each connection waits for its answer before it asks again, and tells one outcome per
/// exchange, so the connections' own bound bounds it.
///
/// Every call's future asks the throttle and waits for its answer: dropped, it leaves what it asked with the throttle, which
/// answers it and finds no one to tell.
#[derive(Debug, Clone)]
pub struct ThrottleHandle {
    inbox: mpsc::UnboundedSender<LoginAsked>,
}

impl ThrottleHandle {
    pub(crate) fn new(inbox: mpsc::UnboundedSender<LoginAsked>) -> Self {
        Self { inbox }
    }

    /// Asks the throttle what `build` says and waits for its answer; `None` when the throttle stopped.
    /// A future dropped before its answer leaves the ask with the throttle, which answers it and finds no one to tell.
    async fn ask<T>(&self, build: impl FnOnce(oneshot::Sender<T>) -> LoginAsked) -> Option<T> {
        let (reply, answer) = oneshot::channel();
        self.inbox.unbounded_send(build(reply)).ok()?;
        answer.await.ok()
    }

    /// How long an attempt from `source` waits before its exchange (design D15): nothing for a Unix socket's peer, which
    /// has no address, nor once the throttle stopped.
    pub async fn delay(&self, source: Option<IpAddr>) -> Duration {
        let Some(source) = source else {
            return Duration::ZERO;
        };
        self.ask(|reply| LoginAsked::Delay { source, reply })
            .await
            .unwrap_or(Duration::ZERO)
    }

    /// Whether a connection from `source`, accepted now, may go on: not while its source is banned, nor once the throttle
    /// stopped.
    pub async fn admits(&self, source: IpAddr) -> bool {
        let at = crate::types::client_session::now();
        self.ask(|reply| LoginAsked::Admit { source, at, reply })
            .await
            .unwrap_or(false)
    }

    /// Tells the throttle an authentication from `source` ended now, `succeeded` or failed; nothing is answered.
    pub fn outcome(&self, source: IpAddr, succeeded: bool) {
        let at = crate::types::client_session::now();
        // A throttle that stopped counts nothing more, and compute is stopping.
        let _sent = self.inbox.unbounded_send(LoginAsked::Outcome {
            source,
            at,
            succeeded,
        });
    }
}
