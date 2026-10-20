/*
 * Copyright 2017 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported in part from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/edge_crosser_test.go; the golden-vector checks are new.
 * Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! CrossingSign, VertexCrossing, EdgeOrVertexCrossing and the chained EdgeCrosser, against Go.

use super::golden::{Checker, load, pt};
use super::predicates_tests::Golden;
use ultragraphdb_core_types::{
    Crossing, EdgeCrosser, Point, R3Vector, crossing_sign, edge_or_vertex_crossing, ordered_ccw,
    vertex_crossing,
};

#[test]
fn crossings_match_go() {
    let g: Golden = load("predicates.json");
    let mut c = Checker::new("crossings");
    for k in &g.crossings {
        let (a, b, p, d) = (pt(&k.a), pt(&k.b), pt(&k.c), pt(&k.d));
        let cs = crossing_sign(a, b, p, d).value();
        c.check(cs == k.crossing, || {
            format!(
                "crossing_sign({a:?}, {b:?}, {p:?}, {d:?}) = {cs} want {}",
                k.crossing
            )
        });
        let vc = vertex_crossing(a, b, p, d);
        c.check(vc == k.vertex, || {
            format!("vertex_crossing({a:?}, {b:?}, {p:?}, {d:?}) = {vc}")
        });
        let ev = edge_or_vertex_crossing(a, b, p, d);
        c.check(ev == k.edge_or_vertex, || {
            format!("edge_or_vertex_crossing({a:?}, {b:?}, {p:?}, {d:?}) = {ev}")
        });
        let oc = ordered_ccw(a, b, p, d);
        c.check(oc == k.ordered_ccw, || {
            format!("ordered_ccw({a:?}, {b:?}, {p:?}, {d:?}) = {oc}")
        });
    }
    for (n, k) in g.chains.iter().enumerate() {
        let (a, b) = (pt(&k.a), pt(&k.b));
        let chain: Vec<Point> = k.chain.iter().map(pt).collect();
        let mut e1 = EdgeCrosser::new_chain(a, b, chain[0]);
        let mut e2 = EdgeCrosser::new_chain(a, b, chain[0]);
        for (i, q) in chain[1..].iter().enumerate() {
            let s = e1.chain_crossing_sign(*q).value();
            c.check(s == k.chain_crossing_sign[i], || {
                format!("chain {n} step {i}: chain_crossing_sign = {s}")
            });
            let ev = e2.edge_or_vertex_chain_crossing(*q);
            c.check(ev == k.edge_or_vertex_chain[i], || {
                format!("chain {n} step {i}: edge_or_vertex_chain_crossing = {ev}")
            });
        }
        let mut e3 = EdgeCrosser::new(a, b);
        for i in 0..chain.len() - 1 {
            if i % 7 == 3 {
                e3.restart_at(chain[i]);
            }
            let got = if i % 2 == 0 {
                e3.crossing_sign(chain[i], chain[i + 1]).value()
            } else {
                e3.edge_or_vertex_crossing(chain[i], chain[i + 1]) as i32
            };
            c.check(got == k.mixed[i], || {
                format!("chain {n} mixed step {i} = {got} want {}", k.mixed[i])
            });
        }
    }
    c.finish();
}

fn test_crossing(
    msg: &str,
    a: Point,
    b: Point,
    c: Point,
    d: Point,
    robust: Crossing,
    edge_or_vertex: bool,
) {
    // Modify the expected result if two vertices from different edges match.
    let robust = if a == c || a == d || b == c || b == d {
        Crossing::MaybeCross
    } else {
        robust
    };
    let input = format!("{msg}: a: {a:?}, b: {b:?}, c: {c:?}, d: {d:?}");

    let mut crosser = EdgeCrosser::new_chain(a, b, c);
    assert_eq!(
        crosser.chain_crossing_sign(d),
        robust,
        "{input}, ChainCrossingSign(d)"
    );
    assert_eq!(
        crosser.chain_crossing_sign(c),
        robust,
        "{input}, ChainCrossingSign(c)"
    );
    assert_eq!(
        crosser.crossing_sign(d, c),
        robust,
        "{input}, CrossingSign(d, c)"
    );
    assert_eq!(
        crosser.crossing_sign(c, d),
        robust,
        "{input}, CrossingSign(c, d)"
    );

    crosser.restart_at(c);
    assert_eq!(
        crosser.edge_or_vertex_chain_crossing(d),
        edge_or_vertex,
        "{input}, EdgeOrVertexChainCrossing(d)"
    );
    assert_eq!(
        crosser.edge_or_vertex_chain_crossing(c),
        edge_or_vertex,
        "{input}, EdgeOrVertexChainCrossing(c)"
    );
    assert_eq!(
        crosser.edge_or_vertex_crossing(d, c),
        edge_or_vertex,
        "{input}, EdgeOrVertexCrossing(d, c)"
    );
    assert_eq!(
        crosser.edge_or_vertex_crossing(c, d),
        edge_or_vertex,
        "{input}, EdgeOrVertexCrossing(c, d)"
    );
}

// Ported from golang/geo s2/edge_crosser_test.go, TestEdgeCrosserCrossings.
#[test]
fn edge_crosser_crossings() {
    let na1 = ultragraphdb_core_types::go_nextafter(1.0, 0.0);
    let na2 = ultragraphdb_core_types::go_nextafter(1.0, 2.0);
    let v = |x: f64, y: f64, z: f64| R3Vector::new(x, y, z);
    let tests = [
        (
            "two regular edges that cross",
            v(1.0, 2.0, 1.0),
            v(1.0, -3.0, 0.5),
            v(1.0, -0.5, -3.0),
            v(0.1, 0.5, 3.0),
            Crossing::Cross,
            true,
        ),
        (
            "two regular edges that intersect antipodal points",
            v(1.0, 2.0, 1.0),
            v(1.0, -3.0, 0.5),
            v(-1.0, 0.5, 3.0),
            v(-0.1, -0.5, -3.0),
            Crossing::DoNotCross,
            false,
        ),
        (
            "two edges on the same great circle that start at antipodal points",
            v(0.0, 0.0, -1.0),
            v(0.0, 1.0, 0.0),
            v(0.0, 0.0, 1.0),
            v(0.0, 1.0, 1.0),
            Crossing::DoNotCross,
            false,
        ),
        (
            "two edges that cross where one vertex is the OriginPoint",
            v(1.0, 0.0, 0.0),
            Point::origin().vector(),
            v(1.0, -0.1, 1.0),
            v(1.0, 1.0, -0.1),
            Crossing::Cross,
            true,
        ),
        (
            "two edges that intersect antipodal points where one vertex is the OriginPoint",
            v(1.0, 0.0, 0.0),
            Point::origin().vector(),
            v(1.0, 0.1, -1.0),
            v(1.0, 1.0, -0.1),
            Crossing::DoNotCross,
            false,
        ),
        (
            "two edges that cross antipodal points",
            v(1.0, 0.0, 0.0),
            v(0.0, 1.0, 0.0),
            v(0.0, 0.0, -1.0),
            v(-1.0, -1.0, 1.0),
            Crossing::DoNotCross,
            false,
        ),
        (
            "two edges that share an endpoint",
            v(2.0, 3.0, 4.0),
            v(-1.0, 2.0, 5.0),
            v(7.0, -2.0, 3.0),
            v(2.0, 3.0, 4.0),
            Crossing::MaybeCross,
            false,
        ),
        (
            "two edges that barely cross near the middle of one edge",
            v(1.0, 1.0, 1.0),
            v(1.0, na1, -1.0),
            v(11.0, -12.0, -1.0),
            v(10.0, 10.0, 1.0),
            Crossing::Cross,
            true,
        ),
        (
            "two edges that barely cross near the middle separated by a distance of about 1e-15",
            v(1.0, 1.0, 1.0),
            v(1.0, na2, -1.0),
            v(1.0, -1.0, 0.0),
            v(1.0, 1.0, 0.0),
            Crossing::DoNotCross,
            false,
        ),
        (
            "two edges that barely cross each other near the end of both edges",
            v(0.0, 0.0, 1.0),
            v(2.0, -1e-323, 1.0),
            v(1.0, -1.0, 1.0),
            v(1e-323, 0.0, 1.0),
            Crossing::Cross,
            true,
        ),
        (
            "two edges that barely cross each other near the end separated by a distance of about 1e-640",
            v(0.0, 0.0, 1.0),
            v(2.0, 1e-323, 1.0),
            v(1.0, -1.0, 1.0),
            v(1e-323, 0.0, 1.0),
            Crossing::DoNotCross,
            false,
        ),
        (
            "two edges that barely cross each other near the middle of one edge",
            v(1.0, -1e-323, -1e-323),
            v(1e-323, 1.0, 1e-323),
            v(1.0, -1.0, 1e-323),
            v(1.0, 1.0, 0.0),
            Crossing::Cross,
            true,
        ),
        (
            "two edges that barely cross each other near the middle separated by a distance of about 1e-640",
            v(1.0, 1e-323, -1e-323),
            v(-1e-323, 1.0, 1e-323),
            v(1.0, -1.0, 1e-323),
            v(1.0, 1.0, 0.0),
            Crossing::DoNotCross,
            false,
        ),
    ];
    for (msg, a, b, c, d, robust, edge_or_vertex) in tests {
        // just normalize them once
        let a = Point::from_vector(a.normalize());
        let b = Point::from_vector(b.normalize());
        let c = Point::from_vector(c.normalize());
        let d = Point::from_vector(d.normalize());
        test_crossing(msg, a, b, c, d, robust, edge_or_vertex);
        test_crossing(msg, b, a, c, d, robust, edge_or_vertex);
        test_crossing(msg, a, b, d, c, robust, edge_or_vertex);
        test_crossing(msg, b, a, d, c, robust, edge_or_vertex);

        // test degenerate cases
        test_crossing(msg, a, a, c, d, Crossing::DoNotCross, false);
        test_crossing(msg, a, b, c, c, Crossing::DoNotCross, false);
        test_crossing(msg, a, a, c, c, Crossing::DoNotCross, false);

        test_crossing(msg, a, b, a, b, Crossing::MaybeCross, true);
        test_crossing(
            msg,
            c,
            d,
            a,
            b,
            robust,
            edge_or_vertex != (robust == Crossing::MaybeCross),
        );
    }
}
