/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, algo/uidlist.go. Changed for ultragraphdb: translated to Rust; goroutines are scoped
 * threads, the parallel merge gives each thread a buffer of its own rather than a slice of one shared buffer, and
 * index-of returns None where Go returns -1. ToUintsListForTest is not ported.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::thread;

use ultragraphdb_core_codec::{Decoder, SeekPos, exact_len};
use ultragraphdb_proto_pb::pb::{List, UidPack};

use crate::types::elem::Elem;
use crate::types::uint64_heap::Uint64Heap;

const JUMP: usize = 32; // Jump size in InsersectWithJump.
const LIN_VS_BIN_RATIO: usize = 10; // When is linear search better than binary

/// Keeps the UIDs of `u` for which `f(uid, index)` holds, `index` counting from 0 in the original list.
pub fn apply_filter(u: &mut List, mut f: impl FnMut(u64, usize) -> bool) {
    let mut i = 0;
    u.uids.retain(|&uid| {
        let keep = f(uid, i);
        i += 1;
        keep
    });
}

/// Intersects the packed list's UIDs from `after_uid` on with `v`, writing the result to `o`. Leaves `o` unchanged
/// when `pack` is `None`.
///
/// # Panics
///
/// As Dgraph's does, when `after_uid` lies past the pack's last UID and the binary strategy is chosen, which
/// [`Decoder::seek_to_block`] then refuses.
pub fn intersect_compressed_with(pack: Option<&UidPack>, after_uid: u64, v: &List, o: &mut List) {
    let Some(pack) = pack else {
        return;
    };
    let mut dec = Decoder::unpositioned(pack);
    dec.seek(after_uid, SeekPos::Start);
    let mut n = dec.approx_len();
    let mut m = v.uids.len();

    if n > m {
        (n, m) = (m, n);
    }
    o.uids.clear();

    // If n equals 0, set it to 1 to avoid division by zero.
    if n == 0 {
        n = 1;
    }

    // Select appropriate function based on heuristics.
    let ratio = m as f64 / n as f64;
    if ratio < LIN_VS_BIN_RATIO as f64 {
        intersect_compressed_with_lin_jump(&mut dec, &v.uids, &mut o.uids);
    } else {
        intersect_compressed_with_bin(&mut dec, &v.uids, &mut o.uids);
    }
}

/// Intersects the decoder's UIDs from its current block on with `v` linearly, appending to `o`.
pub fn intersect_compressed_with_lin_jump(dec: &mut Decoder, v: &[u64], o: &mut Vec<u64>) {
    let m = v.len();
    let mut k = 0;
    let (_, off) = intersect_with_lin(dec.uids(), &v[k..], o);
    k += off;

    while k < m {
        let u = dec.linear_seek(v[k]);
        if u.is_empty() {
            break;
        }
        let (_, mut off) = intersect_with_lin(u, &v[k..], o);
        if off == 0 {
            off = 1; // If v[k] isn't in u, move forward.
        }

        k += off;
    }
}

