/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Ported from Dgraph v25.5.0: `codec`: packed UID lists (UidPack) with group-varint deltas.
//!
//! A sorted UID list packs into [`UidPack`](ultragraphdb_proto_pb::pb::UidPack) blocks: a block holds at most the
//! block size of UIDs, all sharing their upper 32 bits, stores its first UID as `base` and the others as u32 deltas
//! in group-varint groups of four. The encoding is byte-identical to Dgraph's.

mod codec;
mod errors;
mod groupvarint;
mod types;

pub use crate::codec::{
    approx_len, decode, decode_to_buffer, encode, encode_from_buffer, exact_len,
};
pub use crate::errors::codec_error::{CodecError, CodecErrorEnum};
pub use crate::types::decoder::Decoder;
pub use crate::types::encoder::Encoder;
pub use crate::types::seek_pos::SeekPos;
