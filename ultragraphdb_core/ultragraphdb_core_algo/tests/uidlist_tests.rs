/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, algo/uidlist_test.go. Changed for ultragraphdb: translated to Rust; the benchmarks
 * are left out, TestMergeSortedRandom checks against a sorted, deduplicated reference since the single-threaded
 * merge is private, and the golden-vector test at the end is ultragraphdb's.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};

use serde::Deserialize;
use ultragraphdb_core_algo::{
    apply_filter, difference, index_of, intersect_compressed_with, intersect_compressed_with_bin,
    intersect_compressed_with_lin_jump, intersect_sorted, intersect_with, intersect_with_bin,
    intersect_with_jump, intersect_with_lin, merge_sorted, merge_sorted_more_mem,
};
use ultragraphdb_core_codec::{Decoder, Encoder, SeekPos, encode};
use ultragraphdb_proto_pb::pb::List;
use ultragraphdb_test_bdd::{load_golden, testdata_dir};

/// Go's math/rand in the tests, as SplitMix64 with a fixed seed so that a failure reproduces.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

fn new_list(data: &[u64]) -> List {
    List {
        uids: data.to_vec(),
    }
}

fn merged(input: &[List]) -> Vec<u64> {
    merge_sorted(&input.iter().collect::<Vec<_>>()).uids
}

fn intersected(input: &[List]) -> Vec<u64> {
    intersect_sorted(&input.iter().collect::<Vec<_>>()).uids
}

/// Go's `IntersectWith(u, v, u)`: the result replaces `u`.
fn intersect_into_u(u: &mut List, v: &List) {
    let mut o = List::default();
    intersect_with(u, v, &mut o);
    *u = o;
}

#[test]
fn test_merge_sorted1() {
    let input = [new_list(&[55])];
    assert_eq!(merged(&input), vec![55]);
}

#[test]
fn test_merge_sorted2() {
    let input = [new_list(&[1, 3, 6, 8, 10]), new_list(&[2, 4, 5, 7, 15])];
    assert_eq!(merged(&input), vec![1, 2, 3, 4, 5, 6, 7, 8, 10, 15]);
}

#[test]
fn test_merge_sorted3() {
    let input = [new_list(&[1, 3, 6, 8, 10]), new_list(&[])];
    assert_eq!(merged(&input), vec![1, 3, 6, 8, 10]);
}

#[test]
fn test_merge_sorted4() {
    let input = [new_list(&[]), new_list(&[1, 3, 6, 8, 10])];
    assert_eq!(merged(&input), vec![1, 3, 6, 8, 10]);
}

#[test]
fn test_merge_sorted5() {
    let input = [new_list(&[]), new_list(&[])];
    assert!(merged(&input).is_empty());
}

#[test]
fn test_merge_sorted6() {
    let input = [
        new_list(&[11, 13, 16, 18, 20]),
        new_list(&[12, 14, 15, 15, 16, 16, 17, 25]),
        new_list(&[1, 2]),
    ];
    assert_eq!(
        merged(&input),
        vec![1, 2, 11, 12, 13, 14, 15, 16, 17, 18, 20, 25]
    );
}

#[test]
fn test_merge_sorted_random() {
    let mut rng = Rng(1);
    let mut create_list = |n: usize| {
        let mut list: Vec<u64> = (0..n).map(|_| rng.next_u64() >> 1).collect();
        list.sort_unstable();
        List { uids: list }
    };

    let input: Vec<List> = (0..1000).map(|_| create_list(1000)).collect();
    let refs: Vec<&List> = input.iter().collect();

    let mut want: Vec<u64> = input.iter().flat_map(|l| l.uids.iter().copied()).collect();
    want.sort_unstable();
    want.dedup();
    assert_eq!(merge_sorted(&refs).uids, want);
    assert_eq!(merge_sorted_more_mem(&refs).uids, want);
}

