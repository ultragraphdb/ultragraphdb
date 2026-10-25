/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported in part from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/cellunion_test.go.
 * Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! The deterministic CellUnion tests of golang/geo's cellunion_test.go. Normalize, Denormalize,
//! the bounds and the probes are also compared with Go's vectors in cellid_tests.

use super::golden::load;
use serde::Deserialize;
use ultragraphdb_core_types::{
    Cap, Cell, CellId, CellUnion, LatLng, MAX_LEVEL, Point, R3Vector, Region,
};

fn union(ids: &[u64]) -> CellUnion {
    CellUnion::new(ids.iter().map(|&i| CellId::new(i)).collect())
}

fn ids(u: &CellUnion) -> Vec<u64> {
    u.iter().map(|c| c.id()).collect()
}

// Ported from golang/geo s2/cellunion_test.go, TestCellUnionNormalization.
#[test]
fn cell_union_normalization() {
    let mut cu = union(&[
        0x80855c0000000000, // A: a cell over Pittsburg CA
        0x80855d0000000000, // B, a child of A
        0x8085634000000000, // first child of X, disjoint from A
        0x808563c000000000, // second child of X
        0x80855dc000000000, // a child of B
        0x808562c000000000, // third child of X
        0x8085624000000000, // fourth child of X
        0x80855d0000000000, // B again
    ]);
    cu.normalize();
    assert_eq!(
        ids(&cu),
        vec![
            0x80855c0000000000, // A
            0x8085630000000000, // X
        ]
    );
}

// The sibling test of normalize (golang/geo's TestCellUnionAreSiblings, through Normalize):
// four children collapse to their parent, cascading upwards, and the six faces never
// collapse.
#[test]
fn cell_union_normalize_collapses_siblings_only() {
    let id = CellId::from_lat_lng(LatLng::from_point(Point::from_coords(1.0, 0.0, 0.0))).parent(10);
    let [c0, c1, c2, c3] = id.children();

    let mut u = CellUnion::new(vec![c3, c1, c0, c2]);
    u.normalize();
    assert_eq!(u.as_slice(), &[id]);

    // Three children and the parent itself: the parent contains the children.
    let mut u = CellUnion::new(vec![c1, id, c2, c3]);
    u.normalize();
    assert_eq!(u.as_slice(), &[id]);

    // Three children only: nothing collapses, the result is sorted.
    let mut u = CellUnion::new(vec![c3, c1, c0]);
    u.normalize();
    assert_eq!(u.as_slice(), &[c0, c1, c3]);

    // All sixteen grandchildren collapse twice, to id.
    let mut grand: Vec<CellId> = id.children().iter().flat_map(|c| c.children()).collect();
    grand.reverse();
    let mut u = CellUnion::new(grand);
    u.normalize();
    assert_eq!(u.as_slice(), &[id]);

    // Duplicates are removed.
    let mut u = CellUnion::new(vec![c0, c0, c1, c1]);
    u.normalize();
    assert_eq!(u.as_slice(), &[c0, c1]);

    // The six faces are not siblings of any parent.
    let faces: Vec<CellId> = (0..6).rev().map(CellId::from_face).collect();
    let mut u = CellUnion::new(faces);
    u.normalize();
    assert_eq!(
        u.as_slice(),
        &(0..6).map(CellId::from_face).collect::<Vec<_>>()[..]
    );
}

// Ported from golang/geo s2/cellunion_test.go, TestCellUnionBasic.
#[test]
fn cell_union_basic() {
    let mut empty = CellUnion::default();
    empty.normalize();
    assert_eq!(empty.len(), 0);
    assert!(empty.is_empty());

    let face1_id = CellId::from_face(1);
    let face1_cell = Cell::from_cell_id(face1_id);
    let mut face1_union = CellUnion::new(vec![face1_id]);
    face1_union.normalize();
    assert_eq!(face1_union.as_slice(), &[face1_id]);
    assert!(face1_union.contains_cell(&face1_cell));

    let face2_id = CellId::from_face(2);
    let face2_cell = Cell::from_cell_id(face2_id);
    let mut face2_union = CellUnion::new(vec![face2_id]);
    face2_union.normalize();
    assert_eq!(face2_union.as_slice(), &[face2_id]);

    assert!(!face1_union.contains_cell(&face2_cell));
    assert!(!face1_union.intersects_cell(&face2_cell));
    assert!(face1_union.intersects_cell(&face1_cell));
}

