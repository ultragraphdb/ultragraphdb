/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The planar ray test `contains` runs for a point query against a polygon, checked against exact integer geometry:
//! the point at the origin, whose degrees round-trip exactly, and triangles with integer vertices, whose orientations
//! integer arithmetic decides without rounding.

use ultragraphdb_core_types::{Geometry, Layout, QueryType, query_tokens_geo};
use ultragraphdb_tool_bdd::TestResult;

/// The span of the grid the triangles' first two vertices lie on, around the origin.
const SPAN: i64 = 4;

/// The third vertex of every triangle, off the grid, so its edges to the grid cross the origin's horizontal at many
/// slopes.
const APEX: (i64, i64) = (0, SPAN + 1);

/// Twice the signed area of the triangle `o`, `a`, `b`: positive when `b` lies left of `o` to `a`.
fn cross(o: (i64, i64), a: (i64, i64), b: (i64, i64)) -> i64 {
    (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
}

/// Whether the origin lies on the segment `a`, `b`.
fn origin_on_segment(a: (i64, i64), b: (i64, i64)) -> bool {
    cross(a, b, (0, 0)) == 0
        && a.0.min(b.0) <= 0
        && 0 <= a.0.max(b.0)
        && a.1.min(b.1) <= 0
        && 0 <= a.1.max(b.1)
}

/// Whether the closed triangle holds the origin: on an edge, or strictly inside a triangle of nonzero area.
fn holds_origin(a: (i64, i64), b: (i64, i64), c: (i64, i64)) -> bool {
    if [(a, b), (b, c), (c, a)]
        .into_iter()
        .any(|(u, v)| origin_on_segment(u, v))
    {
        return true;
    }
    let sides = [
        cross(a, b, (0, 0)),
        cross(b, c, (0, 0)),
        cross(c, a, (0, 0)),
    ];
    sides.iter().all(|&s| s > 0) || sides.iter().all(|&s| s < 0)
}

fn coord(v: (i64, i64)) -> Vec<f64> {
    vec![v.0 as f64, v.1 as f64]
}

#[test]
fn test_a_point_in_a_triangle_matches_exact_geometry() -> TestResult {
    let (_, query) = query_tokens_geo(
        QueryType::Contains,
        &Geometry::point(Layout::XY, &[0.0, 0.0])?,
        0.0,
    )?;
    let grid: Vec<(i64, i64)> = (-SPAN..=SPAN)
        .flat_map(|x| (-SPAN..=SPAN).map(move |y| (x, y)))
        .collect();
    let mut failures = Vec::new();
    for &a in &grid {
        for &b in &grid {
            let ring = vec![coord(a), coord(b), coord(APEX), coord(a)];
            let triangle = Geometry::polygon(Layout::XY, &[ring])?;
            let want = holds_origin(a, b, APEX);
            if query.matches_filter(&triangle) != want {
                failures.push(format!("{a:?} {b:?} {APEX:?}: want {want}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}
