/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, algo/packed_test.go. Changed for ultragraphdb: translated to Rust over system IDs, Go's
 * numbers standing for IDs as in `uidlist_tests.rs`; the tests of packs holding duplicates are gone, as a pack holds
 * none, and inputs Go packs with duplicates are packed without them; the golden-vector test and the tests of empty
 * blocks are ultragraphdb's.
 * Copyright (c) 2026. Marvin Hansen.
 */

use serde::Deserialize;
use ultragraphdb_core_algo::{
    apply_filter_packed, difference_packed, index_of_packed, intersect_sorted_packed,
    intersect_with_lin_packed, merge_sorted_packed,
};
use ultragraphdb_core_codec::{UidBlock, UidPack, decode, encode};
use ultragraphdb_tool_bdd::{Context, TestResult, load_golden, testdata_dir};

use crate::uidlist_tests::{idn, ids, nums};

fn new_uid_pack(numbers: &[u64]) -> TestResult<Option<UidPack>> {
    // A small block size, so the tests run over several blocks.
    Ok(encode(&ids(numbers)?, 5)?)
}

/// Go's `codec.Decode` of a pack that may be nil, as numbers.
fn decoded(pack: Option<UidPack>) -> Vec<u64> {
    pack.map(|p| nums(&decode(&p, 0))).unwrap_or_default()
}

fn refs(packs: &[Option<UidPack>]) -> Vec<Option<&UidPack>> {
    packs.iter().map(Option::as_ref).collect()
}

fn packs(lists: &[&[u64]]) -> TestResult<Vec<Option<UidPack>>> {
    lists.iter().map(|l| new_uid_pack(l)).collect()
}

fn lin(u: &[u64], v: &[u64]) -> TestResult<Vec<u64>> {
    Ok(decoded(intersect_with_lin_packed(
        new_uid_pack(u)?.as_ref(),
        new_uid_pack(v)?.as_ref(),
    )))
}

fn diff(u: &[u64], v: &[u64]) -> TestResult<Vec<u64>> {
    Ok(decoded(difference_packed(
        new_uid_pack(u)?.as_ref(),
        new_uid_pack(v)?.as_ref(),
    )))
}

#[test]
fn test_merge_sorted_packed() -> TestResult {
    // Go's TestMergeSorted1Packed to 10 but 9, whose only list holds a duplicate: (inputs, merge). Go's 6 and 10 pack
    // lists with duplicates; here each is packed once.
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
            &[&[11, 13, 16, 18, 20], &[12, 14, 15, 16, 17, 25], &[1, 2]],
            &[1, 2, 11, 12, 13, 14, 15, 16, 17, 18, 20, 25],
        ),
        (&[&[5, 6, 7], &[3, 4], &[1, 2], &[]], &[1, 2, 3, 4, 5, 6, 7]),
        (&[], &[]),
        (&[&[1, 2, 3, 6], &[4, 8, 9]], &[1, 2, 3, 4, 6, 8, 9]),
    ];
    for (input, want) in cases {
        let packed = packs(input)?;
        for (pack, list) in packed.iter().zip(input) {
            assert_eq!(decoded(pack.clone()), *list);
        }
        assert_eq!(
            decoded(merge_sorted_packed(&refs(&packed))),
            want,
            "{input:?}"
        );
    }
    Ok(())
}

#[test]
fn test_uid_list_intersect_packed() -> TestResult {
    // Go's TestUIDListIntersect1Packed to 6: (u, v, u ∩ v).
    let cases: [(&[u64], &[u64], &[u64]); 6] = [
        (&[1, 2, 3], &[], &[]),
        (&[1, 2, 3], &[1, 2, 3, 4, 5], &[1, 2, 3]),
        (&[1, 2, 3], &[2], &[2]),
        (&[1, 2, 3], &[0, 5], &[]),
        (&[1, 2, 3], &[3, 5], &[3]),
        (
            &[1, 2, 3, 4, 5, 6, 7, 9],
            &[1, 3, 5, 7, 8, 9],
            &[1, 3, 5, 7, 9],
        ),
    ];
    for (u, v, want) in cases {
        assert_eq!(lin(u, v)?, want, "{u:?} ∩ {v:?}");
    }
    assert_eq!(
        intersect_with_lin_packed(None, new_uid_pack(&[1])?.as_ref()),
        None
    );
    Ok(())
}

#[test]
fn test_intersect_sorted_packed() -> TestResult {
    let cases: [(&[&[u64]], &[u64]); 6] = [
        (&[&[1, 2, 3], &[2, 3, 4, 5]], &[2, 3]),
        (&[&[1, 2, 3]], &[1, 2, 3]),
        (&[], &[]),
        (&[&[100, 101]], &[100, 101]),
        (&[&[1, 2, 3], &[2, 3, 4, 5], &[4, 5, 6]], &[]),
        (&[&[10, 12, 13], &[2, 3, 4, 13], &[4, 5, 6]], &[]),
    ];
    for (input, want) in cases {
        assert_eq!(
            decoded(intersect_sorted_packed(&refs(&packs(input)?))),
            want,
            "{input:?}"
        );
    }
    // Go's TestIntersectSorted7Packed: 1..=11 down to 1..=1 intersect to 1.
    let nested: Vec<Option<UidPack>> = (1..=11u64)
        .rev()
        .map(|n| new_uid_pack(&(1..=n).collect::<Vec<u64>>()))
        .collect::<TestResult<_>>()?;
    assert_eq!(vec![1], decoded(intersect_sorted_packed(&refs(&nested))));
    Ok(())
}

