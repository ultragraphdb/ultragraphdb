/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, algo/packed_test.go. Changed for ultragraphdb: translated to Rust; the golden-vector
 * test at the end is ultragraphdb's.
 * Copyright (c) 2026. Marvin Hansen.
 */

use serde::Deserialize;
use ultragraphdb_core_algo::{
    apply_filter_packed, difference_packed, index_of_packed, intersect_sorted_packed,
    intersect_with_lin_packed, merge_sorted_packed,
};
use ultragraphdb_core_codec::{Encoder, decode, encode};
use ultragraphdb_proto_pb::pb::UidPack;
use ultragraphdb_test_bdd::{load_golden, testdata_dir};

fn new_uid_pack(data: &[u64]) -> Option<UidPack> {
    // Using a small block size to make sure multiple blocks are used by the tests.
    let mut encoder = Encoder::new(5);
    for &uid in data {
        encoder.add(uid);
    }
    encoder.done()
}

/// Go's `codec.Decode` of a pack that may be nil.
fn decoded(pack: Option<UidPack>) -> Vec<u64> {
    pack.map(|p| decode(&p, 0)).unwrap_or_default()
}

fn refs(packs: &[Option<UidPack>]) -> Vec<Option<&UidPack>> {
    packs.iter().map(Option::as_ref).collect()
}

fn merged(input: &[Option<UidPack>]) -> Vec<u64> {
    decoded(merge_sorted_packed(&refs(input)))
}

fn intersected(input: &[Option<UidPack>]) -> Vec<u64> {
    decoded(intersect_sorted_packed(&refs(input)))
}

fn lin(u: &[u64], v: &[u64]) -> Vec<u64> {
    decoded(intersect_with_lin_packed(
        new_uid_pack(u).as_ref(),
        new_uid_pack(v).as_ref(),
    ))
}

fn diff(u: &[u64], v: &[u64]) -> Vec<u64> {
    decoded(difference_packed(
        new_uid_pack(u).as_ref(),
        new_uid_pack(v).as_ref(),
    ))
}

#[test]
fn test_merge_sorted1_packed() {
    let input = [new_uid_pack(&[55])];
    assert_eq!(vec![55], merged(&input));
}

#[test]
fn test_merge_sorted2_packed() {
    let input = [
        new_uid_pack(&[1, 3, 6, 8, 10]),
        new_uid_pack(&[2, 4, 5, 7, 15]),
    ];
    assert_eq!(vec![1, 3, 6, 8, 10], decoded(input[0].clone()));
    assert_eq!(vec![2, 4, 5, 7, 15], decoded(input[1].clone()));

    assert_eq!(vec![1, 2, 3, 4, 5, 6, 7, 8, 10, 15], merged(&input));
}

#[test]
fn test_merge_sorted3_packed() {
    let input = [new_uid_pack(&[1, 3, 6, 8, 10]), new_uid_pack(&[])];
    assert_eq!(vec![1, 3, 6, 8, 10], merged(&input));
}

#[test]
fn test_merge_sorted4_packed() {
    let input = [new_uid_pack(&[]), new_uid_pack(&[1, 3, 6, 8, 10])];
    assert_eq!(vec![1, 3, 6, 8, 10], merged(&input));
}

#[test]
fn test_merge_sorted5_packed() {
    let input = [new_uid_pack(&[]), new_uid_pack(&[])];
    assert!(merged(&input).is_empty());
}

#[test]
fn test_merge_sorted6_packed() {
    let input = [
        new_uid_pack(&[11, 13, 16, 18, 20]),
        new_uid_pack(&[12, 14, 15, 15, 16, 16, 17, 25]),
        new_uid_pack(&[1, 2]),
    ];
    assert_eq!(
        vec![1, 2, 11, 12, 13, 14, 15, 16, 17, 18, 20, 25],
        merged(&input)
    );
}

