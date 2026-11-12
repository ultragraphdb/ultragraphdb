/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, algo/packed.go. Changed for ultragraphdb: translated to Rust; system IDs in place of
 * UIDs; a nil pack is None, and index-of returns None where Go returns -1; IntersectSortedPacked folds every list,
 * and DifferencePacked is the plain difference over all blocks.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_codec::{
    Decoder, Encoder, List, SeekPos, UidPack, decode, encode, exact_len,
};
use ultragraphdb_core_ids::SystemId;

use crate::types::elem::Elem;
use crate::types::id_heap::IdHeap;
use crate::uidlist::difference;

/// Adds `id` to `encoder`; every caller adds its IDs in strictly increasing order, so the encoder takes each.
fn add(encoder: &mut Encoder, id: SystemId) {
    let added = encoder.add(id);
    debug_assert!(
        added.is_ok(),
        "the IDs are added in strictly increasing order"
    );
}

/// A pack's block size as the encoder takes it.
fn block_size_of(pack: &UidPack) -> usize {
    usize::try_from(pack.block_size()).unwrap_or(usize::MAX)
}

/// Packs the IDs of `u` for which `f(id, index)` holds, `index` counting from 0 in `u`, with `u`'s block size.
#[must_use]
pub fn apply_filter_packed(
    u: &UidPack,
    mut f: impl FnMut(SystemId, usize) -> bool,
) -> Option<UidPack> {
    let mut index = 0;
    let mut decoder = Decoder::new(u);
    let mut encoder = Encoder::new(block_size_of(u));

    while decoder.valid() {
        for &id in decoder.ids() {
            if f(id, index) {
                add(&mut encoder, id);
            }
            index += 1;
        }
        // The block is read at the loop's top, once its validity is checked.
        let _block: &[SystemId] = decoder.next_block();
    }

    encoder.done()
}

/// Intersects two packed lists linearly, packing the result with `u`'s block size; `None` when either is `None`.
#[must_use]
pub fn intersect_with_lin_packed(u: Option<&UidPack>, v: Option<&UidPack>) -> Option<UidPack> {
    let (Some(u), Some(v)) = (u, v) else {
        return None;
    };

    let mut u_dec = Decoder::new(u);
    let mut v_dec = Decoder::new(v);
    let (mut u_idx, mut v_idx) = (0, 0);
    let mut result = Encoder::new(block_size_of(u));

    // Each pass consumes a UID or a block of one list, so the loop ends after either list's last block.
    while !u_dec.ids().is_empty() && !v_dec.ids().is_empty() {
        // Load the next block of the encoded lists if necessary.
        // A decoder holding IDs is on a block, so the next block is its next one, or nothing past the last, which
        // ends the loop.
        if u_idx == u_dec.ids().len() {
            // The block is read below, beside the other list's.
            let _u_block: &[SystemId] = u_dec.next_block();
            u_idx = 0;
        }
        if v_idx == v_dec.ids().len() {
            let _v_block: &[SystemId] = v_dec.next_block();
            v_idx = 0;
        }

        let (uuids, vuids) = (u_dec.ids(), v_dec.ids());

        while let (Some(&uid), Some(&vid)) = (uuids.get(u_idx), vuids.get(v_idx)) {
            if uid > vid {
                v_idx += 1;
                while vuids.get(v_idx).is_some_and(|&x| x < uid) {
                    v_idx += 1;
                }
            } else if uid == vid {
                add(&mut result, uid);
                v_idx += 1;
                u_idx += 1;
            } else {
                u_idx += 1;
                while uuids.get(u_idx).is_some_and(|&x| x < vid) {
                    u_idx += 1;
                }
            }
        }
    }
    result.done()
}

