/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::system_link::SystemLink;

/// A session's commit relayed to its partition and not yet answered, outliving the client connection that sent it: the system
/// group holds the session in flight, so a full registry does not evict it, until the role connection's stream ends, answered
/// or closed, and then this hold's drop sends its Ended (`PROTOCOL.md` §11.5).
#[derive(Debug)]
pub(crate) struct CommitHold {
    system: SystemLink,
    session: u128,
}

impl CommitHold {
    pub(crate) fn new(system: SystemLink, session: u128) -> Self {
        Self { system, session }
    }
}

impl Drop for CommitHold {
    fn drop(&mut self) {
        self.system.ended(self.session.to_be_bytes());
    }
}
