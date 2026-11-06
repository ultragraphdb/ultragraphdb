/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_metadata::MetadataCommand;
use ultragraphdb_cluster_placement::{BucketRange, NodeId, PartitionId};
use ultragraphdb_core_ids::{Epoch, Tenant, TypePrefix};
use ultragraphdb_error_cluster::{ClusterError, ClusterErrorEnum};
use ultragraphdb_protocol_scram::ScramVerifier;
use ultragraphdb_tool_bdd::{Context, TestResult};

/// The command's encoding.
fn encoded(command: &MetadataCommand) -> Vec<u8> {
    let mut out = Vec::new();
    command.encode(&mut out);
    out
}

/// The refusal of decoding `bytes`.
fn refusal(bytes: &[u8]) -> TestResult<ClusterErrorEnum> {
    Ok(MetadataCommand::decode(bytes).err().context("a refusal")?.0)
}

/// The malformed refusal naming `what` at `offset`.
fn malformed(what: &'static str, offset: usize) -> ClusterErrorEnum {
    ClusterError::malformed(what, offset).0
}

/// One command of every kind, every field distinct from the others.
fn every_kind() -> TestResult<Vec<MetadataCommand>> {
    Ok(vec![
        MetadataCommand::OfferCatalog {
            tenant: Tenant::new(3),
            predecessor: 2,
            bytes: b"v3".to_vec(),
        },
        MetadataCommand::RecordDefault {
            tenant: Tenant::new(4),
            predicate: "score".to_string(),
            entry: vec![9, 8],
        },
        MetadataCommand::LeaseBlock {
            prefix: TypePrefix::STATEMENT,
            partition: PartitionId::new(5)?,
            start: 1_001,
            end: 2_001,
        },
        MetadataCommand::InitPlacement {
            partitions: 3,
            leader: NodeId::new(6),
        },
        MetadataCommand::ChangePlacement {
            assignments: vec![
                (BucketRange::new(0, 100)?, PartitionId::new(1)?),
                (BucketRange::new(100, 16_777_216)?, PartitionId::new(2)?),
            ],
        },
        MetadataCommand::SetLeader {
            partition: PartitionId::new(7)?,
            node: NodeId::new(8),
        },
        MetadataCommand::CreatePrincipal {
            id: 9,
            name: "ann".to_string(),
            verifier: ScramVerifier::new(vec![1, 2, 3], 4_096, [10; 32], [11; 32]),
            administrator: true,
        },
        MetadataCommand::RemovePrincipal {
            name: "bob".to_string(),
            evicts: vec![16],
        },
        MetadataCommand::RegisterSession {
            session: 12,
            principal: 13,
            evicts: vec![14, 15],
        },
        MetadataCommand::LearnClose {
            epoch: Epoch::new(39)?,
            versions: vec![(Tenant::new(3), 3)],
            placement: 2,
            evictions: vec![14],
        },
        MetadataCommand::ReportUntrimmed {
            partition: PartitionId::new(2)?,
            epoch: Epoch::new(30)?,
        },
        MetadataCommand::Chunk {
            offset: 16,
            bytes: vec![5, 6, 7],
        },
        MetadataCommand::Commit {
            length: 19,
            checksum: 0x0102_0304_0506_0708,
        },
    ])
}

#[test]
fn test_every_kind_round_trips() -> TestResult {
    for command in every_kind()? {
        let bytes = encoded(&command);
        assert_eq!(MetadataCommand::decode(&bytes)?, command);
    }
    Ok(())
}

#[test]
fn test_the_kind_bytes_count_from_1_in_declaration_order() -> TestResult {
    let kinds: Vec<u8> = every_kind()?
        .iter()
        .map(|command| encoded(command).first().copied().unwrap_or_default())
        .collect();
    assert_eq!(kinds, (1..=13).collect::<Vec<u8>>());
    Ok(())
}

