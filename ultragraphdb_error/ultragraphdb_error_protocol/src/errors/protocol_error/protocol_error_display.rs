/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::protocol_error::{ProtocolError, ProtocolErrorEnum};

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            ProtocolErrorEnum::ShortHeader { len } => {
                write!(
                    f,
                    "frame of {len} bytes is shorter than its 256-byte header"
                )
            }
            ProtocolErrorEnum::HeaderChecksum { stored, computed } => write!(
                f,
                "header checksum {stored:#018x} does not match the computed {computed:#018x}"
            ),
            ProtocolErrorEnum::UnknownCommand { command } => write!(f, "unknown command {command}"),
            ProtocolErrorEnum::ReservedNotZero { offset } => {
                write!(f, "reserved header byte {offset} is not zero")
            }
            ProtocolErrorEnum::WrongCluster { expected, found } => write!(
                f,
                "frame of cluster {found:#018x}, not this cluster {expected:#018x}"
            ),
            ProtocolErrorEnum::SizeBelowHeader { size } => {
                write!(f, "frame size {size} is below its 256-byte header")
            }
            ProtocolErrorEnum::FrameTooLarge { size, limit } => {
                write!(f, "frame of {size} bytes exceeds the {limit}-byte limit")
            }
            ProtocolErrorEnum::LengthMismatch { size, len } => {
                write!(
                    f,
                    "frame size {size} does not match the {len} bytes received"
                )
            }
            ProtocolErrorEnum::BodyLength { len } => write!(
                f,
                "body of {len} bytes has no split into chunks and their table"
            ),
            ProtocolErrorEnum::ChunkTable { stored, computed } => write!(
                f,
                "chunk table checksum {computed:#018x} does not match checksum_body {stored:#018x}"
            ),
            ProtocolErrorEnum::Chunk { index, start, end } => write!(
                f,
                "chunk {index}, body bytes {start}..{end}, does not match its leaf"
            ),
        }
    }
}
