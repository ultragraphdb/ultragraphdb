/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Go's math functions as golang/geo calls them, against Go on their real argument ranges.

use super::golden::{Checker, f, hx, load, same};
use serde::Deserialize;
use ultragraphdb_core_types::{
    go_asin, go_atan2, go_cos, go_frexp, go_ilogb, go_ldexp, go_max, go_min, go_nextafter,
    go_remainder, go_sin,
};

#[derive(Deserialize)]
struct Ldexp {
    frac: String,
    exp: i64,
    out: String,
}

#[derive(Deserialize)]
struct Frexp {
    #[serde(rename = "in")]
    input: String,
    frac: String,
    exp: i64,
}

#[derive(Deserialize)]
struct Ilogb {
    #[serde(rename = "in")]
    input: String,
    out: i64,
}

#[derive(Deserialize)]
struct Golden {
    sin: Vec<[String; 2]>,
    cos: Vec<[String; 2]>,
    atan2: Vec<[String; 3]>,
    asin: Vec<[String; 2]>,
    remainder: Vec<[String; 3]>,
    ldexp: Vec<Ldexp>,
    frexp: Vec<Frexp>,
    ilogb: Vec<Ilogb>,
    nextafter: Vec<[String; 3]>,
    max: Vec<[String; 3]>,
    min: Vec<[String; 3]>,
    sqrt: Vec<[String; 2]>,
}

fn unary(c: &mut Checker, name: &str, cases: &[[String; 2]], g: impl Fn(f64) -> f64) {
    for k in cases {
        let (x, want) = (f(&k[0]), f(&k[1]));
        let got = g(x);
        c.check(same(got, want), || {
            format!("{name}({x:e} {}) = {} want {}", k[0], hx(got), k[1])
        });
    }
}

fn binary(c: &mut Checker, name: &str, cases: &[[String; 3]], g: impl Fn(f64, f64) -> f64) {
    for k in cases {
        let (x, y, want) = (f(&k[0]), f(&k[1]), f(&k[2]));
        let got = g(x, y);
        c.check(same(got, want), || {
            format!("{name}({}, {}) = {} want {}", k[0], k[1], hx(got), k[2])
        });
    }
}

#[test]
fn go_math_matches_go() {
    let g: Golden = load("gomath.json");
    let mut c = Checker::new("gomath");
    unary(&mut c, "sin", &g.sin, go_sin);
    unary(&mut c, "cos", &g.cos, go_cos);
    unary(&mut c, "asin", &g.asin, go_asin);
    unary(&mut c, "sqrt", &g.sqrt, f64::sqrt);
    binary(&mut c, "atan2", &g.atan2, go_atan2);
    binary(&mut c, "remainder", &g.remainder, go_remainder);
    binary(&mut c, "nextafter", &g.nextafter, go_nextafter);
    binary(&mut c, "max", &g.max, go_max);
    binary(&mut c, "min", &g.min, go_min);
    for k in &g.ldexp {
        let got = go_ldexp(f(&k.frac), k.exp);
        c.check(same(got, f(&k.out)), || {
            format!("ldexp({}, {}) = {} want {}", k.frac, k.exp, hx(got), k.out)
        });
    }
    for k in &g.frexp {
        let (fr, e) = go_frexp(f(&k.input));
        c.check(same(fr, f(&k.frac)) && e == k.exp, || {
            format!(
                "frexp({}) = {} {e} want {} {}",
                k.input,
                hx(fr),
                k.frac,
                k.exp
            )
        });
        let l = go_ilogb(f(&k.input));
        let want = g.ilogb.iter().find(|i| i.input == k.input).map(|i| i.out);
        c.check(Some(l) == want, || {
            format!("ilogb({}) = {l} want {want:?}", k.input)
        });
    }
    c.finish();
}