#[test]
fn test_commands_encode_as_written_by_hand() -> TestResult {
    let commands = every_kind()?;
    // OfferCatalog: kind 1, tenant 3, predecessor 2, length 2, "v3".
    let mut offer = vec![
        1, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 2,
    ];
    offer.extend_from_slice(b"v3");
    // RecordDefault: kind 2, tenant 4, predicate length 5 as 2 bytes, "score", entry length 2 as 4, 9 8.
    let mut default = vec![2, 0, 0, 0, 0, 0, 0, 0, 4, 0, 5];
    default.extend_from_slice(b"score");
    default.extend_from_slice(&[0, 0, 0, 2, 9, 8]);
    // LeaseBlock: kind 3, prefix 2, partition 5, start 1,001 (0x3E9), end 2,001 (0x7D1).
    let lease = vec![
        3, 0, 2, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0x03, 0xE9, 0, 0, 0, 0, 0, 0, 0x07, 0xD1,
    ];
    // InitPlacement: kind 4, 3 partitions, leader node 6.
    let init = vec![4, 0, 0, 0, 3, 0, 0, 0, 6];
    // ChangePlacement: kind 5, 2 ranges: 0..100 to 1, 100..2^24 to 2.
    let change = vec![
        5, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 100, 0, 0, 0, 1, 0, 0, 0, 100, 1, 0, 0, 0, 0, 0, 0, 2,
    ];
    // SetLeader: kind 6, partition 7, node 8.
    let leader = vec![6, 0, 0, 0, 7, 0, 0, 0, 8];
    // CreatePrincipal: kind 7, id 9, name "ann" after its length 3, salt 1 2 3 after its length 3, 4,096 iterations
    // (0x1000), stored key 32 × 10, server key 32 × 11, administrator 1.
    let mut create = vec![7, 0, 0, 0, 0, 0, 0, 0, 9, 3];
    create.extend_from_slice(b"ann");
    create.extend_from_slice(&[3, 1, 2, 3, 0, 0, 0x10, 0]);
    create.extend_from_slice(&[10; 32]);
    create.extend_from_slice(&[11; 32]);
    create.push(1);
    // RemovePrincipal: kind 8, "bob" after its length 3, 1 eviction, 16 as 16 bytes.
    let mut remove = vec![8, 3];
    remove.extend_from_slice(b"bob");
    remove.extend_from_slice(&[0, 0, 0, 1]);
    remove.extend_from_slice(&16_u128.to_be_bytes());
    // RegisterSession: kind 9, session 12 as 16 bytes, principal 13 as 8, 2 evictions, 14 and 15 as 16 bytes each.
    let mut register = vec![9];
    register.extend_from_slice(&12_u128.to_be_bytes());
    register.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 13, 0, 0, 0, 2]);
    register.extend_from_slice(&14_u128.to_be_bytes());
    register.extend_from_slice(&15_u128.to_be_bytes());
    // LearnClose: kind 10, epoch 39, 1 version (tenant 3, version 3), 1 eviction, 14, placement version 2.
    let mut learn = vec![10, 0, 0, 0, 0, 0, 0, 0, 39, 0, 0, 0, 1];
    learn.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 1]);
    learn.extend_from_slice(&14_u128.to_be_bytes());
    learn.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 2]);
    // ReportUntrimmed: kind 11, partition 2, epoch 30.
    let report = vec![11, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 30];
    // Chunk: kind 12, offset 16, length 3, 5 6 7. Commit: kind 13, length 19, checksum 0x0102030405060708.
    let chunk = vec![12, 0, 0, 0, 0, 0, 0, 0, 16, 0, 0, 0, 3, 5, 6, 7];
    let commit = vec![13, 0, 0, 0, 0, 0, 0, 0, 19, 1, 2, 3, 4, 5, 6, 7, 8];
    let expected = [
        offer, default, lease, init, change, leader, create, remove, register, learn, report,
        chunk, commit,
    ];
    for (command, bytes) in commands.iter().zip(expected) {
        assert_eq!(encoded(command), bytes, "{command:?}");
        assert_eq!(&MetadataCommand::decode(&bytes)?, command);
    }
    Ok(())
}

