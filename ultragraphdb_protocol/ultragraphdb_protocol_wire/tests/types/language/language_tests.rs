/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::Language;

#[test]
fn test_languages_keep_their_numbers() {
    let all = [Language::Pattern, Language::Gql, Language::Sparql];
    for (number, language) in (0u8..).zip(all) {
        assert_eq!(language.number(), number);
        assert_eq!(Language::from_number(number), Some(language));
    }
    assert_eq!(Language::from_number(3), None);
}
