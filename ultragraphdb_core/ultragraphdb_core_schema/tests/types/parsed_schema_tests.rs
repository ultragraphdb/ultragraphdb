/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_schema::parse;

#[test]
fn parsed_schema_hands_out_its_parts() {
    let parsed = parse("name: string .\ntype T {\n name\n}").expect("parses");
    assert_eq!(parsed.preds().len(), 1);
    assert_eq!(parsed.types().len(), 1);
    let (preds, types) = parsed.into_parts();
    assert_eq!(preds[0].predicate, "0-name");
    assert_eq!(types[0].type_name, "0-T");
}
