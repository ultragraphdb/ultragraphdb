/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SessionToken;

use crate::types::resend::Resend;

/// A session's one request counter: the next request's number, whether a request is in flight, the request in doubt with
/// what resends it, and the token of the highest epoch among its answers.
#[derive(Debug, Clone)]
pub(crate) struct SessionState {
    pub next_request: u64,
    pub in_flight: bool,
    pub in_doubt: Option<(u64, Resend)>,
    pub token: Option<SessionToken>,
}

impl SessionState {
    /// A session whose next request is numbered `next_request`.
    pub(crate) fn new(next_request: u64) -> Self {
        Self {
            next_request,
            in_flight: false,
            in_doubt: None,
            token: None,
        }
    }

    /// `token`, kept when it is of a higher epoch than the one held; a reply with no token replaces nothing.
    pub(crate) fn keep_token(&mut self, token: Option<&SessionToken>) {
        let higher = token.filter(|new| {
            self.token
                .as_ref()
                .is_none_or(|held| new.epoch() > held.epoch())
        });
        if let Some(new) = higher {
            self.token = Some(new.clone());
        }
    }
}
