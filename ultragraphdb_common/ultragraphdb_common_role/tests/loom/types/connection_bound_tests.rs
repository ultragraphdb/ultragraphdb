/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use loom::thread;
use ultragraphdb_common_role::ConnectionBound;

/// Two connections entering a bound of one on two cores at once: whichever comes first in any interleaving, exactly
/// one lies within it, and once both end the bound holds none.
#[test]
fn test_two_entering_a_bound_of_one_admit_exactly_one() {
    loom::model(|| {
        let bound = ConnectionBound::new(1);
        let other = bound.clone();
        let theirs = thread::spawn(move || other.enter());
        let mine = bound.enter();
        let theirs = theirs.join();
        assert!(theirs.is_ok(), "the other core's connection");
        let within = [
            mine.within(),
            theirs.as_ref().is_ok_and(|held| held.within()),
        ];
        assert_eq!(within.iter().filter(|w| **w).count(), 1, "{within:?}");
        drop((mine, theirs));
        assert_eq!(bound.held(), 0);
    });
}

/// Three connections entering a bound of two on three cores at once: exactly two lie within it.
#[test]
fn test_three_entering_a_bound_of_two_admit_exactly_two() {
    loom::model(|| {
        let bound = ConnectionBound::new(2);
        let (one, two) = (bound.clone(), bound.clone());
        let first = thread::spawn(move || one.enter());
        let second = thread::spawn(move || two.enter());
        let mine = bound.enter();
        let (first, second) = (first.join(), second.join());
        assert!(
            first.is_ok() && second.is_ok(),
            "the other cores' connections"
        );
        let within = [
            mine.within(),
            first.as_ref().is_ok_and(|held| held.within()),
            second.as_ref().is_ok_and(|held| held.within()),
        ];
        assert_eq!(within.iter().filter(|w| **w).count(), 2, "{within:?}");
        drop((mine, first, second));
        assert_eq!(bound.held(), 0);
    });
}

/// A connection ending on one core while another enters on a second: the newcomer lies within a bound of one exactly
/// when the first had left, never both within at once, and the bound ends holding none.
#[test]
fn test_a_connection_leaving_as_another_enters_frees_its_place() {
    loom::model(|| {
        let bound = ConnectionBound::new(1);
        let first = bound.enter();
        assert!(first.within());
        let leaving = thread::spawn(move || drop(first));
        let newcomer = bound.enter();
        let held_with_first = bound.held();
        assert!(leaving.join().is_ok(), "the leaving core");
        assert!(
            !newcomer.within() || held_with_first == 1,
            "the newcomer within while the first was held"
        );
        drop(newcomer);
        assert_eq!(bound.held(), 0);
    });
}
