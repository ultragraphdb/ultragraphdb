/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_compute::{LookupAt, prefix_end, read_times_of};
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::{Graph, QuadKey};
use ultragraphdb_protocol_wire::ErrorCode;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::played_reads::played_cluster::{PlayedCluster, run};
use crate::played_reads::played_data::{id_of, ids_list, name, name_key, tenant_over};
use crate::played_reads::played_partition::{PLAYED_EPOCH, PlayedPartition};

#[test]
fn test_the_key_past_a_prefix_raises_its_last_byte() {
    assert_eq!(prefix_end(&[1, 2, 3]), [1, 2, 4]);
}

#[test]
fn test_the_key_past_a_prefix_drops_what_cannot_rise() {
    assert_eq!(prefix_end(&[1, 2, 255, 255]), [1, 3]);
}

#[test]
fn test_no_key_lies_past_a_prefix_of_every_byte_at_its_top() {
    assert_eq!(prefix_end(&[255, 255]), Vec::<u8>::new());
}

const PREDICATE: &str = "http://ex.org/name";

/// The value index's key of `token` of the `term` tokenizer under `predicate`.
fn token_key(tenant: u64, predicate: SystemId, token: &[u8]) -> Vec<u8> {
    QuadKey::ValueIndex {
        tenant,
        graph: Graph::Default,
        predicate,
        tokenizer: 1,
        token: token.to_vec(),
    }
    .to_bytes()
}

/// A partition holding the tokens `ann` and `bob` under the predicate, each listing a subject of its own, the second subject
/// named as `second` says: the subjects' IDs.
fn two_tokens(part: &PlayedPartition, tenant: u64, second: bool) -> TestResult<[SystemId; 2]> {
    let predicate = id_of(tenant, 1)?;
    name(part, tenant, PREDICATE, predicate);
    let subjects = [id_of(tenant, 2)?, id_of(tenant, 3)?];
    name(part, tenant, "http://ex.org/ann", subjects[0]);
    if second {
        name(part, tenant, "http://ex.org/bob", subjects[1]);
    }
    part.put(
        token_key(tenant, predicate, b"ann"),
        ids_list(&subjects[..1]),
    );
    part.put(
        token_key(tenant, predicate, b"bob"),
        ids_list(&subjects[1..]),
    );
    Ok(subjects)
}

/// A lookup of one token reads that token's list alone, by a range holding its key and nothing past it, and the predicate's and
/// subjects' dictionary entries likewise (`PROTOCOL.md` §13.13: a lookup of one key is a range holding it alone).
#[test]
fn test_a_lookup_reads_the_one_key_it_names() -> TestResult {
    run(async {
        let cluster = PlayedCluster::start(1, read_times_of(None, None)).await?;
        let tenant = tenant_over(1, 1)?;
        let _subjects = two_tokens(cluster.partition(1)?, tenant, true)?;
        let at = LookupAt::new(PLAYED_EPOCH, 1_000);
        let ann = cluster
            .core
            .value_index(tenant, at, PREDICATE, (1, b"ann"))
            .await;
        assert_eq!(ann.ok(), Some(vec!["http://ex.org/ann".to_string()]));
        let bob = cluster
            .core
            .value_index(tenant, at, PREDICATE, (1, b"bob"))
            .await;
        assert_eq!(bob.ok(), Some(vec!["http://ex.org/bob".to_string()]));
        Ok(())
    })
}

/// A list naming an ID its partition's dictionary lacks refuses the lookup with code 17, the message naming the missing entry's
/// key in hexadecimal.
#[test]
fn test_a_dangling_id_is_refused_naming_its_key() -> TestResult {
    run(async {
        let cluster = PlayedCluster::start(1, read_times_of(None, None)).await?;
        let tenant = tenant_over(1, 1)?;
        let subjects = two_tokens(cluster.partition(1)?, tenant, false)?;
        let at = LookupAt::new(PLAYED_EPOCH, 1_000);
        let refused = cluster
            .core
            .value_index(tenant, at, PREDICATE, (1, b"bob"))
            .await
            .err()
            .context("a refusal")?;
        assert_eq!(refused.code(), ErrorCode::Unavailable);
        let missing = subjects.get(1).context("the second subject")?;
        let key: String = name_key(tenant, *missing)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert!(
            refused.message().contains(&key),
            "{} names {key}",
            refused.message()
        );
        Ok(())
    })
}
