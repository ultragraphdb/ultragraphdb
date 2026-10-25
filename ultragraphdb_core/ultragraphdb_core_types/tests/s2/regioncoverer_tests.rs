/*
 * Copyright 2015 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported in part from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/regioncoverer_test.go; the golden-vector checks are new.
 * Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! The deterministic RegionCoverer tests of golang/geo's regioncoverer_test.go. The coverings
//! themselves are compared with Go in loop_tests and cellid_tests.

use super::golden::{cell, f, load, tokens_hash};
use super::loop_tests::dgraph_loop;
use serde::Deserialize;
use ultragraphdb_core_types::{
    Angle, Cell, CellId, CellUnion, LatLng, Loop, Point, Region, RegionCoverer,
};

#[derive(Deserialize)]
struct CellCase {
    id: String,
}

#[derive(Deserialize)]
struct CellGolden {
    cells: Vec<CellCase>,
}

// Ported from golang/geo s2/regioncoverer_test.go, TestCovererRandomCells, over the cells of
// the golden set instead of Go's random ones.
#[test]
fn coverer_random_cells() {
    let rc = RegionCoverer::new(0, 30, 1, 1);
    let g: CellGolden = load("cellid.json");
    for k in &g.cells {
        let id = cell(&k.id);
        let covering = rc.covering(&Cell::from_cell_id(id));
        assert_eq!(
            covering.as_slice(),
            &[id],
            "cell id token {}",
            id.to_token()
        );
    }
}

/// checkCoveringTight: cover completely covers the region, and holds no cell that does not
/// intersect it.
fn check_covering_tight<R: Region>(r: &R, cover: &CellUnion, id: CellId) {
    if !id.is_valid() {
        for face in 0..6 {
            check_covering_tight(r, cover, CellId::from_face(face));
        }
        return;
    }

    if !r.intersects_cell(&Cell::from_cell_id(id)) {
        // If region does not intersect id, then neither should the covering.
        assert!(
            !cover.intersects_cell_id(id),
            "covering intersects {} outside the region",
            id.to_token()
        );
    } else if !cover.contains_cell_id(id) {
        // The region may intersect id, but we can't assert that the covering
        // intersects id because we may discover that the region does not actually
        // intersect upon further subdivision.  (IntersectsCell is not exact.)
        assert!(
            !r.contains_cell(&Cell::from_cell_id(id)),
            "region contains uncovered {}",
            id.to_token()
        );
        assert!(!id.is_leaf(), "uncovered leaf {}", id.to_token());
        let end = id.child_end();
        let mut child = id.child_begin();
        while child != end {
            check_covering_tight(r, cover, child);
            child = child.next();
        }
    }
}

/// checkCovering: the levels obey the coverer and the covering is a tight cover.
fn check_covering<R: Region>(rc: &RegionCoverer, r: &R, covering: &CellUnion) {
    let mut min_level_cells = std::collections::BTreeMap::new();
    for ci in covering.iter() {
        let level = ci.level();
        assert!(
            level >= rc.min_level(),
            "{} level {level} < min level",
            ci.to_token()
        );
        assert!(
            level <= rc.max_level(),
            "{} level {level} > max level",
            ci.to_token()
        );
        assert_eq!(
            (level - rc.min_level()) % rc.level_mod().max(1),
            0,
            "{} level mod",
            ci.to_token()
        );
        *min_level_cells
            .entry(ci.parent(rc.min_level()))
            .or_insert(0) += 1;
    }
    if covering.len() as i64 > rc.max_cells() {
        // If the covering has more than the requested number of cells, then check
        // that the cell count cannot be reduced by using the parent of some cell.
        for (ci, count) in min_level_cells {
            assert_eq!(count, 1, "min level cell {}", ci.to_token());
        }
    }
    let mut temp = covering.clone();
    temp.normalize();
    check_covering_tight(r, &temp, CellId::new(0));
}

/// Dgraph's covering parameters (types/s2index.go): MinCellLevel 5, MaxCellLevel 16, LevelMod 0
/// (clamped to 1) and MaxCells 18.
fn dgraph_coverer() -> RegionCoverer {
    RegionCoverer::new(5, 16, 0, 18)
}

#[derive(Deserialize)]
struct Spec {
    name: String,
    ring: Option<Vec<[String; 2]>>,
}

#[derive(Deserialize)]
struct LoopCase {
    spec: Spec,
}

// The covering checks of golang/geo's TestCovererRandomCaps, applied to the loops Dgraph's
// tests index, with Dgraph's parameters and with LevelMod 2 and 3.
#[test]
fn coverings_of_dgraph_loops_are_valid() {
    let cases: Vec<LoopCase> = load("loops_testdata.json");
    let mut loops: Vec<(String, Loop)> = cases
        .iter()
        .filter_map(|k| {
            let ring: Vec<[f64; 2]> = k
                .spec
                .ring
                .as_ref()?
                .iter()
                .map(|p| [f(&p[0]), f(&p[1])])
                .collect();
            if k.spec.name.ends_with("poison") {
                return None;
            }
            Some((k.spec.name.clone(), dgraph_loop(&ring).0))
        })
        .collect();
    let near = Point::from_lat_lng(LatLng::from_degrees(37.4249518, -122.082506));
    loops.push((
        "near-1000m".to_string(),
        Loop::regular(near, Angle::from_radians(1000.0 / 6371000.0), 100),
    ));
    for (name, l) in &loops {
        for rc in [
            dgraph_coverer(),
            RegionCoverer::new(4, 20, 2, 12),
            RegionCoverer::new(3, 15, 3, 10),
        ] {
            let covering = rc.covering(l);
            assert!(!covering.is_empty(), "{name}");
            check_covering(&rc, l, &covering);
            // Check that Covering is deterministic.
            assert_eq!(rc.covering(l), covering, "{name}");
            let mut d = covering.clone();
            d.denormalize(rc.min_level(), rc.level_mod().max(1));
            check_covering(&rc, l, &d);
        }
    }
}

// Go's int64 arithmetic wraps on a max cells near its limits; the port's covering is Go's.
// The expected coverings were generated by golang/geo for RegularLoop(10:20, 0.01 rad, 8)
// with MinLevel 5, MaxLevel 16, LevelMod 0.
#[test]
fn extreme_max_cells_match_go() {
    let l = loop_10_20();
    let tiny: GoCovering = (
        4,
        "a875fb39c8a32a1a",
        Some(&["10d7", "10d84", "1127f", "1129"]),
    );
    let fast: GoCovering = (
        4,
        "db5278ddf978aa1f",
        Some(&["10d4", "10dc", "1124", "112c"]),
    );
    let huge: GoCovering = (3255, "a8338802deffcd86", None);
    for (max_cells, covering) in [
        (i64::MIN, &tiny),
        (-1, &tiny),
        (0, &tiny),
        (i64::MAX, &huge),
    ] {
        let rc = RegionCoverer::new(5, 16, 0, max_cells);
        assert_go_covering(&format!("covering {max_cells}"), &rc.covering(&l), covering);
        assert_go_covering(
            &format!("fast covering {max_cells}"),
            &rc.fast_covering(&l),
            &fast,
        );
    }
}

// Go's Denormalize requires 0 <= minLevel <= 30 and levelMod in 1..=3, and overflows or never
// ends on others; the port clamps them as the RegionCoverer clamps its own (a deliberate
// deviation documented on CellUnion::denormalize).
#[test]
fn denormalize_clamps_out_of_range_parameters() {
    let face2 = CellId::from_face(2);
    let level9 = face2.child_begin_at_level(9);
    let mut u = CellUnion::new(vec![level9]);
    // Clamped to min level 0 and level mod 1: nothing changes.
    u.denormalize(i64::MIN, i64::MIN);
    assert_eq!(u.as_slice(), &[level9]);
    // Clamped to level mod 3 from min level 4: level 9 is not 4 + 3k, so Go's
    // newLevel += (30 - (9 - 4)) % 3 makes it level 10, its four children.
    u.denormalize(4, i64::MAX);
    assert_eq!(u.as_slice(), &level9.children());
    // A min level above 30 is clamped to 30: every cell becomes its leaves; a level-28 cell
    // has 16 of them.
    let level28 = face2.child_begin_at_level(28);
    let mut u = CellUnion::new(vec![level28]);
    u.denormalize(i64::MAX, 1);
    assert_eq!(u.len(), 16);
    assert!(u.iter().all(|c| c.is_leaf() && level28.contains(*c)));
    assert_eq!(u.as_slice()[0], level28.range_min());
    assert_eq!(u.as_slice()[15], level28.range_max());
}

/// A covering as golang/geo returns it: the cell count, the FNV hash of its tokens (the
/// generator's covering hash) and, for short coverings, the tokens themselves.
type GoCovering = (usize, &'static str, Option<&'static [&'static str]>);

/// A region's name, the coverer's (MinLevel, MaxLevel, LevelMod, MaxCells), whether the
/// covering is FastCovering (else CellUnion), and Go's covering.
type GoCoveringCase = (&'static str, (i64, i64, i64, i64), bool, GoCovering);

/// golang/geo's test helper parsePoints: "lat:lng, lat:lng, ..." in degrees.
fn loop_of(s: &str) -> Loop {
    Loop::from_points(
        s.split(',')
            .map(|ll| {
                let (lat, lng) = ll.trim().split_once(':').expect("lat:lng");
                Point::from_lat_lng(LatLng::from_degrees(
                    lat.parse().expect("lat"),
                    lng.parse().expect("lng"),
                ))
            })
            .collect(),
    )
}

fn loop_10_20() -> Loop {
    Loop::regular(
        Point::from_lat_lng(LatLng::from_degrees(10.0, 20.0)),
        Angle::from_radians(0.01),
        8,
    )
}

fn assert_go_covering(tag: &str, got: &CellUnion, want: &GoCovering) {
    let t: Vec<String> = got.iter().map(|c| c.to_token()).collect();
    if let Some(w) = want.2 {
        assert_eq!(t, w, "{tag}");
    }
    assert_eq!(
        (t.len(), tokens_hash(&t).as_str()),
        (want.0, want.1),
        "{tag}: count and token hash"
    );
}

// FastCovering (fast == true) and CellUnion (fast == false) of five regions under six
// parameter sets, generated by golang/geo v0.0.0-20260427214057-41a1a8c7eb2a (with
// -gcflags=all=-d=fmahash=n) from the same regions: a regular loop at 10:20, the 1000 m loop
// near Mountain View, golang/geo's arctic80 and candyCane loops, and the normalized NYC/SFO
// cell union of TestCellUnion.
#[rustfmt::skip]
const GO_COVERINGS: &[GoCoveringCase] = &[
    ("loop10_20", (0, 30, 1, 8), true, (4, "db5278ddf978aa1f", Some(&["10d4", "10dc", "1124", "112c"]))),
    ("loop10_20", (0, 30, 1, 8), false, (8, "1a811ead78192477", Some(&["10d63", "10d64b", "10d7c", "10d84", "1127f", "11284", "1129b4", "1129d"]))),
    ("loop10_20", (5, 16, 0, 18), true, (4, "db5278ddf978aa1f", Some(&["10d4", "10dc", "1124", "112c"]))),
    ("loop10_20", (5, 16, 0, 18), false, (12, "c752f586b3e717e7", Some(&["10d63", "10d64b", "10d7c", "10d804", "10d81d", "10d81f", "10d8204", "1127e4", "1127fc", "11284", "1129b4", "1129d"]))),
    ("loop10_20", (4, 20, 2, 12), true, (2, "1f76f0220da2a725", Some(&["10d", "113"]))),
    ("loop10_20", (4, 20, 2, 12), false, (9, "eb5b973497be5a9b", Some(&["10d7", "10d81", "10d821", "1127f", "11284", "1129b3", "1129b5", "1129b7", "1129d"]))),
    ("loop10_20", (3, 15, 3, 10), true, (2, "68243582aea1d375", Some(&["10c", "114"]))),
    ("loop10_20", (3, 15, 3, 10), false, (8, "debdb3ac5570143e", Some(&["10d7", "10d804", "10d81c", "10d8201", "10d8207", "1127e4", "1127fc", "1129"]))),
    ("loop10_20", (0, 30, 1, 1), true, (1, "456c5118181fcf01", Some(&["11"]))),
    ("loop10_20", (0, 30, 1, 1), false, (1, "456c5118181fcf01", Some(&["11"]))),
    ("loop10_20", (7, 9, 1, 3), true, (64, "a72d6c82077e54ed", None)),
    ("loop10_20", (7, 9, 1, 3), false, (6, "8913c3c20ba4528b", Some(&["10d64", "10d7c", "10d84", "1127f", "11284", "1129c"]))),
    ("near1000m", (0, 30, 1, 8), true, (4, "bd6a2adfaffb4cf8", Some(&["808fb0c", "808fb74", "808fb9c", "808fba4"]))),
    ("near1000m", (0, 30, 1, 8), false, (8, "cd28027474739df0", Some(&["808fb755", "808fb7564", "808fb9e4", "808fb9effc", "808fb9f4", "808fb9fc", "808fba04", "808fba1c"]))),
    ("near1000m", (5, 16, 0, 18), true, (4, "bd6a2adfaffb4cf8", Some(&["808fb0c", "808fb74", "808fb9c", "808fba4"]))),
    ("near1000m", (5, 16, 0, 18), false, (15, "b7f780811525b9b9", None)),
    ("near1000m", (4, 20, 2, 12), true, (4, "a8ca6f7cf9b30050", Some(&["808fb1", "808fb7", "808fb9", "808fbb"]))),
    ("near1000m", (4, 20, 2, 12), false, (9, "c685e75d990ea7ff", Some(&["808fb755", "808fb7561", "808fb7563", "808fb9f", "808fba01", "808fba03", "808fba05", "808fba06b", "808fba1c"]))),
    ("near1000m", (3, 15, 3, 10), true, (2, "653d164c9289c9e2", Some(&["808fb4", "808fbc"]))),
    ("near1000m", (3, 15, 3, 10), false, (5, "cd0b3134aaa932d3", Some(&["808fb7554", "808fb755c", "808fb7564", "808fb9f", "808fba1"]))),
    ("near1000m", (0, 30, 1, 1), true, (1, "61a6700e7b2cf74d", Some(&["808fb"]))),
    ("near1000m", (0, 30, 1, 1), false, (1, "61a6700e7b2cf74d", Some(&["808fb"]))),
    ("near1000m", (7, 9, 1, 3), true, (2, "653d164c9289c9e2", Some(&["808fb4", "808fbc"]))),
    ("near1000m", (7, 9, 1, 3), false, (2, "653d164c9289c9e2", Some(&["808fb4", "808fbc"]))),
    ("arctic80", (0, 30, 1, 8), true, (1, "07eb1607b4aeab94", Some(&["5"]))),
    ("arctic80", (0, 30, 1, 8), false, (8, "006d06d67c13724a", Some(&["453", "455", "4564", "4f9", "4ff", "501", "507", "5ac"]))),
    ("arctic80", (5, 16, 0, 18), true, (1024, "3956d9e0275b4f8d", None)),
    ("arctic80", (5, 16, 0, 18), false, (18, "2fdde2970d66dec2", None)),
    ("arctic80", (4, 20, 2, 12), true, (256, "18f92bb6b68fdfc5", None)),
    ("arctic80", (4, 20, 2, 12), false, (12, "55cec3bb773d0c43", Some(&["453", "455", "4561", "45675", "4f9", "4ff", "501", "507", "5a98b", "5a9f", "5ab", "5ad"]))),
    ("arctic80", (3, 15, 3, 10), true, (64, "6a3d2a68c95d9dc9", None)),
    ("arctic80", (3, 15, 3, 10), false, (4, "c5600d00a0463857", Some(&["454", "4fc", "504", "5ac"]))),
    ("arctic80", (0, 30, 1, 1), true, (1, "07eb1607b4aeab94", Some(&["5"]))),
    ("arctic80", (0, 30, 1, 1), false, (1, "07eb1607b4aeab94", Some(&["5"]))),
    ("arctic80", (7, 9, 1, 3), true, (16384, "ad579ad6232eaac5", None)),
    ("arctic80", (7, 9, 1, 3), false, (78, "2404a611e94162bc", None)),
    ("candy", (0, 30, 1, 8), true, (6, "33935a94ceac40de", Some(&["1", "3", "5", "7", "9", "b"]))),
    ("candy", (0, 30, 1, 8), false, (8, "d1dca2035d0f33f8", Some(&["1", "24", "34", "3c", "7", "94", "9c", "a4"]))),
    ("candy", (5, 16, 0, 18), true, (6144, "17a287de3c626f25", None)),
    ("candy", (5, 16, 0, 18), false, (339, "297a6b14cbf45ff1", None)),
    ("candy", (4, 20, 2, 12), true, (1536, "7187e521601ec9b5", None)),
    ("candy", (4, 20, 2, 12), false, (141, "b66d5d1704cb0fcf", None)),
    ("candy", (3, 15, 3, 10), true, (384, "cd703aca3d82899d", None)),
    ("candy", (3, 15, 3, 10), false, (67, "8e86f79ee9a4a05f", None)),
    ("candy", (0, 30, 1, 1), true, (6, "33935a94ceac40de", Some(&["1", "3", "5", "7", "9", "b"]))),
    ("candy", (0, 30, 1, 1), false, (5, "9eb1e7c1e879bfc8", Some(&["1", "3", "7", "9", "a4"]))),
    ("candy", (7, 9, 1, 3), true, (98304, "44d763131bfa0ba5", None)),
    ("candy", (7, 9, 1, 3), false, (1550, "2f8430527003b07f", None)),
    ("nyc_sfo", (0, 30, 1, 8), true, (3, "e96ac3f736d1ca4a", Some(&["5", "7", "9"]))),
    ("nyc_sfo", (0, 30, 1, 8), false, (8, "69c9cd4fb8e21e60", Some(&["808581", "808587", "808f7d", "808f7f", "89c259", "89c25b", "89c2f5", "89c2f7"]))),
    ("nyc_sfo", (5, 16, 0, 18), true, (3072, "3330f9524c31471d", None)),
    ("nyc_sfo", (5, 16, 0, 18), false, (8, "69c9cd4fb8e21e60", Some(&["808581", "808587", "808f7d", "808f7f", "89c259", "89c25b", "89c2f5", "89c2f7"]))),
    ("nyc_sfo", (4, 20, 2, 12), true, (768, "be8ba41d38631b65", None)),
    ("nyc_sfo", (4, 20, 2, 12), false, (8, "69c9cd4fb8e21e60", Some(&["808581", "808587", "808f7d", "808f7f", "89c259", "89c25b", "89c2f5", "89c2f7"]))),
    ("nyc_sfo", (3, 15, 3, 10), true, (192, "5bb526146bc4ff69", None)),
    ("nyc_sfo", (3, 15, 3, 10), false, (4, "1b176df91c6611a0", Some(&["808584", "808f7c", "89c25c", "89c2f4"]))),
    ("nyc_sfo", (0, 30, 1, 1), true, (3, "e96ac3f736d1ca4a", Some(&["5", "7", "9"]))),
    ("nyc_sfo", (0, 30, 1, 1), false, (1, "08141607b4d1b0a8", Some(&["9"]))),
    ("nyc_sfo", (7, 9, 1, 3), true, (49152, "70b1ce0d94572885", None)),
    ("nyc_sfo", (7, 9, 1, 3), false, (4, "1b176df91c6611a0", Some(&["808584", "808f7c", "89c25c", "89c2f4"]))),
];

#[test]
fn fast_covering_and_cell_union_match_go() {
    let mut nyc_sfo = CellUnion::new(
        [
            0x89c25b0000000000u64,
            0x89c2590000000000,
            0x89c2f70000000000,
            0x89c2f50000000000,
            0x8085870000000000,
            0x8085810000000000,
            0x808f7d0000000000,
            0x808f7f0000000000,
        ]
        .into_iter()
        .map(CellId::new)
        .collect(),
    );
    nyc_sfo.normalize();
    let near = Point::from_lat_lng(LatLng::from_degrees(37.4249518, -122.082506));
    let loops = [
        ("loop10_20", loop_10_20()),
        (
            "near1000m",
            Loop::regular(near, Angle::from_radians(1000.0 / 6371000.0), 100),
        ),
        ("arctic80", loop_of("80:-150, 80:-30, 80:90")),
        (
            "candy",
            loop_of("-20:150, -20:-70, 0:70, 10:-150, 10:70, -10:-70"),
        ),
    ];
    for (name, (min, max, lmod, cells), fast, want) in GO_COVERINGS {
        let rc = RegionCoverer::new(*min, *max, *lmod, *cells);
        let tag = format!("{name} ({min}, {max}, {lmod}, {cells}) fast {fast}");
        let got = match loops.iter().find(|(n, _)| n == name) {
            Some((_, l)) if *fast => rc.fast_covering(l),
            Some((_, l)) => rc.cell_union(l),
            None if *fast => rc.fast_covering(&nyc_sfo),
            None => rc.cell_union(&nyc_sfo),
        };
        assert_go_covering(&tag, &got, want);
    }
}

// Go's NewRegionCoverer: MinLevel 0, MaxLevel 30, LevelMod 1, MaxCells 8; its Covering of
// a region is then its CellUnion (Denormalize(0, 1) changes nothing).
#[test]
fn default_coverer_is_go_new_region_coverer() {
    let rc = RegionCoverer::default();
    assert_eq!(rc, RegionCoverer::new(0, 30, 1, 8));
    assert_eq!(
        (
            rc.min_level(),
            rc.max_level(),
            rc.level_mod(),
            rc.max_cells()
        ),
        (0, 30, 1, 8)
    );
    assert_go_covering(
        "loop10_20 default covering",
        &rc.covering(&loop_10_20()),
        &(
            8,
            "1a811ead78192477",
            Some(&[
                "10d63", "10d64b", "10d7c", "10d84", "1127f", "11284", "1129b4", "1129d",
            ]),
        ),
    );
}
