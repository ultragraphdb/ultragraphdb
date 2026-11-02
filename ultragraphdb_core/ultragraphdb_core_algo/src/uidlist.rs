/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, algo/uidlist.go. Changed for ultragraphdb: translated to Rust; system IDs in place of
 * UIDs; goroutines are scoped threads, the parallel merge gives each thread a buffer of its own rather than a slice
 * of one shared buffer, the binary intersection walks an explicit stack and the large merge splits once rather than
 * recursing (Ultra Style rule 1), and index-of returns None where Go returns -1. ToUintsListForTest is not ported.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::panic;
use std::thread;

use ultragraphdb_core_codec::{Decoder, List, SeekPos, UidPack, exact_len};
use ultragraphdb_core_ids::SystemId;

use crate::types::elem::Elem;
use crate::types::id_heap::IdHeap;

const JUMP: usize = 32; // Jump size in InsersectWithJump.
const LIN_VS_BIN_RATIO: usize = 10; // When is linear search better than binary

/// Keeps the IDs of `u` for which `f(id, index)` holds, `index` counting from 0 in the original list.
pub fn apply_filter(u: &mut List, mut f: impl FnMut(SystemId, usize) -> bool) {
    let mut i = 0;
    u.ids_mut().retain(|&uid| {
        let keep = f(uid, i);
        i += 1;
        keep
    });
}

/// Intersects the packed list's IDs from `after` on, any 128-bit value, with `v`, writing the result to `o`. Leaves
/// `o` unchanged when `pack` is `None`, and writes nothing when `after` lies past the pack's last ID, where Dgraph's
/// panics with the binary strategy (ledger 3).
pub fn intersect_compressed_with(pack: Option<&UidPack>, after: u128, v: &List, o: &mut List) {
    let Some(pack) = pack else {
        return;
    };
    let mut dec = Decoder::sought(pack, after, SeekPos::Start);
    o.ids_mut().clear();
    // No ID lies at or after `after`. Dgraph goes on and panics in SeekToBlock past the last block, or, when the plain
    // list starts at UID 0, rewinds to the first block and intersects the whole pack (ledger 3).
    if dec.ids().is_empty() {
        return;
    }
    let mut n = dec.approx_len();
    let mut m = v.ids().len();

    if n > m {
        (n, m) = (m, n);
    }

    // If n equals 0, set it to 1 to avoid division by zero.
    if n == 0 {
        n = 1;
    }

    // Select appropriate function based on heuristics.
    let ratio = m as f64 / n as f64;
    if ratio < LIN_VS_BIN_RATIO as f64 {
        intersect_compressed_with_lin_jump(&mut dec, v.ids(), o.ids_mut());
    } else {
        intersect_compressed_with_bin(&mut dec, v.ids(), o.ids_mut());
    }
}

/// Intersects the decoder's UIDs from its current block on with `v` linearly, appending to `o`.
pub fn intersect_compressed_with_lin_jump(
    dec: &mut Decoder,
    v: &[SystemId],
    o: &mut Vec<SystemId>,
) {
    let m = v.len();
    let mut k = 0;
    let (_, off) = intersect_with_lin(dec.ids(), &v[k..], o);
    k += off;

    while k < m {
        let u = dec.linear_seek(v[k].to_u128());
        if u.is_empty() {
            break;
        }
        let (_, mut step) = intersect_with_lin(u, &v[k..], o);
        if step == 0 {
            step = 1; // If v[k] isn't in u, move forward.
        }

        k += step;
    }
}

/// Intersects the decoder's UIDs with `q` by binary search over the shorter list, appending to `o`. Based on the paper "Fast Intersection Algorithms for Sorted Sequences",
/// <https://link.springer.com/chapter/10.1007/978-3-642-12476-1_3>.
pub fn intersect_compressed_with_bin(dec: &mut Decoder, q: &[SystemId], o: &mut Vec<SystemId>) {
    let ld = exact_len(dec.pack());
    let lq = q.len();

    if lq == 0 {
        // Not checking whether ld == 0 because that is an approximate size.
        // If the actual length of the list is zero, the loop below will
        // do nothing as expected.
        return;
    }

    // Pick the shorter list and do binary search
    if ld <= lq {
        let mut q = q;
        let mut block_uids = dec.ids();
        // Each pass moves to the next block, so the loop ends after the pack's last block.
        while !block_uids.is_empty() {
            let (_, off) = intersect_with_jump(block_uids, q, o);
            q = &q[off..];
            if q.is_empty() {
                return;
            }
            block_uids = dec.next_block();
        }
        return;
    }

    let mut uids = dec.ids();
    let mut qidx = 0;
    // Each pass moves qidx forward by at least one, so the loop ends after q's last UID.
    while let Some(&u) = q.get(qidx) {
        if uids.last().is_none_or(|&last| u > last) {
            uids = if lq * LIN_VS_BIN_RATIO < ld {
                dec.linear_seek(u.to_u128())
            } else {
                dec.seek_to_block(u.to_u128(), SeekPos::Current)
            };
            if uids.is_empty() {
                return;
            }
        }
        let (_, mut off) = intersect_with_jump(uids, &q[qidx..], o);
        if off == 0 {
            off = 1; // if v[k] isn't in u, move forward
        }
        qidx += off;
    }
}