#[test]
fn test_difference_packed() -> TestResult {
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
        assert_eq!(diff(u, v)?, want, "{u:?} - {v:?}");
    }
    Ok(())
}

fn stepped_pack(n: u64) -> TestResult<Option<UidPack>> {
    Ok(encode(&ids(&(0..n).collect::<Vec<_>>())?, 10)?)
}

#[test]
fn test_index_of_packed() -> TestResult {
    let pack = stepped_pack(1000)?;
    for (i, n) in (0..1000u64).enumerate() {
        assert_eq!(Some(i), index_of_packed(pack.as_ref(), idn(n)?));
    }
    assert_eq!(None, index_of_packed(pack.as_ref(), idn(1000)?));
    let small = stepped_pack(100)?;
    for n in [100u64, 101, 1000, u64::MAX - 1] {
        assert_eq!(None, index_of_packed(small.as_ref(), idn(n)?));
    }
    assert_eq!(None, index_of_packed(None, idn(0)?));
    Ok(())
}

#[test]
fn test_apply_filter_uint_packed() -> TestResult {
    let l = [1u64, 2, 3, 4, 5, 6, 7, 8, 9];
    let u = new_uid_pack(&l)?.context("the list is not empty")?;
    let res = apply_filter_packed(&u, |_, idx| l.get(idx).is_some_and(|n| n % 2 == 1));
    assert_eq!(vec![1, 3, 5, 7, 9], decoded(res));
    let four = idn(4)?;
    assert_eq!(
        vec![1, 2, 3, 5, 6, 7, 8, 9],
        decoded(apply_filter_packed(&u, |id, _| id != four))
    );
    assert_eq!(apply_filter_packed(&u, |_, _| false), None);
    Ok(())
}

/// Sequences 1 to 10 and 21 to 30 in blocks of 10, an empty block of first ID 11 between them.
fn with_empty_block() -> TestResult<UidPack> {
    let first = new_uid_pack(&(0..10).collect::<Vec<_>>())?.context("a pack")?;
    let last = new_uid_pack(&(20..30).collect::<Vec<_>>())?.context("a pack")?;
    let blocks = [
        first.blocks().to_vec(),
        vec![UidBlock::new(idn(10)?, Vec::new(), 0)?],
        last.blocks().to_vec(),
    ]
    .concat();
    Ok(UidPack::new(5, blocks)?)
}

#[test]
fn test_packed_operations_step_over_an_empty_block() -> TestResult {
    // An empty block contributes nothing: Dgraph's merge indexes it and panics, and its linear intersection stops at
    // it; here both read on.
    let pack = with_empty_block()?;
    let other = new_uid_pack(&[5, 25])?.context("a pack")?;
    let want: Vec<u64> = (0..10).chain(20..30).collect();
    assert_eq!(
        want,
        decoded(merge_sorted_packed(&[Some(&pack), Some(&other)]))
    );
    assert_eq!(
        vec![5, 25],
        decoded(intersect_with_lin_packed(Some(&pack), Some(&other)))
    );
    assert_eq!(
        (0..10)
            .chain(20..30)
            .filter(|n| *n != 5 && *n != 25)
            .collect::<Vec<_>>(),
        decoded(difference_packed(Some(&pack), Some(&other)))
    );
    assert_eq!(Some(10), index_of_packed(Some(&pack), idn(20)?));
    Ok(())
}

#[test]
fn test_merge_sorted_packed_takes_the_first_present_lists_block_size() -> TestResult {
    // Dgraph v25.5.0 MergeSortedPacked over (nil, [1 2] packed by 3, [2 5] packed by 7), run in Go: BlockSize 3, UIDs
    // [1 2 5].
    let a = encode(&ids(&[1, 2])?, 3)?;
    let b = encode(&ids(&[2, 5])?, 7)?;
    let merged = merge_sorted_packed(&[None, a.as_ref(), b.as_ref()]).context("a pack")?;
    assert_eq!(merged.block_size(), 3);
    assert_eq!(decoded(Some(merged)), vec![1, 2, 5]);
    Ok(())
}

