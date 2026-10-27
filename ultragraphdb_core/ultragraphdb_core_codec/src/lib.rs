/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Ported from Dgraph v25.5.0: `codec`: packed UID lists (UidPack) with group-varint deltas; and the posting lists'
//! types with their stored format.
//!
//! A sorted UID list packs into [`UidPack`] blocks: a block holds at most the block size of UIDs, all sharing their
//! upper 32 bits, stores its first UID as `base` and the others as u32 deltas in group-varint groups of four. The
//! encoding is byte-identical to Dgraph's.
//!
//! [`PostingList`], [`Posting`], [`UidPack`], [`UidBlock`], [`List`], [`Facet`] and [`BackupPostingList`] are the
//! repository's own, in place of Dgraph's protobuf messages, carrying the fields the code reads. A posting list is
//! stored in a versioned, checksummed format of the repository's own: [`PostingList::encode`] and
//! [`PostingList::decode`].

mod codec;
mod errors;
mod groupvarint;
mod types;

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