#[test]
fn test_merge_sorted7() {
    let input = [
        new_list(&[5, 6, 7]),
        new_list(&[3, 4]),
        new_list(&[1, 2]),
        new_list(&[]),
    ];
    assert_eq!(merged(&input), vec![1, 2, 3, 4, 5, 6, 7]);
}

#[test]
fn test_merge_sorted8() {
    let input: [List; 0] = [];
    assert!(merged(&input).is_empty());
}

#[test]
fn test_merge_sorted9() {
    let input = [new_list(&[1, 1, 1])];
    assert_eq!(merged(&input), vec![1]);
}

#[test]
fn test_merge_sorted10() {
    let input = [new_list(&[1, 2, 3, 3, 6]), new_list(&[4, 8, 9])];
    assert_eq!(merged(&input), vec![1, 2, 3, 4, 6, 8, 9]);
}

#[test]
fn test_intersect_sorted1() {
    let input = [new_list(&[1, 2, 3]), new_list(&[2, 3, 4, 5])];
    assert_eq!(vec![2, 3], intersected(&input));
}

#[test]
fn test_intersect_sorted2() {
    let input = [new_list(&[1, 2, 3])];
    assert_eq!(intersected(&input), vec![1, 2, 3]);
}

#[test]
fn test_intersect_sorted3() {
    let input: [List; 0] = [];
    assert!(intersected(&input).is_empty());
}

#[test]
fn test_intersect_sorted4() {
    let input = [new_list(&[100, 101])];
    assert_eq!(intersected(&input), vec![100, 101]);
}

#[test]
fn test_intersect_sorted5() {
    let input = [
        new_list(&[1, 2, 3]),
        new_list(&[2, 3, 4, 5]),
        new_list(&[4, 5, 6]),
    ];
    assert!(intersected(&input).is_empty());
}

#[test]
fn test_intersect_sorted6() {
    let input = [
        new_list(&[10, 12, 13]),
        new_list(&[2, 3, 4, 13]),
        new_list(&[4, 5, 6]),
    ];
    assert!(intersected(&input).is_empty());
}

#[test]
fn test_diff_sorted1() {
    let output = difference(&new_list(&[1, 2, 3]), &new_list(&[1]));
    assert_eq!(vec![2, 3], output.uids);
}

#[test]
fn test_diff_sorted2() {
    let output = difference(&new_list(&[1, 2, 3]), &new_list(&[2]));
    assert_eq!(vec![1, 3], output.uids);
}

#[test]
fn test_diff_sorted3() {
    let output = difference(&new_list(&[1, 2, 3]), &new_list(&[3]));
    assert_eq!(vec![1, 2], output.uids);
}

#[test]
fn test_diff_sorted4() {
    let output = difference(&new_list(&[1, 2, 3]), &new_list(&[]));
    assert_eq!(vec![1, 2, 3], output.uids);
}

#[test]
fn test_diff_sorted5() {
    let output = difference(&new_list(&[]), &new_list(&[1, 2]));
    assert_eq!(Vec::<u64>::new(), output.uids);
}

#[test]
fn test_sub_sorted1() {
    let output = difference(&new_list(&[1, 2, 3]), &new_list(&[2, 3, 4, 5]));
    assert_eq!(vec![1], output.uids);
}

#[test]
fn test_sub_sorted6() {
    let output = difference(&new_list(&[10, 12, 13]), &new_list(&[2, 3, 4, 13]));
    assert_eq!(vec![10, 12], output.uids);
}

#[test]
fn test_uid_list_intersect1() {
    let mut u = new_list(&[1, 2, 3]);
    let v = new_list(&[]);
    intersect_into_u(&mut u, &v);
    assert!(u.uids.is_empty());
}

#[test]
fn test_uid_list_intersect2() {
    let mut u = new_list(&[1, 2, 3]);
    let v = new_list(&[1, 2, 3, 4, 5]);
    intersect_into_u(&mut u, &v);
    assert_eq!(vec![1, 2, 3], u.uids);
    assert_eq!(vec![1, 2, 3, 4, 5], v.uids);
}

