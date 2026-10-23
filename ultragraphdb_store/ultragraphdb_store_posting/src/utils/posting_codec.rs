/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The stored form of a posting list: its protobuf encoding, decoded and checked. Dgraph unmarshals what it reads
//! and trusts it; the port refuses a value that does not decode or whose structure no encoder writes, so that a
//! corrupted value is an error naming its key rather than a wrong list, a panic or an unbounded allocation.
//!
//! A check of structure cannot see every corruption: a changed byte inside a value, a facet or a uid delta can
//! decode to another well-formed list.

use prost::Message;
use ultragraphdb_core_codec::Decoder;
use ultragraphdb_proto_pb::pb::{Posting, PostingList, UidPack, posting};

use crate::errors::posting_error::PostingError;
use crate::utils::constants::{DEL, OVR, SET};

/// The encoded bytes of `plist`.
pub fn marshal(plist: &PostingList) -> Vec<u8> {
    plist.encode_to_vec()
}

/// The delta stored for `key`: a list of postings without a pack or splits.
pub fn unmarshal_delta(key: &[u8], bytes: &[u8]) -> Result<PostingList, PostingError> {
    let plist = decode(key, bytes)?;
    if plist.pack.is_some() || !plist.splits.is_empty() {
        return Err(PostingError::corrupted(
            key,
            "a delta carries a pack or splits",
        ));
    }
    plist
        .postings
        .iter()
        .try_for_each(|p| check_posting(key, p, true))?;
    Ok(plist)
}

/// The complete list stored for `key`, or one of its parts: a pack whose uids ascend, postings sorted by uid and
/// each in the pack, or, for a split list, the splits alone, ascending from 1.
pub fn unmarshal_complete(key: &[u8], bytes: &[u8]) -> Result<PostingList, PostingError> {
    let plist = decode(key, bytes)?;
    if !plist.splits.is_empty() {
        let ascending = plist.splits.windows(2).all(|w| w[0] < w[1]);
        if plist.splits[0] != 1 || !ascending {
            return Err(PostingError::corrupted(
                key,
                "the splits of a list do not ascend from 1",
            ));
        }
        if plist.pack.is_some() || !plist.postings.is_empty() {
            return Err(PostingError::corrupted(
                key,
                "a split list carries uids of its own",
            ));
        }
        return Ok(plist);
    }
    let uids = match &plist.pack {
        Some(pack) => checked_uids(key, pack)?,
        None => Vec::new(),
    };
    let mut in_pack = uids.iter().peekable();
    let mut previous: Option<u64> = None;
    for p in &plist.postings {
        check_posting(key, p, false)?;
        if previous.is_some_and(|prev| prev >= p.uid) {
            return Err(PostingError::corrupted(
                key,
                "the postings of a list are not sorted by uid",
            ));
        }
        previous = Some(p.uid);
        while in_pack.next_if(|&&u| u < p.uid).is_some() {}
        if in_pack.next_if(|&&u| u == p.uid).is_none() {
            return Err(PostingError::corrupted(
                key,
                format!("posting {} is not in the list's pack", p.uid),
            ));
        }
    }
    Ok(plist)
}

fn decode(key: &[u8], bytes: &[u8]) -> Result<PostingList, PostingError> {
    PostingList::decode(bytes).map_err(|e| PostingError::corrupted(key, e.to_string()))
}

/// The uids of `pack`, refused unless every block holds between one uid and the block size, and the uids ascend.
fn checked_uids(key: &[u8], pack: &UidPack) -> Result<Vec<u64>, PostingError> {
    let most = pack.block_size.max(1);
    if pack
        .blocks
        .iter()
        .any(|b| b.num_uids == 0 || b.num_uids > most)
    {
        return Err(PostingError::corrupted(
            key,
            "a block holds no uid or more than the block size",
        ));
    }
    let mut uids = Vec::with_capacity(pack.blocks.iter().map(|b| b.num_uids as usize).sum());
    let mut decoder = Decoder::new(pack);
    while decoder.valid() {
        uids.extend_from_slice(decoder.uids());
        decoder.next_block();
    }
    if uids.windows(2).any(|w| w[0] >= w[1]) {
        return Err(PostingError::corrupted(
            key,
            "the uids of a pack do not ascend",
        ));
    }
    Ok(uids)
}

fn check_posting(key: &[u8], p: &Posting, delta: bool) -> Result<(), PostingError> {
    let known = posting::ValType::try_from(p.val_type).is_ok()
        && posting::PostingType::try_from(p.posting_type).is_ok();
    let op_ok = if delta {
        [SET, DEL, OVR].contains(&p.op)
    } else {
        // A rollup keeps the postings it iterated, which never include a delete.
        [0, SET, OVR].contains(&p.op)
    };
    if known && op_ok {
        Ok(())
    } else {
        Err(PostingError::corrupted(
            key,
            format!("posting {} has an unknown type or operation", p.uid),
        ))
    }
}
