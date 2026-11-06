/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::rc::Weak;

use crate::types::owner_handle::OwnerHandle;

/// A session's commit relayed to its partition and not yet answered, outliving the client connection that sent it: the owner
/// holds the session in flight, so a full registry does not evict it, until the role connection's stream ends, answered or
/// closed (`PROTOCOL.md` §11.5). It holds the owner weakly, as an endpoint core gone has nothing in flight to release.
#[derive(Debug)]
pub(crate) struct CommitHold {
    owner: Weak<OwnerHandle>,
    session: u128,
}

impl CommitHold {
    pub(crate) fn new(owner: Weak<OwnerHandle>, session: u128) -> Self {
        Self { owner, session }
    }
}

impl Drop for CommitHold {
    fn drop(&mut self) {
        if let Some(owner) = self.owner.upgrade() {
            owner.commit_ended(self.session);
        }
    }
}