#[test]
fn test_uid_list_intersect3() {
    let mut u = new_list(&[1, 2, 3]);
    let v = new_list(&[2]);
    intersect_into_u(&mut u, &v);
    assert_eq!(vec![2], u.uids);
    assert_eq!(vec![2], v.uids);
}

#[test]
fn test_uid_list_intersect4() {
    let mut u = new_list(&[1, 2, 3]);
    let v = new_list(&[0, 5]);
    intersect_into_u(&mut u, &v);
    assert!(u.uids.is_empty());
    assert_eq!(vec![0, 5], v.uids);
}

#[test]
fn test_uid_list_intersect5() {
    let mut u = new_list(&[1, 2, 3]);
    let v = new_list(&[3, 5]);
    intersect_into_u(&mut u, &v);
    assert_eq!(vec![3], u.uids);
}

#[test]
fn test_uid_list_intersect6() {
    let (common, other, _) = fill_nums(&mut Rng(6), 10, 500);
    let mut u = new_list(&common);
    let v = new_list(&other);
    intersect_into_u(&mut u, &v);
    assert_eq!(common, u.uids);
}

#[test]
fn test_uid_list_intersect7() {
    let (common, other, _) = fill_nums(&mut Rng(7), 10, 2500);
    let mut u = new_list(&common);
    let v = new_list(&other);
    intersect_into_u(&mut u, &v);
    assert_eq!(common, u.uids);
}

#[test]
fn test_uid_list_intersect8() {
    let (common, other, _) = fill_nums(&mut Rng(8), 10, 20000);
    let u = new_list(&common);
    let v = new_list(&other);
    let mut o = new_list(&[]);
    intersect_with(&u, &v, &mut o);
    assert_eq!(common, u.uids);
    assert_eq!(common, o.uids);
}

#[test]
fn test_uid_list_intersect_dup_first() {
    let mut u = new_list(&[1, 1, 2, 3]);
    let v = new_list(&[1, 2]);
    intersect_into_u(&mut u, &v);
    assert_eq!(vec![1, 2], u.uids);
}

#[test]
fn test_uid_list_intersect_dup_both() {
    let mut u = new_list(&[1, 1, 2, 3, 5]);
    let v = new_list(&[1, 1, 2, 4]);
    intersect_into_u(&mut u, &v);
    assert_eq!(vec![1, 1, 2], u.uids);
}

#[test]
fn test_uid_list_intersect_dup_second() {
    let mut u = new_list(&[1, 2, 3, 5]);
    let v = new_list(&[1, 1, 2, 4]);
    intersect_into_u(&mut u, &v);
    assert_eq!(vec![1, 2], u.uids);
}

#[test]
fn test_apply_filter_uint() {
    let l = [1u64, 2, 3, 4, 5];
    let mut u = new_list(&l);
    apply_filter(&mut u, |_, idx| l[idx] % 2 == 1);
    assert_eq!(vec![1, 3, 5], u.uids);
}

fn fill_nums_diff(
    rng: &mut Rng,
    n1: usize,
    n2: usize,
    n3: usize,
) -> (Vec<u64>, Vec<u64>, Vec<u64>) {
    let mut common_nums = vec![0u64; n1];
    let mut block_nums = vec![0u64; n1 + n2];
    let mut other_nums = vec![0u64; n1 + n3];
    let mut all_c = HashSet::new();

    for i in 0..n1 {
        let val = rng.next_u64() % 1000;
        common_nums[i] = val;
        block_nums[i] = val;
        other_nums[i] = val;
        all_c.insert(val);
    }

    for item in block_nums.iter_mut().skip(n1) {
        let val = rng.next_u64() % 1000;
        *item = val;
        all_c.insert(val);
    }

    for item in other_nums.iter_mut().skip(n1) {
        let mut val = rng.next_u64() % 1000;
        while all_c.contains(&val) {
            val = rng.next_u64() % 1000;
        }
        *item = val;
    }

    common_nums.sort_unstable();
    block_nums.sort_unstable();
    other_nums.sort_unstable();

    (common_nums, block_nums, other_nums)
}

