/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Loading and comparing the golden vectors written by the Go generator (`s2gen`). Floats are
//! the 16 hex digits of their IEEE-754 bits; cell ids are 16 hex digits.
//!
//! The vectors come from golang/geo compiled with `-gcflags=all=-d=fmahash=n`, which keeps the
//! arm64 compiler from fusing multiply-adds and so computes what Go computes on amd64
//! (tools/golden/types_s2.go).

use serde::de::DeserializeOwned;
use ultragraphdb_core_types::{CellId, Point, R1Interval, R3Vector, Rect, S1Interval};

/// Loads `testdata/golden/types_s2_<name>`; each file holds its vectors as its one case.
pub fn load<T: DeserializeOwned>(name: &str) -> T {
    let stem = name.trim_end_matches(".json");
    let path = ultragraphdb_test_bdd::testdata_dir("ultragraphdb_core_types")
        .join(format!("golden/types_s2_{stem}.json"));
    let golden = ultragraphdb_test_bdd::load_golden::<serde_json::Value>(&path);
    serde_json::from_value(golden.cases()[0].clone())
        .unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()))
}

pub fn f(s: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(s, 16).expect("hex float"))
}

pub fn hx(x: f64) -> String {
    format!("{:016x}", x.to_bits())
}

pub fn pt(v: &[String; 3]) -> Point {
    Point::from_vector(R3Vector::new(f(&v[0]), f(&v[1]), f(&v[2])))
}

pub fn cell(s: &str) -> CellId {
    CellId::new(u64::from_str_radix(s, 16).expect("hex cell id"))
}

pub fn rect(r: &[String; 4]) -> Rect {
    Rect::new(
        R1Interval::new(f(&r[0]), f(&r[1])),
        S1Interval::new(f(&r[2]), f(&r[3])),
    )
}

/// Bit equality, with every NaN equal to every NaN (NaN payloads differ between CPUs).
pub fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

pub fn same_point(a: Point, b: &[String; 3]) -> bool {
    same(a.x(), f(&b[0])) && same(a.y(), f(&b[1])) && same(a.z(), f(&b[2]))
}

pub fn same_rect(a: Rect, b: &[String; 4]) -> bool {
    same(a.lat().lo(), f(&b[0]))
        && same(a.lat().hi(), f(&b[1]))
        && same(a.lng().lo(), f(&b[2]))
        && same(a.lng().hi(), f(&b[3]))
}

pub fn ids(v: &[CellId]) -> Vec<String> {
    v.iter().map(|c| format!("{:016x}", c.id())).collect()
}

pub fn tokens(v: &[CellId]) -> Vec<String> {
    v.iter().map(|c| c.to_token()).collect()
}

/// FNV-1a 64 over the tokens, each followed by ',' (the generator's covering hash).
pub fn tokens_hash(t: &[String]) -> String {
    let mut h: u64 = 14695981039346656037;
    for s in t {
        for b in s.bytes().chain(std::iter::once(b',')) {
            h ^= b as u64;
            h = h.wrapping_mul(1099511628211);
        }
    }
    format!("{h:016x}")
}

/// FNV-1a 64 over the bits of every vertex coordinate (the generator's vertex hash).
pub fn vertex_hash(vs: &[Point]) -> String {
    let mut h: u64 = 14695981039346656037;
    for v in vs {
        for x in [v.x(), v.y(), v.z()] {
            let b = x.to_bits();
            for k in 0..8 {
                h ^= (b >> (8 * k)) & 0xff;
                h = h.wrapping_mul(1099511628211);
            }
        }
    }
    format!("{h:016x}")
}

/// Collects mismatches so that one run reports all of them, then fails with a sample.
pub struct Checker {
    name: &'static str,
    checks: usize,
    failures: Vec<String>,
}

impl Checker {
    pub fn new(name: &'static str) -> Checker {
        Checker {
            name,
            checks: 0,
            failures: Vec::new(),
        }
    }

    pub fn check(&mut self, ok: bool, msg: impl FnOnce() -> String) {
        self.checks += 1;
        if !ok {
            self.failures.push(msg());
        }
    }

    pub fn finish(self) {
        println!(
            "{}: {} checks against Go's vectors, {} mismatches",
            self.name,
            self.checks,
            self.failures.len()
        );
        assert!(
            self.checks > 0,
            "{}: no golden cases were checked",
            self.name
        );
        if !self.failures.is_empty() {
            let sample: Vec<&String> = self.failures.iter().take(25).collect();
            panic!(
                "{}: {} of {} checks differ from Go:\n{:#?}",
                self.name,
                self.failures.len(),
                self.checks,
                sample
            );
        }
    }
}