/// Intersects the sorted lists `u` and `v` into `o`, choosing the linear, jumping or binary strategy by the ratio of
/// their lengths. Go writes the result over `u` when `o` is `u`; here `o` is a list of its own.
pub fn intersect_with(u: &List, v: &List, o: &mut List) {
    let mut n = u.ids().len();
    let mut m = v.ids().len();

    if n > m {
        (n, m) = (m, n);
    }
    o.ids_mut().clear();
    o.ids_mut().reserve(n);
    if n == 0 {
        n = 1;
    }
    // Select appropriate function based on heuristics. The whole of both lists is intersected, so how far each
    // function got is not needed.
    let ratio = m as f64 / n as f64;
    if ratio < 100.0 {
        let _reached: (usize, usize) = intersect_with_lin(u.ids(), v.ids(), o.ids_mut());
    } else if ratio < 500.0 {
        let _reached: (usize, usize) = intersect_with_jump(u.ids(), v.ids(), o.ids_mut());
    } else {
        let _reached: usize = intersect_with_bin(u.ids(), v.ids(), o.ids_mut());
    }
}

/// Intersects `u` and `v` linearly, appending to `o`; returns how far it got into each.
pub fn intersect_with_lin(u: &[SystemId], v: &[SystemId], o: &mut Vec<SystemId>) -> (usize, usize) {
    debug_assert!(u.is_sorted(), "the first list is sorted");
    debug_assert!(v.is_sorted(), "the second list is sorted");
    let n = u.len();
    let m = v.len();
    let (mut i, mut k) = (0, 0);
    while i < n && k < m {
        let uid = u[i];
        let vid = v[k];
        if uid > vid {
            k += 1;
            while k < m && v[k] < uid {
                k += 1;
            }
        } else if uid == vid {
            o.push(uid);
            k += 1;
            i += 1;
        } else {
            i += 1;
            while i < n && u[i] < vid {
                i += 1;
            }
        }
    }
    (i, k)
}

/// Intersects `u` and `v` linearly, jumping 32 places where it can, appending to `o`; returns how far it got into
/// each.
pub fn intersect_with_jump(
    u: &[SystemId],
    v: &[SystemId],
    o: &mut Vec<SystemId>,
) -> (usize, usize) {
    debug_assert!(u.is_sorted(), "the first list is sorted");
    debug_assert!(v.is_sorted(), "the second list is sorted");
    let n = u.len();
    let m = v.len();
    let (mut i, mut k) = (0, 0);
    while i < n && k < m {
        let uid = u[i];
        let vid = v[k];
        if uid == vid {
            o.push(uid);
            k += 1;
            i += 1;
        } else if k + JUMP < m && uid > v[k + JUMP] {
            k += JUMP;
        } else if i + JUMP < n && vid > u[i + JUMP] {
            i += JUMP;
        } else if uid > vid {
            k += 1;
            while k < m && v[k] < uid {
                k += 1;
            }
        } else {
            i += 1;
            while i < n && u[i] < vid {
                i += 1;
            }
        }
    }
    (i, k)
}

/// Intersects `d` and `q` by binary search, appending to `o`. Returns where to move the second array to, in the
/// shorter of the two; 0 means not found. Based on the paper "Fast Intersection Algorithms for Sorted Sequences",
/// <https://link.springer.com/chapter/10.1007/978-3-642-12476-1_3>.
pub fn intersect_with_bin(d: &[SystemId], q: &[SystemId], o: &mut Vec<SystemId>) -> usize {
    debug_assert!(d.is_sorted(), "the first list is sorted");
    debug_assert!(q.is_sorted(), "the second list is sorted");
    let (d, q) = if d.len() < q.len() { (q, d) } else { (d, q) };
    let (ld, lq) = (d.len(), q.len());
    if ld == 0 || lq == 0 || d[ld - 1] < q[0] || q[lq - 1] < d[0] {
        return 0;
    }

    let first = d[0];
    let minq = q.partition_point(|&x| x < first);

    let last = d[ld - 1];
    let maxq = q.partition_point(|&x| x <= last);

    bin_intersect(d, &q[minq..maxq], o);
    maxq
}

