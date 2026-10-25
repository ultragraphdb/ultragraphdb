/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported in part from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/cell_test.go.
 * Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Cell accessors, containment and intersection of cells, and the bounds' containment of the
//! cell's vertices. Cell geometry is checked bit for bit against Go in cellid_tests.

use ultragraphdb_core_types::{Cell, CellId, LatLng, MAX_LEVEL, Point, R3Vector};

fn face0_level2() -> CellId {
    CellId::from_face(0).child_begin_at_level(2)
}

// Ported from golang/geo s2/cell_test.go, TestCellFaces (the accessors; the edge and vertex
// multiplicities need Cell.Edge, which is not ported).
#[test]
fn cell_faces() {
    for face in 0..6 {
        let id = CellId::from_face(face);
        let cell = Cell::from_cell_id(id);
        assert_eq!(cell.id(), id);
        assert_eq!(cell.face(), face);
        assert_eq!(cell.level(), 0);
        // Top-level faces have alternating orientations to get right-handed coordinates.
        assert_eq!(cell.orientation(), face & 1);
        assert!(!cell.is_leaf());
        assert_eq!(cell.cell_union_bound(), vec![id]);
    }
    let leaf = Cell::from_cell_id(CellId::from_face_pos_level(3, 0x12345678, MAX_LEVEL));
    assert!(leaf.is_leaf());
    assert_eq!(leaf.level(), MAX_LEVEL);
    assert_eq!(leaf.face(), 3);
}

#[test]
fn cell_from_point_is_the_leaf_cell_of_the_point() {
    let tests = [
        (Point::from_coords(1.0, 0.0, 0.0), 0),
        (Point::from_coords(0.0, 1.0, 0.0), 1),
        (Point::from_coords(0.0, 0.0, 1.0), 2),
        (Point::from_coords(-1.0, 0.0, 0.0), 3),
        (Point::from_coords(0.0, -1.0, 0.0), 4),
        (Point::from_coords(0.0, 0.0, -1.0), 5),
    ];
    for (p, face) in tests {
        let cell = Cell::from_point(p);
        assert_eq!(cell.face(), face, "{p:?}");
        assert!(cell.is_leaf(), "{p:?}");
        assert!(cell.contains_point(p), "{p:?}");
        assert!(CellId::from_face(face).contains(cell.id()), "{p:?}");
    }
}

// Ported from golang/geo s2/cell_test.go, TestCellIntersectsCell.
#[test]
fn cell_intersects_cell() {
    let c = Cell::from_cell_id(face0_level2());
    let tests = [
        (c, Cell::from_cell_id(face0_level2()), true),
        (
            c,
            Cell::from_cell_id(face0_level2().child_begin_at_level(5)),
            true,
        ),
        (c, Cell::from_cell_id(face0_level2().next()), false),
    ];
    for (a, b, want) in tests {
        assert_eq!(
            a.intersects_cell(&b),
            want,
            "{:?} intersects {:?}",
            a.id(),
            b.id()
        );
    }
}

// Ported from golang/geo s2/cell_test.go, TestCellContainsCell.
#[test]
fn cell_contains_cell() {
    let c = Cell::from_cell_id(face0_level2());
    let child = Cell::from_cell_id(face0_level2().child_begin_at_level(5));
    let next = Cell::from_cell_id(face0_level2().next());
    let tests = [
        (c, c, true),
        (c, child, true),
        (child, c, false),
        (next, c, false),
        (c, next, false),
    ];
    for (a, b, want) in tests {
        assert_eq!(
            a.contains_cell(&b),
            want,
            "{:?} contains {:?}",
            a.id(),
            b.id()
        );
    }
}

// Ported from golang/geo s2/cell_test.go, TestCellRectBound.
#[test]
fn cell_rect_bound_contains_the_vertices() {
    for (lat, lng) in [
        (50.0, 50.0),
        (-50.0, 50.0),
        (50.0, -50.0),
        (-50.0, -50.0),
        (0.0, 0.0),
        (0.0, 180.0),
        (0.0, -179.0),
    ] {
        let c = Cell::from_cell_id(CellId::from_lat_lng(LatLng::from_degrees(lat, lng)));
        let rect = c.rect_bound();
        for i in 0..4 {
            let v = c.vertex(i).unwrap();
            assert!(
                rect.contains_lat_lng(LatLng::from_point(v)),
                "({lat}, {lng}) vertex {i}"
            );
        }
    }
}

// Ported from golang/geo s2/cell_test.go, TestCellRectBoundAroundPoleMinLat.
#[test]
fn cell_rect_bound_around_pole_min_lat() {
    let tests = [
        (
            CellId::from_face_pos_level(2, 0, 0),
            LatLng::from_degrees(3.0, 0.0),
            false,
        ),
        (
            CellId::from_face_pos_level(2, 0, 0),
            LatLng::from_degrees(50.0, 0.0),
            true,
        ),
        (
            CellId::from_face_pos_level(5, 0, 0),
            LatLng::from_degrees(-3.0, 0.0),
            false,
        ),
        (
            CellId::from_face_pos_level(5, 0, 0),
            LatLng::from_degrees(-50.0, 0.0),
            true,
        ),
    ];
    for (id, ll, want) in tests {
        assert_eq!(
            Cell::from_cell_id(id).rect_bound().contains_lat_lng(ll),
            want,
            "{id:?} {ll:?}"
        );
    }
}

// Ported from golang/geo s2/cell_test.go, TestCellCapBound.
#[test]
fn cell_cap_bound_contains_the_vertices() {
    let c = Cell::from_cell_id(CellId::from_face(0).child_begin_at_level(20));
    let cap = c.cap_bound();
    for i in 0..4 {
        assert!(cap.contains_point(c.vertex(i).unwrap()), "vertex {i}");
    }
}

// Ported from golang/geo s2/cell_test.go, TestCellContainsPoint.
#[test]
fn cell_contains_point() {
    let c = Cell::from_cell_id(face0_level2());
    let child = Cell::from_cell_id(face0_level2().child_begin_at_level(5));
    let next_child = Cell::from_cell_id(face0_level2().next().child_begin_at_level(5));
    let tests = [
        (c, child.vertex(1).unwrap(), true),
        (c, c.vertex(1).unwrap(), true),
        (child, next_child.vertex(1).unwrap(), false),
    ];
    for (cell, p, want) in tests {
        assert_eq!(
            cell.contains_point(p),
            want,
            "{:?} contains {p:?}",
            cell.id()
        );
    }
    // A point on the opposite face is never contained.
    assert!(!c.contains_point(Point::from_coords(-1.0, 0.0, 0.0)));
    // A point that is not finite is never contained.
    assert!(!c.contains_point(Point::from_vector(R3Vector::new(f64::NAN, 0.0, 0.0))));
}

// Ported from golang/geo s2/cell_test.go, TestCellContainsPointConsistentWithS2CellIDFromPointExample1
// and TestCellContainsPointContainsAmbiguousPoint.
#[test]
#[allow(clippy::excessive_precision)]
fn cell_contains_point_of_its_own_id() {
    let p = Point::from_coords(
        0.38203141040035632,
        0.030196609707941954,
        0.9236558700239289,
    );
    assert!(Cell::from_point(p).contains_point(p));
    // x is ~6.1e-17 rather than 0, so s rounds to 0.5 and the cell id picks a neighbouring cell;
    // the expanded bound still contains the point.
    let p = Point::from_lat_lng(LatLng::from_degrees(-2.0, 90.0));
    let cell = Cell::from_cell_id(Cell::from_point(p).id().parent(1));
    assert!(cell.contains_point(p));
}