#[test]
fn test_merge_sorted_packed_skips_a_list_of_no_ids() -> TestResult {
    // A pack whose one block holds no IDs is skipped, though its block size, the first present list's, is the
    // result's. Dgraph v25.5.0 MergeSortedPacked over (such a pack with block size 4, [1 2] packed by 3), run in Go:
    // BlockSize 4, UIDs [1 2].
    let empty = UidPack::new(4, vec![UidBlock::new(idn(7)?, Vec::new(), 0)?])?;
    let b = encode(&ids(&[1, 2])?, 3)?;
    let merged = merge_sorted_packed(&[Some(&empty), b.as_ref()]).context("a pack")?;
    assert_eq!(merged.block_size(), 4);
    assert_eq!(decoded(Some(merged)), vec![1, 2]);
    Ok(())
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
/// packed operation, the ledger entry of `openspec/notes/archive/dgraph-bugs-found-in-port.md`, and the plain operation
/// whose result the port's equals. Entry 1: IntersectSortedPacked returns the intersection of the two shortest lists;
/// entry 2: DifferencePacked drops `u`'s blocks after `v` runs out; entry 25: a UID of `u` in a later block of `v`
/// survives. The 14 entries on cases with duplicates are retired with those cases.
const DEVIATIONS: &[(&str, &str, u32, &str)] = &[
    (
        "lists/fourth-narrows",
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
        "pair/n=3/m=400/dups=false",
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
        "pair/n=11/m=400/dups=false",
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
        "pair/n=100/m=100/dups=false",
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
        "pair/n=400/m=3/dups=false",
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
        "pair/n=400/m=400/dups=false",
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

/// The golden result a packed operation's result `r` in `case` is checked against, and whether it is one of the
/// `DEVIATIONS`: the plain operation's result, which Dgraph's packed one equals unless it is a deviation; with no
/// plain result, Dgraph's packed one, whose nil pack the port's `actual_is_none` matches.
fn packed_want<'a>(
    case: &'a AlgoCase,
    r: &'a OpResult,
    actual_is_none: bool,
) -> TestResult<(&'a Vec<u64>, bool)> {
    let plain_name = plain_op(&r.op).context("a packed operation has a plain one")?;
    let plain = case.results.iter().find(|p| p.op == plain_name);
    let deviation = DEVIATIONS
        .iter()
        .find(|&&(name, op, _, _)| name == case.name && op == r.op);
    let Some(plain) = plain else {
        assert!(
            case.lists.is_empty() && deviation.is_none(),
            "{} {}: no plain result",
            case.name,
            r.op
        );
        assert_eq!(
            r.out_nil, actual_is_none,
            "{} {}: nil pack",
            case.name, r.op
        );
        return Ok((&r.out, false));
    };
    assert!(!plain.panics, "{} {}: Dgraph panicked", case.name, plain.op);
    if let Some(&(_, _, entry, expected)) = deviation {
        assert_eq!(expected, plain.op, "{} {}", case.name, r.op);
        assert_ne!(
            r.out, plain.out,
            "{} {}: entry {entry} equals the plain result",
            case.name, r.op
        );
    } else {
        assert_eq!(
            r.out, plain.out,
            "{} {}: differs and is not a deviation",
            case.name, r.op
        );
    }
    Ok((&plain.out, deviation.is_some()))
}

/// The packed operations' results equal the plain operations' golden results in the same case, which hold lists
/// across 2^32 and the inputs where Dgraph's DifferencePacked and IntersectSortedPacked drop or keep UIDs. Dgraph's own
/// packed result equals its plain one in every case but the `DEVIATIONS`; with no lists there is no plain result, and
/// Dgraph's packed one, no pack, is the expected one. A case with a list holding duplicates is retired: the packed
/// format refuses such a list.
#[test]
fn packed_operations_match_golden() -> TestResult {
    let path = testdata_dir()?.join("golden/algo_setops.json");
    let golden = load_golden::<AlgoCase>(&path)?;
    let (mut checked, mut deviations_seen, mut retired) = (0, 0, 0);
    for case in golden.cases() {
        let Ok(packed) = case
            .lists
            .iter()
            .map(|l| Ok(encode(&ids(l)?, case.block_size)?))
            .collect::<TestResult<Vec<Option<UidPack>>>>()
        else {
            retired += case
                .results
                .iter()
                .filter(|r| r.op.ends_with("_packed"))
                .count();
            continue;
        };
        let packs = refs(&packed);
        let pack = |i: usize| packs.get(i).copied().flatten();
        for r in &case.results {
            let actual = match r.op.as_str() {
                "intersect_with_lin_packed" => intersect_with_lin_packed(pack(0), pack(1)),
                "difference_packed" => difference_packed(pack(0), pack(1)),
                "merge_sorted_packed" => merge_sorted_packed(&packs),
                "intersect_sorted_packed" => intersect_sorted_packed(&packs),
                "index_of_packed" => {
                    let want = usize::try_from(r.index).ok();
                    assert_eq!(
                        want,
                        index_of_packed(pack(0), idn(r.arg)?),
                        "{} {}",
                        case.name,
                        r.arg
                    );
                    checked += 1;
                    continue;
                }
                _ => continue,
            };
            assert!(!r.panics, "{} {}: Dgraph panicked", case.name, r.op);
            let (want, deviation) = packed_want(case, r, actual.is_none())?;
            deviations_seen += usize::from(deviation);
            // An encoder that was given no ID returns no pack, so an empty result is no pack.
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
        checked > 150,
        "the golden file holds packed results, {checked} checked"
    );
    // The packed results of the cases with duplicates, counted from the golden file.
    assert_eq!(retired, 72, "the results retired with Dgraph's format");
    Ok(())
}
