/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, algo/packed.go. Changed for ultragraphdb: translated to Rust; a nil pack is None, and
 * index-of returns None where Go returns -1; IntersectSortedPacked folds every list, and DifferencePacked is the plain
 * difference over all blocks.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_codec::{Decoder, Encoder, SeekPos, decode, encode, exact_len};
use ultragraphdb_proto_pb::pb::{List, UidBlock, UidPack};

use crate::types::elem::Elem;
use crate::types::uint64_heap::Uint64Heap;
use crate::uidlist::difference;

/// Packs the UIDs of `u` for which `f(uid, index)` holds, `index` counting from 0 in `u`, with `u`'s block size.
pub fn apply_filter_packed(u: &UidPack, mut f: impl FnMut(u64, usize) -> bool) -> Option<UidPack> {
    let mut index = 0;
    let mut decoder = Decoder::new(u);
    let mut encoder = Encoder::new(u.block_size as usize);

    while decoder.valid() {
        for &uid in decoder.uids() {
            if f(uid, index) {
                encoder.add(uid);
            }
            index += 1;
        }
        decoder.next_block();
    }

    encoder.done()
}

/// Intersects two packed lists linearly, packing the result with `u`'s block size; `None` when either is `None`.
pub fn intersect_with_lin_packed(u: Option<&UidPack>, v: Option<&UidPack>) -> Option<UidPack> {
    let (Some(u), Some(v)) = (u, v) else {
        return None;
    };

    let mut u_dec = Decoder::new(u);
    let mut v_dec = Decoder::new(v);
    let (mut u_idx, mut v_idx) = (0, 0);
    let mut result = Encoder::new(u.block_size as usize);

    loop {
        // Break if the end of a list has been reached.
        if u_dec.uids().is_empty() || v_dec.uids().is_empty() {
            break;
        }

        // Load the next block of the encoded lists if necessary.
        if u_idx == u_dec.uids().len() {
            if !u_dec.valid() {
                break;
            }
            u_dec.next_block();
            u_idx = 0;
        }
        if v_idx == v_dec.uids().len() {
            if !v_dec.valid() {
                break;
            }
            v_dec.next_block();
            v_idx = 0;
        }

        let (uuids, vuids) = (u_dec.uids(), v_dec.uids());
        let (u_len, v_len) = (uuids.len(), vuids.len());

        while u_idx < u_len && v_idx < v_len {
            let uid = uuids[u_idx];
            let vid = vuids[v_idx];
            if uid > vid {
                v_idx += 1;
                while v_idx < v_len && vuids[v_idx] < uid {
                    v_idx += 1;
                }
            } else if uid == vid {
                result.add(uid);
                v_idx += 1;
                u_idx += 1;
            } else {
                u_idx += 1;
                while u_idx < u_len && uuids[u_idx] < vid {
                    u_idx += 1;
                }
            }
        }
    }
    result.done()
}

/// Intersects packed lists from the shortest to the longest, as `intersect_sorted` intersects plain ones. Lists of
/// equal length keep their order.
pub fn intersect_sorted_packed(lists: &[Option<&UidPack>]) -> Option<UidPack> {
    if lists.is_empty() {
        return Encoder::new(10).done();
    }
    let mut ls: Vec<(Option<&UidPack>, usize)> =
        lists.iter().map(|&l| (l, l.map_or(0, exact_len))).collect();
    // Sort the lists based on length.
    ls.sort_by_key(|&(_, length)| length);

    if ls.len() == 1 {
        // Return a copy of the UidPack.
        return copy_uid_pack(ls[0].0);
    }

    let mut out = intersect_with_lin_packed(ls[0].0, ls[1].0);
    // Intersect from smallest to largest.
    for &(l, _) in &ls[2..] {
        out = intersect_with_lin_packed(out.as_ref(), l);
        // Break if we reach size 0 as we can no longer
        // add any element.
        if out.as_ref().map_or(0, exact_len) == 0 {
            break;
        }
    }
    out
}

