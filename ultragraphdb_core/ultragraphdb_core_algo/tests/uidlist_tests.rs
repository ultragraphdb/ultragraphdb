/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, algo/uidlist_test.go. Changed for ultragraphdb: translated to Rust over system IDs, Go's
 * numbers n standing for the IDs of type 1, bucket 0 and sequence n + 1, an order-preserving map, so every expected
 * value keeps its Go source; the lists packed hold no duplicates, as the packed format requires. The plain-oracle
 * tests across types and buckets and the golden-vector test at the end are ultragraphdb's.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::{BTreeSet, HashSet};

use serde::Deserialize;
use ultragraphdb_core_algo::{
    apply_filter, difference, index_of, intersect_compressed_with, intersect_compressed_with_bin,
    intersect_compressed_with_lin_jump, intersect_sorted, intersect_with, intersect_with_bin,
    intersect_with_jump, intersect_with_lin, merge_sorted, merge_sorted_more_mem,
};
use ultragraphdb_core_codec::{CodecErrorEnum, Decoder, List, UidPack, encode};
use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_tool_bdd::{Context, TestResult, load_golden, testdata_dir};

/// Go's math/rand in the tests, as SplitMix64 with a fixed seed so that a failure reproduces.
pub struct Rng(pub u64);

impl Rng {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

pub fn id(prefix: u16, bucket: u32, sequence: u64) -> TestResult<SystemId> {
    Ok(SystemId::new(
        TypePrefix::new(prefix)?,
        Bucket::new(bucket)?,
        Sequence::new(sequence)?,
    ))
}

/// The ID Go's number `n` stands for: type 1, bucket 0, sequence `n + 1`.
pub fn idn(n: u64) -> TestResult<SystemId> {
    id(1, 0, n.checked_add(1).context("a number below u64::MAX")?)
}

/// The IDs Go's numbers stand for.
pub fn ids(numbers: &[u64]) -> TestResult<Vec<SystemId>> {
    numbers.iter().map(|&n| idn(n)).collect()
}

/// The numbers IDs of type 1 and bucket 0 stand for.
pub fn nums(ids: &[SystemId]) -> Vec<u64> {
    ids.iter().map(|id| id.sequence().get() - 1).collect()
}

fn new_list(numbers: &[u64]) -> TestResult<List> {
    Ok(List::new(ids(numbers)?))
}

fn merged(input: &[List]) -> Vec<u64> {
    nums(merge_sorted(&input.iter().collect::<Vec<_>>()).ids())
}

fn intersected(input: &[List]) -> Vec<u64> {
    nums(intersect_sorted(&input.iter().collect::<Vec<_>>()).ids())
}

/// Go's `IntersectWith(u, v, u)`: the result replaces `u`.
fn intersect_into_u(u: &mut List, v: &List) {
    let mut o = List::default();
    intersect_with(u, v, &mut o);
    *u = o;
}

#[test]
fn test_merge_sorted() -> TestResult {
    // Go's TestMergeSorted1 to 10 but the random one: (inputs, merge).
    let cases: [(&[&[u64]], &[u64]); 9] = [
        (&[&[55]], &[55]),
        (
            &[&[1, 3, 6, 8, 10], &[2, 4, 5, 7, 15]],
            &[1, 2, 3, 4, 5, 6, 7, 8, 10, 15],
        ),
        (&[&[1, 3, 6, 8, 10], &[]], &[1, 3, 6, 8, 10]),
        (&[&[], &[1, 3, 6, 8, 10]], &[1, 3, 6, 8, 10]),
        (&[&[], &[]], &[]),
        (
            &[
                &[11, 13, 16, 18, 20],
                &[12, 14, 15, 15, 16, 16, 17, 25],
                &[1, 2],
            ],
            &[1, 2, 11, 12, 13, 14, 15, 16, 17, 18, 20, 25],
        ),
        (&[&[5, 6, 7], &[3, 4], &[1, 2], &[]], &[1, 2, 3, 4, 5, 6, 7]),
        (&[&[1, 1, 1]], &[1]),
        (&[&[1, 2, 3, 3, 6], &[4, 8, 9]], &[1, 2, 3, 4, 6, 8, 9]),
    ];
    for (input, want) in cases {
        let lists = input
            .iter()
            .map(|l| new_list(l))
            .collect::<TestResult<Vec<_>>>()?;
        assert_eq!(merged(&lists), want, "{input:?}");
        assert_eq!(
            nums(merge_sorted_more_mem(&lists.iter().collect::<Vec<_>>()).ids()),
            want
        );
    }
    let none: [List; 0] = [];
    assert!(merged(&none).is_empty());
    Ok(())
}

#[test]
fn test_merge_sorted_random() -> TestResult {
    // 1,000 lists merge in threads, each path of both merges taken: Go's 1000 × 1000, at a tenth of the lengths.
    let mut rng = Rng(1);
    let input: Vec<List> = (0..1000)
        .map(|_| {
            let mut list: Vec<u64> = (0..100).map(|_| rng.next_u64() >> 1).collect();
            list.sort_unstable();
            new_list(&list)
        })
        .collect::<TestResult<_>>()?;
    let refs: Vec<&List> = input.iter().collect();
    let want: Vec<SystemId> = input
        .iter()
        .flat_map(|l| l.ids().iter().copied())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    assert_eq!(merge_sorted(&refs).into_ids(), want);
    assert_eq!(merge_sorted_more_mem(&refs).into_ids(), want);
    Ok(())
}

#[test]
fn test_intersect_sorted() -> TestResult {
    let cases: [(&[&[u64]], &[u64]); 5] = [
        (&[&[1, 2, 3], &[2, 3, 4, 5]], &[2, 3]),
        (&[&[1, 2, 3]], &[1, 2, 3]),
        (&[&[100, 101]], &[100, 101]),
        (&[&[1, 2, 3], &[2, 3, 4, 5], &[4, 5, 6]], &[]),
        (&[&[10, 12, 13], &[2, 3, 4, 13], &[4, 5, 6]], &[]),
    ];
    for (input, want) in cases {
        let lists = input
            .iter()
            .map(|l| new_list(l))
            .collect::<TestResult<Vec<_>>>()?;
        assert_eq!(intersected(&lists), want, "{input:?}");
    }
    let none: [List; 0] = [];
    assert!(intersected(&none).is_empty());
    Ok(())
}

#[test]
fn test_difference() -> TestResult {
    // Go's TestDiffSorted1 to 5 and TestSubSorted1 and 6: (u, v, u - v).
    let cases: [(&[u64], &[u64], &[u64]); 7] = [
        (&[1, 2, 3], &[1], &[2, 3]),
        (&[1, 2, 3], &[2], &[1, 3]),
        (&[1, 2, 3], &[3], &[1, 2]),
        (&[1, 2, 3], &[], &[1, 2, 3]),
        (&[], &[1, 2], &[]),
        (&[1, 2, 3], &[2, 3, 4, 5], &[1]),
        (&[10, 12, 13], &[2, 3, 4, 13], &[10, 12]),
    ];
    for (u, v, want) in cases {
        let output = difference(&new_list(u)?, &new_list(v)?);
        assert_eq!(nums(output.ids()), want, "{u:?} - {v:?}");
    }
    Ok(())
}

#[test]
fn test_uid_list_intersect() -> TestResult {
    // Go's TestUIDListIntersect1 to 5 and the duplicates: (u, v, u ∩ v), v unchanged.
    let cases: [(&[u64], &[u64], &[u64]); 8] = [
        (&[1, 2, 3], &[], &[]),
        (&[1, 2, 3], &[1, 2, 3, 4, 5], &[1, 2, 3]),
        (&[1, 2, 3], &[2], &[2]),
        (&[1, 2, 3], &[0, 5], &[]),
        (&[1, 2, 3], &[3, 5], &[3]),
        (&[1, 1, 2, 3], &[1, 2], &[1, 2]),
        (&[1, 1, 2, 3, 5], &[1, 1, 2, 4], &[1, 1, 2]),
        (&[1, 2, 3, 5], &[1, 1, 2, 4], &[1, 2]),
    ];
    for (u_numbers, v_numbers, want) in cases {
        let mut u = new_list(u_numbers)?;
        let v = new_list(v_numbers)?;
        intersect_into_u(&mut u, &v);
        assert_eq!(nums(u.ids()), want, "{u_numbers:?} ∩ {v_numbers:?}");
        assert_eq!(nums(v.ids()), v_numbers);
    }
    Ok(())
}

#[test]
fn test_uid_list_intersect_by_every_strategy() -> TestResult {
    // Go's TestUIDListIntersect6 to 8: ratios of 50, 250 and 2,000 choose the linear, jumping and binary strategy.
    for (seed, n2) in [(6u64, 500usize), (7, 2500), (8, 20_000)] {
        let (common, other, _) = fill_nums(&mut Rng(seed), 10, n2);
        let u = new_list(&common)?;
        let v = new_list(&other)?;
        let mut o = List::default();
        intersect_with(&u, &v, &mut o);
        assert_eq!(nums(o.ids()), common, "seed {seed}");
        assert_eq!(nums(u.ids()), common);
    }
    Ok(())
}

#[test]
fn test_apply_filter_uint() -> TestResult {
    let l = [1u64, 2, 3, 4, 5];
    let mut u = new_list(&l)?;
    apply_filter(&mut u, |_, idx| l.get(idx).is_some_and(|n| n % 2 == 1));
    assert_eq!(vec![1, 3, 5], nums(u.ids()));
    let mut by_id = new_list(&l)?;
    let three = idn(3)?;
    apply_filter(&mut by_id, |id, _| id != three);
    assert_eq!(vec![1, 2, 4, 5], nums(by_id.ids()));
    Ok(())
}

/// Go's fillNumsDiff: `n1` numbers below 1,000 in all three lists, `n2` more in the second and `n3` in the third,
/// none of them in the first two; the second holds each number once here, as a pack must.
fn fill_nums_diff(
    rng: &mut Rng,
    n1: usize,
    n2: usize,
    n3: usize,
) -> (Vec<u64>, Vec<u64>, Vec<u64>) {
    let mut common_nums = Vec::with_capacity(n1);
    let mut block_nums = BTreeSet::new();
    let mut other_nums = Vec::with_capacity(n1 + n3);
    let mut all_c = HashSet::new();
    for _ in 0..n1 {
        let val = rng.next_u64() % 1000;
        common_nums.push(val);
        other_nums.push(val);
        // Values may repeat: the sets hold each once, whether or not it is new.
        let _new_in_block: bool = block_nums.insert(val);
        let _new: bool = all_c.insert(val);
    }
    for _ in 0..n2 {
        let val = rng.next_u64() % 1000;
        let _new_in_block: bool = block_nums.insert(val);
        let _new: bool = all_c.insert(val);
    }
    for _ in 0..n3 {
        let mut val = rng.next_u64() % 1000;
        while all_c.contains(&val) {
            val = rng.next_u64() % 1000;
        }
        other_nums.push(val);
    }
    common_nums.sort_unstable();
    common_nums.dedup();
    other_nums.sort_unstable();
    (common_nums, block_nums.into_iter().collect(), other_nums)
}

/// Go's fillNums: `n1` numbers in all three lists and `n2` more in each of the second and third; below 2^63, so each
/// stands for an ID.
fn fill_nums(rng: &mut Rng, n1: usize, n2: usize) -> (Vec<u64>, Vec<u64>, Vec<u64>) {
    let mut common_nums: Vec<u64> = (0..n1).map(|_| rng.next_u64() >> 1).collect();
    let mut block_nums = common_nums.clone();
    let mut other_nums = common_nums.clone();
    for _ in 0..n2 {
        block_nums.push(rng.next_u64() >> 1);
        other_nums.push(rng.next_u64() >> 1);
    }
    common_nums.sort_unstable();
    block_nums.sort_unstable();
    other_nums.sort_unstable();
    (common_nums, block_nums, other_nums)
}

fn block_pack(numbers: &[u64], block_size: usize) -> TestResult<Option<UidPack>> {
    Ok(encode(&ids(numbers)?, block_size)?)
}

#[test]
fn test_intersect_compressed_with_lin_jump() -> TestResult {
    let lengths = [0usize, 1, 3, 11, 100];
    let mut rng = Rng(9);
    for &n1 in &lengths {
        for &n2 in &lengths {
            // The intersection of block_nums and other_nums is common_nums.
            let (common_nums, block_nums, other_nums) = fill_nums(&mut rng, n1, n2);
            let pack = block_pack(&block_nums, 10)?.unwrap_or_default();
            let mut dec = Decoder::new(&pack);
            let mut actual = Vec::new();
            intersect_compressed_with_lin_jump(&mut dec, &ids(&other_nums)?, &mut actual);
            assert_eq!(common_nums, nums(&actual), "n1={n1} n2={n2}");
        }
    }
    Ok(())
}

#[test]
fn test_intersect_compressed_with_lin_jump_past_the_end_finds_nothing() -> TestResult {
    // A decoder sought past the pack's last ID holds nothing, and a linear seek from there finds no block.
    let pack = block_pack(&[1, 2, 3], 2)?.context("a pack")?;
    let mut dec = Decoder::sought(&pack, u128::MAX, ultragraphdb_core_codec::SeekPos::Start);
    let mut actual = Vec::new();
    intersect_compressed_with_lin_jump(&mut dec, &ids(&[1, 2, 3])?, &mut actual);
    assert!(actual.is_empty(), "{actual:?}");
    Ok(())
}

#[test]
fn test_intersect_compressed_with_bin() -> TestResult {
    let mut rng = Rng(10);
    // Go loops over the one-element tables {11} and {3}.
    let (n1, n2) = (11usize, 3usize);
    let (common_nums, block_nums, other_nums) = fill_nums_diff(&mut rng, n1 / 10, n1, n2);
    let pack = block_pack(&block_nums, 10)?.context("the list is not empty")?;
    let mut dec = Decoder::new(&pack);
    let mut actual = Vec::new();
    intersect_compressed_with_bin(&mut dec, &ids(&other_nums)?, &mut actual);
    assert_eq!(common_nums, nums(&actual));
    Ok(())
}

#[test]
fn test_intersect_compressed_with_bin_missing_size() -> TestResult {
    let lengths = [0usize, 1, 3, 11, 100, 500, 1000];
    let mut rng = Rng(11);
    for &n1 in &lengths {
        for &n2 in &lengths {
            let (common_nums, block_nums, other_nums) = fill_nums(&mut rng, n1, n2);
            // Block size 0, to check that the method still works in this case.
            let pack = block_pack(&block_nums, 0)?.unwrap_or_default();
            let mut dec = Decoder::new(&pack);
            let mut actual = Vec::new();
            intersect_compressed_with_bin(&mut dec, &ids(&other_nums)?, &mut actual);
            assert_eq!(common_nums, nums(&actual), "n1={n1} n2={n2}");
        }
    }
    Ok(())
}

#[test]
fn test_index_of() -> TestResult {
    let u = new_list(&[2, 4, 6, 8])?;
    assert_eq!(Some(0), index_of(&u, idn(2)?));
    assert_eq!(Some(3), index_of(&u, idn(8)?));
    assert_eq!(None, index_of(&u, idn(5)?));
    assert_eq!(None, index_of(&u, idn(9)?));
    assert_eq!(None, index_of(&List::default(), idn(0)?));
    Ok(())
}

/// Lists across types and buckets: IDs whose type, sequence and bucket each decide an order somewhere.
fn spread_lists() -> TestResult<Vec<Vec<SystemId>>> {
    let mut rng = Rng(12);
    (0..6)
        .map(|n| {
            let mut set = BTreeSet::new();
            for _ in 0..(n * 37) {
                let r = rng.next_u64();
                let prefix = 1 + u16::from(r.is_multiple_of(3));
                let bucket = u32::from(u8::try_from(r >> 56).context("a byte")?);
                let _new: bool = set.insert(id(prefix, bucket % 4, (r >> 8) % 60 + 1)?);
            }
            Ok(set.into_iter().collect())
        })
        .collect()
}

#[test]
fn test_operations_across_types_and_buckets_match_a_plain_oracle() -> TestResult {
    let lists = spread_lists()?;
    let plain: Vec<List> = lists.iter().cloned().map(List::new).collect();
    let sets: Vec<BTreeSet<SystemId>> = lists.iter().map(|l| l.iter().copied().collect()).collect();
    for (i, (u, su)) in plain.iter().zip(&sets).enumerate() {
        for (v, sv) in plain.iter().zip(&sets) {
            let both: Vec<SystemId> = su.intersection(sv).copied().collect();
            let mut o = List::default();
            intersect_with(u, v, &mut o);
            assert_eq!(o.ids(), both, "list {i}");
            let mut lin = Vec::new();
            let _reached: (usize, usize) = intersect_with_lin(u.ids(), v.ids(), &mut lin);
            assert_eq!(lin, both);
            let mut jump = Vec::new();
            let _jumped: (usize, usize) = intersect_with_jump(u.ids(), v.ids(), &mut jump);
            assert_eq!(jump, both);
            let mut bin = Vec::new();
            let _moved: usize = intersect_with_bin(u.ids(), v.ids(), &mut bin);
            assert_eq!(bin, both);
            let only: Vec<SystemId> = su.difference(sv).copied().collect();
            assert_eq!(difference(u, v).ids(), only);
            let union: Vec<SystemId> = su.union(sv).copied().collect();
            assert_eq!(merge_sorted(&[u, v]).ids(), union);
            let packed = encode(u.ids(), 8)?;
            let mut mixed = List::default();
            intersect_compressed_with(packed.as_ref(), 0, v, &mut mixed);
            assert_eq!(mixed.ids(), if packed.is_some() { &both[..] } else { &[] });
        }
        for (k, id) in u.ids().iter().enumerate() {
            assert_eq!(index_of(u, *id), Some(k));
        }
    }
    Ok(())
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

/// What one plain-list operation of the golden file gave on a case's lists.
enum Ran {
    /// Its output and, for the pairwise intersections, the offsets it returned.
    Out(Vec<u64>, usize, usize),
    /// An index-of, checked in place.
    Checked,
    /// An operation of the packed tests.
    Packed,
    /// An operation that packs a list holding duplicates, which the packed format refuses: retired with Dgraph's
    /// format.
    Retired,
}

/// The first list of `case` packed, or why it cannot be.
fn pack_first(case: &AlgoCase) -> TestResult<Result<Option<UidPack>, CodecErrorEnum>> {
    Ok(encode(
        &ids(case.lists.first().context("a list")?)?,
        case.block_size,
    )
    .map_err(|e| e.0))
}

fn run_plain(case: &AlgoCase, r: &OpResult) -> TestResult<Ran> {
    let lists: Vec<List> = case
        .lists
        .iter()
        .map(|l| new_list(l))
        .collect::<TestResult<_>>()?;
    let refs: Vec<&List> = lists.iter().collect();
    let pair = || -> TestResult<(&List, &List)> {
        Ok((*refs.first().context("u")?, *refs.get(1).context("v")?))
    };
    let mut out = Vec::new();
    let (mut i, mut k) = (0, 0);
    match r.op.as_str() {
        "intersect_with" => {
            let mut o = List::default();
            intersect_with(pair()?.0, pair()?.1, &mut o);
            out = o.into_ids();
        }
        "intersect_with_lin" => {
            (i, k) = intersect_with_lin(pair()?.0.ids(), pair()?.1.ids(), &mut out)
        }
        "intersect_with_jump" => {
            (i, k) = intersect_with_jump(pair()?.0.ids(), pair()?.1.ids(), &mut out)
        }
        "intersect_with_bin" => k = intersect_with_bin(pair()?.0.ids(), pair()?.1.ids(), &mut out),
        "difference" => out = difference(pair()?.0, pair()?.1).into_ids(),
        "intersect_compressed_with" => {
            let Ok(pack) = pack_first(case)? else {
                return Ok(Ran::Retired);
            };
            let mut o = List::default();
            intersect_compressed_with(pack.as_ref(), idn(r.arg)?.to_u128(), pair()?.1, &mut o);
            out = o.into_ids();
        }
        "intersect_compressed_with_lin_jump" | "intersect_compressed_with_bin" => {
            let Ok(pack) = pack_first(case)? else {
                return Ok(Ran::Retired);
            };
            let pack = pack.context("the golden list is not empty")?;
            let mut dec = Decoder::new(&pack);
            if r.op == "intersect_compressed_with_bin" {
                intersect_compressed_with_bin(&mut dec, pair()?.1.ids(), &mut out);
            } else {
                intersect_compressed_with_lin_jump(&mut dec, pair()?.1.ids(), &mut out);
            }
        }
        "merge_sorted" => out = merge_sorted(&refs).into_ids(),
        "merge_sorted_more_mem" => out = merge_sorted_more_mem(&refs).into_ids(),
        "intersect_sorted" => out = intersect_sorted(&refs).into_ids(),
        "index_of" => {
            let u = *refs.first().context("u")?;
            let index = index_of(u, idn(r.arg)?).map_or(Ok(-1), i64::try_from)?;
            assert_eq!(r.index, index, "{} index_of {}", case.name, r.arg);
            return Ok(Ran::Checked);
        }
        _ => return Ok(Ran::Packed),
    }
    Ok(Ran::Out(nums(&out), i, k))
}

/// A golden case where the port differs from Dgraph: its name, operation and argument, the ledger entry, the port's
/// result, and whether Dgraph panics.
type Deviation = (&'static str, &'static str, u64, u32, &'static [u64], bool);

/// The golden cases where the port differs from Dgraph: the case, its operation and argument, the ledger entry
/// (`openspec/notes/archive/dgraph-bugs-found-in-port.md`), the port's result, and whether Dgraph panics. With
/// `after_uid` past the pack's last UID and the binary strategy chosen, Dgraph indexes past the last block and panics
/// (ledger 3), unless the plain list's first UID is 0: `SeekToBlock(0)` rewinds to the first block, and the
/// intersection runs over the whole pack. No system ID is 0, so the port gives its ledger-3 answer, nothing, in all.
/// The two siblings of the first on lists with duplicates are retired with those lists.
const DEVIATIONS: &[Deviation] = &[
    (
        "pair/n=400/m=400/dups=false",
        "intersect_compressed_with",
        1600,
        3,
        &[],
        true,
    ),
    (
        "pair/n=100/m=11/dups=false",
        "intersect_compressed_with",
        219,
        3,
        &[],
        false,
    ),
    (
        "pair/n=100/m=100/dups=false",
        "intersect_compressed_with",
        397,
        3,
        &[],
        false,
    ),
    (
        "pair/n=400/m=100/dups=false",
        "intersect_compressed_with",
        1001,
        3,
        &[],
        false,
    ),
];

/// The plain-list operations' results equal Dgraph's on the golden inputs, duplicates included, where the
/// strategies differ; where Dgraph panics, the port returns the result its [`DEVIATIONS`] entry names. The results of
/// the operations that pack a list holding duplicates are retired: the packed format refuses such a list.
#[test]
fn plain_operations_match_golden() -> TestResult {
    let path = testdata_dir()?.join("golden/algo_setops.json");
    let golden = load_golden::<AlgoCase>(&path)?;
    let (mut checked, mut deviating, mut retired) = (0, 0, 0);
    for case in golden.cases() {
        for r in &case.results {
            let label = format!("{} {} {}", case.name, r.op, r.arg);
            let (out, i, k) = match run_plain(case, r)? {
                Ran::Out(out, i, k) => (out, i, k),
                Ran::Checked => {
                    checked += 1;
                    continue;
                }
                Ran::Packed => continue,
                Ran::Retired => {
                    retired += 1;
                    continue;
                }
            };
            let deviation = DEVIATIONS
                .iter()
                .find(|(name, op, arg, ..)| *name == case.name && *op == r.op && *arg == r.arg);
            let go_panics = deviation.is_some_and(|(.., panics)| *panics);
            assert_eq!(
                r.panics, go_panics,
                "{label}: Dgraph panics on the deviations it names"
            );
            match deviation {
                Some((_, _, _, ledger, want, _)) => {
                    assert_eq!(*want, out, "{label}: the port's result for ledger {ledger}");
                    deviating += 1;
                }
                None => {
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
    assert_eq!(
        deviating,
        DEVIATIONS.len(),
        "every deviation is in the golden file"
    );
    // The intersections that pack a first list holding duplicates, counted from the golden file.
    assert_eq!(retired, 130, "the results retired with Dgraph's format");
    Ok(())
}
