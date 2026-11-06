/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cell::Cell;
use std::rc::Rc;

use crate::types::read_pins::ReadPins;
use crate::types::read_request::ReadRequest;

/// What every partition of one read shares of it.
#[derive(Debug)]
pub(crate) struct ReadShared {
    pub(crate) read: ReadRequest,
    /// The rows the read still wants, 0 for no limit: how far a Scan need go.
    pub(crate) remaining: Cell<u32>,
    pub(crate) pins: Rc<ReadPins>,
}
