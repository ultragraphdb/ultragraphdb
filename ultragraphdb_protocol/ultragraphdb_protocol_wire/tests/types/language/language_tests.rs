/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::Language;

#[test]
fn test_languages_keep_their_numbers() {
    // PROTOCOL.md sections 7.4 and 8.1: 0 a quad pattern, 1 GQL, 2 SPARQL, 3 UQL.
    let all = [
        Language::Pattern,
        Language::Gql,
        Language::Sparql,
        Language::Uql,
    ];
    for (number, language) in (0u8..).zip(all) {
        assert_eq!(language.number(), number);
        assert_eq!(Language::from_number(number), Some(language));
    }
    assert_eq!(Language::from_number(4), None);
    assert_eq!(Language::from_number(255), None);
}
