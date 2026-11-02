/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_placement::{BucketRange, NodeId, PartitionId, PlacementMap};
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_error_cluster::ClusterErrorEnum;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::utils::reference_model::END;

fn assign(start: u32, end: u32, partition: u32) -> TestResult<(BucketRange, PartitionId)> {
    Ok((BucketRange::new(start, end)?, PartitionId::new(partition)?))
}

/// An even map over one partition led by node 7, by hand: format 1, one layout from epoch 0 holding one range at
/// bucket 0 on partition 1, one leader, partition 1 on node 7.
const ONE: [u8; 35] = [
    0x01, 0x00, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0,
    0, 1, 0, 0, 0, 7,
];

/// [`ONE`] changed from epoch 5 to 0..100 on partition 2 and the rest on 1, by hand. Offsets: the format 0, the
/// layout count 1, the first layout's epoch 3, range count 11, range 15 and 19; the second layout's epoch 23, range
/// count 31, ranges 35, 39, 43 and 47; the leader count 51, the leader 55 and 59.
const TWO: [u8; 63] = [
    0x01, 0x00, 0x02, // format, two layouts
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, // from 0: bucket 0 on 1
    0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 2, // from 5, two ranges
    0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0x64, 0, 0, 0, 1, // bucket 0 on 2, bucket 100 on 1
    0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 7, // one leader: partition 1 on node 7
];

fn two() -> TestResult<PlacementMap> {
    let mut map = PlacementMap::even(1, NodeId::new(7))?;
    map.change(Epoch::new(5)?, &[assign(0, 100, 2)?, assign(100, END, 1)?])?;
    Ok(map)
}

fn encoded(map: &PlacementMap) -> Vec<u8> {
    let mut out = Vec::new();
    map.encode(&mut out);
    out
}

/// `bytes` with the bytes at `offset` replaced by `with`.
fn patched(bytes: &[u8], offset: usize, with: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    for (i, byte) in with.iter().enumerate() {
        if let Some(slot) = out.get_mut(offset + i) {
            *slot = *byte;
        }
    }
    out
}

fn refusal(bytes: &[u8]) -> TestResult<ClusterErrorEnum> {
    Ok(PlacementMap::decode(bytes).err().context("a refusal")?.0)
}

fn malformed(what: &'static str, offset: usize) -> ClusterErrorEnum {
    ClusterErrorEnum::Malformed { what, offset }
}

#[test]
fn test_maps_encode_as_written_by_hand() -> TestResult {
    assert_eq!(encoded(&PlacementMap::even(1, NodeId::new(7))?), ONE);
    assert_eq!(encoded(&two()?), TWO);
    Ok(())
}

#[test]
fn test_the_hand_written_encodings_decode() -> TestResult {
    assert_eq!(
        PlacementMap::decode(&ONE)?,
        PlacementMap::even(1, NodeId::new(7))?
    );
    assert_eq!(PlacementMap::decode(&TWO)?, two()?);
    Ok(())
}

#[test]
fn test_a_map_round_trips() -> TestResult {
    let mut map = PlacementMap::even(3, NodeId::new(2))?;
    map.set_preferred_leader(PartitionId::new(3)?, NodeId::new(9))?;
    let mut layout = map.ranges_at(Epoch::new(0)?);
    layout.retain(|(range, _)| range.start() >= 65_536);
    layout.push(assign(0, 65_536, 2)?);
    map.change(Epoch::new(40)?, &layout)?;
    let bytes = encoded(&map);
    let decoded = PlacementMap::decode(&bytes)?;
    assert_eq!(decoded, map);
    assert_eq!(encoded(&decoded), bytes);
    Ok(())
}

#[test]
fn test_encoding_appends_to_the_buffer() -> TestResult {
    let mut out = vec![0xEE];
    PlacementMap::even(1, NodeId::new(7))?.encode(&mut out);
    assert_eq!(out.first(), Some(&0xEE));
    assert_eq!(out.get(1..), Some(&ONE[..]));
    Ok(())
}

#[test]
fn test_the_header_is_checked() -> TestResult {
    assert_eq!(refusal(&[])?, malformed("the format byte 1", 0));
    assert_eq!(
        refusal(&patched(&ONE, 0, &[2]))?,
        malformed("the format byte 1", 0)
    );
    assert_eq!(
        refusal(&patched(&ONE, 1, &[0, 0]))?,
        malformed("a layout count from 1 to 256", 1)
    );
    assert_eq!(
        refusal(&patched(&ONE, 1, &[1, 1]))?,
        malformed("a layout count from 1 to 256", 1)
    );
    Ok(())
}

#[test]
fn test_layout_epochs_are_checked() -> TestResult {
    assert_eq!(
        refusal(&patched(&ONE, 10, &[1]))?,
        malformed("the first layout from epoch 0", 3)
    );
    assert_eq!(
        refusal(&patched(&TWO, 23, &[0, 0, 0x04, 0, 0, 0, 0, 0]))?,
        malformed("an epoch of 42 bits", 23)
    );
    assert_eq!(
        refusal(&patched(&TWO, 30, &[0]))?,
        malformed("a layout after the one before", 23)
    );
    Ok(())
}

