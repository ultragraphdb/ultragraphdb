/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU16;

use ultragraphdb_core_mapping::VectorIndexSpec;

fn options() -> Vec<(String, String)> {
    vec![("metric".to_string(), "cosine".to_string())]
}

#[test]
fn test_a_spec_states_no_dimension_until_given_one() {
    let spec = VectorIndexSpec::new("hnsw", options());
    assert_eq!(spec.dimension(), None);
    assert_eq!(spec.factory(), "hnsw");
    assert_eq!(spec.options(), options().as_slice());
}

#[test]
fn test_a_dimension_given_is_held_beside_the_factory_and_options() {
    let spec =
        VectorIndexSpec::new("hnsw", options()).with_dimension(NonZeroU16::new(768).unwrap());
    assert_eq!(spec.dimension(), NonZeroU16::new(768));
    assert_eq!(spec.factory(), "hnsw");
    assert_eq!(spec.options(), options().as_slice());
    // The largest dimension a vector literal holds, 65,535.
    let widest = VectorIndexSpec::new("hnsw", Vec::new()).with_dimension(NonZeroU16::MAX);
    assert_eq!(widest.dimension().map(NonZeroU16::get), Some(65_535));
}

#[test]
fn test_two_specs_differing_only_in_dimension_differ() {
    let of = |d: u16| {
        VectorIndexSpec::new("hnsw", options()).with_dimension(NonZeroU16::new(d).unwrap())
    };
    assert_eq!(of(4), of(4));
    assert_ne!(of(4), of(5));
    assert_ne!(of(4), VectorIndexSpec::new("hnsw", options()));
    // A later dimension replaces an earlier one.
    assert_eq!(of(4).with_dimension(NonZeroU16::new(5).unwrap()), of(5));
}
