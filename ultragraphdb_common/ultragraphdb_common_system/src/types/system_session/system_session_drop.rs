/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::system_session::SystemSession;

impl Drop for SystemSession {
    /// The connection closed: the group holds the marks its checks began for twice `transaction_ms` (`PROTOCOL.md`
    /// §11.5).
    fn drop(&mut self) {
        if let Some(connection) = self.connection {
            self.shared.owner.disconnect(connection);
        }
    }
}