#[test]
fn test_merge_sorted7_packed() {
    let input = [
        new_uid_pack(&[5, 6, 7]),
        new_uid_pack(&[3, 4]),
        new_uid_pack(&[1, 2]),
        new_uid_pack(&[]),
    ];
    assert_eq!(vec![1, 2, 3, 4, 5, 6, 7], merged(&input));
}

#[test]
fn test_merge_sorted8_packed() {
    let input: [Option<UidPack>; 0] = [];
    assert!(merged(&input).is_empty());
}

#[test]
fn test_merge_sorted9_packed() {
    let input = [new_uid_pack(&[1, 1, 1])];
    assert_eq!(vec![1], merged(&input));
}

#[test]
fn test_merge_sorted10_packed() {
    let input = [new_uid_pack(&[1, 2, 3, 3, 6]), new_uid_pack(&[4, 8, 9])];
    assert_eq!(vec![1, 2, 3, 4, 6, 8, 9], merged(&input));
}

#[test]
fn test_uid_list_intersect1_packed() {
    assert!(lin(&[1, 2, 3], &[]).is_empty());
}

#[test]
fn test_uid_list_intersect2_packed() {
    assert_eq!(vec![1, 2, 3], lin(&[1, 2, 3], &[1, 2, 3, 4, 5]));
}

#[test]
fn test_uid_list_intersect3_packed() {
    assert_eq!(vec![2], lin(&[1, 2, 3], &[2]));
}

#[test]
fn test_uid_list_intersect4_packed() {
    assert!(lin(&[1, 2, 3], &[0, 5]).is_empty());
}

#[test]
fn test_uid_list_intersect5_packed() {
    assert_eq!(vec![3], lin(&[1, 2, 3], &[3, 5]));
}

#[test]
fn test_uid_list_intersect6_packed() {
    assert_eq!(
        vec![1, 3, 5, 7, 9],
        lin(&[1, 2, 3, 4, 5, 6, 7, 9], &[1, 3, 5, 7, 8, 9])
    );
}

#[test]
fn test_uid_list_intersect_dup_first_packed() {
    assert_eq!(vec![1, 2], lin(&[1, 1, 2, 3], &[1, 2]));
}

#[test]
fn test_uid_list_intersect_dup_both_packed() {
    assert_eq!(vec![1, 1, 2], lin(&[1, 1, 2, 3, 5], &[1, 1, 2, 4]));
}

#[test]
fn test_uid_list_intersect_dup_second_packed() {
    assert_eq!(vec![1, 2], lin(&[1, 2, 3, 5], &[1, 1, 2, 4]));
}

#[test]
fn test_intersect_sorted1_packed() {
    let input = [new_uid_pack(&[1, 2, 3]), new_uid_pack(&[2, 3, 4, 5])];
    assert_eq!(vec![2, 3], intersected(&input));
}

#[test]
fn test_intersect_sorted2_packed() {
    let input = [new_uid_pack(&[1, 2, 3])];
    assert_eq!(vec![1, 2, 3], intersected(&input));
}

#[test]
fn test_intersect_sorted3_packed() {
    let input: [Option<UidPack>; 0] = [];
    assert!(intersected(&input).is_empty());
}

#[test]
fn test_intersect_sorted4_packed() {
    let input = [new_uid_pack(&[100, 101])];
    assert_eq!(vec![100, 101], intersected(&input));
}

#[test]
fn test_intersect_sorted5_packed() {
    let input = [
        new_uid_pack(&[1, 2, 3]),
        new_uid_pack(&[2, 3, 4, 5]),
        new_uid_pack(&[4, 5, 6]),
    ];
    assert!(intersected(&input).is_empty());
}

#[test]
fn test_intersect_sorted6_packed() {
    let input = [
        new_uid_pack(&[10, 12, 13]),
        new_uid_pack(&[2, 3, 4, 13]),
        new_uid_pack(&[4, 5, 6]),
    ];
    assert!(intersected(&input).is_empty());
}

