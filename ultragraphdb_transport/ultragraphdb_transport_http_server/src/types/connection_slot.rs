/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::rc::Rc;

use crate::types::connection_slots::ConnectionSlots;

/// A connection's place among those its listener serves at once, given back when it is dropped: as the connection's task
/// ends, however it ends.
pub(crate) struct ConnectionSlot {
    slots: Rc<ConnectionSlots>,
}

impl ConnectionSlot {
    /// The slot just taken of `slots`.
    pub(crate) fn new(slots: Rc<ConnectionSlots>) -> Self {
        Self { slots }
    }
}

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        self.slots.free();
    }
}
