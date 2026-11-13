/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod statistics_codec;

use std::collections::BTreeMap;

use xxhash_rust::xxh3::xxh3_64;

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
    /// Statistics of nothing: no statements, no kinds, empty sketches.
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

    /// The count of statements.
    pub fn statements(&self) -> i64 {
        self.statements
    }

    /// The count of objects of `kind`: `iri`, `blank`, or a literal's datatype IRI.
    pub fn kind(&self, kind: &str) -> i64 {
        self.kinds.get(kind).copied().unwrap_or(0)
    }

    /// The count of objects of each kind.
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

    /// Whether the statistics count nothing and their sketches are empty.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

// A register's index is read from the hash's low byte.
const _: () = assert!(256 % SKETCH_REGISTERS == 0);

/// Raises the register `value`'s hash selects to the position of the hash's first set bit after the index bits.
fn observe(registers: &mut [u8; SKETCH_REGISTERS], value: &[u8]) {
    let hash = xxh3_64(value);
    // The low 6 bits select the register: 64 divides 256, so the low byte holds them.
    let [low, ..] = hash.to_le_bytes();
    let index = usize::from(low) % SKETCH_REGISTERS;
    // At most 59, so the count's low byte is the whole of it.
    let [rank, ..] = (((hash >> 6) | (1 << 58)).trailing_zeros() + 1).to_le_bytes();
    // `index` is below the register count, so the register is always there.
    if let Some(register) = registers.get_mut(index) {
        *register = (*register).max(rank);
    }
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
    let rounded = if raw <= 2.5 * m && zeros > 0 {
        (m * (m / zeros as f64).ln()).round()
    } else {
        raw.round()
    };
    // The float saturates into an i64, and an estimate, a count, is never below zero.
    u64::try_from(rounded as i64).unwrap_or(0)
}
