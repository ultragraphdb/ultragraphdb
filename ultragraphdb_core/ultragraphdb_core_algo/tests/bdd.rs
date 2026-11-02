/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The sorted-set scenarios, bound to `features/`: plain and packed lists of system IDs intersected, merged and
//! subtracted.

use std::collections::BTreeSet;

use cucumber::{World, then, when};
use ultragraphdb_core_algo::{
    difference_packed, intersect_compressed_with, intersect_sorted_packed, intersect_with,
    intersect_with_lin_packed, merge_sorted, merge_sorted_packed,
};
use ultragraphdb_core_codec::{List, UidPack, decode, encode};
use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_tool_bdd::{TestError, TestResult};

#[derive(Debug, Default, World)]
struct Steps {
    results: Vec<Vec<SystemId>>,
    result: Vec<SystemId>,
}

fn id(prefix: u16, bucket: u32, sequence: u64) -> TestResult<SystemId> {
    Ok(SystemId::new(
        TypePrefix::new(prefix)?,
        Bucket::new(bucket)?,
        Sequence::new(sequence)?,
    ))
}

/// The IDs of type 1, bucket 0 and the sequences given.
fn sequences(sequences: impl IntoIterator<Item = u64>) -> TestResult<Vec<SystemId>> {
    sequences.into_iter().map(|s| id(1, 0, s)).collect()
}

fn decoded(pack: Option<UidPack>) -> Vec<SystemId> {
    pack.map(|p| decode(&p, 0)).unwrap_or_default()
}

/// A list of `n` distinct IDs across two types and two buckets, from `seed`; every other ID is shared between the
/// lists of seeds 1 and 2, so a pair has common IDs.
fn spread(n: u64, seed: u64) -> TestResult<Vec<SystemId>> {
    let mut ids = BTreeSet::new();
    let mut state = seed;
    for i in 0..n {
        // SplitMix64, for a fixed spread of sequences that reproduces.
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        let sequence = if i % 2 == 0 {
            i + 1
        } else {
            ((z ^ (z >> 31)) % 5000) + 1
        };
        let prefix = if sequence % 2 == 0 { 1 } else { 2 };
        let bucket = if sequence % 3 == 0 { 0 } else { 7 };
        let _new: bool = ids.insert(id(prefix, bucket, sequence)?);
    }
    Ok(ids.into_iter().collect())
}

#[when("two lists of system IDs are intersected as plain lists and as packed lists")]
fn intersect_pairs(w: &mut Steps) -> TestResult {
    for n in [0u64, 1, 3, 11, 100, 1000] {
        for m in [0u64, 1, 3, 11, 100, 1000] {
            let (u, v) = (spread(n, 1)?, spread(m, 2)?);
            let mut plain = List::default();
            intersect_with(&List::new(u.clone()), &List::new(v.clone()), &mut plain);
            let (pu, pv) = (encode(&u, 10)?, encode(&v, 10)?);
            let packed = decoded(intersect_with_lin_packed(pu.as_ref(), pv.as_ref()));
            let mut mixed = List::default();
            intersect_compressed_with(pu.as_ref(), 0, &List::new(v.clone()), &mut mixed);
            w.results.push(plain.into_ids());
            w.results.push(packed);
            w.results.push(mixed.into_ids());
        }
    }
    Ok(())
}

#[then("both results are equal, sorted and free of duplicates")]
fn results_agree(w: &mut Steps) -> TestResult {
    assert_eq!(w.results.len(), 3 * 36);
    let mut nonempty = 0;
    for triple in w.results.chunks(3) {
        let [plain, packed, mixed] = triple else {
            return Err(TestError::new("three results a pair"));
        };
        assert_eq!(plain, packed);
        assert_eq!(plain, mixed);
        assert!(
            plain.windows(2).all(|p| p.first() < p.get(1)),
            "sorted, no duplicate"
        );
        nonempty += usize::from(!plain.is_empty());
    }
    assert!(nonempty > 10, "pairs share IDs");
    Ok(())
}

