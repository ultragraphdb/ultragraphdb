/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Packed lists of 128-bit system IDs, after Dgraph v25.5.0's `codec`, in a format of the repository's own; and the
//! posting lists' types with their stored format.
//!
//! A strictly increasing list of [`SystemId`](ultragraphdb_core_ids::SystemId)s packs into [`UidPack`] blocks of at
//! most the block size: a block stores its first ID whole and each later ID as the difference of its first 104 bits
//! (type, sequence and bucket; the reserved bits, always zero, left out) from those of the ID before it, in LEB128. A
//! pack's bytes begin with its format version, [`UidPack::VERSION`]. A [`Decoder`] reads a pack back block by block
//! and seeks to the block holding any 128-bit target.
//!
//! [`PostingList`], [`Posting`], [`UidPack`], [`UidBlock`], [`List`], [`Facet`] and [`BackupPostingList`] are the
//! repository's own, in place of Dgraph's protobuf messages, carrying the fields the code reads. A posting list is
//! stored in a versioned, checksummed format of the repository's own: [`PostingList::encode`] and
//! [`PostingList::decode`].

// Arithmetic on decoded store input is checked: an overflow here would be a malformed input
// accepted with a wrapped value, so it is refused instead (user decision 2026-11-12; the lint is not in the
// workspace table because elsewhere it flags provably bounded index math).
#![deny(clippy::arithmetic_side_effects)]

mod codec;
mod errors;
mod types;
mod utils;

pub use crate::codec::{
    approx_len, decode, decode_to_buffer, encode, encode_from_buffer, exact_len,
};
pub use crate::errors::codec_error::{CodecError, CodecErrorEnum};
pub use crate::types::backup_posting_list::BackupPostingList;
pub use crate::types::decoder::Decoder;
pub use crate::types::encoder::Encoder;
pub use crate::types::facet::Facet;
pub use crate::types::list::List;
pub use crate::types::posting::Posting;
pub use crate::types::posting_list::PostingList;
pub use crate::types::posting_type::PostingType;
pub use crate::types::seek_pos::SeekPos;
pub use crate::types::uid_block::UidBlock;
pub use crate::types::uid_pack::UidPack;
