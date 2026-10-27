/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! `ProtocolError`, why a frame of the binary protocol was refused: one variant per validation rule.

mod errors;

pub use crate::errors::protocol_error::{ProtocolError, ProtocolErrorEnum};
