/*
 * Copyright 2015 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported in part from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/loop_test.go; the golden-vector checks are new.
 * Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Loops built as Dgraph builds them (from GeoJSON rings) and regular loops: bounds,
//! ContainsPoint before and after the index is built, coverings and cell relations, run in
//! the same call order as the Go generator so the index is built at the same moment.

use super::cellid_tests::{CapJson, same_cap};
use super::golden::{
    Checker, cell, f, hx, ids, load, pt, same, same_point, same_rect, tokens, tokens_hash,
    vertex_hash,
};
use serde::Deserialize;
use ultragraphdb_core_types::{Angle, Cell, LatLng, Loop, Point, R3Vector, RegionCoverer};

#[derive(Deserialize, Clone, Copy)]
pub struct Config {
    pub min_level: i64,
    pub max_level: i64,
    pub level_mod: i64,
    pub max_cells: i64,
}

impl Config {
    pub fn coverer(&self) -> RegionCoverer {
        RegionCoverer::new(
            self.min_level,
            self.max_level,
            self.level_mod,
            self.max_cells,
        )
    }
}

#[derive(Deserialize)]
pub struct Covering {
    pub config: Config,
    pub count: usize,
    pub hash: String,
    pub tokens: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct Spec {
    name: String,
    ring: Option<Vec<[String; 2]>>,
    center: Option<[String; 3]>,
    radius: Option<String>,
    num_vertices_in: Option<usize>,
    poison_cover: Option<bool>,
}

#[derive(Deserialize)]
struct Case {
    spec: Spec,
    reversed: bool,
    inverted: bool,
    num_vertices: usize,
    num_edges: usize,
    vertex_hash: String,
    vertex0: [String; 3],
    origin_inside: bool,
    rect_bound: [String; 4],
    cap_bound: CapJson,
    cap_radius_deg: String,
    points: Vec<[String; 3]>,
    contains_stale: Vec<bool>,
    cell_union_bound: Vec<String>,
    coverings: Vec<Covering>,
    probe_cells: Vec<String>,
    contains_cell: Vec<bool>,
    intersects_cell: Vec<bool>,
    contains_fresh: Vec<bool>,
}

/// Dgraph's planar shoelace test (types/s2index.go isClockwise) on [lng, lat] coordinates.
fn is_clockwise(ring: &[[f64; 2]]) -> bool {
    let n = ring.len();
    let mut a = 0.0;
    for i in 0..n {
        let p1 = ring[i];
        let p2 = ring[(i + 1) % n];
        a += (p2[0] - p1[0]) * (p1[1] + p2[1]);
    }
    a > 0.0
}

/// Dgraph's loopFromPolygon (types/s2index.go) for an outer ring of [lng, lat] degrees whose
/// last point repeats the first: returns the loop and whether it was reversed and inverted.
pub fn dgraph_loop(ring: &[[f64; 2]]) -> (Loop, bool, bool) {
    let reversed = is_clockwise(ring);
    let n = ring.len();
    let pts = (0..n - 1)
        .map(|i| {
            let c = if reversed { ring[n - 1 - i] } else { ring[i] };
            Point::from_lat_lng(LatLng::from_degrees(c[1], c[0]))
        })
        .collect();
    let mut l = Loop::from_points(pts);
    let mut inverted = false;
    if l.cap_bound().radius().degrees() > 90.0 {
        l.invert();
        inverted = true;
    }
    (l, reversed, inverted)
}

pub fn check_covering(
    c: &mut Checker,
    tag: &str,
    got: &[ultragraphdb_core_types::CellId],
    want: &Covering,
) {
    let t = tokens(got);
    let cfg = want.config;
    c.check(
        t.len() == want.count && tokens_hash(&t) == want.hash,
        || {
            format!(
                "{tag}: covering({}, {}, {}, {}) has {} cells (hash {}) want {} (hash {}){}",
                cfg.min_level,
                cfg.max_level,
                cfg.level_mod,
                cfg.max_cells,
                t.len(),
                tokens_hash(&t),
                want.count,
                want.hash,
                match &want.tokens {
                    Some(w) => format!("\n  got  {t:?}\n  want {w:?}"),
                    None => String::new(),
                }
            )
        },
    );
    if let Some(w) = &want.tokens {
        c.check(&t == w, || format!("{tag}: covering tokens"));
    }
}

fn run(file: &'static str) {
    let cases: Vec<Case> = load(file);
    let mut c = Checker::new(file);
    for k in &cases {
        let tag = &k.spec.name;
        let (mut l, reversed, mut inverted) = match &k.spec.ring {
            Some(ring) => {
                let ring: Vec<[f64; 2]> = ring.iter().map(|p| [f(&p[0]), f(&p[1])]).collect();
                dgraph_loop(&ring)
            }
            None => {
                let center = pt(k.spec.center.as_ref().expect("regular loop center"));
                let radius =
                    Angle::from_radians(f(k.spec.radius.as_ref().expect("regular loop radius")));
                (
                    Loop::regular(
                        center,
                        radius,
                        k.spec.num_vertices_in.expect("regular loop vertices"),
                    ),
                    false,
                    false,
                )
            }
        };
        if k.spec.poison_cover == Some(true) {
            k.coverings[0].config.coverer().covering(&l);
            l.invert();
            inverted = !inverted;
        }
        c.check(reversed == k.reversed && inverted == k.inverted, || {
            format!("{tag}: reversed {reversed} inverted {inverted}")
        });
        c.check(
            l.num_vertices() == k.num_vertices && l.num_edges() == k.num_edges,
            || format!("{tag}: vertex/edge count"),
        );
        c.check(vertex_hash(l.vertices()) == k.vertex_hash, || {
            format!("{tag}: vertex hash")
        });
        c.check(same_point(l.vertex(0), &k.vertex0), || {
            format!("{tag}: vertex 0 {:?}", l.vertex(0))
        });
        c.check(l.contains_origin() == k.origin_inside, || {
            format!("{tag}: origin inside")
        });
        c.check(same_rect(l.rect_bound(), &k.rect_bound), || {
            format!(
                "{tag}: rect bound {:?} want {:?}",
                l.rect_bound(),
                k.rect_bound
            )
        });
        c.check(same_cap(l.cap_bound(), &k.cap_bound), || {
            format!("{tag}: cap bound {:?}", l.cap_bound())
        });
        let deg = l.cap_bound().radius().degrees();
        c.check(same(deg, f(&k.cap_radius_deg)), || {
            format!(
                "{tag}: cap radius degrees {} want {}",
                hx(deg),
                k.cap_radius_deg
            )
        });
        let points: Vec<Point> = k.points.iter().map(pt).collect();
        for (i, p) in points.iter().enumerate() {
            let got = l.contains_point(*p);
            c.check(got == k.contains_stale[i], || {
                format!("{tag}: contains_point (index not built) {i} {p:?} = {got}")
            });
        }
        c.check(ids(&l.cell_union_bound()) == k.cell_union_bound, || {
            format!("{tag}: cell union bound")
        });
        for cov in &k.coverings {
            let got = cov.config.coverer().covering(&l);
            check_covering(&mut c, tag, got.as_slice(), cov);
        }
        for (i, id) in k.probe_cells.iter().enumerate() {
            let cl = Cell::from_cell_id(cell(id));
            let cc = l.contains_cell(&cl);
            c.check(cc == k.contains_cell[i], || {
                format!("{tag}: contains_cell({id}) = {cc}")
            });
            let ic = l.intersects_cell(&cl);
            c.check(ic == k.intersects_cell[i], || {
                format!("{tag}: intersects_cell({id}) = {ic}")
            });
        }
        for (i, p) in points.iter().enumerate() {
            let got = l.contains_point(*p);
            c.check(got == k.contains_fresh[i], || {
                format!("{tag}: contains_point (index built) {i} {p:?} = {got}")
            });
        }
    }
    c.finish();
}

#[test]
fn dgraph_testdata_loops_match_go() {
    run("loops_testdata.json");
}

#[test]
fn us_loops_match_go() {
    run("loops_us.json");
}

#[test]
fn random_loops_match_go() {
    run("loops_random.json");
}

/// golang/geo's test helper parsePoints: "lat:lng, lat:lng, ..." in degrees.
fn parse_points(s: &str) -> Vec<Point> {
    s.split(',')
        .map(|ll| {
            let (lat, lng) = ll.trim().split_once(':').expect("lat:lng");
            Point::from_lat_lng(LatLng::from_degrees(
                lat.parse().expect("lat"),
                lng.parse().expect("lng"),
            ))
        })
        .collect()
}

fn make_loop(s: &str) -> Loop {
    Loop::from_points(parse_points(s))
}

/// golang/geo's test helper rotate: the loop with its first vertex moved to the end.
fn rotate(l: &Loop) -> Loop {
    let mut v: Vec<Point> = l.vertices()[1..].to_vec();
    v.push(l.vertex(0));
    Loop::from_points(v)
}

// Ported from golang/geo s2/loop_test.go, TestLoopEmptyLoop and TestLoopFullLoop.
#[test]
fn loop_empty_and_full() {
    let e = Loop::empty();
    assert!(e.is_empty() && !e.is_full());
    assert_eq!(e.num_edges(), 0);
    assert!(e.rect_bound().is_empty());
    assert!(e.cap_bound().is_empty());
    let f = Loop::full();
    assert!(f.is_full() && !f.is_empty());
    assert_eq!(f.num_edges(), 0);
    assert!(f.rect_bound().is_full());
    assert!(f.cap_bound().is_full());
}

// Ported from golang/geo s2/loop_test.go, TestLoopContainsPoint.
#[test]
fn loop_contains_point() {
    let north = Point::from_vector(R3Vector::new(0.0, 0.0, 1.0));
    let south = Point::from_vector(R3Vector::new(0.0, 0.0, -1.0));
    let east = Point::from_coords(0.0, 1.0, 0.0);
    let west = Point::from_coords(0.0, -1.0, 0.0);

    assert!(
        !Loop::empty().contains_point(north),
        "empty loop should not have any points"
    );
    assert!(
        Loop::full().contains_point(south),
        "full loop should have full point vertex"
    );

    let ll = |lat: f64, lng: f64| Point::from_lat_lng(LatLng::from_degrees(lat, lng));
    let tests = [
        (
            "north hemisphere",
            make_loop("0:-180, 0:-90, 0:0, 0:90"),
            north,
            south,
        ),
        (
            "south hemisphere",
            make_loop("0:90, 0:0, 0:-90, 0:-180"),
            south,
            north,
        ),
        (
            "west hemisphere",
            make_loop("0:-180, -90:0, 0:0, 90:0"),
            west,
            east,
        ),
        (
            "east hemisphere",
            make_loop("90:0, 0:0, -90:0, 0:-180"),
            east,
            west,
        ),
        (
            "candy cane",
            make_loop("-20:150, -20:-70, 0:70, 10:-150, 10:70, -10:-70"),
            ll(5.0, 71.0),
            ll(-8.0, 71.0),
        ),
    ];
    for (name, l, inside, outside) in tests {
        let mut l = l;
        for i in 0..4 {
            assert!(
                l.contains_point(inside),
                "{name} loop should contain {inside:?} at rotation {i}"
            );
            assert!(
                !l.contains_point(outside),
                "{name} loop shouldn't contain {outside:?} at rotation {i}"
            );
            l = rotate(&l);
        }
    }

    // This code checks each cell vertex is contained by exactly one of
    // the adjacent cells.
    for level in 0..3 {
        // set of unique points across all loops at this level.
        let mut points = std::collections::BTreeSet::new();
        let mut loops = Vec::new();
        let end = ultragraphdb_core_types::CellId::from_face(5).child_end_at_level(level);
        let mut id = ultragraphdb_core_types::CellId::from_face(0).child_begin_at_level(level);
        while id != end {
            let cl = Cell::from_cell_id(id);
            let mut vertices = Vec::new();
            let c = cl.center();
            points.insert((c.x().to_bits(), c.y().to_bits(), c.z().to_bits()));
            for k in 0..4 {
                let v = cl.vertex(k).unwrap();
                vertices.push(v);
                points.insert((v.x().to_bits(), v.y().to_bits(), v.z().to_bits()));
            }
            loops.push(Loop::from_points(vertices));
            id = id.next();
        }
        for (x, y, z) in points {
            let q = Point::from_vector(R3Vector::new(
                f64::from_bits(x),
                f64::from_bits(y),
                f64::from_bits(z),
            ));
            let count = loops.iter().filter(|l| l.contains_point(q)).count();
            assert_eq!(
                count, 1,
                "point {q:?} should only be contained by one loop at level {level}, got {count}"
            );
        }
    }
}

// Ported from golang/geo s2/loop_test.go, TestLoopRectBound (with Go's rectError tolerance,
// 10 and 1 machine epsilons in latitude and longitude).
#[test]
fn loop_rect_bound() {
    let lat_err = 10.0 * f64::EPSILON;
    let lng_err = f64::EPSILON;
    let near = |a: f64, b: f64, e: f64| (a - b).abs() <= e;
    let candy_cane = make_loop("-20:150, -20:-70, 0:70, 10:-150, 10:70, -10:-70");
    assert!(candy_cane.rect_bound().lng().is_full());
    assert!(candy_cane.rect_bound().lat().lo() < -0.349066);
    assert!(candy_cane.rect_bound().lat().hi() > 0.174533);
    assert!(make_loop("35:20, 45:20, 40:25").rect_bound().is_full());

    let deg = |d: f64| d.to_radians();
    let arctic80 = make_loop("80:-150, 80:-30, 80:90");
    let b = arctic80.rect_bound();
    assert!(
        near(b.lat().lo(), deg(80.0), lat_err) && near(b.lat().hi(), deg(90.0), lat_err),
        "{b:?}"
    );
    assert!(
        near(b.lng().lo(), deg(-180.0), lng_err) && near(b.lng().hi(), deg(180.0), lng_err),
        "{b:?}"
    );
    let antarctic80 = make_loop("-80:120, -80:0, -80:-120");
    let b = antarctic80.rect_bound();
    assert!(
        near(b.lat().lo(), deg(-90.0), lat_err) && near(b.lat().hi(), deg(-80.0), lat_err),
        "{b:?}"
    );

    let south_hemi = make_loop("0:90, 0:0, 0:-90, 0:-180");
    assert!(south_hemi.rect_bound().lng().is_full());
    let lat = south_hemi.rect_bound().lat();
    assert!(
        near(lat.lo(), -core::f64::consts::FRAC_PI_2, lat_err) && near(lat.hi(), 0.0, lat_err),
        "{lat:?}"
    );

    // Create a loop that contains the complement of the arctic80 loop.
    let mut arctic80_inv = arctic80.clone();
    arctic80_inv.invert();
    // The highest latitude of each edge is attained at its midpoint.
    let v = arctic80_inv.vertices();
    let mid = Point::from_vector(v[0].vector().add(v[1].vector()).mul(0.5));
    let want = LatLng::from_point(mid).lat().radians();
    let got = arctic80_inv.rect_bound().lat().hi();
    assert!(near(got, want, 10.0 * f64::EPSILON), "{got} {want}");
}

// Ported from golang/geo s2/loop_test.go, TestLoopRegularLoop.
#[test]
fn loop_regular_loop() {
    let l = Loop::regular(
        Point::from_lat_lng(LatLng::from_degrees(80.0, 135.0)),
        Angle::from_degrees(20.0),
        4,
    );
    assert_eq!(
        l.num_vertices(),
        4,
        "RegularLoop with 4 vertices should have 4 vertices"
    );
    // Regular loops' points are compared with Go in loops_random.json; here the geometry: every vertex lies
    // 20 degrees from the center, consecutive vertices are a quarter turn apart, and the
    // loop is CCW around its center.
    let center = Point::from_lat_lng(LatLng::from_degrees(80.0, 135.0));
    for i in 0..4 {
        let r = center.vector().angle(l.vertex(i).vector()).degrees();
        assert!((r - 20.0).abs() < 1e-12, "vertex {i} at {r} degrees");
    }
    let side = l.vertex(0).vector().angle(l.vertex(1).vector()).radians();
    for i in 1..4 {
        let s = l
            .vertex(i)
            .vector()
            .angle(l.vertex(i + 1).vector())
            .radians();
        assert!((s - side).abs() < 1e-12, "side {i}");
    }
    assert!(l.contains_point(center));
}

// Ported from golang/geo s2/loop_test.go, TestLoopVertex (wrapping indices).
#[test]
fn loop_vertex_wraps() {
    let l = make_loop("0:0, 0:1, 1:0");
    assert_eq!(l.vertex(3), l.vertex(0));
    assert_eq!(l.vertex(4), l.vertex(1));
    assert_eq!(l.vertex(7), l.vertex(1));
}

#[test]
fn a_loop_of_no_points_is_the_empty_loop() {
    // Go's loop of no vertices divides by its vertex count in ContainsPoint and panics.
    let l = Loop::from_points(Vec::new());
    assert!(l.is_empty());
    assert!(!l.contains_point(Point::from_lat_lng(LatLng::from_degrees(1.0, 2.0))));
    assert!(
        Loop::regular(
            Point::from_lat_lng(LatLng::from_degrees(1.0, 2.0)),
            Angle::from_radians(0.1),
            0
        )
        .is_empty()
    );
    // A loop of points that are not finite: Go's exact predicates panic on them.
    let nan = Point::from_vector(R3Vector::new(f64::NAN, 0.0, 1.0));
    let p = Point::from_lat_lng(LatLng::from_degrees(1.0, 2.0));
    let q = Point::from_lat_lng(LatLng::from_degrees(3.0, 4.0));
    let l = Loop::from_points(vec![p, nan, q]);
    let _ = l.contains_point(p);
}

// golang/geo's standard test loops (s2/loop_test.go).
const NORTH_HEMI: &str = "0:-180, 0:-90, 0:0, 0:90";
const NORTH_HEMI3: &str = "0:-180, 0:-60, 0:60";
const SOUTH_HEMI: &str = "0:90, 0:0, 0:-90, 0:-180";
const WEST_HEMI: &str = "0:-180, -90:0, 0:0, 90:0";
const EAST_HEMI: &str = "90:0, 0:0, -90:0, 0:-180";
const NEAR_HEMI: &str = "0:-90, -90:0, 0:90, 90:0";
const FAR_HEMI: &str = "90:0, 0:90, -90:0, 0:-90";
const CANDY_CANE: &str = "-20:150, -20:-70, 0:70, 10:-150, 10:70, -10:-70";
const SMALL_NECW: &str = "35:20, 45:20, 40:25";
const ARCTIC80: &str = "80:-150, 80:-30, 80:90";
const ANTARCTIC80: &str = "-80:120, -80:0, -80:-120";
const LINE_TRIANGLE: &str = "0:1, 0:2, 0:3";
const SKINNY_CHEVRON: &str = "0:0, -1e-320:80, 0:1e-320, 1e-320:80";
const LOOP_A: &str = "0:178, -1:180, 0:-179, 1:-180";

// Ported from golang/geo s2/loop_test.go, TestLoopOriginInside.
#[test]
fn loop_origin_inside() {
    let tests = [
        (NORTH_HEMI, true),
        (NORTH_HEMI3, true),
        (SOUTH_HEMI, false),
        (WEST_HEMI, false),
        (EAST_HEMI, true),
        (NEAR_HEMI, false),
        (FAR_HEMI, true),
        (CANDY_CANE, false),
        (SMALL_NECW, true),
        (ARCTIC80, true),
        (ANTARCTIC80, false),
        (LOOP_A, false),
    ];
    for (s, want) in tests {
        assert_eq!(make_loop(s).contains_origin(), want, "{s}");
    }
    assert!(!Loop::empty().contains_origin());
    assert!(Loop::full().contains_origin());
}

fn rect_from_degrees(
    lat_lo: f64,
    lng_lo: f64,
    lat_hi: f64,
    lng_hi: f64,
) -> ultragraphdb_core_types::Rect {
    ultragraphdb_core_types::Rect::new(
        ultragraphdb_core_types::R1Interval::new(lat_lo.to_radians(), lat_hi.to_radians()),
        ultragraphdb_core_types::S1Interval::new(lng_lo.to_radians(), lng_hi.to_radians()),
    )
}

// Ported from golang/geo s2/loop_test.go, TestLoopCapBound (Cap.ApproxEqual: centers and
// radii within 1e-14).
#[test]
fn loop_cap_bound() {
    assert!(Loop::empty().cap_bound().is_empty());
    assert!(Loop::full().cap_bound().is_full());
    assert!(make_loop(SMALL_NECW).cap_bound().is_full());
    let approx = |got: ultragraphdb_core_types::Cap, want: ultragraphdb_core_types::Cap| {
        let (g, w) = (got.center(), want.center());
        (g.x() - w.x()).abs() <= 1e-14
            && (g.y() - w.y()).abs() <= 1e-14
            && (g.z() - w.z()).abs() <= 1e-14
            && (got.radius().radians() - want.radius().radians()).abs() <= 1e-14
    };
    let got = make_loop(ARCTIC80).cap_bound();
    let want = rect_from_degrees(80.0, -180.0, 90.0, 180.0).cap_bound();
    assert!(approx(got, want), "{got:?} want {want:?}");
    // golang/geo: centred exactly on the north pole, radius 10.000000000000025 degrees.
    assert_eq!(
        got.center(),
        Point::from_vector(R3Vector::new(0.0, 0.0, 1.0))
    );
    assert!((got.radius().degrees() - 10.0).abs() < 1e-9, "{got:?}");
    let got = make_loop(ANTARCTIC80).cap_bound();
    let want = rect_from_degrees(-90.0, -180.0, -80.0, 180.0).cap_bound();
    assert!(approx(got, want), "{got:?} want {want:?}");
    // golang/geo: centred exactly on the south pole, radius 10.000000000000025 degrees.
    assert_eq!(
        got.center(),
        Point::from_vector(R3Vector::new(0.0, 0.0, -1.0))
    );
    assert!((got.radius().degrees() - 10.0).abs() < 1e-9, "{got:?}");
}

// Ported from golang/geo s2/loop_test.go, TestLoopNumEdges and TestLoopVertex.
#[test]
fn loop_num_edges_and_vertices() {
    assert_eq!(
        (Loop::empty().num_edges(), Loop::empty().num_vertices()),
        (0, 1)
    );
    assert_eq!(
        (Loop::full().num_edges(), Loop::full().num_vertices()),
        (0, 1)
    );
    for (s, want) in [
        (FAR_HEMI, 4),
        (CANDY_CANE, 6),
        (SMALL_NECW, 3),
        (ARCTIC80, 3),
        (ANTARCTIC80, 3),
        (LINE_TRIANGLE, 3),
        (SKINNY_CHEVRON, 4),
    ] {
        let l = make_loop(s);
        assert_eq!((l.num_edges(), l.num_vertices()), (want, want), "{s}");
    }
    let north = Point::from_vector(R3Vector::new(0.0, 0.0, 1.0));
    let south = Point::from_vector(R3Vector::new(0.0, 0.0, -1.0));
    assert_eq!(Loop::empty().vertex(0), north);
    assert_eq!(Loop::empty().vertex(1), north);
    assert_eq!(Loop::full().vertex(0), south);
    assert_eq!(Loop::full().vertex(1), south);
    let arctic = make_loop(ARCTIC80);
    let p = parse_points("80:-150, 80:-30, 80:90");
    assert_eq!(
        [
            arctic.vertex(0),
            arctic.vertex(1),
            arctic.vertex(2),
            arctic.vertex(3)
        ],
        [p[0], p[1], p[2], p[0]]
    );
    assert_eq!(arctic.vertex(5), arctic.vertex(2));
    assert_eq!(arctic.vertex(2 + 3 * 3), arctic.vertex(2));
}

// Ported from golang/geo s2/loop_test.go, TestLoopContainsMatchesCrossingSign. The cell and
// its children's points were checked against golang/geo: cell 0x4000000000040000 and a0 =
// [0x3fe279a724b7ecbc, 0x3fe279a724b7ecbc, 0x3fe279a78740bf2b].
#[test]
fn loop_contains_matches_crossing_sign() {
    use ultragraphdb_core_types::{Crossing, EdgeCrosser};
    let bits = |p: Point| [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()];
    let id = Cell::from_point(Point::from_vector(R3Vector::new(1.0, 1.0, 1.0)))
        .id()
        .parent(21);
    assert_eq!(id.id(), 0x4000000000040000);
    let children = id.children();
    let points: Vec<Point> = children
        .iter()
        .map(|c| Point::from_vector(Cell::from_cell_id(*c).center().vector().normalize()))
        .collect();
    assert_eq!(
        bits(points[0]),
        [0x3fe279a724b7ecbd, 0x3fe279a724b7ecbd, 0x3fe279a78740bf2c]
    );
    let l = Loop::from_points(points.clone());
    let grandchild = Cell::from_cell_id(children[0].children()[2]);
    let a0 = grandchild.vertex(0).expect("vertex 0");
    assert_eq!(
        bits(a0),
        [0x3fe279a724b7ecbc, 0x3fe279a724b7ecbc, 0x3fe279a78740bf2b]
    );
    // This test depends on rounding errors that should make a0 slightly different from points[0]
    assert_ne!(points[0], a0);

    // The edge from a0 to the origin crosses one boundary.
    let o = Point::origin();
    let want = [
        Crossing::DoNotCross,
        Crossing::Cross,
        Crossing::DoNotCross,
        Crossing::DoNotCross,
    ];
    for (i, w) in want.iter().enumerate() {
        let got = EdgeCrosser::new_chain(a0, o, l.vertex(i)).chain_crossing_sign(l.vertex(i + 1));
        assert_eq!(got, *w, "edge {i}");
    }
    // Contains should return false for the origin, and true for a0.
    assert!(!l.contains_point(o));
    assert!(l.contains_point(a0));
    // Since a0 is inside the loop, it should be inside the bound.
    assert!(l.rect_bound().contains_point(a0));
}

// Ported from golang/geo s2/loop_test.go, TestLoopBasic (the parts the port exposes) and
// TestLoopEdge: the edges a loop's index reads run from vertex i to vertex i + 1.
#[test]
fn loop_basic() {
    let l = make_loop("0:0, 0:1, 1:0");
    assert_eq!(l.num_edges(), 3);
    assert!(!l.is_empty() && !l.is_full());
    assert!(!l.contains_origin());
    let ll = |lat: f64, lng: f64| Point::from_lat_lng(LatLng::from_degrees(lat, lng));
    assert_eq!((l.vertex(2), l.vertex(3)), (ll(1.0, 0.0), ll(0.0, 0.0)));
    let candy = make_loop(CANDY_CANE);
    let p = parse_points(CANDY_CANE);
    for i in 0..6 {
        assert_eq!(
            (candy.vertex(i), candy.vertex(i + 1)),
            (p[i], p[(i + 1) % 6])
        );
    }
    let chevron = make_loop(SKINNY_CHEVRON);
    assert_eq!(chevron.vertex(3), ll(1e-320, 80.0));
    assert_eq!(chevron.vertex(4), ll(0.0, 0.0));
    // TestLoopEdge's farHemi edge 2 and skinnyChevron edge 2, as golang/geo's bits.
    let bits = |p: Point| [p.x().to_bits(), p.y().to_bits(), p.z().to_bits()];
    let far = make_loop(FAR_HEMI);
    assert_eq!(
        (bits(far.vertex(2)), bits(far.vertex(3))),
        (
            [0x3c91a62633145c00, 0x0, 0xbff0000000000000],
            [0x3c91a62633145c00, 0xbff0000000000000, 0x0]
        )
    );
    assert_eq!(
        (bits(chevron.vertex(2)), bits(chevron.vertex(3))),
        (
            [0x3ff0000000000000, 0x23, 0x0],
            [0x3fc63a1a7e0b738c, 0x3fef838b8c811c18, 0x23]
        )
    );
}

// ContainsCell and IntersectsCell of golang/geo's standard loops against seven probe cells,
// generated by golang/geo v0.0.0-20260427214057-41a1a8c7eb2a.
#[test]
fn standard_loop_cell_relations_match_go() {
    use ultragraphdb_core_types::CellId;
    let probes = [
        0x1000000000000000u64,
        0x5000000000000000,
        0xb000000000000000,
        0x4ff4000000000000,
        0x13584f0000000000,
        0x3b68300000000000,
        0xa2c0000000000000,
    ]
    .map(|i| Cell::from_cell_id(CellId::new(i)));
    let (t, f) = (true, false);
    let tests: [(&str, Loop, [bool; 7], [bool; 7]); 6] = [
        ("empty", Loop::empty(), [f; 7], [f; 7]),
        ("full", Loop::full(), [t; 7], [t; 7]),
        (
            "north_hemi",
            make_loop(NORTH_HEMI),
            [f, t, f, t, t, t, f],
            [t, t, f, t, t, t, f],
        ),
        (
            "candy_cane",
            make_loop(CANDY_CANE),
            [f, f, f, f, f, t, f],
            [t, f, t, f, f, t, f],
        ),
        (
            "small_necw",
            make_loop(SMALL_NECW),
            [f, f, t, t, f, t, t],
            [t, t, t, t, f, t, t],
        ),
        (
            "arctic80",
            make_loop(ARCTIC80),
            [f, f, f, t, f, f, f],
            [f, t, f, t, f, f, f],
        ),
    ];
    for (name, l, contains, intersects) in tests {
        let got_c: Vec<bool> = probes.iter().map(|c| l.contains_cell(c)).collect();
        let got_i: Vec<bool> = probes.iter().map(|c| l.intersects_cell(c)).collect();
        assert_eq!(got_c, contains, "{name} contains_cell");
        assert_eq!(got_i, intersects, "{name} intersects_cell");
    }
}

/// golang/geo's ContainsBruteForce (s2/shapeutil.go) for a loop: crossings counted from the
/// origin, whose containment the loop records.
fn contains_brute_force(l: &Loop, p: Point) -> bool {
    let origin = Point::origin();
    if p == origin {
        return l.contains_origin();
    }
    let mut crosser = ultragraphdb_core_types::EdgeCrosser::new(origin, p);
    let mut inside = l.contains_origin();
    for i in 0..l.num_edges() {
        inside = inside != crosser.edge_or_vertex_crossing(l.vertex(i), l.vertex(i + 1));
    }
    inside
}

// golang/geo's TestShapeutilContainsBruteForceConsistentWithLoop and
// TestShapeIndexLoopSpanningThreeFaces: loops of more than 32 vertices answer ContainsPoint,
// ContainsCell and IntersectsCell from their ShapeIndex, whose answers must agree with
// counting crossings. The first loop is centred on the cube corner (1, 1, 1), so its edges are
// clipped to three faces.
#[test]
fn indexed_loops_agree_with_brute_force() {
    use ultragraphdb_core_types::CellId;
    let corner = Point::from_coords(1.0, 1.0, 1.0);
    let centers = [
        corner,
        Point::from_lat_lng(LatLng::from_degrees(37.4, -122.1)),
        Point::from_lat_lng(LatLng::from_degrees(89.0, 10.0)),
        Point::from_lat_lng(LatLng::from_degrees(0.0, 180.0)),
    ];
    let mut checked = 0;
    for center in centers {
        for (radius_deg, n) in [(10.0, 200), (0.5, 64), (60.0, 33)] {
            let radius = Angle::from_degrees(radius_deg);
            let l = Loop::regular(center, radius, n);
            assert_eq!(l.num_vertices(), n);
            let inner = Loop::regular(center, Angle::from_degrees(radius_deg * 0.98), n);
            let outer = Loop::regular(center, Angle::from_degrees(radius_deg * 1.02), n);
            let antipode = Point::from_vector(center.vector().mul(-1.0));
            // The geometry decides these, independently of either algorithm.
            assert!(l.contains_point(center), "center of {radius_deg} deg loop");
            assert!(!l.contains_point(antipode));
            for p in inner.vertices() {
                assert!(l.contains_point(*p), "inner ring {p:?}");
            }
            for p in outer.vertices() {
                assert!(!l.contains_point(*p), "outer ring {p:?}");
            }
            // The vertices themselves and a lat/lng grid: index and brute force agree.
            let grid = (-8..=8).flat_map(|i| {
                (-18..18).map(move |j| {
                    Point::from_lat_lng(LatLng::from_degrees(i as f64 * 11.0, j as f64 * 10.0))
                })
            });
            for p in l.vertices().iter().copied().chain(grid) {
                assert_eq!(l.contains_point(p), contains_brute_force(&l, p), "{p:?}");
                checked += 1;
            }
            // Cells: a contained cell has its center and vertices inside, a cell the loop
            // does not intersect has them outside.
            let level = if radius_deg < 1.0 { 9 } else { 4 };
            let start = CellId::from_lat_lng(LatLng::from_point(center)).parent(level);
            let mut cells = vec![start];
            cells.extend(start.vertex_neighbors(level));
            for id in cells {
                let cl = Cell::from_cell_id(id);
                let pts: Vec<Point> = (0..4)
                    .map(|k| cl.vertex(k).expect("vertex"))
                    .chain([cl.center()])
                    .collect();
                if l.contains_cell(&cl) {
                    assert!(l.intersects_cell(&cl));
                    assert!(pts.iter().all(|p| contains_brute_force(&l, *p)), "{id:?}");
                }
                if !l.intersects_cell(&cl) {
                    assert!(pts.iter().all(|p| !contains_brute_force(&l, *p)), "{id:?}");
                }
            }
        }
    }
    assert!(checked > 5000, "{checked}");
}

// Invert complements the loop: every point off the boundary changes sides, the origin's
// containment flips, and inverting twice restores the vertices.
#[test]
fn invert_complements_the_loop() {
    for (s, n) in [(CANDY_CANE, 0), (SMALL_NECW, 0), ("", 40)] {
        let l = if n > 0 {
            Loop::regular(
                Point::from_lat_lng(LatLng::from_degrees(20.0, 30.0)),
                Angle::from_degrees(5.0),
                n,
            )
        } else {
            make_loop(s)
        };
        let mut inv = l.clone();
        inv.invert();
        assert_eq!(inv.contains_origin(), !l.contains_origin());
        let mut rev = l.vertices().to_vec();
        rev.reverse();
        assert_eq!(inv.vertices(), rev.as_slice());
        for i in -8..=8 {
            for j in -18..18 {
                let p = Point::from_lat_lng(LatLng::from_degrees(i as f64 * 10.5, j as f64 * 9.5));
                assert_ne!(inv.contains_point(p), l.contains_point(p), "{s} {p:?}");
            }
        }
        inv.invert();
        assert_eq!(inv.vertices(), l.vertices());
        assert_eq!(inv.contains_origin(), l.contains_origin());
    }
    let mut e = Loop::empty();
    e.invert();
    assert!(e.is_full() && e.rect_bound().is_full());
    e.invert();
    assert!(e.is_empty() && e.rect_bound().is_empty());
}

// A loop of one or two vertices that is not the empty or the full loop: Go returns from
// initOriginAndBound before computing a bound or adding the loop to its index, so the loop
// contains no point (origin outside, no edge crossed twice) and no cell.
#[test]
fn degenerate_loops_of_one_or_two_vertices() {
    let p = Point::from_lat_lng(LatLng::from_degrees(1.0, 2.0));
    let q = Point::from_lat_lng(LatLng::from_degrees(3.0, 4.0));
    let l = Loop::from_points(vec![p, q]);
    assert!(!l.contains_origin() && !l.is_empty() && !l.is_full());
    assert_eq!(l.num_edges(), 2);
    let r = l.rect_bound();
    assert_eq!(
        (r.lat().lo(), r.lat().hi(), r.lng().lo(), r.lng().hi()),
        (0.0, 0.0, 0.0, 0.0)
    );
    assert!(!l.contains_point(Point::from_lat_lng(LatLng::from_degrees(2.0, 3.0))));
    let cl = Cell::from_point(p);
    assert!(!l.contains_cell(&cl) && !l.intersects_cell(&cl));
    // A one-vertex loop is the empty loop if its vertex is in the northern hemisphere and the
    // full loop otherwise.
    assert!(Loop::from_points(vec![p]).is_empty());
    assert!(
        Loop::from_points(vec![Point::from_lat_lng(LatLng::from_degrees(-1.0, 2.0))]).is_full()
    );
}

// Go's exact predicates panic on vertices that are not finite; the port answers. A loop of
// more than 32 vertices builds its ShapeIndex from them. The answers are not Go's (Go has
// none); only that they come back is asserted.
#[test]
fn indexed_loop_with_non_finite_vertices_answers() {
    let mut v = Loop::regular(
        Point::from_lat_lng(LatLng::from_degrees(10.0, 20.0)),
        Angle::from_degrees(1.0),
        40,
    )
    .vertices()
    .to_vec();
    v[7] = Point::from_vector(R3Vector::new(f64::NAN, 0.0, 1.0));
    v[20] = Point::from_vector(R3Vector::new(f64::INFINITY, 1.0, 0.0));
    let l = Loop::from_points(v);
    assert_eq!(l.num_vertices(), 40);
    let p = Point::from_lat_lng(LatLng::from_degrees(10.0, 20.0));
    let _ = l.contains_point(p);
    let cl = Cell::from_point(p);
    let _ = (l.contains_cell(&cl), l.intersects_cell(&cl));
    let _ = RegionCoverer::new(5, 16, 0, 18).covering(&l);
}