fn fill_nums(rng: &mut Rng, n1: usize, n2: usize) -> (Vec<u64>, Vec<u64>, Vec<u64>) {
    let mut common_nums = vec![0u64; n1];
    let mut block_nums = vec![0u64; n1 + n2];
    let mut other_nums = vec![0u64; n1 + n2];

    for i in 0..n1 {
        let val = rng.next_u64();
        common_nums[i] = val;
        block_nums[i] = val;
        other_nums[i] = val;
    }

    for i in n1..n1 + n2 {
        block_nums[i] = rng.next_u64();
        other_nums[i] = rng.next_u64();
    }

    common_nums.sort_unstable();
    block_nums.sort_unstable();
    other_nums.sort_unstable();

    (common_nums, block_nums, other_nums)
}

fn block_pack(nums: &[u64], block_size: usize) -> Option<ultragraphdb_proto_pb::pb::UidPack> {
    let mut enc = Encoder::new(block_size);
    for &num in nums {
        enc.add(num);
    }
    enc.done()
}

#[test]
fn test_intersect_compressed_with_lin_jump() {
    let lengths = [0usize, 1, 3, 11, 100];
    let mut rng = Rng(9);

    for &n1 in &lengths {
        for &n2 in &lengths {
            // Intersection of blockNums and otherNums is commonNums.
            let (common_nums, block_nums, other_nums) = fill_nums(&mut rng, n1, n2);

            let pack = block_pack(&block_nums, 10).unwrap_or_default();
            let mut dec = Decoder::unpositioned(&pack);
            dec.seek(0, SeekPos::Start);

            let mut actual = Vec::new();
            intersect_compressed_with_lin_jump(&mut dec, &other_nums, &mut actual);
            assert_eq!(common_nums, actual, "n1={n1} n2={n2}");
        }
    }
}

#[test]
fn test_intersect_compressed_with_bin() {
    let mut rng = Rng(10);
    // Go loops over the one-element tables {11} and {3}.
    let (n1, n2) = (11usize, 3usize);
    // Intersection of blockNums and otherNums is commonNums.
    let (common_nums, block_nums, other_nums) = fill_nums_diff(&mut rng, n1 / 10, n1, n2);

    let pack = block_pack(&block_nums, 10).expect("the list is not empty");
    let mut dec = Decoder::unpositioned(&pack);
    dec.seek(0, SeekPos::Start);

    let mut actual = Vec::new();
    intersect_compressed_with_bin(&mut dec, &other_nums, &mut actual);
    assert_eq!(common_nums, actual);
}

#[test]
fn test_intersect_compressed_with_bin_missing_size() {
    let lengths = [0usize, 1, 3, 11, 100, 500, 1000];
    let mut rng = Rng(11);

    for &n1 in &lengths {
        for &n2 in &lengths {
            // Intersection of blockNums and otherNums is commonNums.
            let (common_nums, block_nums, other_nums) = fill_nums(&mut rng, n1, n2);

            // Set the block size to 0 to verify that the method still works in this case.
            let pack = block_pack(&block_nums, 0).unwrap_or_default();
            let mut dec = Decoder::unpositioned(&pack);
            dec.seek(0, SeekPos::Start);

            let mut actual = Vec::new();
            intersect_compressed_with_bin(&mut dec, &other_nums, &mut actual);
            assert_eq!(common_nums, actual, "n1={n1} n2={n2}");
        }
    }
}

#[test]
fn test_index_of() {
    let u = new_list(&[2, 4, 6, 8]);
    assert_eq!(Some(0), index_of(&u, 2));
    assert_eq!(Some(3), index_of(&u, 8));
    assert_eq!(None, index_of(&u, 5));
    assert_eq!(None, index_of(&u, 9));
    assert_eq!(None, index_of(&new_list(&[]), 0));
}