/// Intersects the decoder's UIDs with `q` by binary search over the shorter list, appending to `o`. The decoder
/// must have been sought. Based on the paper "Fast Intersection Algorithms for Sorted Sequences",
/// <https://link.springer.com/chapter/10.1007/978-3-642-12476-1_3>.
pub fn intersect_compressed_with_bin(dec: &mut Decoder, q: &[u64], o: &mut Vec<u64>) {
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
        loop {
            let block_uids = dec.uids();
            if block_uids.is_empty() {
                break;
            }
            let (_, off) = intersect_with_jump(block_uids, q, o);
            q = &q[off..];
            if q.is_empty() {
                return;
            }
            dec.next_block();
        }
        return;
    }

    let mut uids = dec.uids();
    let mut qidx = 0;
    loop {
        if qidx >= q.len() {
            return;
        }
        let u = q[qidx];
        if uids.last().is_none_or(|&last| u > last) {
            uids = if lq * LIN_VS_BIN_RATIO < ld {
                dec.linear_seek(u)
            } else {
                dec.seek_to_block(u, SeekPos::Current)
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
    let mut n = u.uids.len();
    let mut m = v.uids.len();

    if n > m {
        (n, m) = (m, n);
    }
    o.uids.clear();
    o.uids.reserve(n);
    if n == 0 {
        n = 1;
    }
    // Select appropriate function based on heuristics.
    let ratio = m as f64 / n as f64;
    if ratio < 100.0 {
        intersect_with_lin(&u.uids, &v.uids, &mut o.uids);
    } else if ratio < 500.0 {
        intersect_with_jump(&u.uids, &v.uids, &mut o.uids);
    } else {
        intersect_with_bin(&u.uids, &v.uids, &mut o.uids);
    }
}

/// Intersects `u` and `v` linearly, appending to `o`; returns how far it got into each.
pub fn intersect_with_lin(u: &[u64], v: &[u64], o: &mut Vec<u64>) -> (usize, usize) {
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
pub fn intersect_with_jump(u: &[u64], v: &[u64], o: &mut Vec<u64>) -> (usize, usize) {
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
pub fn intersect_with_bin(d: &[u64], q: &[u64], o: &mut Vec<u64>) -> usize {
    let (d, q) = if d.len() < q.len() { (q, d) } else { (d, q) };
    let (ld, lq) = (d.len(), q.len());
    if ld == 0 || lq == 0 || d[ld - 1] < q[0] || q[lq - 1] < d[0] {
        return 0;
    }

    let val = d[0];
    let minq = q.partition_point(|&x| x < val);

    let val = d[ld - 1];
    let maxq = q.partition_point(|&x| x <= val);

    bin_intersect(d, &q[minq..maxq], o);
    maxq
}

/// The recursion of [`intersect_with_bin`]. `d` must be at least as long as `q`.
fn bin_intersect(d: &[u64], q: &[u64], fin: &mut Vec<u64>) {
    if d.is_empty() || q.is_empty() {
        return;
    }
    let midq = q.len() / 2;
    let qval = q[midq];
    let midd = d.partition_point(|&x| x < qval);

    let (dd, qq) = (&d[..midd], &q[..midq]);
    if dd.len() > qq.len() {
        // D > Q
        bin_intersect(dd, qq, fin);
    } else {
        bin_intersect(qq, dd, fin);
    }

    if midd >= d.len() {
        return;
    }
    // Go steps midd back on a miss and resumes at midd + 1, so the right half starts after a match and at midd
    // otherwise.
    let rest = if d[midd] == qval {
        fin.push(qval);
        midd + 1
    } else {
        midd
    };

    let (dd, qq) = (&d[rest..], &q[midq + 1..]);
    if dd.len() > qq.len() {
        // D > Q
        bin_intersect(dd, qq, fin);
    } else {
        bin_intersect(qq, dd, fin);
    }
}

/// Intersects the lists from the shortest to the longest.
pub fn intersect_sorted(lists: &[&List]) -> List {
    if lists.is_empty() {
        return List::default();
    }
    let mut ls: Vec<&List> = lists.to_vec();
    // Sort the lists based on length.
    ls.sort_by_key(|l| l.uids.len());
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
        if out.uids.is_empty() {
            break;
        }
    }
    out
}

/// The UIDs of `u` not in `v`.
pub fn difference(u: &List, v: &List) -> List {
    let (u, v) = (&u.uids, &v.uids);
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
    List { uids: out }
}

/// Merges sorted lists into one without duplicates, splitting more than 100 lists over 10 threads recursively.
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
                s.spawn(move || merge_sorted_more_mem(chunk))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("a merge thread panicked"))
            .collect()
    });
    merge_sorted_more_mem(&res.iter().collect::<Vec<_>>())
}

/// Merges sorted lists into one without duplicates.
fn internal_merge_sort(lists: &[&List]) -> List {
    let sz = lists.iter().map(|l| l.uids.len()).sum();
    internal_merge_sort_with_buffer(lists, Vec::with_capacity(sz))
}

fn internal_merge_sort_with_buffer(lists: &[&List], mut output: Vec<u64>) -> List {
    output.clear();
    if lists.is_empty() {
        return List { uids: output };
    }

    let elems = lists
        .iter()
        .enumerate()
        .filter_map(|(i, l)| l.uids.first().map(|&val| Elem::new(val, i)))
        .collect();
    let mut h = Uint64Heap::new(elems);

    let mut idx = vec![0usize; lists.len()];
    let mut last = 0u64;

    while let Some(&me) = h.get(0) {
        if output.is_empty() || me.val() != last {
            output.push(me.val());
            last = me.val();
        }
        let l = &lists[me.list_idx()].uids;
        if idx[me.list_idx()] >= l.len() - 1 {
            h.pop();
        } else {
            idx[me.list_idx()] += 1;
            h.set(0, Elem::new(l[idx[me.list_idx()]], me.list_idx()));
            h.fix(0);
        }
    }

    List { uids: output }
}

const NUM_THREADS: usize = 10;
const NUM_LIST_PER_THREAD: usize = 10;

/// Merges sorted lists into one without duplicates; 100 lists or more are merged in 10 threads first.
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
                    let cap = chunk.iter().map(|l| l.uids.len()).sum();
                    internal_merge_sort_with_buffer(chunk, Vec::with_capacity(cap))
                }))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("a merge thread panicked"))
            .collect()
    });

    // Leave out empty results.
    let valid: Vec<&List> = intermediate.iter().filter(|l| !l.uids.is_empty()).collect();
    let total = valid.iter().map(|l| l.uids.len()).sum();
    internal_merge_sort_with_buffer(&valid, Vec::with_capacity(total))
}

/// The index of `uid` in the sorted list, by binary search.
pub fn index_of(u: &List, uid: u64) -> Option<usize> {
    let i = u.uids.partition_point(|&x| x < uid);
    (u.uids.get(i) == Some(&uid)).then_some(i)
}
