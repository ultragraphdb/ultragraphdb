/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The ordered-log interface, `OrderedLog`: the write-ahead log implements it on one node, VSR from stage 10.

mod traits;
mod types;

pub use crate::traits::ordered_log::OrderedLog;
pub use crate::types::log_event::LogEvent;
pub use crate::types::position::Position;
pub use crate::types::round::Round;
pub use crate::types::round_ref::RoundRef;
