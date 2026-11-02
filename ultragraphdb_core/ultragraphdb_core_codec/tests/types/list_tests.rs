/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::List;
use ultragraphdb_tool_bdd::TestResult;

use crate::codec_tests::sequences;

#[test]
fn test_a_list_holds_its_ids() -> TestResult {
    let ids = sequences([3, 1, 2].into_iter())?;
    let list = List::new(ids.clone());
    assert_eq!(list.ids(), &ids[..]);
    assert_eq!(list.into_ids(), ids);
    assert!(List::default().ids().is_empty());
    Ok(())
}

#[test]
fn test_a_list_is_changed_in_place() -> TestResult {
    let mut list = List::new(sequences(1..=2)?);
    list.ids_mut().extend(sequences(9..=9)?);
    let one = sequences(1..=1)?;
    list.ids_mut().retain(|id| !one.contains(id));
    assert_eq!(list, List::new(sequences([2, 9].into_iter())?));
    Ok(())
}
