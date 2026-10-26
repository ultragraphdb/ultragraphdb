/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod statistics_codec;

use std::collections::BTreeMap;

use ultragraphdb_common_utils::go_farm::fingerprint64;

/// The registers of each distinct-count sketch: a HyperLogLog of 64, about 13 % standard error.
pub const SKETCH_REGISTERS: usize = 64;

/// What a graph's predicate holds, or what a write changes of it: the statements, a count per object kind and literal
/// datatype, and sketches estimating its distinct subjects and objects. A write adds a delta and reads none; a read
/// merges the deltas: counts add, sketch registers take their maximum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statistics {
    statements: i64,
    /// By kind: `iri`, `blank`, or a literal's datatype IRI.
    kinds: BTreeMap<String, i64>,
    subjects: [u8; SKETCH_REGISTERS],
    objects: [u8; SKETCH_REGISTERS],
}

impl Default for Statistics {
    fn default() -> Self {
        Self {
            statements: 0,
            kinds: BTreeMap::new(),
            subjects: [0; SKETCH_REGISTERS],
            objects: [0; SKETCH_REGISTERS],
        }
    }
}

impl Statistics {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a statement inserted, of a subject with encoding `subject` and an object with encoding `object` of
    /// `kind`.
    pub fn inserted(&mut self, subject: &[u8], object: &[u8], kind: &str) {
        self.statements += 1;
        *self.kinds.entry(kind.to_string()).or_default() += 1;
        observe(&mut self.subjects, subject);
        observe(&mut self.objects, object);
    }

    /// Records a held statement deleted, its object of `kind`. The sketches estimate the distinct values ever seen.
    pub fn deleted(&mut self, kind: &str) {
        self.statements -= 1;
        *self.kinds.entry(kind.to_string()).or_default() -= 1;
    }

    /// Merges `other` in: counts add, registers take their maximum.
    pub fn merge(&mut self, other: &Statistics) {
        self.statements += other.statements;
        for (kind, n) in &other.kinds {
            *self.kinds.entry(kind.clone()).or_default() += n;
        }
        self.kinds.retain(|_, n| *n != 0);
        merge_registers(&mut self.subjects, &other.subjects);
        merge_registers(&mut self.objects, &other.objects);
    }

    pub fn statements(&self) -> i64 {
        self.statements
    }

    /// The count of objects of `kind`: `iri`, `blank`, or a literal's datatype IRI.
    pub fn kind(&self, kind: &str) -> i64 {
        self.kinds.get(kind).copied().unwrap_or(0)
    }

    pub fn kinds(&self) -> &BTreeMap<String, i64> {
        &self.kinds
    }

    /// The estimated distinct subjects.
    pub fn distinct_subjects(&self) -> u64 {
        estimate(&self.subjects)
    }

    /// The estimated distinct objects.
    pub fn distinct_objects(&self) -> u64 {
        estimate(&self.objects)
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Raises the register `value`'s hash selects to the position of the hash's first set bit after the index bits.
fn observe(registers: &mut [u8; SKETCH_REGISTERS], value: &[u8]) {
    let hash = fingerprint64(value);
    let index = (hash & (SKETCH_REGISTERS as u64 - 1)) as usize;
    let rank = ((hash >> 6) | (1 << 58)).trailing_zeros() as u8 + 1;
    registers[index] = registers[index].max(rank);
}

fn merge_registers(into: &mut [u8; SKETCH_REGISTERS], from: &[u8; SKETCH_REGISTERS]) {
    into.iter_mut()
        .zip(from)
        .for_each(|(a, b)| *a = (*a).max(*b));
}

/// HyperLogLog's estimate with its linear-counting correction for small counts.
fn estimate(registers: &[u8; SKETCH_REGISTERS]) -> u64 {
    let m = SKETCH_REGISTERS as f64;
    let zeros = registers.iter().filter(|r| **r == 0).count();
    if zeros == SKETCH_REGISTERS {
        return 0;
    }
    let raw = 0.709 * m * m
        / registers
            .iter()
            .map(|r| 2f64.powi(-i32::from(*r)))
            .sum::<f64>();
    if raw <= 2.5 * m && zeros > 0 {
        (m * (m / zeros as f64).ln()).round() as u64
    } else {
        raw.round() as u64
    }
}
