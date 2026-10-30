/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/persistent_factory.go (persistentIndexFactory). Changed for ultragraphdb:
 * translated to Rust; the factory keeps no map of indexes, every index living for one insert or one search.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_store_hnsw::{HnswError, HnswErrorEnum, HnswFactory};
use ultragraphdb_traits_index::IndexFactory;

use crate::utils::helpers::{opts, vkeys};

#[test]
fn test_the_factory_names_itself_and_its_options() {
    let f = HnswFactory::<f32>::new();
    assert_eq!(f.name(), "hnsw");
    assert_eq!(
        f.allowed_options().get_option_list(),
        vec![
            "efConstruction",
            "efSearch",
            "exponent",
            "maxLevels",
            "metric"
        ]
    );
    assert_eq!(
        f.get_options(&opts(&[("maxLevels", "4"), ("metric", "cosine")])),
        r#"("maxLevels":"4","metric":"cosine")"#
    );
}

/// The factory builds a fresh index on every call, as Dgraph's index maintenance and search build one through
/// `CreateOrReplace`; it keeps no map, so `find` finds none and `remove` has nothing to remove.
#[test]
fn test_every_index_is_created_fresh() {
    let f = HnswFactory::<f32>::new();
    let o = opts(&[("exponent", "4")]);
    let keys = vkeys("0-emb");
    let ix = f.create(&keys, &o, 32).unwrap();
    assert_eq!(
        (ix.max_levels(), ix.ef_construction(), ix.ef_search()),
        (4, 200, 120)
    );
    // An exponent of 4 sets 4 levels, 50 * 4 and 30 * 4 (the spec's "The exponent sets the parameters").
    let replaced = f.create_or_replace(&keys, &o, 32).unwrap();
    assert_eq!(replaced.keys().name(), "0-emb");
    assert_eq!(
        (
            replaced.max_levels(),
            replaced.ef_construction(),
            replaced.ef_search()
        ),
        (4, 200, 120)
    );
    assert!(f.find(&keys).unwrap().is_none());
    assert_eq!(f.remove(&keys), Ok(()));
}

/// The factory hands the float width through: a 32-bit index refuses 64 float bits whichever way it is made.
#[test]
fn test_the_factory_refuses_another_float_width() {
    let f = HnswFactory::<f32>::new();
    let (keys, o) = (vkeys("0-emb"), opts(&[]));
    for e in [
        f.create(&keys, &o, 64).unwrap_err(),
        f.create_or_replace(&keys, &o, 64).unwrap_err(),
    ] {
        assert_eq!(
            e.0,
            HnswErrorEnum::FloatBits {
                bits: 64,
                expected: 32
            }
        );
    }
}

#[test]
fn test_an_option_below_one_is_refused() {
    let f = HnswFactory::<f32>::new();
    let err = f
        .create(&vkeys("0-emb"), &opts(&[("efSearch", "0")]), 32)
        .unwrap_err();
    assert!(
        matches!(
            err,
            HnswError(HnswErrorEnum::InvalidParameter {
                name: "efSearch",
                value: 0
            })
        ),
        "{err:?}"
    );
}
