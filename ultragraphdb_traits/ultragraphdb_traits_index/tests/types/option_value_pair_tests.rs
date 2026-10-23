/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_index::OptionValuePair;

#[test]
fn test_a_pair_keeps_its_option_and_text() {
    let p = OptionValuePair::new("metric", "cosine");
    assert_eq!((p.option(), p.value()), ("metric", "cosine"));
    assert_eq!(p.clone(), p);
}