/// The most tasks [`bin_intersect`]'s stack holds: the probed half at least halves at each level, so a path of the
/// search is at most 65 levels deep for any slice, and each level leaves at most a right half and a match pending.
const BIN_INTERSECT_STACK_MAX: usize = 2 * 65 + 1;

/// A step of [`bin_intersect`]'s search: a pair of slices to intersect, the longer first, or a match to append once
/// the left halves before it are done.
enum BinTask<'a> {
    Pair(&'a [SystemId], &'a [SystemId]),
    Match(SystemId),
}

/// The pair of `a` and `b` with the longer first, as Go's recursion orders its arguments: `a` first only when it is
/// strictly longer.
fn longer_first<'a>(a: &'a [SystemId], b: &'a [SystemId]) -> BinTask<'a> {
    if a.len() > b.len() {
        BinTask::Pair(a, b)
    } else {
        BinTask::Pair(b, a)
    }
}

/// Go's recursive binary intersection, walked with an explicit stack (Ultra Style rule 1). `d` must be at least as
/// long as `q`. A pair splits at `q`'s middle value into its left halves, the middle value when `d` holds it, and its
/// right halves; they are pushed in reverse so that the left halves are appended first, keeping `fin` sorted.
fn bin_intersect(d_whole: &[SystemId], q_whole: &[SystemId], fin: &mut Vec<SystemId>) {
    let mut stack = Vec::with_capacity(BIN_INTERSECT_STACK_MAX);
    stack.push(BinTask::Pair(d_whole, q_whole));
    while let Some(task) = stack.pop() {
        debug_assert!(
            stack.len() < BIN_INTERSECT_STACK_MAX,
            "the search's depth is bounded"
        );
        let (d, q) = match task {
            BinTask::Match(value) => {
                fin.push(value);
                continue;
            }
            BinTask::Pair(d, q) => (d, q),
        };
        let midq = q.len() / 2;
        let Some(&qval) = q.get(midq) else {
            continue;
        };
        // Each pair is pushed with the longer slice first, so d holds at least q's IDs and is not empty here.
        debug_assert!(d.len() >= q.len(), "the longer slice is searched");
        let midd = d.partition_point(|&x| x < qval);
        // Go steps midd back on a miss and resumes at midd + 1, so the right half starts after a match and at midd
        // otherwise; with midd past d's end there is no match and no right half.
        if let Some(&at_midd) = d.get(midd) {
            let rest = if at_midd == qval { midd + 1 } else { midd };
            stack.push(longer_first(&d[rest..], &q[midq + 1..]));
            if at_midd == qval {
                stack.push(BinTask::Match(qval));
            }
        }
        stack.push(longer_first(&d[..midd], &q[..midq]));
    }
}

/// Intersects the lists from the shortest to the longest.
#[must_use]
pub fn intersect_sorted(lists: &[&List]) -> List {
    if lists.is_empty() {
        return List::default();
    }
    let mut ls: Vec<&List> = lists.to_vec();
    // Sort the lists based on length.
    ls.sort_by_key(|l| l.ids().len());
    if ls.len() == 1 {
        return ls[0].clone();
    }

    let mut out = List::default();
    intersect_with(ls[0], ls[1], &mut out);
    // Intersect from smallest to largest.
    let mut scratch = List::default();
    for l in &ls[2..] {
        intersect_with(&out, l, &mut scratch);
        std::mem::swap(&mut out, &mut scratch);
        // Break if we reach size 0 as we can no longer
        // add any element.
        if out.ids().is_empty() {
            break;
        }
    }
    out
}

/// The UIDs of `u` not in `v`.
#[must_use]
pub fn difference(u: &List, v: &List) -> List {
    let (u, v) = (u.ids(), v.ids());
    debug_assert!(u.is_sorted(), "the first list is sorted");
    debug_assert!(v.is_sorted(), "the second list is sorted");
    let n = u.len();
    let m = v.len();
    let mut out = Vec::with_capacity(n / 2);
    let (mut i, mut k) = (0, 0);
    while i < n && k < m {
        let uid = u[i];
        let vid = v[k];
        if uid < vid {
            while i < n && u[i] < vid {
                out.push(u[i]);
                i += 1;
            }
        } else if uid == vid {
            i += 1;
            k += 1;
        } else {
            k += 1;
            while k < m && v[k] < uid {
                k += 1;
            }
        }
    }
    while i < n && k >= m {
        out.push(u[i]);
        i += 1;
    }
    debug_assert!(out.is_sorted(), "a difference of sorted lists is sorted");
    debug_assert!(
        out.len() <= n,
        "a difference holds no more than its first list"
    );
    List::new(out)
}

