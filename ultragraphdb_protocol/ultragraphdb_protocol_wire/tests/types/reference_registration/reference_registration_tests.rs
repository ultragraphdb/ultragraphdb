/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Bucket, BucketGroup, Sequence, SystemId, TypePrefix};
use ultragraphdb_protocol_wire::{ABSENT, ReferenceRegistration, ReferenceStatus};
use ultragraphdb_tool_bdd::TestResult;

fn entity(sequence: u64, bucket: u32) -> TestResult<SystemId> {
    Ok(SystemId::new(
        TypePrefix::RESOURCE,
        Bucket::new(bucket)?,
        Sequence::new(sequence)?,
    ))
}

#[test]
fn test_a_request_by_iri_encodes_each_field_at_its_offset() -> TestResult {
    let group = BucketGroup::new(0x0002_0304)?;
    let request = ReferenceRegistration::request(
        0x0A0B_0C0D,
        Some(group),
        Some(7),
        None,
        0x1112_1314_1516_1718,
    );
    assert_eq!(request.outcome(), 0);
    assert_eq!(request.close(), 0);
    assert_eq!(request.entity(), None);
    let bytes = request.encode();
    assert_eq!(&bytes[0..4], &0x0A0B_0C0Du32.to_le_bytes());
    assert_eq!(&bytes[4..8], &0x0002_0304u32.to_le_bytes());
    assert_eq!(&bytes[8..12], &7u32.to_le_bytes());
    assert_eq!(&bytes[12..16], &[0; 4]);
    assert_eq!(&bytes[16..24], &0x1112_1314_1516_1718u64.to_le_bytes());
    assert_eq!(&bytes[24..48], &[0; 24]);
    Ok(())
}

#[test]
fn test_a_request_by_entity_names_no_iri_and_no_group() -> TestResult {
    let id = entity(9, 0x40)?;
    let bytes = ReferenceRegistration::request(5, None, None, Some(id), 0).encode();
    assert_eq!(&bytes[4..8], &ABSENT.to_le_bytes());
    assert_eq!(&bytes[8..12], &ABSENT.to_le_bytes());
    assert_eq!(&bytes[32..48], &id.to_be_bytes());
    Ok(())
}

#[test]
fn test_an_accepted_reply_fills_entity_generation_and_close() -> TestResult {
    let id = entity(77, 0xA80)?;
    let reply = ReferenceRegistration::request(3, Some(BucketGroup::new(0x2A)?), Some(0), None, 4)
        .accepted(id, 5, 41);
    assert_eq!(reply.outcome(), 0);
    assert_eq!(reply.entity(), Some(id));
    assert_eq!(reply.generation(), 5);
    assert_eq!(reply.close(), 41);
    assert_eq!(reply.iri(), Some(0));
    assert_eq!(reply.partition(), 3);
    let bytes = reply.encode();
    assert_eq!(&bytes[24..32], &41u64.to_le_bytes());
    assert_eq!(&bytes[32..48], &id.to_be_bytes());
    Ok(())
}

#[test]
fn test_a_refused_reply_keeps_the_entity_as_sent_and_clears_generation_and_close() -> TestResult {
    let id = entity(9, 0x40)?;
    let by_iri = ReferenceRegistration::request(8, Some(BucketGroup::new(0x10)?), Some(1), None, 6)
        .accepted(id, 7, 8)
        .refused(26);
    assert_eq!(by_iri.outcome(), 26);
    assert_eq!(by_iri.generation(), 0);
    assert_eq!(by_iri.close(), 0);
    assert_eq!(by_iri.entity(), None);
    assert_eq!(&by_iri.encode()[12..14], &26u16.to_le_bytes());
    let by_id = ReferenceRegistration::request(8, None, None, Some(id), 0).refused(7);
    assert_eq!(by_id.entity(), Some(id));
    assert_eq!(by_id.outcome(), 7);
    Ok(())
}

#[test]
fn test_accepting_a_refused_registration_clears_its_outcome() -> TestResult {
    let accepted = ReferenceRegistration::request(3, None, Some(0), None, 0)
        .refused(26)
        .accepted(entity(5, 6)?, 0, 9);
    assert_eq!(accepted.outcome(), 0);
    assert_eq!(&accepted.encode()[12..14], &[0, 0]);
    Ok(())
}

/// Each answer sets the reply fields `PROTOCOL.md` §13.15 gives it, whatever the registration held before: a get-only
/// answer of a live entity its outcome 0, its status, the member's generation and the latest close; a refusal its status,
/// generation and close 0; a get-or-create acceptance its status 0, which mode 0 holds.
#[test]
fn test_each_answer_sets_status_outcome_generation_and_close() -> TestResult {
    let group = BucketGroup::new(0x2A)?;
    let id = entity(5, 6)?;
    let live = ReferenceRegistration::request(3, Some(group), Some(0), None, 3)
        .refused(17)
        .answered(ReferenceStatus::Live, Some(id), 4, 30);
    assert_eq!(
        (live.outcome(), live.status(), live.generation(), live.close()),
        (0, Some(ReferenceStatus::Live), 4, 30)
    );
    assert_eq!(live.entity(), Some(id));
    let refused = live.refused(26);
    assert_eq!(
        (
            refused.outcome(),
            refused.status(),
            refused.generation(),
            refused.close()
        ),
        (26, None, 0, 0)
    );
    let accepted = live.accepted(id, 5, 31);
    assert_eq!(
        (
            accepted.outcome(),
            accepted.status(),
            accepted.generation(),
            accepted.close()
        ),
        (0, None, 5, 31)
    );
    Ok(())
}