#[test]
fn test_ranges_are_checked() -> TestResult {
    assert_eq!(
        refusal(&patched(&ONE, 14, &[0]))?,
        malformed("a range count from 1 to 262144", 11)
    );
    assert_eq!(
        refusal(&patched(&ONE, 11, &[0, 4, 0, 1]))?,
        malformed("a range count from 1 to 262144", 11)
    );
    assert_eq!(
        refusal(&patched(&ONE, 18, &[1]))?,
        malformed("the first range at bucket 0", 15)
    );
    assert_eq!(
        refusal(&patched(&TWO, 46, &[0]))?,
        malformed("a range after the one before", 43)
    );
    assert_eq!(
        refusal(&patched(&TWO, 43, &[1, 0, 0, 0]))?,
        malformed("a range below bucket 16777216", 43)
    );
    assert_eq!(
        refusal(&patched(&ONE, 22, &[0]))?,
        malformed("a partition from 1", 19)
    );
    assert_eq!(
        refusal(&patched(&TWO, 42, &[1]))?,
        malformed("a partition other than its neighbour's", 47)
    );
    Ok(())
}

#[test]
fn test_a_layout_of_too_many_partitions_is_refused() -> TestResult {
    // One layout of 1,025 single-bucket ranges, each on its own partition, the last running to the end.
    let mut bytes = vec![1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0];
    bytes.extend_from_slice(&1_025u32.to_be_bytes());
    for bucket in 0..1_025u32 {
        bytes.extend_from_slice(&bucket.to_be_bytes());
        bytes.extend_from_slice(&(bucket + 1).to_be_bytes());
    }
    bytes.extend_from_slice(&0u32.to_be_bytes());
    // The 1,025th partition is the last range's, at 15 + 1,024 * 8 + 4.
    assert_eq!(
        refusal(&bytes)?,
        malformed("at most 1024 partitions in a layout", 8_211)
    );
    Ok(())
}

#[test]
fn test_leaders_are_checked() -> TestResult {
    assert_eq!(
        refusal(&patched(&ONE, 23, &[0, 0, 4, 1]))?,
        malformed("a leader count up to 1024", 23)
    );
    assert_eq!(
        refusal(&patched(&ONE, 30, &[0]))?,
        malformed("a partition from 1", 27)
    );
    assert_eq!(
        refusal(&patched(&TWO, 58, &[3]))?,
        malformed("a leader of a partition the latest layout holds", 55)
    );
    // Partitions 1 and 2 each with a leader, in the wrong order: 2 first.
    let mut bytes = TWO.get(..51).context("the layouts")?.to_vec();
    bytes.extend_from_slice(&[0, 0, 0, 2, 0, 0, 0, 2, 0, 0, 0, 7, 0, 0, 0, 1, 0, 0, 0, 7]);
    assert_eq!(
        refusal(&bytes)?,
        malformed("leaders in partition order", 63)
    );
    // Partition 1 twice.
    let mut repeated = TWO.get(..51).context("the layouts")?.to_vec();
    repeated.extend_from_slice(&[0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0, 7, 0, 0, 0, 1, 0, 0, 0, 8]);
    assert_eq!(
        refusal(&repeated)?,
        malformed("leaders in partition order", 63)
    );
    Ok(())
}

#[test]
fn test_bytes_after_the_end_are_refused() -> TestResult {
    let mut bytes = ONE.to_vec();
    bytes.push(0);
    assert_eq!(refusal(&bytes)?, malformed("the end of the encoding", 35));
    Ok(())
}

#[test]
fn test_every_truncation_is_refused() -> TestResult {
    for len in 0..TWO.len() {
        let refused = refusal(TWO.get(..len).context("a prefix")?)?;
        let ClusterErrorEnum::Malformed { offset, .. } = refused else {
            return Err(TestError::new(format!("length {len}: {refused:?}")));
        };
        assert!(offset <= len, "length {len}: offset {offset}");
    }
    Ok(())
}

#[test]
fn test_every_fuzz_seed_decodes_as_its_name_says() -> TestResult {
    // The seeds of `placement_decode`, encoded by testdata/seeds.py from the format's documentation alone.
    let dir = ultragraphdb_tool_bdd::testdata_dir()?
        .join("fuzz_seeds")
        .join("placement_decode");
    let seeds = ultragraphdb_tool_bdd::fuzz_regressions_in(&dir)?;
    assert_eq!(seeds.len(), 10);
    for (name, bytes) in seeds {
        match PlacementMap::decode(&bytes) {
            Ok(map) => {
                assert!(!name.starts_with("malformed"), "{name} accepted");
                assert_eq!(encoded(&map), bytes, "{name}");
            }
            Err(error) => {
                assert!(name.starts_with("malformed"), "{name}: {error}");
                assert!(
                    matches!(error.0, ClusterErrorEnum::Malformed { .. }),
                    "{name}: {error}"
                );
            }
        }
    }
    Ok(())
}