#[when("the lists of type 1, bucket 0 and sequences `[1,3,5]`, `[3,4]` and `[5,6]` are merged")]
fn merge_three(w: &mut Steps) -> TestResult {
    let lists = [
        sequences([1, 3, 5])?,
        sequences([3, 4])?,
        sequences([5, 6])?,
    ];
    let plain: Vec<List> = lists.iter().cloned().map(List::new).collect();
    w.results
        .push(merge_sorted(&plain.iter().collect::<Vec<_>>()).into_ids());
    let packs = lists
        .iter()
        .map(|l| encode(l, 2))
        .collect::<Result<Vec<_>, _>>()?;
    w.results.push(decoded(merge_sorted_packed(
        &packs.iter().map(Option::as_ref).collect::<Vec<_>>(),
    )));
    Ok(())
}

#[then("the result holds sequences `[1,3,4,5,6]` of type 1, bucket 0")]
fn merged_result(w: &mut Steps) -> TestResult {
    let want = sequences([1, 3, 4, 5, 6])?;
    assert_eq!(w.results, vec![want.clone(), want]);
    Ok(())
}

#[when(
    "the IDs of type 1, bucket 0 and sequences 1 to 100, packed in blocks of 10, are intersected with sequences 1 to 11 after sequence 200"
)]
fn intersect_after(w: &mut Steps) -> TestResult {
    let pack = encode(&sequences(1..=100)?, 10)?;
    let v = List::new(sequences(1..=11)?);
    let after = id(1, 0, 200)?.to_u128();
    w.result = std::panic::catch_unwind(|| {
        let mut out = List::default();
        intersect_compressed_with(pack.as_ref(), after, &v, &mut out);
        out.into_ids()
    })
    .map_err(|_| TestError::new("the intersection never panics"))?;
    Ok(())
}

#[then("the result is empty and nothing panics")]
fn result_empty_no_panic(w: &mut Steps) {
    assert!(w.result.is_empty(), "{:?}", w.result);
}

#[when(
    "the sequences `[1,2,3,9]`, `[2,3,9,10,11]` and `[9,20,21,22,23,24]` of type 1, bucket 0 are packed and intersected"
)]
fn intersect_packed(w: &mut Steps) -> TestResult {
    let packs = [
        encode(&sequences([1, 2, 3, 9])?, 2)?,
        encode(&sequences([2, 3, 9, 10, 11])?, 2)?,
        encode(&sequences([9, 20, 21, 22, 23, 24])?, 2)?,
    ];
    w.result = decoded(intersect_sorted_packed(
        &packs.iter().map(Option::as_ref).collect::<Vec<_>>(),
    ));
    Ok(())
}

#[then("the result is the ID of sequence 9")]
fn result_nine(w: &mut Steps) -> TestResult {
    assert_eq!(w.result, sequences([9])?);
    Ok(())
}

#[when(
    "sequences `2..15` packed in blocks of 5 have sequence `[1]` subtracted, all of type 1, bucket 0"
)]
fn subtract_one(w: &mut Steps) -> TestResult {
    let u = encode(&sequences(2..=15)?, 5)?;
    let v = encode(&sequences([1])?, 5)?;
    w.result = decoded(difference_packed(u.as_ref(), v.as_ref()));
    Ok(())
}

#[then("the result is sequences `2..15`")]
fn result_two_to_fifteen(w: &mut Steps) -> TestResult {
    assert_eq!(w.result, sequences(2..=15)?);
    Ok(())
}

#[when(
    "sequence `[642]` has sequences `[1,2,3,4,5,642]` packed in blocks of 5 subtracted, all of type 1, bucket 0"
)]
fn subtract_later_block(w: &mut Steps) -> TestResult {
    let u = encode(&sequences([642])?, 5)?;
    let v = encode(&sequences([1, 2, 3, 4, 5, 642])?, 5)?;
    w.result = decoded(difference_packed(u.as_ref(), v.as_ref()));
    Ok(())
}

#[then("the result is empty")]
fn result_empty(w: &mut Steps) {
    assert!(w.result.is_empty(), "{:?}", w.result);
}

#[when("sequences `[1,2,3]` have an empty pack of block size 5 subtracted")]
fn subtract_empty_pack(w: &mut Steps) -> TestResult {
    let u = encode(&sequences([1, 2, 3])?, 5)?;
    let v = UidPack::new(5, Vec::new())?;
    w.result = decoded(difference_packed(u.as_ref(), Some(&v)));
    Ok(())
}

#[then("the result is sequences `[1,2,3]`")]
fn result_one_two_three(w: &mut Steps) -> TestResult {
    assert_eq!(w.result, sequences([1, 2, 3])?);
    Ok(())
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
