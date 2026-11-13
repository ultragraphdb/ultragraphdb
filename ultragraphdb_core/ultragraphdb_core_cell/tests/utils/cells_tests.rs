/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_cell::{CellId, EARTH_RADIUS_METRES, cells_around};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::fixtures::{PUBLISHED, face_cell, leaf, radius_cells};

/// A radius of zero, or below, files the point under its leaf.
#[test]
fn test_no_radius_is_the_points_leaf() -> TestResult {
    let own = vec![CellId::new(0x47a1cbd595522b39)];
    assert_eq!(radius_cells(49.703498679, 11.770681595, 0.0)?, own);
    assert_eq!(radius_cells(49.703498679, 11.770681595, -5.0)?, own);
    Ok(())
}

/// The four cells around the centre of face 2, at level 5: the published vertex neighbours of the north pole
/// (`TestCellIDVertexNeighbors`). A radius of a hundredth of the Earth's is the angle 0.01, which sets level 5: the
/// finest whose cells are at least twice that wide.
#[test]
fn test_a_radius_files_the_four_cells_meeting_at_the_nearest_vertex() -> TestResult {
    let around = radius_cells(90.0, 0.0, EARTH_RADIUS_METRES / 100.0)?;
    assert_eq!(around.len(), 4);
    for cell in &around {
        assert_eq!((cell.face(), cell.level()), (Some(2), Some(5)));
    }
    // A point just off the pole in each quadrant lies in one of the four.
    for lng in [45.0, 135.0, -135.0, -45.0] {
        let quadrant = leaf(89.999, lng)?.parent(5).context("a level-5 ancestor")?;
        assert!(around.contains(&quadrant), "{lng}");
    }
    Ok(())
}

/// The corner of faces 0, 4 and 5 has three neighbours, the three faces (`TestCellIDVertexNeighbors`).
#[test]
fn test_a_cube_corner_has_three_cells() -> TestResult {
    assert_eq!(
        radius_cells(-30.0, -40.0, 1_911_303.0)?,
        vec![face_cell(0), face_cell(4), face_cell(5)]
    );
    Ok(())
}

/// A radius wider than a face is filed under all six.
#[test]
fn test_a_radius_wider_than_a_face_is_every_face() -> TestResult {
    assert_eq!(
        radius_cells(10.0, 10.0, 10_000_000.0)?,
        (0..6).map(face_cell).collect::<Vec<_>>()
    );
    Ok(())
}

/// Near the edge of faces 0 and 1 the cells around the point lie on both.
#[test]
fn test_a_covering_crosses_a_face_edge() -> TestResult {
    let around = radius_cells(0.0, 44.999_99, 1_000.0)?;
    let faces: Vec<u8> = around.iter().filter_map(|cell| cell.face()).collect();
    assert!(faces.contains(&0) && faces.contains(&1), "{faces:?}");
    assert_eq!(around.len(), 4, "an edge, not a corner, has four");
    Ok(())
}

/// A coordinate or a radius that is NaN or infinite has no cells.
#[test]
fn test_a_non_finite_coordinate_or_radius_has_no_cells() {
    for (lat, lng, radius) in [
        (f64::NAN, 0.0, 0.0),
        (0.0, f64::NAN, 0.0),
        (f64::INFINITY, 0.0, 0.0),
        (0.0, f64::NEG_INFINITY, 0.0),
        (0.0, 0.0, f64::NAN),
        (0.0, 0.0, f64::INFINITY),
    ] {
        assert_eq!(
            cells_around(lat, lng, radius),
            None,
            "{lat}, {lng}, {radius}"
        );
    }
}

/// Across each of the cube's twelve edges, the cells around a point just off the edge include the cell the other
/// side's point lies in, computed directly on that face: the wrap onto the neighbouring face lands where the
/// projection does.
#[test]
fn test_every_face_edge_wraps_onto_its_neighbour() -> TestResult {
    // Each edge as a point on it, and the direction across it, in degrees of latitude and longitude.
    let edges: [(f64, f64, f64, f64); 12] = [
        (10.0, 45.0, 0.0, 1.0),   // faces 0 and 1
        (10.0, -45.0, 0.0, 1.0),  // faces 0 and 4
        (10.0, 135.0, 0.0, 1.0),  // faces 1 and 3
        (10.0, -135.0, 0.0, 1.0), // faces 3 and 4
        (45.0, 0.0, 1.0, 0.0),    // faces 0 and 2
        (45.0, 90.0, 1.0, 0.0),   // faces 1 and 2
        (45.0, 180.0, 1.0, 0.0),  // faces 2 and 3
        (45.0, -90.0, 1.0, 0.0),  // faces 2 and 4
        (-45.0, 0.0, 1.0, 0.0),   // faces 0 and 5
        (-45.0, 90.0, 1.0, 0.0),  // faces 1 and 5
        (-45.0, 180.0, 1.0, 0.0), // faces 3 and 5
        (-45.0, -90.0, 1.0, 0.0), // faces 4 and 5
    ];
    let step = 0.000_5;
    for (lat, lng, d_lat, d_lng) in edges {
        let near = (lat - d_lat * step, lng - d_lng * step);
        let far = (lat + d_lat * step, lng + d_lng * step);
        for (from, to) in [(near, far), (far, near)] {
            let around = radius_cells(from.0, from.1, 200.0)?;
            let at = around
                .first()
                .and_then(|cell| cell.level())
                .context("a cell")?;
            let across = leaf(to.0, to.1)?.parent(at).context("an ancestor")?;
            assert_ne!(
                leaf(from.0, from.1)?.face(),
                across.face(),
                "{from:?} crosses an edge"
            );
            assert!(around.contains(&across), "{from:?} to {to:?}: {around:?}");
        }
    }
    Ok(())
}

/// Around any point, wherever it lies in its cell, the cells at the nearest vertex are distinct, share one level,
/// and include the point's own; and the same inputs give the same cells.
#[test]
fn test_the_cells_around_any_point_are_distinct_and_include_its_own() -> TestResult {
    for (_, lat, lng) in PUBLISHED {
        let around = radius_cells(lat, lng, 1_000.0)?;
        assert_eq!(around, radius_cells(lat, lng, 1_000.0)?);
        let at = around
            .first()
            .and_then(|cell| cell.level())
            .context("a cell")?;
        assert!(around.iter().all(|cell| cell.level() == Some(at)));
        let mut distinct = around.clone();
        distinct.dedup();
        assert_eq!(distinct.len(), around.len(), "{lat}, {lng}: {around:?}");
        assert!(around.len() >= 3);
        assert!(around.contains(&leaf(lat, lng)?.parent(at).context("an ancestor")?));
    }
    Ok(())
}