struct UnionCase {
    cells: Vec<CellId>,
    contained: Vec<CellId>,
    overlaps: Vec<CellId>,
    disjoint: Vec<CellId>,
}

fn c(id: u64) -> CellId {
    CellId::new(id)
}

// Ported from golang/geo s2/cellunion_test.go, TestCellUnion.
#[test]
fn cell_union_contains_and_intersects() {
    let nyc = c(0x89c25c0000000000);
    let tests = [
        // Single cell around NYC, and some simple nearby probes
        UnionCase {
            cells: vec![nyc],
            contained: vec![nyc.child_begin(), nyc.child_begin_at_level(28)],
            overlaps: vec![nyc.parent(nyc.level() - 1), CellId::from_face(nyc.face())],
            disjoint: vec![
                nyc.next(),                          // Cell next to this one at same level
                nyc.next().child_begin_at_level(28), // Cell next to this one at deep level
                c(0x89c2700000000000),               // Big(er) neighbor cell
                c(0x89e9000000000000),               // Very big next door cell.
                c(0x89c1000000000000),               // Very big cell, smaller value than probe
            ],
        },
        // NYC and SFO:
        UnionCase {
            cells: [
                0x89c25b0000000000, // NYC
                0x89c2590000000000, // NYC
                0x89c2f70000000000, // NYC
                0x89c2f50000000000, // NYC
                0x8085870000000000, // SFO
                0x8085810000000000, // SFO
                0x808f7d0000000000, // SFO
                0x808f7f0000000000, // SFO
            ]
            .map(c)
            .to_vec(),
            contained: [
                0x808f7ef300000000, // SFO
                0x808f7e5cf0000000, // SFO
                0x808587f000000000, // SFO
                0x89c25ac000000000, // NYC
                0x89c259a400000000, // NYC
                0x89c258fa10000000, // NYC
                0x89c258f174007000, // NYC
            ]
            .map(c)
            .to_vec(),
            overlaps: [
                0x808c000000000000, // Big SFO
                0x89c4000000000000, // Big NYC
            ]
            .map(c)
            .to_vec(),
            disjoint: [
                0x89c15a4fcb1bb000, // outside NYC
                0x89c15a4e4aa95000, // outside NYC
                0x8094000000000000, // outside SFO (big)
                0x8096f10000000000, // outside SFO (smaller)
                0x87c0000000000000, // Midwest very big
            ]
            .map(c)
            .to_vec(),
        },
        // CellUnion with cells at many levels:
        UnionCase {
            cells: [
                0x8100000000000000, // starting around california
                0x8740000000000000, // adjacent cells at increasing
                0x8790000000000000, // levels, moving eastward.
                0x87f4000000000000,
                0x87f9000000000000, // going down across the midwest
                0x87ff400000000000,
                0x87ff900000000000,
                0x87fff40000000000,
                0x87fff90000000000,
                0x87ffff4000000000,
                0x87ffff9000000000,
                0x87fffff400000000,
                0x87fffff900000000,
                0x87ffffff40000000,
                0x87ffffff90000000,
                0x87fffffff4000000,
                0x87fffffff9000000,
                0x87ffffffff400000, // to a very small cell in Wisconsin
            ]
            .map(c)
            .to_vec(),
            contained: [
                0x808f400000000000,
                0x80eb118b00000000,
                0x8136a7a11d000000,
                0x8136a7a11dac0000,
                0x876c7c0000000000,
                0x87f96d0000000000,
                0x87ffffffff400000,
            ]
            .map(c)
            .to_vec(),
            overlaps: vec![
                c(0x8100000000000000).parent(c(0x8100000000000000).level() - 1),
                c(0x8740000000000000).parent(c(0x8740000000000000).level() - 1),
            ],
            disjoint: [
                0x52aaaaaaab300000,
                0x52aaaaaaacd00000,
                0x87fffffffa100000,
                0x87ffffffed500000,
                0x87ffffffa0100000,
                0x87fffffed5540000,
                0x87fffffed6240000,
                0x52aaaacccb340000,
                0x87a0000400000000,
                0x87a000001f000000,
                0x87a0000029d00000,
                0x9500000000000000,
            ]
            .map(c)
            .to_vec(),
        },
    ];
    for test in tests {
        let mut u = CellUnion::new(test.cells.clone());
        u.normalize();
        // Ensure self-containment tests are correct.
        for id in test.cells.iter().chain(&test.contained) {
            assert!(u.intersects_cell_id(*id), "{u:?} should intersect {id:?}");
            assert!(u.contains_cell_id(*id), "{u:?} should contain {id:?}");
            let cl = Cell::from_cell_id(*id);
            assert!(u.intersects_cell(&cl) && u.contains_cell(&cl), "{id:?}");
            assert!(
                u.contains_point(id.point()),
                "{u:?} should contain the center of {id:?}"
            );
        }
        // Make sure the CellUnion intersect these cells but do not contain.
        for id in &test.overlaps {
            assert!(u.intersects_cell_id(*id), "{u:?} should intersect {id:?}");
            assert!(!u.contains_cell_id(*id), "{u:?} should not contain {id:?}");
        }
        // Negative cases make sure the CellUnion neither contain nor intersect these cells
        for id in &test.disjoint {
            assert!(
                !u.intersects_cell_id(*id),
                "{u:?} should not intersect {id:?}"
            );
            assert!(!u.contains_cell_id(*id), "{u:?} should not contain {id:?}");
            assert!(
                !u.contains_point(id.point()),
                "{u:?} should not contain the center of {id:?}"
            );
        }
    }
}

