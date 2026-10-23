/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported in part from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/cellid_test.go; the golden-vector checks are new.
 * Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! CellId arithmetic, tokens and cell geometry, against Go.

use super::golden::{Checker, cell, f, ids, load, same, same_point, same_rect};
use serde::Deserialize;
use ultragraphdb_core_types::{Cell, CellId, LatLng, MAX_LEVEL};

#[derive(Deserialize)]
pub struct CapJson {
    pub center: [String; 3],
    pub chord: String,
    pub radius: String,
    pub height: String,
    pub empty: bool,
    pub full: bool,
}

pub fn same_cap(c: ultragraphdb_core_types::Cap, want: &CapJson) -> bool {
    same_point(c.center(), &want.center)
        && same(c.chord_radius().length2(), f(&want.chord))
        && same(c.radius().radians(), f(&want.radius))
        && same(c.height(), f(&want.height))
        && c.is_empty() == want.empty
        && c.is_full() == want.full
}

#[derive(Deserialize)]
struct CellCase {
    id: String,
    token: String,
    level: i64,
    face: usize,
    is_leaf: bool,
    valid: bool,
    parents: Vec<String>,
    range_min: String,
    range_max: String,
    children: Option<Vec<String>>,
    child_begin: String,
    child_end: String,
    next: String,
    prev: String,
    point: [String; 3],
    latlng: [String; 2],
    vertex_neighbors: Option<Vec<String>>,
    from_token: String,
    bound_uv: [String; 4],
    center: [String; 3],
    vertices: Vec<[String; 3]>,
    rect_bound: [String; 4],
    cap_bound: CapJson,
    probes: Vec<[String; 3]>,
    probe_in: Vec<bool>,
    child_begin_at: Option<Vec<String>>,
    child_end_at: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct PairCase {
    a: String,
    b: String,
    contains: bool,
    intersects: bool,
    common_level: i64,
    common_ok: bool,
}

#[derive(Deserialize)]
struct TokenCase {
    token: String,
    id: String,
}

#[derive(Deserialize)]
struct UnionCase {
    input: Vec<String>,
    normalized: Vec<String>,
    min_level: i64,
    level_mod: i64,
    denormalized: Option<Vec<String>>,
    denormalized_count: usize,
    denormalized_hash: String,
    probes: Vec<String>,
    contains_id: Vec<bool>,
    intersects_id: Vec<bool>,
    cap_bound: CapJson,
    rect_bound: [String; 4],
    from_range: Option<Vec<Vec<String>>>,
    coverings: Vec<super::loop_tests::Covering>,
}

#[derive(Deserialize)]
struct Golden {
    cells: Vec<CellCase>,
    pairs: Vec<PairCase>,
    tokens: Vec<TokenCase>,
    unions: Vec<UnionCase>,
}

fn hex(c: CellId) -> String {
    format!("{:016x}", c.id())
}

#[test]
fn cell_ids_match_go() {
    let g: Golden = load("cellid.json");
    let mut c = Checker::new("cellid");
    for k in &g.cells {
        let id = cell(&k.id);
        let t = &k.id;
        c.check(id.to_token() == k.token, || {
            format!("{t} token {} want {}", id.to_token(), k.token)
        });
        c.check(hex(CellId::from_token(&k.token)) == k.from_token, || {
            format!("{t} from_token")
        });
        c.check(
            id.level() == k.level
                && id.face() == k.face
                && id.is_leaf() == k.is_leaf
                && id.is_valid() == k.valid,
            || format!("{t} level/face/leaf/valid"),
        );
        let parents: Vec<String> = (0..=id.level()).map(|l| hex(id.parent(l))).collect();
        c.check(parents == k.parents, || format!("{t} parents"));
        c.check(
            hex(id.range_min()) == k.range_min && hex(id.range_max()) == k.range_max,
            || format!("{t} range"),
        );
        c.check(hex(id.next()) == k.next && hex(id.prev()) == k.prev, || {
            format!("{t} next/prev")
        });
        if let Some(ch) = &k.children {
            c.check(&ids(&id.children()) == ch, || format!("{t} children"));
            c.check(
                hex(id.child_begin()) == k.child_begin && hex(id.child_end()) == k.child_end,
                || format!("{t} child begin/end"),
            );
            let begins: Vec<String> = (id.level()..=(id.level() + 3).min(MAX_LEVEL))
                .map(|l| hex(id.child_begin_at_level(l)))
                .collect();
            let ends: Vec<String> = (id.level()..=(id.level() + 3).min(MAX_LEVEL))
                .map(|l| hex(id.child_end_at_level(l)))
                .collect();
            c.check(
                Some(&begins) == k.child_begin_at.as_ref()
                    && Some(&ends) == k.child_end_at.as_ref(),
                || format!("{t} child at level"),
            );
        }
        c.check(same_point(id.point(), &k.point), || {
            format!("{t} point {:?} want {:?}", id.point(), k.point)
        });
        let ll = id.lat_lng();
        c.check(
            same(ll.lat().radians(), f(&k.latlng[0])) && same(ll.lng().radians(), f(&k.latlng[1])),
            || format!("{t} latlng"),
        );
        if let Some(n) = &k.vertex_neighbors {
            c.check(&ids(&id.vertex_neighbors(id.level() - 1)) == n, || {
                format!("{t} vertex neighbors")
            });
        }
        let cl = Cell::from_cell_id(id);
        let uv = cl.bound_uv();
        c.check(
            same(uv.x().lo(), f(&k.bound_uv[0]))
                && same(uv.x().hi(), f(&k.bound_uv[1]))
                && same(uv.y().lo(), f(&k.bound_uv[2]))
                && same(uv.y().hi(), f(&k.bound_uv[3])),
            || format!("{t} bound uv"),
        );
        c.check(same_point(cl.center(), &k.center), || {
            format!("{t} cell center")
        });
        for (i, v) in k.vertices.iter().enumerate() {
            c.check(same_point(cl.vertex(i).unwrap(), v), || {
                format!("{t} vertex {i}")
            });
        }
        c.check(same_rect(cl.rect_bound(), &k.rect_bound), || {
            format!(
                "{t} rect bound {:?} want {:?}",
                cl.rect_bound(),
                k.rect_bound
            )
        });
        c.check(same_cap(cl.cap_bound(), &k.cap_bound), || {
            format!("{t} cap bound {:?}", cl.cap_bound())
        });
        for (p, want) in k.probes.iter().zip(&k.probe_in) {
            let got = cl.contains_point(super::golden::pt(p));
            c.check(got == *want, || {
                format!("{t} contains_point({p:?}) = {got}")
            });
        }
    }
    for k in &g.pairs {
        let (a, b) = (cell(&k.a), cell(&k.b));
        c.check(
            a.contains(b) == k.contains && a.intersects(b) == k.intersects,
            || format!("{} {} contains/intersects", k.a, k.b),
        );
        let common = a.common_ancestor_level(b);
        let want = if k.common_ok {
            Some(k.common_level)
        } else {
            None
        };
        c.check(common == want, || {
            format!("{} {} common ancestor {common:?} want {want:?}", k.a, k.b)
        });
    }
    for k in &g.tokens {
        let got = hex(CellId::from_token(&k.token));
        c.check(got == k.id, || {
            format!("from_token({:?}) = {got} want {}", k.token, k.id)
        });
    }
    for k in &g.unions {
        let mut u =
            ultragraphdb_core_types::CellUnion::new(k.input.iter().map(|s| cell(s)).collect());
        u.normalize();
        c.check(ids(u.as_slice()) == k.normalized, || {
            format!("normalize {:?}", k.input)
        });
        let mut d = u.clone();
        d.denormalize(k.min_level, k.level_mod);
        let tok: Vec<String> = d.iter().map(|c| c.to_token()).collect();
        c.check(
            d.len() == k.denormalized_count
                && super::golden::tokens_hash(&tok) == k.denormalized_hash,
            || {
                format!(
                    "denormalize({}, {}) of {:?}",
                    k.min_level, k.level_mod, k.normalized
                )
            },
        );
        if let Some(den) = &k.denormalized {
            c.check(&ids(d.as_slice()) == den, || "denormalized ids".to_string());
        }
        for ((p, ci), ii) in k.probes.iter().zip(&k.contains_id).zip(&k.intersects_id) {
            let id = cell(p);
            c.check(
                u.contains_cell_id(id) == *ci && u.intersects_cell_id(id) == *ii,
                || format!("union probe {p}"),
            );
        }
        c.check(same_cap(u.cap_bound(), &k.cap_bound), || {
            format!("union cap bound {:?}", u.cap_bound())
        });
        c.check(same_rect(u.rect_bound(), &k.rect_bound), || {
            format!("union rect bound {:?}", u.rect_bound())
        });
        for (ci, want) in u.iter().zip(k.from_range.iter().flatten()) {
            let r = ultragraphdb_core_types::CellUnion::from_range(
                ci.range_min(),
                ci.range_max().next(),
            );
            c.check(&ids(r.as_slice()) == want, || {
                format!("from_range of {:016x}", ci.id())
            });
        }
        for cov in &k.coverings {
            let got = cov.config.coverer().covering(&u);
            super::loop_tests::check_covering(&mut c, "cell union region", got.as_slice(), cov);
        }
    }
    c.finish();
}

// Ported from golang/geo s2/cellid_test.go, TestCellIDFromFace.
#[test]
fn cell_id_from_face() {
    for face in 0..6 {
        let fpl = CellId::from_face_pos_level(face, 0, 0);
        let f = CellId::from_face(face);
        assert_eq!(fpl, f, "face {face}");
    }
}

// Ported from golang/geo s2/cellid_test.go, TestCellIDSentinelRangeMinMax.
#[test]
fn cell_id_sentinel_range_min_max() {
    let s = CellId::SENTINEL;
    assert_eq!(s.range_min(), s);
    assert_eq!(s.range_max(), s);
}

// Ported from golang/geo s2/cellid_test.go, TestCellIDParentChildRelationships.
#[test]
fn cell_id_parent_child_relationships() {
    let ci = CellId::from_face_pos_level(3, 0x12345678, MAX_LEVEL - 4);
    assert!(ci.is_valid());
    assert_eq!(ci.face(), 3);
    assert_eq!(ci.pos(), 0x12345700);
    assert_eq!(ci.level(), 26); // 26 is MaxLevel - 4
    assert!(!ci.is_leaf());

    assert_eq!(ci.child_begin_at_level(ci.level() + 2).pos(), 0x12345610);
    assert_eq!(ci.child_begin().pos(), 0x12345640);
    assert_eq!(ci.children()[0].pos(), 0x12345640);
    assert_eq!(ci.parent(ci.level() - 1).pos(), 0x12345400);
    assert_eq!(ci.parent(ci.level() - 2).pos(), 0x12345000);

    assert!(ci.child_begin().id() < ci.id());
    assert!(ci.child_end().id() > ci.id());
    assert_eq!(ci.child_end(), ci.child_begin().next().next().next().next());
    assert_eq!(ci.range_min(), ci.child_begin_at_level(MAX_LEVEL));
    assert_eq!(ci.range_max().next(), ci.child_end_at_level(MAX_LEVEL));
}

// Ported from golang/geo s2/cellid_test.go, TestCellIDContainment.
#[test]
fn cell_id_containment() {
    let a = CellId::new(0x80855c0000000000); // Pittsburg
    let b = CellId::new(0x80855d0000000000); // child of a
    let c = CellId::new(0x80855dc000000000); // child of b
    let d = CellId::new(0x8085630000000000); // part of Pittsburg disjoint from a
    let tests = [
        (a, a, true, true),
        (a, b, true, true),
        (a, c, true, true),
        (a, d, false, false),
        (b, b, true, true),
        (b, c, true, true),
        (b, d, false, false),
        (c, c, true, true),
        (c, d, false, false),
        (d, d, true, true),
    ];
    for (x, y, contains, intersects) in tests {
        assert_eq!(x.contains(y), contains, "{x:?}.contains({y:?})");
        assert_eq!(x.intersects(y), intersects, "{x:?}.intersects({y:?})");
    }
}

// Ported from golang/geo s2/cellid_test.go, TestCellIDLatLng.
#[test]
fn cell_id_lat_lng() {
    // You can generate these with the s2cellid2latlngtestcase C++ program in this directory.
    let tests: [(u64, f64, f64); 9] = [
        (0x47a1cbd595522b39, 49.703498679, 11.770681595),
        (0x46525318b63be0f9, 55.685376759, 12.588490937),
        (0x52b30b71698e729d, 45.486546517, -93.449700022),
        (0x46ed8886cfadda85, 58.299984854, 23.049300056),
        (0x3663f18a24cbe857, 34.364439040, 108.330699969),
        (0x10a06c0a948cf5d, -30.694551352, -30.048758753),
        (0x2b2bfd076787c5df, -25.285264027, 133.823116966),
        (0xb09dff882a7809e1, -75.000000031, 0.000000133),
        (0x94daa3d000000001, -24.694439215, -47.537363213),
    ];
    for (id, lat, lng) in tests {
        let cell = CellId::new(id);
        let ll = LatLng::from_degrees(lat, lng);
        let l2 = cell.lat_lng();
        assert!(
            (l2.lat().radians() - ll.lat().radians()).abs() < 1e-9,
            "{id:x}"
        );
        assert!(
            (l2.lng().radians() - ll.lng().radians()).abs() < 1e-9,
            "{id:x}"
        );
        assert_eq!(CellId::from_lat_lng(ll), cell, "{id:x}");
    }
}

// Ported from golang/geo s2/cellid_test.go, TestCellIDVertexNeighbors. Go builds the expected
// cells with the unexported cellIDFromFaceIJ; here each is checked to be a distinct level-5
// cell of face 2 whose closed cell contains the face center they surround.
#[test]
fn cell_id_vertex_neighbors() {
    // Check the vertex neighbors of the center of face 2 at level 5.
    let center = ultragraphdb_core_types::Point::from_coords(0.0, 0.0, 1.0);
    let id = CellId::from_lat_lng(LatLng::from_point(center));
    let mut neighbors = id.vertex_neighbors(5);
    neighbors.sort();
    neighbors.dedup();
    assert_eq!(neighbors.len(), 4);
    for nbr in &neighbors {
        assert_eq!(nbr.level(), 5);
        assert_eq!(nbr.face(), 2);
        assert!(Cell::from_cell_id(*nbr).contains_point(center), "{nbr:?}");
    }

    // Check the vertex neighbors of the corner of faces 0, 4, and 5.
    let id = CellId::from_face_pos_level(0, 0, MAX_LEVEL);
    let mut neighbors = id.vertex_neighbors(0);
    neighbors.sort();
    assert_eq!(neighbors.len(), 3);
    assert_eq!(neighbors[0], CellId::from_face(0));
    assert_eq!(neighbors[1], CellId::from_face(4));
    assert_eq!(neighbors[2], CellId::from_face(5));
}

// Ported from golang/geo s2/cellid_test.go, TestCellIDTokensNominal.
#[test]
fn cell_id_tokens_nominal() {
    let tests: [(&str, u64); 22] = [
        ("1", 0x1000000000000000),
        ("3", 0x3000000000000000),
        ("14", 0x1400000000000000),
        ("41", 0x4100000000000000),
        ("094", 0x0940000000000000),
        ("537", 0x5370000000000000),
        ("3fec", 0x3fec000000000000),
        ("72f3", 0x72f3000000000000),
        ("52b8c", 0x52b8c00000000000),
        ("990ed", 0x990ed00000000000),
        ("4476dc", 0x4476dc0000000000),
        ("2a724f", 0x2a724f0000000000),
        ("7d4afc4", 0x7d4afc4000000000),
        ("b675785", 0xb675785000000000),
        ("40cd6124", 0x40cd612400000000),
        ("3ba32f81", 0x3ba32f8100000000),
        ("08f569b5c", 0x08f569b5c0000000),
        ("385327157", 0x3853271570000000),
        ("166c4d1954", 0x166c4d1954000000),
        ("96f48d8c39", 0x96f48d8c39000000),
        ("0bca3c7f74c", 0x0bca3c7f74c00000),
        ("1ae3619d12f", 0x1ae3619d12f00000),
    ];
    for (token, id) in tests {
        let ci = CellId::from_token(token);
        assert_eq!(ci.id(), id, "{token}");
        assert_eq!(ci.to_token(), token, "{token}");
    }
}

// Ported from golang/geo s2/cellid_test.go, TestCellIDFromTokensErrorCases.
#[test]
fn cell_id_from_tokens_error_cases() {
    let none_token = CellId::new(0).to_token();
    assert_eq!(none_token, "X");
    let none_id = CellId::from_token(&none_token);
    assert_eq!(none_id, CellId::new(0));

    // Sentinel is invalid.
    let token = CellId::SENTINEL.to_token();
    assert_eq!(CellId::from_token(&token), CellId::SENTINEL);

    // Check an invalid face.
    let face7 = CellId::from_face_pos_level(7, 0, 0);
    let token = face7.to_token();
    assert_eq!(CellId::from_token(&token), face7);

    for token in ["876b e99", "876bee99\n", "876[ee99", " 876bee99"] {
        assert_eq!(CellId::from_token(token), CellId::new(0), "{token:?}");
    }
}

// Ported from golang/geo s2/cellid_test.go, TestCellIDCommonAncestorLevel.
#[test]
fn cell_id_common_ancestor_level() {
    let face = |f: usize| CellId::from_face(f);
    let tests: [(CellId, CellId, Option<i64>); 8] = [
        // Identical cell IDs.
        (face(0), face(0), Some(0)),
        (
            face(0).child_begin_at_level(30),
            face(0).child_begin_at_level(30),
            Some(30),
        ),
        // One cell is a descendant of the other.
        (face(0).child_begin_at_level(30), face(0), Some(0)),
        (face(5), face(5).child_end_at_level(30).prev(), Some(0)),
        // No common ancestors.
        (face(0), face(5), None),
        (
            face(2).child_begin_at_level(30),
            face(3).child_end_at_level(20),
            None,
        ),
        // Common ancestor distinct from both.
        (
            face(5)
                .child_begin_at_level(9)
                .next()
                .child_begin_at_level(15),
            face(5).child_begin_at_level(9).child_begin_at_level(20),
            Some(8),
        ),
        (
            face(0).child_begin_at_level(2).child_begin_at_level(30),
            face(0)
                .child_begin_at_level(2)
                .next()
                .child_begin_at_level(5),
            Some(1),
        ),
    ];
    for (a, b, want) in tests {
        assert_eq!(a.common_ancestor_level(b), want, "{a:?} {b:?}");
        assert_eq!(b.common_ancestor_level(a), want, "{b:?} {a:?}");
    }
}

#[test]
fn cell_inputs_go_panics_on_answer() {
    // Go indexes past a cell's four vertices and past the face table for an invalid id, and shifts by a level
    // outside 0 to 30 as Go's shifts do, to 0.
    let cell = Cell::from_cell_id(CellId::from_lat_lng(LatLng::from_degrees(10.0, 20.0)));
    assert!(cell.vertex(3).is_some());
    assert_eq!(cell.vertex(4), None);
    assert_eq!(cell.vertex_raw(9), None);
    let _ = Cell::from_cell_id(CellId::SENTINEL).rect_bound();
    let id = CellId::from_lat_lng(LatLng::from_degrees(10.0, 20.0));
    for level in [-1, 31, 64, i64::MIN, i64::MAX] {
        let _ = id.parent(level);
        let _ = id.child_begin_at_level(level);
        let _ = id.child_end_at_level(level);
        let _ = CellId::from_face_pos_level(1, 0, level);
    }
    let _ = id.vertex_neighbors(30);
    let _ = id.vertex_neighbors(i64::MAX);
    assert_eq!(id.parent(MAX_LEVEL), id);
}
