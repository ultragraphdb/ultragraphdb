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

use crate::utils::helpers::opts;

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
    let ix = f.create("0-emb", &o, 32).unwrap();
    assert_eq!(
        (ix.max_levels(), ix.ef_construction(), ix.ef_search()),
        (4, 200, 120)
    );
    assert_eq!(
        f.create_or_replace("0-emb", &o, 32).unwrap().pred(),
        "0-emb"
    );
    assert!(f.find("0-emb").unwrap().is_none());
    assert!(f.remove("0-emb").is_ok());
}

#[test]
fn test_an_option_below_one_is_refused() {
    let f = HnswFactory::<f32>::new();
    let err = f
        .create("0-emb", &opts(&[("efSearch", "0")]), 32)
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
