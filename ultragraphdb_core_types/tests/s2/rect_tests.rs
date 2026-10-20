/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Rect operations on pairs of loop bounds and ExpandForSubregions, against Go.

use super::cellid_tests::{CapJson, same_cap};
use super::golden::{Checker, ids, load, pt, rect, same_rect};
use serde::Deserialize;
use ultragraphdb_core_types::expand_for_subregions;

#[derive(Deserialize)]
struct Pair {
    a: [String; 4],
    b: [String; 4],
    intersects: bool,
    contains: bool,
    union: [String; 4],
    union_is_full: bool,
}

#[derive(Deserialize)]
struct Single {
    r: [String; 4],
    is_empty: bool,
    is_full: bool,
    cap_bound: CapJson,
    expand_for_subregions: [String; 4],
    points: Vec<[String; 3]>,
    contains_point: Vec<bool>,
    cell_union_bound: Vec<String>,
}

#[derive(Deserialize)]
struct Golden {
    pairs: Vec<Pair>,
    singles: Vec<Single>,
}

#[test]
fn rects_match_go() {
    let g: Golden = load("rects.json");
    let mut c = Checker::new("rects");
    for k in &g.pairs {
        let (a, b) = (rect(&k.a), rect(&k.b));
        c.check(a.intersects(b) == k.intersects, || {
            format!("{a:?}.intersects({b:?})")
        });
        c.check(a.contains(b) == k.contains, || {
            format!("{a:?}.contains({b:?})")
        });
        let u = a.union(b);
        c.check(same_rect(u, &k.union), || {
            format!("{a:?}.union({b:?}) = {u:?}")
        });
        c.check(u.is_full() == k.union_is_full, || {
            format!("{u:?}.is_full()")
        });
    }
    for k in &g.singles {
        let r = rect(&k.r);
        c.check(
            r.is_empty() == k.is_empty && r.is_full() == k.is_full,
            || format!("{r:?} empty/full"),
        );
        c.check(same_cap(r.cap_bound(), &k.cap_bound), || {
            format!("{r:?} cap bound {:?}", r.cap_bound())
        });
        let e = expand_for_subregions(r);
        c.check(same_rect(e, &k.expand_for_subregions), || {
            format!("{r:?} expand_for_subregions = {e:?}")
        });
        for (p, want) in k.points.iter().zip(&k.contains_point) {
            c.check(r.contains_point(pt(p)) == *want, || {
                format!("{r:?} contains_point({p:?})")
            });
        }
        c.check(ids(&r.cell_union_bound()) == k.cell_union_bound, || {
            format!("{r:?} cell union bound")
        });
    }
    c.finish();
}