/// The UIDs of `u` not in `v`, packed with `u`'s block size: a copy of `u` when `v` is `None`, `None` when `u` is or
/// when no UID remains. The result is `difference` over every block of both lists, duplicates included.
pub fn difference_packed(u: Option<&UidPack>, v: Option<&UidPack>) -> Option<UidPack> {
    let Some(v) = v else {
        // If v is None, then it's empty so the value of u - v is just u.
        return copy_uid_pack(u);
    };
    let u = u?;

    let out = difference(&List { uids: decode(u, 0) }, &List { uids: decode(v, 0) });
    encode(&out.uids, u.block_size as usize)
}

/// Merges sorted packed lists into one without duplicates, packed with the first present list's block size (the
/// next one's while it is 0). `None` lists are skipped.
pub fn merge_sorted_packed(lists: &[Option<&UidPack>]) -> Option<UidPack> {
    if lists.is_empty() {
        return None;
    }

    // Each list's decoder and its position in the current block, indexed by the heap elements' list index.
    let mut decoders = Vec::with_capacity(lists.len());
    let mut positions = Vec::with_capacity(lists.len());
    let mut elems = Vec::with_capacity(lists.len());
    let mut block_size = 0usize;

    for l in lists.iter().flatten() {
        if block_size == 0 {
            block_size = l.block_size as usize;
        }
        let decoder = Decoder::new(l);
        let Some(&first) = decoder.uids().first() else {
            continue;
        };
        if exact_len(l) > 0 {
            elems.push(Elem::new(first, decoders.len()));
            decoders.push(decoder);
            positions.push(0usize);
        }
    }
    let mut h = Uint64Heap::new(elems);

    // Our final result.
    let mut result = Encoder::new(block_size);
    // empty_result tracks whether the encoder holds data, which it cannot tell itself.
    let mut empty_result = true;
    let mut last = 0u64; // Last element added to sorted / final result.

    while let Some(&me) = h.get(0) {
        // While heap is not empty.
        if empty_result || me.val() != last {
            result.add(me.val()); // Add if unique.
            last = me.val();
            empty_result = false;
        }

        let li = me.list_idx();
        let (Some(decoder), Some(position)) = (decoders.get_mut(li), positions.get_mut(li)) else {
            h.pop();
            continue;
        };

        // Increment counters.
        *position += 1;
        // Reached the end of the current block. Decode the next block and reset the block counter; an empty block,
        // which only a corrupt pack holds and on which Dgraph panics, contributes nothing.
        while *position >= decoder.uids().len() && decoder.valid() {
            decoder.next_block();
            *position = 0;
        }

        match decoder.uids().get(*position) {
            // Update current value and re-heapify.
            Some(&next) => {
                h.set(0, Elem::new(next, li));
                h.fix(0); // Faster than pop followed by push.
            }
            // Reached the end of this list. Remove it from the heap.
            None => {
                h.pop();
            }
        }
    }

    result.done()
}

/// The index of `uid` in the packed list, or `None` when it is not there.
pub fn index_of_packed(u: Option<&UidPack>, uid: u64) -> Option<usize> {
    let u = u?;

    let mut decoder = Decoder::sought(u, uid, SeekPos::Start);
    // Need to re-unpack this block since seek might make uids return an array with missing
    // elements in this block. We need them to make the correct calculation.
    let uids = decoder.unpack_block();
    if uids.is_empty() {
        return None;
    }

    let uidx = uids.partition_point(|&x| x < uid);
    if uids.get(uidx) != Some(&uid) {
        return None;
    }

    let before: usize = u
        .blocks
        .iter()
        .take(decoder.block_idx())
        .map(|b| b.num_uids as usize)
        .sum();
    Some(uidx + before)
}

/// Go's `codec.CopyUidPack`: the block size and blocks, with `alloc_ref` 0.
fn copy_uid_pack(pack: Option<&UidPack>) -> Option<UidPack> {
    pack.map(|p| UidPack {
        block_size: p.block_size,
        blocks: p
            .blocks
            .iter()
            .map(|b| UidBlock {
                base: b.base,
                deltas: b.deltas.clone(),
                num_uids: b.num_uids,
            })
            .collect(),
        alloc_ref: 0,
    })
}