#[test]
fn test_encoding_appends_to_the_buffer() -> TestResult {
    let mut out = vec![0xAA];
    MetadataCommand::RemovePrincipal {
        name: "b".to_string(),
        evicts: Vec::new(),
    }
    .encode(&mut out);
    assert_eq!(out, vec![0xAA, 8, 1, b'b', 0, 0, 0, 0]);
    Ok(())
}

#[test]
fn test_empty_and_unknown_kinds_are_refused() -> TestResult {
    assert_eq!(refusal(&[])?, malformed("a kind byte, 1 to 15", 0));
    assert_eq!(refusal(&[0])?, malformed("a kind byte, 1 to 15", 0));
    assert_eq!(refusal(&[16])?, malformed("a kind byte, 1 to 15", 0));
    Ok(())
}

#[test]
fn test_every_truncation_is_refused() -> TestResult {
    for command in every_kind()? {
        let bytes = encoded(&command);
        for cut in 0..bytes.len() {
            let prefix = bytes.get(..cut).context("a prefix")?;
            let refused = refusal(prefix)?;
            assert!(
                matches!(refused, ClusterErrorEnum::Malformed { offset, .. } if offset <= cut),
                "{command:?} cut at {cut}: {refused:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn test_bytes_after_the_end_are_refused() -> TestResult {
    for command in every_kind()? {
        let mut bytes = encoded(&command);
        let end = bytes.len();
        bytes.push(0);
        assert_eq!(refusal(&bytes)?, malformed("the end of the command", end));
    }
    Ok(())
}

#[test]
fn test_values_out_of_range_are_refused_naming_them() -> TestResult {
    // An epoch of 2^42 in a LearnClose.
    let mut epoch = vec![10, 0, 0, 4, 0, 0, 0, 0, 0];
    epoch.extend_from_slice(&[0; 8]);
    assert_eq!(refusal(&epoch)?, malformed("an epoch of 42 bits", 1));
    // A type prefix of 0 in a lease.
    let prefix = vec![3, 0, 0, 0, 0, 0, 1];
    assert_eq!(refusal(&prefix)?, malformed("a type prefix from 1", 1));
    // Partition 0 in a SetLeader.
    let partition = vec![6, 0, 0, 0, 0, 0, 0, 0, 1];
    assert_eq!(refusal(&partition)?, malformed("a partition from 1", 1));
    // An empty bucket range 5..5 in a change.
    let mut empty = vec![5, 0, 0, 0, 1];
    empty.extend_from_slice(&[0, 0, 0, 5, 0, 0, 0, 5, 0, 0, 0, 1]);
    assert_eq!(refusal(&empty)?, malformed("a bucket range within 2^24", 5));
    // A bucket range 0..2^24 + 1 in a change, past the last bucket.
    let mut past = vec![5, 0, 0, 0, 1];
    past.extend_from_slice(&[0, 0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 1]);
    assert_eq!(refusal(&past)?, malformed("a bucket range within 2^24", 5));
    // Partition 0 assigned a range in a change.
    let mut unassigned = vec![5, 0, 0, 0, 1];
    unassigned.extend_from_slice(&[0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(refusal(&unassigned)?, malformed("a partition from 1", 13));
    // A change names no epoch; a report of epoch 2^42 is refused.
    let report = vec![11, 0, 0, 0, 1, 0, 0, 4, 0, 0, 0, 0, 0];
    assert_eq!(refusal(&report)?, malformed("an epoch of 42 bits", 5));
    // Partition 0 in a lease and in a report.
    let lease = vec![3, 0, 1, 0, 0, 0, 0];
    assert_eq!(refusal(&lease)?, malformed("a partition from 1", 3));
    let zero = vec![11, 0, 0, 0, 0];
    assert_eq!(refusal(&zero)?, malformed("a partition from 1", 1));
    // An administrator flag of 2.
    let mut flag = encoded(every_kind()?.get(6).context("CreatePrincipal")?);
    let last = flag.len() - 1;
    if let Some(byte) = flag.last_mut() {
        *byte = 2;
    }
    assert_eq!(refusal(&flag)?, malformed("a flag, 0 or 1", last));
    // A name that is not UTF-8.
    let name = vec![8, 1, 0xFF];
    assert_eq!(refusal(&name)?, malformed("text of UTF-8", 2));
    Ok(())
}

#[test]
fn test_lengths_and_counts_past_their_limits_are_refused_before_their_values() -> TestResult {
    // A catalog version of 523,000 bytes and one, refused from its length alone.
    let mut offer = vec![1];
    offer.extend_from_slice(&[0; 16]);
    offer.extend_from_slice(&523_001_u32.to_be_bytes());
    assert_eq!(
        refusal(&offer)?,
        ClusterErrorEnum::TooMany {
            what: "bytes of a catalog version",
            count: 523_001,
            max: 523_000
        }
    );
    // A change of 2^18 + 1 ranges.
    let mut change = vec![5];
    change.extend_from_slice(&262_145_u32.to_be_bytes());
    assert_eq!(
        refusal(&change)?,
        ClusterErrorEnum::TooMany {
            what: "ranges of a placement change",
            count: 262_145,
            max: 262_144
        }
    );
    // A close announcing 257 versions.
    let mut learn = vec![10, 0, 0, 0, 0, 0, 0, 0, 1];
    learn.extend_from_slice(&257_u32.to_be_bytes());
    assert_eq!(
        refusal(&learn)?,
        ClusterErrorEnum::TooMany {
            what: "catalog versions in a close",
            count: 257,
            max: 256
        }
    );
    // A registration evicting 4,097 sessions.
    let mut register = vec![9];
    register.extend_from_slice(&[0; 24]);
    register.extend_from_slice(&4_097_u32.to_be_bytes());
    assert_eq!(
        refusal(&register)?,
        ClusterErrorEnum::TooMany {
            what: "session evictions",
            count: 4_097,
            max: 4_096
        }
    );
    // A predicate of 4,097 bytes and an entry of 65,537.
    let mut predicate = vec![2, 0, 0, 0, 0, 0, 0, 0, 1];
    predicate.extend_from_slice(&4_097_u16.to_be_bytes());
    assert_eq!(
        refusal(&predicate)?,
        ClusterErrorEnum::TooMany {
            what: "bytes of a predicate's name",
            count: 4_097,
            max: 4_096
        }
    );
    let mut entry = vec![2, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, b'p'];
    entry.extend_from_slice(&65_537_u32.to_be_bytes());
    assert_eq!(
        refusal(&entry)?,
        ClusterErrorEnum::TooMany {
            what: "bytes of a default entry",
            count: 65_537,
            max: 65_536
        }
    );
    Ok(())
}

#[test]
fn test_records_at_their_limits_round_trip() -> TestResult {
    let commands = [
        MetadataCommand::OfferCatalog {
            tenant: Tenant::new(u64::MAX),
            predecessor: u64::MAX - 1,
            bytes: vec![7; 523_000],
        },
        MetadataCommand::RecordDefault {
            tenant: Tenant::new(0),
            predicate: "p".repeat(4_096),
            entry: vec![1; 65_536],
        },
        MetadataCommand::CreatePrincipal {
            id: u64::MAX,
            name: "n".repeat(255),
            verifier: ScramVerifier::new(vec![2; 255], u32::MAX, [0; 32], [255; 32]),
            administrator: false,
        },
        MetadataCommand::LearnClose {
            placement: u64::MAX,
            epoch: Epoch::MAX,
            versions: vec![(Tenant::new(1), 1); 256],
            evictions: vec![u128::MAX; 4_096],
        },
        MetadataCommand::LeaseBlock {
            prefix: TypePrefix::new(u16::MAX)?,
            partition: PartitionId::new(u32::MAX)?,
            start: u64::MAX - 1,
            end: u64::MAX,
        },
    ];
    for command in commands {
        assert_eq!(MetadataCommand::decode(&encoded(&command))?, command);
    }
    Ok(())
}

#[test]
fn test_every_fuzz_seed_decodes_as_its_name_says() -> TestResult {
    // The seeds of `command_decode`, written by the suite's own encoder (test_the_fuzz_seeds_are_the_encoders) and
    // byte for byte as the hand-written encodings above.
    let dir = ultragraphdb_tool_bdd::testdata_dir()?
        .join("fuzz_seeds")
        .join("command_decode");
    let seeds = ultragraphdb_tool_bdd::fuzz_regressions_in(&dir)?;
    assert_eq!(seeds.len(), 16);
    for (name, bytes) in seeds {
        match MetadataCommand::decode(&bytes) {
            Ok(command) => {
                assert!(!name.starts_with("malformed"), "{name} accepted");
                assert_eq!(encoded(&command), bytes, "{name}");
            }
            Err(error) => {
                assert!(name.starts_with("malformed"), "{name}: {error}");
            }
        }
    }
    Ok(())
}

#[test]
fn test_the_fuzz_seeds_are_the_encoders() -> TestResult {
    // Each valid seed is the encoding of one command of every_kind, in kind order.
    let dir = ultragraphdb_tool_bdd::testdata_dir()?
        .join("fuzz_seeds")
        .join("command_decode");
    let mut valid: Vec<(String, Vec<u8>)> = ultragraphdb_tool_bdd::fuzz_regressions_in(&dir)?
        .into_iter()
        .filter(|(name, _)| !name.starts_with("malformed"))
        .collect();
    valid.sort();
    let mut expected: Vec<Vec<u8>> = every_kind()?.iter().map(encoded).collect();
    let mut seeds: Vec<Vec<u8>> = valid.into_iter().map(|(_, bytes)| bytes).collect();
    expected.sort();
    seeds.sort();
    assert_eq!(seeds, expected);
    Ok(())
}

#[test]
fn test_a_chunk_past_its_limit_is_refused_before_its_bytes() -> TestResult {
    let mut chunk = vec![12, 0, 0, 0, 0, 0, 0, 0, 0];
    chunk.extend_from_slice(&131_073_u32.to_be_bytes());
    assert_eq!(
        refusal(&chunk)?,
        ClusterErrorEnum::TooMany {
            what: "bytes of a chunk",
            count: 131_073,
            max: 131_072
        }
    );
    Ok(())
}

#[test]
fn test_a_removal_past_the_evictions_awaiting_a_close_is_refused_before_its_sessions() -> TestResult
{
    // A removal evicting 65,537 sessions, past the evictions awaiting a close.
    let mut removal = vec![8, 1, b'b'];
    removal.extend_from_slice(&65_537_u32.to_be_bytes());
    assert_eq!(
        refusal(&removal)?,
        ClusterErrorEnum::TooMany {
            what: "session evictions of a removal",
            count: 65_537,
            max: 65_536
        }
    );
    Ok(())
}

#[test]
fn test_a_closes_placement_version_is_its_last_8_bytes() -> TestResult {
    // The LearnClose of every_kind: its 49 bytes, then placement version 2.
    let learn = encoded(every_kind()?.get(9).context("LearnClose")?);
    assert_eq!(learn.len(), 57);
    assert_eq!(learn.get(49..), Some([0, 0, 0, 0, 0, 0, 0, 2].as_slice()));
    for cut in 49..57 {
        assert_eq!(
            refusal(learn.get(..cut).context("a prefix")?)?,
            malformed("a placement version", 49),
            "cut at {cut}"
        );
    }
    // Every number decodes as itself; 0 announces none.
    for placement in [0, 1, u64::MAX] {
        let mut bytes = learn.clone();
        bytes.truncate(49);
        bytes.extend_from_slice(&placement.to_be_bytes());
        let MetadataCommand::LearnClose {
            placement: read, ..
        } = MetadataCommand::decode(&bytes)?
        else {
            return Err(ultragraphdb_tool_bdd::TestError::new("a LearnClose"));
        };
        assert_eq!(read, placement);
    }
    Ok(())
}