#[test]
fn test_intersect_sorted7_packed() {
    let input: Vec<Option<UidPack>> = (1..=11)
        .rev()
        .map(|n| new_uid_pack(&(1..=n).collect::<Vec<u64>>()))
        .collect();
    assert_eq!(vec![1], intersected(&input));
}

#[test]
fn test_diff_sorted1_packed() {
    assert_eq!(vec![2, 3], diff(&[1, 2, 3], &[1]));
}

#[test]
fn test_diff_sorted2_packed() {
    assert_eq!(vec![1, 3], diff(&[1, 2, 3], &[2]));
}

#[test]
fn test_diff_sorted3_packed() {
    assert_eq!(vec![1, 2], diff(&[1, 2, 3], &[3]));
}

#[test]
fn test_diff_sorted4_packed() {
    assert_eq!(vec![1, 2, 3], diff(&[1, 2, 3], &[]));
}

#[test]
fn test_diff_sorted5_packed() {
    assert_eq!(Vec::<u64>::new(), diff(&[], &[1, 2]));
}

#[test]
fn test_sub_sorted1_packed() {
    assert_eq!(vec![1], diff(&[1, 2, 3], &[2, 3, 4, 5]));
}

#[test]
fn test_sub_sorted6_packed() {
    assert_eq!(vec![10, 12], diff(&[10, 12, 13], &[2, 3, 4, 13]));
}

fn stepped_pack(n: u64) -> Option<UidPack> {
    let mut encoder = Encoder::new(10);
    for i in 0..n {
        encoder.add(i);
    }
    encoder.done()
}

#[test]
fn test_index_of_packed1() {
    let pack = stepped_pack(1000);

    for i in 0..1000 {
        assert_eq!(Some(i as usize), index_of_packed(pack.as_ref(), i));
    }
    assert_eq!(None, index_of_packed(pack.as_ref(), 1000));
}

#[test]
fn test_index_of_packed2() {
    let pack = stepped_pack(100);

    assert_eq!(None, index_of_packed(pack.as_ref(), 100));
    assert_eq!(None, index_of_packed(pack.as_ref(), 101));
    assert_eq!(None, index_of_packed(pack.as_ref(), 1000));
    assert_eq!(None, index_of_packed(pack.as_ref(), u64::MAX));
}

#[test]
fn test_index_of_packed3() {
    assert_eq!(None, index_of_packed(None, 0));
    assert_eq!(None, index_of_packed(None, u64::MAX));
}

#[test]
fn test_apply_filter_uint_packed() {
    let l = [1u64, 2, 3, 4, 5, 6, 7, 8, 9];
    let u = new_uid_pack(&l).expect("the list is not empty");
    let res = apply_filter_packed(&u, |_, idx| l[idx] % 2 == 1);
    assert_eq!(vec![1, 3, 5, 7, 9], decoded(res));
}