/// Intersects packed lists from the shortest to the longest, as `intersect_sorted` intersects plain ones. Lists of
/// equal length keep their order.
#[must_use]
pub fn intersect_sorted_packed(lists: &[Option<&UidPack>]) -> Option<UidPack> {
    if lists.is_empty() {
        return Encoder::new(10).done();
    }
    let mut ls: Vec<(Option<&UidPack>, usize)> =
        lists.iter().map(|&l| (l, l.map_or(0, exact_len))).collect();
    // Sort the lists based on length.
    ls.sort_by_key(|&(_, length)| length);

    let (smallest, second, rest) = match ls.as_slice() {
        [] => return Encoder::new(10).done(),
        // Return a copy of the UidPack.
        [(only, _)] => return copy_uid_pack(*only),
        [(smallest, _), (second, _), rest @ ..] => (*smallest, *second, rest),
    };

    let mut out = intersect_with_lin_packed(smallest, second);
    // Intersect from smallest to largest.
    for &(l, _) in rest {
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
#[must_use]
pub fn difference_packed(u: Option<&UidPack>, v: Option<&UidPack>) -> Option<UidPack> {
    let Some(v) = v else {
        // If v is None, then it's empty so the value of u - v is just u.
        return copy_uid_pack(u);
    };
    let u = u?;

    let out = difference(&List::new(decode(u, 0)), &List::new(decode(v, 0)));
    let packed = encode(out.ids(), block_size_of(u));
    // A difference of a strictly increasing list is strictly increasing, so it always packs.
    debug_assert!(packed.is_ok(), "a difference of a pack packs");
    packed.unwrap_or_default()
}

/// Merges sorted packed lists into one without duplicates, packed with the first present list's block size (the
/// next one's while it is 0). `None` lists are skipped.
#[must_use]
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
            block_size = block_size_of(l);
        }
        let decoder = Decoder::new(l);
        let Some(&first) = decoder.ids().first() else {
            continue;
        };
        if exact_len(l) > 0 {
            elems.push(Elem::new(first, decoders.len()));
            decoders.push(decoder);
            positions.push(0usize);
        }
    }
    let mut h = IdHeap::new(elems);

    // Our final result.
    let mut result = Encoder::new(block_size);
    // empty_result tracks whether the encoder holds data, which it cannot tell itself.
    let mut empty_result = true;
    let mut last = None; // Last element added to sorted / final result.

    // Each pass takes an ID out of one list or removes an exhausted list, so the loop ends after every ID.
    while let Some((val, li)) = h.get(0).map(|me| (me.val(), me.list_idx())) {
        if empty_result || Some(val) != last {
            add(&mut result, val); // Add if unique.
            last = Some(val);
            empty_result = false;
        }

        // The heap's smallest element is the one removed or replaced.
        let (Some(decoder), Some(position)) = (decoders.get_mut(li), positions.get_mut(li)) else {
            let _orphan: Option<Elem> = h.pop();
            continue;
        };

        // Increment counters.
        *position += 1;
        // Reached the end of the current block. Decode the next block and reset the block counter; an empty block,
        // which only a corrupt pack holds and on which Dgraph panics, contributes nothing.
        while *position >= decoder.ids().len() && decoder.valid() {
            let _block: &[SystemId] = decoder.next_block();
            *position = 0;
        }

        match decoder.ids().get(*position) {
            // Update current value and re-heapify.
            Some(&next) => {
                let _advanced: Option<Elem> = h.set(0, Elem::new(next, li));
                h.fix(0); // Faster than pop followed by push.
            }
            // Reached the end of this list. Remove it from the heap.
            None => {
                let _exhausted: Option<Elem> = h.pop();
            }
        }
    }

    result.done()
}

/// The index of `id` in the packed list, or `None` when it is not there.
#[must_use]
pub fn index_of_packed(u: Option<&UidPack>, id: SystemId) -> Option<usize> {
    let u = u?;

    let mut decoder = Decoder::sought(u, id.to_u128(), SeekPos::Start);
    // Need to re-unpack this block since seek might make uids return an array with missing
    // elements in this block. We need them to make the correct calculation.
    let uids = decoder.unpack_block();
    if uids.is_empty() {
        return None;
    }

    let uidx = uids.partition_point(|&x| x < id);
    if uids.get(uidx) != Some(&id) {
        return None;
    }

    let before: usize = u
        .blocks()
        .iter()
        .take(decoder.block_idx())
        .map(|b| usize::try_from(b.count()).unwrap_or(usize::MAX))
        .sum();
    Some(uidx + before)
}

/// Go's `codec.CopyUidPack`: the block size and blocks.
fn copy_uid_pack(pack: Option<&UidPack>) -> Option<UidPack> {
    pack.cloned()
}