#[derive(Debug, Clone, Deserialize)]
struct OpResult {
    op: String,
    arg: u64,
    out: Vec<u64>,
    index: i64,
    i: usize,
    k: usize,
    panics: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct AlgoCase {
    name: String,
    block_size: usize,
    lists: Vec<Vec<u64>>,
    results: Vec<OpResult>,
}

/// One plain-list operation of the golden file on a case's lists: its output and, for the pairwise intersections,
/// the offsets it returned. `None` for an operation of the packed tests.
fn run_plain(case: &AlgoCase, r: &OpResult) -> Option<(Vec<u64>, usize, usize)> {
    let lists: Vec<List> = case.lists.iter().map(|l| new_list(l)).collect();
    let refs: Vec<&List> = lists.iter().collect();
    let pair = || (&case.lists[0], &case.lists[1]);
    let mut out = Vec::new();
    let (mut i, mut k) = (0, 0);
    match r.op.as_str() {
        "intersect_with" => {
            let mut o = List::default();
            intersect_with(refs[0], refs[1], &mut o);
            out = o.uids;
        }
        "intersect_with_lin" => (i, k) = intersect_with_lin(pair().0, pair().1, &mut out),
        "intersect_with_jump" => (i, k) = intersect_with_jump(pair().0, pair().1, &mut out),
        "intersect_with_bin" => k = intersect_with_bin(pair().0, pair().1, &mut out),
        "difference" => out = difference(refs[0], refs[1]).uids,
        "intersect_compressed_with" => {
            let pack = encode(pair().0, case.block_size);
            let mut o = List::default();
            intersect_compressed_with(pack.as_ref(), r.arg, refs[1], &mut o);
            out = o.uids;
        }
        "intersect_compressed_with_lin_jump" | "intersect_compressed_with_bin" => {
            let pack = encode(pair().0, case.block_size).expect("the golden list is not empty");
            let mut dec = Decoder::unpositioned(&pack);
            dec.seek(0, SeekPos::Start);
            if r.op == "intersect_compressed_with_bin" {
                intersect_compressed_with_bin(&mut dec, pair().1, &mut out);
            } else {
                intersect_compressed_with_lin_jump(&mut dec, pair().1, &mut out);
            }
        }
        "merge_sorted" => out = merge_sorted(&refs).uids,
        "merge_sorted_more_mem" => out = merge_sorted_more_mem(&refs).uids,
        "intersect_sorted" => out = intersect_sorted(&refs).uids,
        "index_of" => {
            let index = index_of(refs[0], r.arg).map_or(-1, |i| i as i64);
            assert_eq!(r.index, index, "{} index_of {}", case.name, r.arg);
        }
        _ => return None,
    }
    Some((out, i, k))
}

/// The plain-list operations' results equal Dgraph's on the golden inputs, duplicates included, where the
/// strategies differ; where Dgraph panics, the port panics too.
#[test]
fn plain_operations_match_golden() {
    let path = testdata_dir().join("golden/algo_setops.json");
    let golden = load_golden::<AlgoCase>(&path);
    let (mut checked, mut panicked) = (0, 0);
    for case in golden.cases() {
        for r in &case.results {
            let label = format!("{} {} {}", case.name, r.op, r.arg);
            match catch_unwind(AssertUnwindSafe(|| run_plain(case, r))) {
                Err(_) => {
                    assert!(r.panics, "{label}: the port panicked, Dgraph did not");
                    panicked += 1;
                }
                Ok(None) => {}
                Ok(Some((out, i, k))) => {
                    assert!(!r.panics, "{label}: Dgraph panicked, the port did not");
                    assert_eq!(r.out, out, "{label}");
                    assert_eq!((r.i, r.k), (i, k), "{label}: offsets");
                    checked += 1;
                }
            }
        }
    }
    assert!(
        checked > 800,
        "the golden file holds plain results, {checked} checked"
    );
    assert!(
        panicked > 0,
        "the golden file holds the inputs Dgraph panics on"
    );
}
