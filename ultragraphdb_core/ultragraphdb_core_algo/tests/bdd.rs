/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use cucumber::{World, given, then, when};
use ultragraphdb_core_algo::{
    difference_packed, intersect_compressed_with, intersect_sorted_packed, intersect_with,
    intersect_with_lin_packed, merge_sorted, merge_sorted_packed,
};
use ultragraphdb_core_codec::{decode, encode};
use ultragraphdb_proto_pb::pb::{List, UidPack};

/// The block size the packed lists use, small so that the lists span many blocks.
const BLOCK_SIZE: usize = 10;

#[derive(Debug, Default, World)]
struct Steps {
    lists: Vec<Vec<u64>>,
    /// Per pair of lists: the plain, the packed and the packed-with-plain intersection.
    intersections: Vec<(String, [Vec<u64>; 3])>,
    merged: Vec<Vec<u64>>,
    pack: Option<UidPack>,
    intersection: Vec<u64>,
    /// The decoded result of a packed intersection or difference.
    result: Vec<u64>,
}

fn parse_uids(text: &str) -> Vec<u64> {
    text.split(',')
        .map(|s| s.trim().parse().expect("a UID"))
        .collect()
}

fn decoded(pack: Option<UidPack>) -> Vec<u64> {
    pack.map(|p| decode(&p, 0)).unwrap_or_default()
}

fn list(uids: &[u64]) -> List {
    List {
        uids: uids.to_vec(),
    }
}

#[given(
    "sorted, duplicate-free UID lists of 0, 1, 3, 11, 100 and 1000 UIDs, each pair sharing some UIDs"
)]
fn pairwise_lists(w: &mut Steps) {
    // Even and multiple-of-three offsets from just below 2^32: two lists share the multiples of six, and the longer
    // ones cross 2^32, which closes a block early.
    let base = (1u64 << 32) - 600;
    for n in [0u64, 1, 3, 11, 100, 1000] {
        w.lists.push((0..n).map(|i| base + 2 * i).collect());
        w.lists.push((0..n).map(|i| base + 3 * i).collect());
    }
}

#[when(
    "each pair is intersected as plain lists, as packed lists and as a packed list with a plain one"
)]
fn intersect_pairs(w: &mut Steps) {
    for u in &w.lists {
        for v in &w.lists {
            let mut plain = List::default();
            intersect_with(&list(u), &list(v), &mut plain);
            let (pu, pv) = (encode(u, BLOCK_SIZE), encode(v, BLOCK_SIZE));
            let packed = decoded(intersect_with_lin_packed(pu.as_ref(), pv.as_ref()));
            let mut mixed = List::default();
            intersect_compressed_with(pu.as_ref(), 0, &list(v), &mut mixed);
            let name = format!("{} with {} UIDs", u.len(), v.len());
            w.intersections
                .push((name, [plain.uids, packed, mixed.uids]));
        }
    }
}

#[then("the three results are equal, sorted and free of duplicates")]
fn intersections_agree(w: &mut Steps) {
    assert_eq!(w.intersections.len(), 144);
    assert!(
        w.intersections.iter().any(|(_, r)| r[0].len() > 100),
        "some pairs share many UIDs"
    );
    for (name, [plain, packed, mixed]) in &w.intersections {
        assert_eq!(plain, packed, "{name}: plain and packed");
        assert_eq!(plain, mixed, "{name}: plain and packed with plain");
        assert!(
            plain.windows(2).all(|p| p[0] < p[1]),
            "{name}: sorted and duplicate-free"
        );
    }
}

#[given(expr = "the UID lists {string}, {string} and {string}")]
fn three_lists(w: &mut Steps, a: String, b: String, c: String) {
    w.lists = vec![parse_uids(&a), parse_uids(&b), parse_uids(&c)];
}

#[when("they are merged as plain lists and as packed lists")]
fn merge(w: &mut Steps) {
    let plain: Vec<List> = w.lists.iter().map(|l| list(l)).collect();
    w.merged
        .push(merge_sorted(&plain.iter().collect::<Vec<_>>()).uids);
    let packs: Vec<Option<UidPack>> = w.lists.iter().map(|l| encode(l, BLOCK_SIZE)).collect();
    w.merged.push(decoded(merge_sorted_packed(
        &packs.iter().map(Option::as_ref).collect::<Vec<_>>(),
    )));
}

#[then(expr = "both results are {string}")]
fn merged_are(w: &mut Steps, text: String) {
    let want = parse_uids(&text);
    assert_eq!(w.merged, vec![want.clone(), want]);
}

#[given(expr = "the UIDs 1 to {int} packed and the plain list of the UIDs 1 to {int}")]
fn packed_and_plain(w: &mut Steps, packed: u64, plain: u64) {
    w.pack = encode(&(1..=packed).collect::<Vec<_>>(), BLOCK_SIZE);
    w.lists = vec![(1..=plain).collect()];
}

#[when(expr = "the packed list is intersected with the plain one after UID {int}")]
fn intersect_after(w: &mut Steps, after: u64) {
    let mut out = List::default();
    intersect_compressed_with(w.pack.as_ref(), after, &list(&w.lists[0]), &mut out);
    w.intersection = out.uids;
}

#[then("the intersection is empty")]
fn intersection_empty(w: &mut Steps) {
    assert!(w.intersection.is_empty(), "{:?}", w.intersection);
}

#[when("they are packed and intersected")]
fn intersect_packed(w: &mut Steps) {
    let packs: Vec<Option<UidPack>> = w.lists.iter().map(|l| encode(l, BLOCK_SIZE)).collect();
    w.result = decoded(intersect_sorted_packed(
        &packs.iter().map(Option::as_ref).collect::<Vec<_>>(),
    ));
}

#[when(
    expr = "the UIDs {int} to {int} packed in blocks of {int} have the UIDs {string} subtracted"
)]
fn subtract_from_range(w: &mut Steps, first: u64, last: u64, block_size: usize, v: String) {
    let u = encode(&(first..=last).collect::<Vec<_>>(), block_size);
    let v = encode(&parse_uids(&v), block_size);
    w.result = decoded(difference_packed(u.as_ref(), v.as_ref()));
}

#[when(expr = "the UIDs {string} have the UIDs {string} packed in blocks of {int} subtracted")]
fn subtract_packed(w: &mut Steps, u: String, v: String, block_size: usize) {
    let u = encode(&parse_uids(&u), block_size);
    let v = encode(&parse_uids(&v), block_size);
    w.result = decoded(difference_packed(u.as_ref(), v.as_ref()));
}

#[when(expr = "the UIDs {string} have an empty pack of block size {int} subtracted")]
fn subtract_empty_pack(w: &mut Steps, u: String, block_size: u32) {
    let u = encode(&parse_uids(&u), BLOCK_SIZE);
    let v = UidPack {
        block_size,
        ..UidPack::default()
    };
    w.result = decoded(difference_packed(u.as_ref(), Some(&v)));
}

#[then(expr = "the result is {string}")]
fn result_is(w: &mut Steps, text: String) {
    assert_eq!(w.result, parse_uids(&text));
}

#[then(expr = "the result is the UIDs {int} to {int}")]
fn result_is_range(w: &mut Steps, first: u64, last: u64) {
    assert_eq!(w.result, (first..=last).collect::<Vec<_>>());
}

#[then("the result is empty")]
fn result_empty(w: &mut Steps) {
    assert!(w.result.is_empty(), "{:?}", w.result);
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_test_bdd::features_dir()),
    );
}
