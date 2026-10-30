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
            ProtocolErrorEnum::SequenceOrder { expected, found } => write!(
                f,
                "frame of sequence {found} where sequence {expected} was due"
            ),
            ProtocolErrorEnum::WrongCommand { expected, found } => write!(
                f,
                "a view of command {expected} was asked of a frame of command {found}"
            ),
            ProtocolErrorEnum::BodyTooShort { command, len, need } => write!(
                f,
                "body of command {command}, {len} bytes, is shorter than its {need}-byte fixed part and section table"
            ),
            ProtocolErrorEnum::SectionOutOfBounds {
                command,
                section,
                offset,
                len,
                body,
            } => write!(
                f,
                "section {section} of command {command}, {len} bytes at {offset}, runs past the {body}-byte body"
            ),
            ProtocolErrorEnum::SectionOffset {
                command,
                section,
                offset,
                expected,
            } => write!(
                f,
                "section {section} of command {command} starts at {offset}, not at {expected}"
            ),
            ProtocolErrorEnum::PaddingNotZero { command, offset } => write!(
                f,
                "padding byte {offset} of a body of command {command} is not zero"
            ),
            ProtocolErrorEnum::TrailingBytes { command, end, len } => write!(
                f,
                "body of command {command} holds {len} bytes, past the end of its last section at {end}"
            ),
            ProtocolErrorEnum::SectionLength {
                command,
                section,
                len,
                unit,
            } => write!(
                f,
                "section {section} of command {command}, {len} bytes, is not a whole number of {unit}-byte records"
            ),
            ProtocolErrorEnum::EntryOutOfBounds {
                command,
                section,
                entry,
                offset,
            } => write!(
                f,
                "entry {entry} of section {section} of command {command}, at {offset}, runs past its section"
            ),
            ProtocolErrorEnum::CountMismatch {
                command,
                field,
                count,
                expected,
            } => write!(
                f,
                "{field} of command {command} counts {count} where the body holds {expected}"
            ),
            ProtocolErrorEnum::NotUtf8 {
                command,
                section,
                offset,
            } => write!(
                f,
                "section {section} of command {command} is not UTF-8 at {offset}"
            ),
            ProtocolErrorEnum::IndexOutOfRange {
                command,
                section,
                at,
                index,
                limit,
            } => write!(
                f,
                "entry {at} of section {section} of command {command} is {index}, past the {limit} it indexes"
            ),
            ProtocolErrorEnum::BadTerm {
                command,
                section,
                at,
            } => write!(
                f,
                "term {at} of section {section} of command {command} is not a term of its place"
            ),
            ProtocolErrorEnum::FieldValue {
                command,
                field,
                value,
            } => write!(
                f,
                "{field} of command {command} holds {value}, which the command does not define"
            ),
            ProtocolErrorEnum::DuplicateTerm {
                command,
                section,
                at,
                earlier,
            } => write!(
                f,
                "entry {at} of the term table in section {section} of command {command} repeats entry {earlier}"
            ),
            ProtocolErrorEnum::TermTooDeep {
                command,
                section,
                at,
                depth,
                limit,
            } => write!(
                f,
                "entry {at} of the term table in section {section} of command {command} nests {depth} deep, past the {limit} allowed"
            ),
            ProtocolErrorEnum::Record { index, cause } => write!(
                f,
                "write-set record {index} of the round: {}",
                ProtocolError(cause.as_ref().clone())
            ),
        }
    }
}
