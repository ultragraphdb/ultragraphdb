/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod subscribe_view_body_view;

use std::marker::PhantomData;

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::command::Command;
use crate::types::role_kind::RoleKind;

/// The body of a received Subscribe frame, read in place; see [`crate::Subscribe`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubscribeView<'a> {
    frame: PhantomData<&'a [u8]>,
    from: u64,
    latest: bool,
}

impl<'a> SubscribeView<'a> {
    /// The last epoch whose close the subscriber holds; 0 for none.
    #[must_use]
    pub fn from(&self) -> u64 {
        self.from
    }

    /// Whether the stream begins with the latest durable close. Only a compute node's connection may ask so, a rule of
    /// the connection's role: the receiving session, which knows the role, enforces it through
    /// [`SubscribeView::check_sender`], and the view reads the field from any sender.
    #[must_use]
    pub fn latest(&self) -> bool {
        self.latest
    }

    /// The body as a connection of role `sender` may send it (`PROTOCOL.md` §13.3), which the system group's session
    /// checks: `latest` 1 on a compute node's connection alone.
    ///
    /// # Errors
    ///
    /// [`ProtocolError::field_value`] (`"latest"`, 1) for `latest` 1 from another role.
    pub fn check_sender(&self, sender: RoleKind) -> Result<(), ProtocolError> {
        if self.latest && sender != RoleKind::Compute {
            return Err(ProtocolError::field_value(
                Command::Subscribe.number(),
                "latest",
                1,
            ));
        }
        Ok(())
    }
}