/// Merges sorted lists into one without duplicates. More than 100 lists are split over 10 threads, each merging its
/// share, and the 10 results are merged; Go recurses into each share, which gives the same union with a deeper call
/// graph (Ultra Style rule 1).
#[expect(
    clippy::disallowed_methods,
    reason = "Dgraph's algo merges large inputs on 10 goroutines; query code is outside the storage simulation"
)]
#[must_use]
pub fn merge_sorted_more_mem(lists: &[&List]) -> List {
    let num_threads = 10;
    if lists.len() <= num_threads * num_threads {
        return internal_merge_sort(lists);
    }
    let k = num_threads;
    let res: Vec<List> = thread::scope(|s| {
        let handles: Vec<_> = (0..k)
            .map(|i| {
                let end = ((i + 1) * lists.len() / k).min(lists.len());
                let chunk = &lists[i * lists.len() / k..end];
                s.spawn(move || internal_merge_sort(chunk))
            })
            .collect();
        // A merge thread runs only the panic-free merge, so it returns; were it to panic, its panic carries on here.
        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or_else(|payload| panic::resume_unwind(payload))
            })
            .collect()
    });
    internal_merge_sort(&res.iter().collect::<Vec<_>>())
}

/// Merges sorted lists into one without duplicates.
fn internal_merge_sort(lists: &[&List]) -> List {
    let sz = lists.iter().map(|l| l.ids().len()).sum();
    internal_merge_sort_with_buffer(lists, Vec::with_capacity(sz))
}

fn internal_merge_sort_with_buffer(lists: &[&List], mut output: Vec<SystemId>) -> List {
    debug_assert!(
        lists.iter().all(|l| l.ids().is_sorted()),
        "every list is sorted"
    );
    output.clear();
    if lists.is_empty() {
        return List::new(output);
    }

    let elems = lists
        .iter()
        .enumerate()
        .filter_map(|(i, l)| l.ids().first().map(|&val| Elem::new(val, i)))
        .collect();
    let mut h = IdHeap::new(elems);

    let mut idx = vec![0usize; lists.len()];

    // Each pass takes an ID out of one list or removes an exhausted list, so the loop ends after every ID.
    while let Some((val, list_idx)) = h.get(0).map(|me| (me.val(), me.list_idx())) {
        if output.last() != Some(&val) {
            output.push(val);
        }
        let l = lists[list_idx].ids();
        // The heap's smallest element is the one removed or replaced.
        if idx[list_idx] >= l.len() - 1 {
            let _exhausted: Option<Elem> = h.pop();
        } else {
            idx[list_idx] += 1;
            let _advanced: Option<Elem> = h.set(0, Elem::new(l[idx[list_idx]], list_idx));
            h.fix(0);
        }
    }

    debug_assert!(output.is_sorted(), "a merge of sorted lists is sorted");
    debug_assert!(
        output.windows(2).all(|w| w[0] != w[1]),
        "a merge holds no duplicate"
    );
    List::new(output)
}

const NUM_THREADS: usize = 10;
const NUM_LIST_PER_THREAD: usize = 10;

/// Merges sorted lists into one without duplicates; 100 lists or more are merged in 10 threads first.
#[expect(
    clippy::disallowed_methods,
    reason = "Dgraph's algo merges large inputs on 10 goroutines; query code is outside the storage simulation"
)]
#[must_use]
pub fn merge_sorted(lists: &[&List]) -> List {
    if lists.len() < NUM_THREADS * NUM_LIST_PER_THREAD {
        return internal_merge_sort(lists);
    }

    let chunk_size = lists.len().div_ceil(NUM_THREADS);
    let intermediate: Vec<List> = thread::scope(|s| {
        let handles: Vec<_> = (0..NUM_THREADS)
            .filter_map(|i| {
                let start = i * chunk_size;
                let end = ((i + 1) * chunk_size).min(lists.len());
                if start > lists.len() {
                    return None;
                }
                let chunk = &lists[start..end];
                Some(s.spawn(move || {
                    let cap = chunk.iter().map(|l| l.ids().len()).sum();
                    internal_merge_sort_with_buffer(chunk, Vec::with_capacity(cap))
                }))
            })
            .collect();
        // A merge thread runs only the panic-free merge, so it returns; were it to panic, its panic carries on here.
        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or_else(|payload| panic::resume_unwind(payload))
            })
            .collect()
    });

    // Leave out empty results.
    let valid: Vec<&List> = intermediate
        .iter()
        .filter(|l| !l.ids().is_empty())
        .collect();
    let total = valid.iter().map(|l| l.ids().len()).sum();
    internal_merge_sort_with_buffer(&valid, Vec::with_capacity(total))
}

/// The index of `id` in the sorted list, by binary search.
#[must_use]
pub fn index_of(u: &List, id: SystemId) -> Option<usize> {
    debug_assert!(u.ids().is_sorted(), "the list is sorted");
    let i = u.ids().partition_point(|&x| x < id);
    (u.ids().get(i) == Some(&id)).then_some(i)
}
