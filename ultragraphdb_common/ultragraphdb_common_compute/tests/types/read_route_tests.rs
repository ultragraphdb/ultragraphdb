/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Which partitions a read asks by its bound positions, and what it is refused for reaching (`PROTOCOL.md` §11.12, §13.12; codes 23
//! and 25 in the table of error codes).

use std::num::NonZeroU64;

use ultragraphdb_common_compute::read_times_of;
use ultragraphdb_core_ids::{BucketGroup, Epoch};
use ultragraphdb_core_keys::QuadKey;
use ultragraphdb_protocol_wire::{ComputeRead, ErrorCode, ReadLevel};
use ultragraphdb_store_quads::{ReferrerSet, iri_bucket};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::played_answers::{Pattern, ReadAnswer, answer_on, iri, read_of, snapshot};
use crate::utils::played_cluster::{Played, TENANT, id, run};

const P: &str = "http://ex.org/p";

/// The `detail` of codes 23 and 25: the transaction's partition, or the lowest reached, in bits 0–31, the lowest other in 32–63.
fn partitions_detail(partition: u32, other: u32) -> u64 {
    u64::from(partition) | (u64::from(other) << 32)
}

/// A read-only read at latest level of `pattern`.
fn latest(pattern: &Pattern) -> TestResult<ComputeRead> {
    read_of(pattern, true, ReadLevel::Latest, Vec::new())
}

/// A read-write transaction's read of `pattern`, the transaction bound to `partition`.
fn read_write(pattern: &Pattern, partition: u32) -> TestResult<ComputeRead> {
    Ok(read_of(pattern, false, ReadLevel::Snapshot, Vec::new())?
        .with_partition(partition)
        .with_transaction([5; 16])
        .with_read_version(ultragraphdb_core_ids::Version::bound_of(Epoch::new(3)?)))
}

/// A cluster of `partitions` partitions where each holds `<s{n}> <p> <o{n}>` for a subject and object it owns; those subjects.
async fn each_holding(partitions: u32) -> TestResult<(Played, Vec<String>)> {
    let played = Played::start(partitions, read_times_of(None, None)).await?;
    let mut subjects = Vec::new();
    for number in 1..=partitions {
        let subject = played.owned_by(number, "s")?;
        let object = played.owned_by(number, "o")?;
        let base = u64::from(number) * 10;
        played
            .partition(number)?
            .quad((&subject, P, &object), (base + 1, 1, base + 2))?;
        subjects.push(subject);
    }
    Ok((played, subjects))
}

/// `read`'s answer from `played` on a connection of its own.
async fn answer(played: &Played, read: &ComputeRead) -> TestResult<ReadAnswer> {
    let mut client = played.client().await?;
    answer_on(&mut client, read).await
}

/// A read binding its subject reads the one partition owning the subject, whatever its level, and no other: at latest level that
/// is no refusal, bound to that partition or to none.
#[test]
fn test_a_read_of_a_bound_subject_reads_its_owner_alone() -> TestResult {
    run(async {
        let (played, subjects) = each_holding(2).await?;
        let subject = subjects.get(1).context("partition 2's subject")?;
        let pattern = Pattern::of(Some(subject), Some(P), None);
        for read in [
            snapshot(&pattern)?,
            latest(&pattern)?,
            latest(&pattern)?.with_partition(2),
        ] {
            let rows = answer(&played, &read).await?.whole()?;
            assert_eq!(rows.len(), 1, "{rows:?}");
        }
        assert!(played.partition(1)?.scans().is_empty());
        Ok(())
    })
}

/// A snapshot read with its subject unbound reads every partition holding a bucket of its tenant's run, here both.
#[test]
fn test_a_snapshot_read_of_an_unbound_subject_reads_every_holder() -> TestResult {
    run(async {
        let (played, _) = each_holding(2).await?;
        let rows = answer(&played, &snapshot(&Pattern::of(None, Some(P), None))?)
            .await?
            .whole()?;
        assert_eq!(rows.len(), 2, "{rows:?}");
        Ok(())
    })
}

/// A read at latest level reaching two partitions is refused with code 25, `detail` the lowest and the other, having read none.
#[test]
fn test_a_latest_read_reaching_two_partitions_is_refused_with_code_25() -> TestResult {
    run(async {
        let (played, _) = each_holding(2).await?;
        let holders: Vec<u32> = played
            .map()
            .holders_of_run(ultragraphdb_core_ids::Tenant::new(TENANT), Epoch::ZERO)
            .into_iter()
            .map(|(holder, _)| holder.get())
            .collect();
        assert_eq!(holders, [1, 2]);
        let answered = answer(&played, &latest(&Pattern::of(None, Some(P), None))?).await?;
        let refused = answered.refusal()?;
        assert_eq!(refused.code, ErrorCode::LevelRefused);
        assert_eq!(refused.detail, partitions_detail(1, 2));
        assert!(played.partition(1)?.scans().is_empty());
        assert!(played.partition(2)?.scans().is_empty());
        Ok(())
    })
}

/// A read at latest level of a transaction bound to one partition that reaches another is refused with code 25: `detail` the
/// transaction's partition, then the other, and the message naming both.
#[test]
fn test_a_latest_read_bound_to_one_partition_reaching_another_is_refused() -> TestResult {
    run(async {
        let (played, subjects) = each_holding(2).await?;
        let subject = subjects.first().context("partition 1's subject")?;
        let read = latest(&Pattern::of(Some(subject), Some(P), None))?.with_partition(2);
        let answered = answer(&played, &read).await?;
        let refused = answered.refusal()?;
        assert_eq!(refused.code, ErrorCode::LevelRefused);
        assert_eq!(refused.detail, partitions_detail(2, 1));
        assert!(refused.message.contains("[1, 2]"), "{}", refused.message);
        Ok(())
    })
}

/// A read-write transaction's read reads its own partition: of a subject that partition owns, or with the subject unbound where its
/// tenant lies on that partition alone.
#[test]
fn test_a_read_write_read_of_its_own_partition_is_answered() -> TestResult {
    run(async {
        let (played, subjects) = each_holding(2).await?;
        let subject = subjects.get(1).context("partition 2's subject")?;
        let read = read_write(&Pattern::of(Some(subject), Some(P), None), 2)?;
        assert_eq!(answer(&played, &read).await?.whole()?.len(), 1);
        let (alone, _) = each_holding(1).await?;
        let unbound = read_write(&Pattern::of(None, Some(P), None), 1)?;
        assert_eq!(answer(&alone, &unbound).await?.whole()?.len(), 1);
        Ok(())
    })
}

/// A read-write transaction's read reaching another partition is refused with code 23, `detail` its partition then the other: a
/// subject another partition owns, or an unbound subject of a tenant other partitions hold too.
#[test]
fn test_a_read_write_read_reaching_another_partition_is_refused_with_code_23() -> TestResult {
    run(async {
        let (played, subjects) = each_holding(2).await?;
        let subject = subjects.first().context("partition 1's subject")?;
        for (read, detail) in [
            (
                read_write(&Pattern::of(Some(subject), Some(P), None), 2)?,
                partitions_detail(2, 1),
            ),
            (
                read_write(&Pattern::of(None, Some(P), None), 1)?,
                partitions_detail(1, 2),
            ),
        ] {
            let answered = answer(&played, &read).await?;
            let refused = answered.refusal()?;
            assert_eq!(refused.code, ErrorCode::CrossPartition);
            assert_eq!(refused.detail, detail);
        }
        Ok(())
    })
}

/// A reverse read at latest level whose object's referrer set names a group another partition holds reaches two partitions, the
/// object's home and that one, and is refused with code 25.
#[test]
fn test_a_latest_reverse_read_reaching_a_referrers_partition_is_refused() -> TestResult {
    run(async {
        let played = Played::start(2, read_times_of(None, None)).await?;
        let object = played.owned_by(1, "o")?;
        let subject = played.owned_by(2, "s")?;
        let home = played.partition(1)?;
        let object_id = id(2)?;
        home.term(&object, object_id);
        let group = BucketGroup::of(iri_bucket(TENANT, &subject).context("a bucket")?);
        let mut set = ReferrerSet::new();
        set.set(group, NonZeroU64::MIN, Epoch::new(1)?);
        let mut value = Vec::new();
        set.encode(&mut value);
        let key = QuadKey::Referrers {
            tenant: TENANT,
            entity: object_id,
        };
        home.put(key.to_bytes(), value);
        let read = latest(&Pattern::of(None, None, Some(iri(&object))))?;
        let answered = answer(&played, &read).await?;
        let refused = answered.refusal()?;
        assert_eq!(refused.code, ErrorCode::LevelRefused);
        assert_eq!(refused.detail, partitions_detail(1, 2));
        assert!(played.partition(2)?.scans().is_empty());
        Ok(())
    })
}