#[derive(Debug, Clone, Deserialize)]
struct OpResult {
    op: String,
    arg: u64,
    out: Vec<u64>,
    out_nil: bool,
    index: i64,
    panics: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct AlgoCase {
    name: String,
    block_size: usize,
    lists: Vec<Vec<u64>>,
    results: Vec<OpResult>,
}

/// The golden cases on which Dgraph's packed operation returns other UIDs than its plain counterpart: the case, the
/// packed operation, the ledger entry of `openspec/notes/dgraph-bugs-found-in-port.md`, and the port's expected
/// result, which is the plain operation's result in the same case. Entry 1: IntersectSortedPacked returns the
/// intersection of the two shortest lists; entry 2: DifferencePacked drops `u`'s blocks after `v` runs out; entry 25:
/// a UID of `u` in a later block of `v` survives.
const DEVIATIONS: &[(&str, &str, u32, &str)] = &[
    (
        "lists/k=3/dups=true",
        "intersect_sorted_packed",
        1,
        "intersect_sorted",
    ),
    (
        "lists/shortest-two-share",
        "intersect_sorted_packed",
        1,
        "intersect_sorted",
    ),
    (
        "pair/n=1/m=100/dups=true",
        "difference_packed",
        25,
        "difference",
    ),
    (
        "pair/n=1/m=400/dups=false",
        "difference_packed",
        25,
        "difference",
    ),
    (
        "pair/n=3/m=100/dups=false",
        "difference_packed",
        25,
        "difference",
    ),
    (
        "pair/n=3/m=100/dups=true",
        "difference_packed",
        25,
        "difference",
    ),
    (
        "pair/n=3/m=400/dups=false",
        "difference_packed",
        25,
        "difference",
    ),
    (
        "pair/n=3/m=400/dups=true",
        "difference_packed",
        25,
        "difference",
    ),
    (
        "pair/n=11/m=100/dups=false",
        "difference_packed",
        25,
        "difference",
    ),
    (
        "pair/n=11/m=100/dups=true",
        "difference_packed",
        25,
        "difference",
    ),
    (
        "pair/n=11/m=400/dups=false",
        "difference_packed",
        25,
        "difference",
    ),
    (
        "pair/n=11/m=400/dups=true",
        "difference_packed",
        25,
        "difference",
    ),
    (
        "pair/n=100/m=1/dups=false",
        "difference_packed",
        2,
        "difference",
    ),
    (
        "pair/n=100/m=1/dups=true",
        "difference_packed",
        2,
        "difference",
    ),
    (
        "pair/n=100/m=3/dups=true",
        "difference_packed",
        2,
        "difference",
    ),
    (
        "pair/n=100/m=100/dups=false",
        "difference_packed",
        25,
        "difference",
    ),
    (
        "pair/n=100/m=100/dups=true",
        "difference_packed",
        25,
        "difference",
    ),
    (
        "pair/n=100/m=400/dups=false",
        "difference_packed",
        25,
        "difference",
    ),
    (
        "pair/n=100/m=400/dups=true",
        "difference_packed",
        25,
        "difference",
    ),
    (
        "pair/n=400/m=1/dups=true",
        "difference_packed",
        2,
        "difference",
    ),
    (
        "pair/n=400/m=3/dups=false",
        "difference_packed",
        2,
        "difference",
    ),
    (
        "pair/n=400/m=3/dups=true",
        "difference_packed",
        2,
        "difference",
    ),
    (
        "pair/n=400/m=11/dups=false",
        "difference_packed",
        2,
        "difference",
    ),
    (
        "pair/n=400/m=100/dups=false",
        "difference_packed",
        25,
        "difference",
    ),
    (
        "pair/n=400/m=100/dups=true",
        "difference_packed",
        25,
        "difference",
    ),
    (
        "pair/n=400/m=400/dups=false",
        "difference_packed",
        25,
        "difference",
    ),
    (
        "pair/n=400/m=400/dups=true",
        "difference_packed",
        25,
        "difference",
    ),
    ("ratio/n=10/m=1200", "difference_packed", 25, "difference"),
    ("ratio/n=10/m=3000", "difference_packed", 25, "difference"),
    ("ratio/n=10/m=6000", "difference_packed", 25, "difference"),
    ("blocks/u-outlasts-v", "difference_packed", 2, "difference"),
];

/// The plain operation whose result a packed operation equals.
fn plain_op(packed: &str) -> Option<&'static str> {
    match packed {
        "intersect_with_lin_packed" => Some("intersect_with"),
        "difference_packed" => Some("difference"),
        "merge_sorted_packed" => Some("merge_sorted"),
        "intersect_sorted_packed" => Some("intersect_sorted"),
        _ => None,
    }
}

