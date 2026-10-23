/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The metadata keyspace: what compaction acts on is durable when its call returns, and read back at open.

use ultragraphdb_traits_store::Store;

use crate::test_store::{TestDir, ns_key, put, write};

#[test]
fn test_the_settings_survive_a_reopen() {
    let dir = TestDir::new("meta");
    let s = dir.open();
    s.set_discard_ts(5).unwrap();
    s.set_tombstone_horizon(3).unwrap();
    s.ban_namespace(7).unwrap();
    drop(s);
    let s = dir.open();
    assert_eq!((s.discard_ts(), s.tombstone_horizon()), (5, 3));
    let seven = ns_key(7, b"x");
    write(&s, vec![put(&seven, 6, b"")]);
    assert_eq!(s.get_at(&seven, 6).unwrap(), None);
}

#[test]
fn test_the_horizon_never_exceeds_the_discard_timestamp() {
    let dir = TestDir::new("meta-horizon");
    let s = dir.open();
    s.set_tombstone_horizon(9).unwrap();
    assert_eq!(s.tombstone_horizon(), 0);
    s.set_discard_ts(4).unwrap();
    assert_eq!(s.tombstone_horizon(), 4);
    s.set_discard_ts(12).unwrap();
    assert_eq!(s.tombstone_horizon(), 9);
}
