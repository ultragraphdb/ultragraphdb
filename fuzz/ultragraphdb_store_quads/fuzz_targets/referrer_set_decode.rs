/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The referrer set's decoder over arbitrary bytes, the value a home's store holds for an entity. Anything it refuses it
//! refuses as corrupted, naming the key; nothing panics. An accepted set encodes back to the bytes it was decoded from,
//! its members are strictly sorted by group, and the generation rule holds on it: a registration naming a generation
//! below a member's changes nothing, and one naming the member's own renews it one above, keeping its epoch.

#![no_main]

use libfuzzer_sys::fuzz_target;
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_store_quads::{QuadErrorEnum, ReferrerSet};

/// The key the refusals name.
const KEY: &[u8] = b"referrers";

fn check(data: &[u8]) {
    let set = match ReferrerSet::decode(KEY, data) {
        Ok(set) => set,
        Err(refusal) => {
            assert!(
                matches!(&refusal.0, QuadErrorEnum::Corrupted { key, .. } if key == KEY),
                "{refusal}"
            );
            return;
        }
    };
    let mut encoded = Vec::new();
    set.encode(&mut encoded);
    assert_eq!(encoded, data, "an accepted set encodes back to its bytes");
    assert!(
        set.members()
            .windows(2)
            .all(|w| w[0].group() < w[1].group()),
        "members are strictly sorted by group"
    );
    for member in set.members() {
        let (group, generation) = (member.group(), member.generation());
        // An epoch other than the member's, so a rule that took the registration's would show.
        let other = Epoch::new(member.epoch().get() ^ 1).unwrap_or(Epoch::ZERO);
        let mut repeated = set.clone();
        let repeat = repeated.register(group, generation.get() - 1, other);
        assert_eq!((repeat.generation, repeat.changed), (generation, false));
        assert_eq!(repeated, set, "a repeat changes nothing");
        let mut renewed = set.clone();
        let renewal = renewed.register(group, generation.get(), other);
        assert_eq!(renewal.generation, generation.saturating_add(1));
        assert!(renewal.changed);
        let kept = renewed.member(group).map(|m| m.epoch());
        assert_eq!(kept, Some(member.epoch()), "a renewal keeps the epoch");
        assert_eq!(renewed.members().len(), set.members().len());
    }
}

fuzz_target!(|data: &[u8]| check(data));
