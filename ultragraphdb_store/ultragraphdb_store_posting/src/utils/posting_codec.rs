/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The stored form of a posting list: the posting format of `ultragraphdb_core_codec`, decoded and checked. Dgraph
//! unmarshals what it reads and trusts it; the port refuses a value the format refuses, whose checksum fails or whose
//! lengths overrun it among them, or whose structure no encoder writes, so that a corrupted value is an error naming
//! its key rather than a wrong list, a panic or an unbounded allocation.

use ultragraphdb_core_codec::{Decoder, Posting, PostingList, UidPack};
use ultragraphdb_core_ids::SystemId;

use crate::errors::posting_error::PostingError;
use crate::types::p_iterator::VALUE_KEYS_END;
use crate::utils::constants::{DEL, OVR, SET};
use crate::utils::list_keys::{compares_values, first_split_start, identity};

/// The encoded bytes of `plist`.
pub fn marshal(plist: &PostingList) -> Vec<u8> {
    plist.encode()
}

/// The delta stored for `key`: a list of postings without a pack or splits.
pub fn unmarshal_delta(key: &[u8], bytes: &[u8]) -> Result<PostingList, PostingError> {
    let plist = decode(key, bytes)?;
    if plist.pack().is_some() || !plist.splits().is_empty() {
        return Err(PostingError::corrupted(
            key,
            "a delta carries a pack or splits",
        ));
    }
    plist
        .postings()
        .iter()
        .try_for_each(|p| check_posting(key, p, true))?;
    Ok(plist)
}

/// The complete list stored for `key`, or one of its parts: a pack whose IDs ascend, postings sorted by key, each
/// reference's ID in the pack and no value's key, or, for a split list, the splits alone, ascending from the first
/// start its keyspace takes.
pub fn unmarshal_complete(key: &[u8], bytes: &[u8]) -> Result<PostingList, PostingError> {
    let plist = decode(key, bytes)?;
    if !plist.splits().is_empty() {
        let ascending = plist.splits().is_sorted_by(|a, b| a < b);
        if plist.splits().first() != Some(&first_split_start(key)) || !ascending {
            return Err(PostingError::corrupted(
                key,
                "the splits of a list do not ascend from its first start",
            ));
        }
        if plist.pack().is_some() || !plist.postings().is_empty() {
            return Err(PostingError::corrupted(
                key,
                "a split list carries uids of its own",
            ));
        }
        return Ok(plist);
    }
    let uids = match &plist.pack() {
        Some(pack) => checked_uids(key, pack)?,
        None => Vec::new(),
    };
    // A quad list's literals whose fingerprints collide share a uid, and sort by their encodings under it.
    let by_value = compares_values(key);
    let mut in_pack = uids.iter().peekable();
    let mut previous: Option<(u128, &[u8])> = None;
    for p in plist.postings() {
        check_posting(key, p, false)?;
        let id = identity(p, by_value);
        if previous.is_some_and(|prev| prev >= id) {
            return Err(PostingError::corrupted(
                key,
                "the postings of a list are not sorted by uid",
            ));
        }
        let shares_uid = previous.is_some_and(|(uid, _)| uid == p.uid());
        previous = Some(id);
        if shares_uid {
            continue;
        }
        if p.uid() < VALUE_KEYS_END {
            // A value's key is its fingerprint, which no pack holds.
            continue;
        }
        // Ends: each turn takes one of the pack's uids.
        while in_pack.next_if(|&&u| u < p.uid()).is_some() {}
        if in_pack.next_if(|&&u| u == p.uid()).is_none() {
            return Err(PostingError::corrupted(
                key,
                format!("posting {} is not in the list's pack", p.uid()),
            ));
        }
    }
    Ok(plist)
}

fn decode(key: &[u8], bytes: &[u8]) -> Result<PostingList, PostingError> {
    Ok(PostingList::decode(key, bytes)?)
}

/// The uids of `pack`, refused unless every block holds between one uid and the block size, and the uids ascend.
fn checked_uids(key: &[u8], pack: &UidPack) -> Result<Vec<u128>, PostingError> {
    let most = pack.block_size().max(1);
    if pack
        .blocks()
        .iter()
        .any(|b| b.count() == 0 || b.count() > most)
    {
        return Err(PostingError::corrupted(
            key,
            "a block holds no uid or more than the block size",
        ));
    }
    // Lossless: a block's count is a u32, and a `usize` holds 32 bits on every target the repository builds for.
    let mut uids = Vec::with_capacity(pack.blocks().iter().map(|b| b.count() as usize).sum());
    let mut decoder = Decoder::new(pack);
    // Ends: each turn moves the decoder to the next of the pack's blocks.
    while decoder.valid() {
        uids.extend(decoder.ids().iter().map(|id| id.to_u128()));
        let _next_ids: &[SystemId] = decoder.next_block();
    }
    if !uids.is_sorted_by(|a, b| a < b) {
        return Err(PostingError::corrupted(
            key,
            "the uids of a pack do not ascend",
        ));
    }
    Ok(uids)
}

fn check_posting(key: &[u8], p: &Posting, delta: bool) -> Result<(), PostingError> {
    let op_ok = if delta {
        [SET, DEL, OVR].contains(&p.op())
    } else {
        // A rollup keeps the postings it iterated, which never include a delete.
        [0, SET, OVR].contains(&p.op())
    };
    if op_ok {
        Ok(())
    } else {
        Err(PostingError::corrupted(
            key,
            format!("posting {} has an unknown operation", p.uid()),
        ))
    }
}
