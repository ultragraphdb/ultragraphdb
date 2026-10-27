/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::Facet;

#[test]
fn a_facet_holds_its_key() {
    assert_eq!(Facet::new("since").key(), "since");
    assert_eq!(Facet::new(String::from("até")).key(), "até");
    assert_eq!(Facet::default().key(), "");
}
