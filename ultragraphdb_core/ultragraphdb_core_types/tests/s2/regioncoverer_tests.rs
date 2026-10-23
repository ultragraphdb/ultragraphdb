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

use super::golden::{cell, f, load};
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