/// The packed operations' results equal the plain operations' golden results in the same case, which hold duplicates,
/// lists across 2^32 and the inputs where Dgraph's DifferencePacked and IntersectSortedPacked drop or keep UIDs. Dgraph's
/// own packed result equals its plain one in every case but the `DEVIATIONS`; with no lists there is no plain result,
/// and Dgraph's packed one, no pack, is the expected one.
#[test]
fn packed_operations_match_golden() {
    let path = testdata_dir().join("golden/algo_setops.json");
    let golden = load_golden::<AlgoCase>(&path);
    let mut checked = 0;
    let mut deviations_seen = 0;
    for case in golden.cases() {
        let packs: Vec<Option<UidPack>> = case
            .lists
            .iter()
            .map(|l| encode(l, case.block_size))
            .collect();
        let packs = refs(&packs);
        for r in &case.results {
            let actual = match r.op.as_str() {
                "intersect_with_lin_packed" => intersect_with_lin_packed(packs[0], packs[1]),
                "difference_packed" => difference_packed(packs[0], packs[1]),
                "merge_sorted_packed" => merge_sorted_packed(&packs),
                "intersect_sorted_packed" => intersect_sorted_packed(&packs),
                "index_of_packed" => {
                    let want = usize::try_from(r.index).ok();
                    assert_eq!(
                        want,
                        index_of_packed(packs[0], r.arg),
                        "{} {} {}",
                        case.name,
                        r.op,
                        r.arg
                    );
                    checked += 1;
                    continue;
                }
                _ => continue,
            };
            assert!(!r.panics, "{} {}: Dgraph panicked", case.name, r.op);
            let plain_name = plain_op(&r.op).expect("a packed operation has a plain one");
            let plain = case.results.iter().find(|p| p.op == plain_name);
            let deviation = DEVIATIONS
                .iter()
                .find(|&&(name, op, _, _)| name == case.name && op == r.op);
            let want = match plain {
                Some(plain) => {
                    assert!(!plain.panics, "{} {}: Dgraph panicked", case.name, plain.op);
                    if let Some(&(_, _, entry, expected)) = deviation {
                        assert_eq!(expected, plain.op, "{} {}", case.name, r.op);
                        assert_ne!(
                            r.out, plain.out,
                            "{} {}: Dgraph's packed result is listed under entry {entry} but equals the plain one",
                            case.name, r.op
                        );
                        deviations_seen += 1;
                    } else {
                        assert_eq!(
                            r.out, plain.out,
                            "{} {}: Dgraph's packed result differs from the plain one and is not a deviation",
                            case.name, r.op
                        );
                    }
                    &plain.out
                }
                None => {
                    assert!(
                        case.lists.is_empty() && deviation.is_none(),
                        "{} {}: no plain result",
                        case.name,
                        r.op
                    );
                    assert_eq!(
                        r.out_nil,
                        actual.is_none(),
                        "{} {}: nil pack",
                        case.name,
                        r.op
                    );
                    &r.out
                }
            };
            // An encoder that was given no UID returns no pack, so an empty result is no pack.
            assert_eq!(
                want.is_empty(),
                actual.is_none(),
                "{} {}: no pack",
                case.name,
                r.op
            );
            assert_eq!(want, &decoded(actual), "{} {}", case.name, r.op);
            checked += 1;
        }
    }
    assert_eq!(
        DEVIATIONS.len(),
        deviations_seen,
        "every deviation names a golden case"
    );
    assert!(
        checked > 200,
        "the golden file holds packed results, {checked} checked"
    );
}

#[test]
fn merge_sorted_packed_skips_an_empty_block() {
    // A corrupt block with no UIDs between two others: Dgraph indexes the empty block and panics; the port moves on.
    let mut pack = new_uid_pack(&(1..=10).collect::<Vec<_>>()).expect("the list is not empty");
    pack.blocks.push(ultragraphdb_proto_pb::pb::UidBlock {
        base: 11,
        deltas: Vec::new(),
        num_uids: 0,
    });
    pack.blocks.extend(
        new_uid_pack(&(21..=30).collect::<Vec<_>>())
            .expect("the list is not empty")
            .blocks,
    );
    let other = new_uid_pack(&[5, 25]).expect("the list is not empty");
    let want: Vec<u64> = (1..=10).chain(21..=30).collect();
    assert_eq!(
        want,
        decoded(merge_sorted_packed(&[Some(&pack), Some(&other)]))
    );
}
