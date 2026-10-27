/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, protos/pb.proto (Posting.PostingType). Changed for ultragraphdb: a Rust enum in place
 * of the protobuf enum, without `VALUE_LANG`, which no code writes.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// What a posting holds: a reference to the UID, or a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PostingType {
    /// A reference: the posting's UID is the object.
    #[default]
    Ref,
    /// A value: the posting's value is the object, and its UID orders it.
    Value,
}

impl PostingType {
    /// The byte the posting format stores.
    pub(crate) fn to_byte(self) -> u8 {
        match self {
            PostingType::Ref => 0,
            PostingType::Value => 1,
        }
    }

    /// The type stored as `byte`, `None` for a byte no type is stored as.
    pub(crate) fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(PostingType::Ref),
            1 => Some(PostingType::Value),
            _ => None,
        }
    }
}