// The caveat of ContainsCellID: a non-normalized union of four children does not contain
// their parent, the normalized one does.
#[test]
fn cell_union_four_children_contain_parent_only_after_normalize() {
    let parent = c(0x89c25c0000000000);
    let mut u = CellUnion::new(parent.children().to_vec());
    assert!(!u.contains_cell_id(parent));
    assert!(u.intersects_cell_id(parent));
    u.normalize();
    assert!(u.contains_cell_id(parent));
}

/// A Denormalize case: name, min level, level mod, input and expected output.
type DenormalizeCase = (&'static str, i64, i64, Vec<CellId>, Vec<CellId>);

// Ported from golang/geo s2/cellunion_test.go, TestCellUnionDenormalize.
#[test]
fn cell_union_denormalize() {
    let f = CellId::from_face;
    let tests: [DenormalizeCase; 4] = [
        (
            "not expanded, level mod == 1",
            10,
            1,
            vec![
                f(2).child_begin_at_level(11),
                f(2).child_begin_at_level(11),
                f(3).child_begin_at_level(14),
                f(0).child_begin_at_level(10),
            ],
            vec![
                f(2).child_begin_at_level(11),
                f(2).child_begin_at_level(11),
                f(3).child_begin_at_level(14),
                f(0).child_begin_at_level(10),
            ],
        ),
        (
            "not expanded, level mod > 1",
            10,
            2,
            vec![
                f(2).child_begin_at_level(12),
                f(2).child_begin_at_level(12),
                f(3).child_begin_at_level(14),
                f(0).child_begin_at_level(10),
            ],
            vec![
                f(2).child_begin_at_level(12),
                f(2).child_begin_at_level(12),
                f(3).child_begin_at_level(14),
                f(0).child_begin_at_level(10),
            ],
        ),
        (
            "expended, (level - min_level) is not multiple of level mod",
            10,
            3,
            vec![f(2).child_begin_at_level(12), f(5).child_begin_at_level(11)],
            f(2).child_begin_at_level(12)
                .children()
                .into_iter()
                .chain(
                    f(5).child_begin_at_level(11)
                        .children()
                        .iter()
                        .flat_map(|c| c.children()),
                )
                .collect(),
        ),
        (
            "expended, level < min_level",
            10,
            3,
            vec![f(2).child_begin_at_level(9)],
            f(2).child_begin_at_level(9).children().to_vec(),
        ),
    ];
    for (name, min_level, level_mod, cells, want) in tests {
        let mut u = CellUnion::new(cells);
        u.denormalize(min_level, level_mod);
        assert_eq!(u.as_slice(), want.as_slice(), "{name}");
    }
}

fn deg(d: f64) -> f64 {
    d * core::f64::consts::PI / 180.0
}

// Ported from golang/geo s2/cellunion_test.go, TestCellUnionRectBound (Go's epsilon 1e-14),
// with Go's literals as written.
#[test]
#[allow(clippy::excessive_precision)]
fn cell_union_rect_bound() {
    assert!(CellUnion::default().rect_bound().is_empty());
    let pi = core::f64::consts::PI;
    let tests: [(&[u64], [f64; 4]); 4] = [
        (
            &[0x3000000000000000], // face 1
            [-pi / 4.0, pi / 4.0, pi / 4.0, 3.0 * pi / 4.0],
        ),
        (
            &[0x808c000000000000], // Big SFO
            [
                deg(34.644220547108482),
                deg(38.011928357226651),
                deg(-124.508522987668428),
                deg(-121.628309835221216),
            ],
        ),
        (
            &[0x89c4000000000000], // Big NYC
            [
                deg(38.794595155857657),
                deg(41.747046884651063),
                deg(-76.456308667788633),
                deg(-73.465162142654819),
            ],
        ),
        (
            &[0x89c4000000000000, 0x808c000000000000], // Big NYC, Big SFO
            [
                deg(34.644220547108482),
                deg(41.747046884651063),
                deg(-124.508522987668428),
                deg(-73.465162142654819),
            ],
        ),
    ];
    assert_eq!(CellId::from_face(1).id(), 0x3000000000000000);
    for (cells, want) in tests {
        let r = union(cells).rect_bound();
        let got = [r.lat().lo(), r.lat().hi(), r.lng().lo(), r.lng().hi()];
        for (g, w) in got.iter().zip(want) {
            assert!(
                (g - w).abs() <= 1e-14,
                "{cells:x?}: rect bound {got:?} want {want:?}"
            );
        }
    }
}

#[derive(Deserialize)]
struct CellCase {
    id: String,
}

#[derive(Deserialize)]
struct CellGolden {
    cells: Vec<CellCase>,
}

fn check_from_range(min: CellId, max: CellId) {
    let cu = CellUnion::from_range(min, max.next());
    let v = cu.as_slice();
    assert!(!v.is_empty(), "from_range({min:?}, {max:?})");
    assert_eq!(v[0].range_min(), min);
    assert_eq!(v[v.len() - 1].range_max(), max);
    for w in v.windows(2) {
        assert_eq!(w[1].range_min(), w[0].range_max().next());
    }
}

// Ported from golang/geo s2/cellunion_test.go, TestCellUnionFromRange, over the leaf cells of
// the golden set's cells instead of Go's random ones.
#[test]
fn cell_union_from_range() {
    let g: CellGolden = load("cellid.json");
    let leaves: Vec<CellId> = g
        .cells
        .iter()
        .map(|k| CellId::new(u64::from_str_radix(&k.id, 16).expect("hex cell id")))
        .filter(|c| c.is_valid())
        .flat_map(|c| [c.range_min(), c.range_max()])
        .collect();
    assert!(leaves.len() > 10, "golden cells");
    for w in leaves.windows(2) {
        let (min, max) = if w[0] <= w[1] {
            (w[0], w[1])
        } else {
            (w[1], w[0])
        };
        check_from_range(min, max);
    }

    // Focus on test cases that generate an empty or full range.

    // Test an empty range before the minimum CellID.
    let id_begin = CellId::from_face(0).child_begin_at_level(MAX_LEVEL);
    assert!(CellUnion::from_range(id_begin, id_begin).is_empty());

    // Test an empty range after the maximum CellID.
    let id_end = CellId::from_face(5).child_end_at_level(MAX_LEVEL);
    assert!(CellUnion::from_range(id_end, id_end).is_empty());

    // Test the full sphere.
    let cu = CellUnion::from_range(id_begin, id_end);
    assert_eq!(
        cu.as_slice(),
        &(0..6).map(CellId::from_face).collect::<Vec<_>>()[..]
    );

    // A single leaf, and a range that is exactly one cell.
    let leaf = c(0x89c25c0000000000).range_min();
    assert_eq!(CellUnion::from_range(leaf, leaf.next()).as_slice(), &[leaf]);
    let cell = c(0x89c25c0000000000);
    assert_eq!(
        CellUnion::from_range(cell.range_min(), cell.range_max().next()).as_slice(),
        &[cell]
    );
}

// Ported from golang/geo s2/cellunion_test.go, TestCellUnionEmpty (the parts this port has),
// plus the empty union's bounds.
#[test]
fn cell_union_empty() {
    let mut empty = CellUnion::default();
    empty.normalize();
    assert!(empty.is_empty());
    empty.denormalize(0, 2);
    assert!(empty.is_empty());

    let face1_id = CellId::from_face(1);
    assert!(!empty.contains_cell_id(face1_id));
    assert!(!empty.intersects_cell_id(face1_id));
    assert!(!empty.contains_point(face1_id.point()));
    assert!(empty.cap_bound().is_empty());
    assert!(empty.rect_bound().is_empty());
    assert_eq!(
        Region::cell_union_bound(&empty),
        Cap::empty().cell_union_bound()
    );
}

// Construction and access: new and From keep the order (no normalization), extend appends.
#[test]
fn cell_union_construction_keeps_order() {
    let a = c(0x89c25c0000000000);
    let b = c(0x808c000000000000);
    let u = CellUnion::new(vec![a, b, a]);
    assert_eq!(u.as_slice(), &[a, b, a]);
    assert_eq!(u.len(), 3);
    assert_eq!(CellUnion::from(vec![a, b, a]), u);
    assert_eq!(u.iter().copied().collect::<Vec<_>>(), vec![a, b, a]);
    let mut v = CellUnion::new(vec![b]);
    v.extend(CellUnion::new(vec![a, a]));
    assert_eq!(v.into_vec(), vec![b, a, a]);
}

// CapBound: an axis at the area-weighted centroid of the cells, widened to every cell's cap.
#[test]
fn cell_union_cap_bound_contains_every_cell() {
    let u = union(&[0x89c4000000000000, 0x808c000000000000]);
    let cap = u.cap_bound();
    for id in u.iter() {
        let cl = Cell::from_cell_id(*id);
        for k in 0..4 {
            assert!(cap.contains_point(cl.vertex(k).expect("vertex")));
        }
        assert!(cap.contains_point(cl.center()));
    }
    // A single face cell: the cap is that face's cell's cap, centred on its center.
    let face = union(&[CellId::from_face(0).id()]);
    let cap = face.cap_bound();
    let center = cap.center();
    assert!(
        (center.x() - 1.0).abs() <= 1e-15 && center.y().abs() <= 1e-15 && center.z().abs() <= 1e-15,
        "{center:?}"
    );
    assert!(!cap.contains_point(Point::from_vector(R3Vector::new(-1.0, 0.0, 0.0))));
}
