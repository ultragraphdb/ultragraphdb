/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::BucketGroup;
use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, FrameView, Header, MAX_EPOCH, MAX_FRAME_SIZE, Reference,
    ReferenceRegistration, ReferenceView,
};
use ultragraphdb_tool_bdd::TestResult;

use crate::types::reference::reference_tests::{BLANK, O, golden_request, sid};
use crate::utils::golden_bodies::{frame_of, put, view_any, with_command_header, with_section};
use crate::utils::golden_roles::{golden_role, role_bytes};
use crate::utils::term_records::{Entry, refusal, term_sections};

/// The golden predicate's system ID, type prefix 1, sequence 9, bucket 0x40, big-endian.
const PREDICATE: [u8; 16] = [0, 1, 0, 0, 0, 0, 0, 0, 0, 9, 0, 0, 0x40, 0, 0, 0];
/// The golden reply's entity, type prefix 1, sequence 77, bucket 0xA80.
const ENTITY: [u8; 16] = [0, 1, 0, 0, 0, 0, 0, 0, 0, 77, 0, 0x0A, 0x80, 0, 0, 0];

/// A change to a registration's 48 bytes.
type Change = fn(&mut [u8]);

fn golden(name: &str) -> TestResult<Vec<u8>> {
    role_bytes(&golden_role(name)?)
}

/// The golden frame `name` with registration `index`'s 48 bytes changed by `change`.
fn with_registration(
    name: &str,
    index: usize,
    change: impl FnOnce(&mut [u8]),
) -> TestResult<Vec<u8>> {
    Ok(with_section(&golden(name)?, 3, 0, |s| {
        change(&mut s[48 * index..48 * (index + 1)])
    }))
}

fn registration(index: u64) -> ProtocolErrorEnum {
    ProtocolErrorEnum::FieldValue {
        command: 526,
        field: "registration",
        value: index,
    }
}

#[test]
fn test_the_golden_request_reads_in_place() -> TestResult {
    let bytes = golden("reference")?;
    let view = ReferenceView::from_frame(&frame_of(&bytes)?)?;
    assert!(!view.is_reply());
    assert_eq!(view.len(), 3);
    assert!(!view.is_empty());
    let group = BucketGroup::new(0x2A)?;
    assert_eq!(
        view.registration(0),
        Some(ReferenceRegistration::request(
            3,
            Some(group),
            Some(0),
            None,
            4
        ))
    );
    assert_eq!(
        view.registration(1),
        Some(ReferenceRegistration::request(
            5,
            None,
            None,
            Some(sid(1, 9, 0x40)?),
            0
        ))
    );
    assert_eq!(view.registration(3), None);
    assert_eq!(
        (view.iri(0), view.iri(1), view.iri(2), view.iri(3)),
        (Some(O), None, Some(BLANK), None)
    );
    assert_eq!(view.terms().len(), 2);
    assert_eq!(view.to_body()?, golden_request()?);
    assert_eq!(view_any(&bytes)?, Ok(()));
    Ok(())
}

#[test]
fn test_the_golden_reply_reads_in_place() -> TestResult {
    let bytes = golden("reference_reply")?;
    let view = ReferenceView::from_frame(&frame_of(&bytes)?)?;
    assert!(view.is_reply());
    let read: Vec<(u16, u64, u64)> = view
        .registrations()
        .map(|r| (r.outcome(), r.generation(), r.close()))
        .collect();
    assert_eq!(read, vec![(0, 5, 41), (0, 0, 41), (26, 0, 0)]);
    assert_eq!(
        view.registration(0).and_then(|r| r.entity()),
        Some(sid(1, 77, 0xA80)?)
    );
    assert_eq!(view.registration(2).and_then(|r| r.entity()), None);
    assert_eq!(view_any(&bytes)?, Ok(()));
    Ok(())
}

/// `PROTOCOL.md` §13.15: `reply` at 0 is boolean, `mode` at 1 and `tenant` at 8 its other fields; bytes 2–7 and 16–127
/// are unused.
#[test]
fn test_the_reply_flag_is_boolean_and_the_unused_bytes_are_zero() -> TestResult {
    let bytes = with_command_header(&golden("reference")?, |h| h[0] = 2);
    assert_eq!(
        refusal::<ReferenceView>(&bytes)?,
        ProtocolErrorEnum::FieldValue {
            command: 526,
            field: "reply",
            value: 2
        }
    );
    for at in [2usize, 127] {
        let unused = with_command_header(&golden("reference")?, |h| h[at] = 1);
        assert_eq!(
            refusal::<ReferenceView>(&unused)?,
            ProtocolErrorEnum::ReservedNotZero { offset: 128 + at }
        );
    }
    Ok(())
}

#[test]
fn test_every_term_is_an_iri() -> TestResult {
    let (records, text) = term_sections(&[Entry::Text(1, O, ""), Entry::Text(3, "x", "")]);
    let mut bytes = with_section(&golden("reference")?, 3, 1, |s| *s = records);
    bytes = with_section(&bytes, 3, 2, |s| *s = text);
    assert_eq!(
        refusal::<ReferenceView>(&bytes)?,
        ProtocolErrorEnum::FieldValue {
            command: 526,
            field: "reference term",
            value: 3
        }
    );
    Ok(())
}

#[test]
fn test_the_registrations_are_whole_and_at_least_one() -> TestResult {
    let empty = with_section(&golden("reference")?, 3, 0, Vec::clear);
    assert_eq!(
        refusal::<ReferenceView>(&empty)?,
        ProtocolErrorEnum::CountMismatch {
            command: 526,
            field: "registrations",
            count: 0,
            expected: 1
        }
    );
    let torn = with_section(&golden("reference")?, 3, 0, |s| s.truncate(143));
    assert_eq!(
        refusal::<ReferenceView>(&torn)?,
        ProtocolErrorEnum::SectionLength {
            command: 526,
            section: 0,
            len: 143,
            unit: 48
        }
    );
    Ok(())
}

/// `PROTOCOL.md` §13.15: a registration's byte 15 is zero, its byte 14 `status`.
#[test]
fn test_the_zero_byte_is_named_at_its_body_offset() -> TestResult {
    let bytes = with_registration("reference", 1, |r| r[15] = 1)?;
    // Section 0 starts at body offset 24, after the table of three sections.
    assert_eq!(
        refusal::<ReferenceView>(&bytes)?,
        ProtocolErrorEnum::PaddingNotZero {
            command: 526,
            offset: 24 + 48 + 15
        }
    );
    Ok(())
}

#[test]
fn test_each_fields_rule_names_its_registration() -> TestResult {
    let cases: [(&str, usize, Change); 8] = [
        ("reference", 2, |r| put(r, 0, 0, 4)),
        ("reference", 0, |r| put(r, 4, 1 << 18, 4)),
        ("reference", 2, |r| put(r, 8, 2, 4)),
        ("reference", 1, |r| r[47] = 1),
        ("reference", 1, |r| put(r, 16, 1, 8)),
        ("reference_reply", 1, |r| put(r, 16, 1, 8)),
        ("reference_reply", 0, |r| put(r, 24, MAX_EPOCH + 1, 8)),
        ("reference_reply", 0, |r| r[33] = 0),
    ];
    for (name, index, change) in cases {
        let bytes = with_registration(name, index, change)?;
        assert_eq!(
            refusal::<ReferenceView>(&bytes)?,
            registration(index as u64),
            "{name} {index}"
        );
    }
    Ok(())
}

#[test]
fn test_the_extremes_of_each_field_are_taken() -> TestResult {
    for (name, index, at, value, width) in [
        ("reference", 0, 4usize, (1u64 << 18) - 1, 4usize),
        ("reference", 0, 8, 1, 4),
        ("reference", 0, 0, u32::MAX.into(), 4),
        ("reference_reply", 0, 24, MAX_EPOCH, 8),
        ("reference_reply", 0, 24, 0, 8),
        ("reference_reply", 1, 12, 7, 2),
    ] {
        let bytes = with_registration(name, index, |r| {
            put(r, at, value, width);
            if at == 12 {
                put(r, 24, 0, 8);
            }
        })?;
        assert!(
            ReferenceView::from_frame(&frame_of(&bytes)?).is_ok(),
            "{name} {index} {at}"
        );
    }
    Ok(())
}

#[test]
fn test_a_request_names_no_outcome_or_close_and_its_entity_one_way() -> TestResult {
    let cases: [(usize, Change); 4] = [
        (0, |r| put(r, 12, 7, 2)),
        (0, |r| put(r, 24, 1, 8)),
        (0, |r| r[32..48].copy_from_slice(&PREDICATE)),
        (1, |r| r[32..48].fill(0)),
    ];
    for (index, change) in cases {
        let bytes = with_registration("reference", index, change)?;
        assert_eq!(
            refusal::<ReferenceView>(&bytes)?,
            registration(index as u64)
        );
    }
    Ok(())
}

#[test]
fn test_a_reply_accepts_with_an_entity_and_refuses_with_nothing_filled() -> TestResult {
    let cases: [(usize, Change); 6] = [
        (2, |r| put(r, 12, 5, 2)),
        (0, |r| r[32..48].fill(0)),
        (0, |r| put(r, 16, 0, 8)),
        (2, |r| put(r, 16, 1, 8)),
        (2, |r| put(r, 24, 1, 8)),
        (2, |r| r[32..48].copy_from_slice(&ENTITY)),
    ];
    for (index, change) in cases {
        let bytes = with_registration("reference_reply", index, change)?;
        assert_eq!(
            refusal::<ReferenceView>(&bytes)?,
            registration(index as u64)
        );
    }
    let refused_by_id = with_registration("reference_reply", 1, |r| {
        put(r, 12, 26, 2);
        put(r, 24, 0, 8);
    })?;
    let view = ReferenceView::from_frame(&frame_of(&refused_by_id)?)?;
    assert_eq!(
        view.registration(1).map(|r| (r.outcome(), r.entity())),
        Some((26, Some(sid(1, 9, 0x40)?)))
    );
    Ok(())
}

/// The batch's tenant crosses the wire in the command header, and reads back into the body.
#[test]
fn test_the_tenant_crosses_in_the_command_header() -> TestResult {
    let mut body = Reference::request().in_tenant(u64::MAX - 1);
    let _at = body.register_iri(1, None, "http://ex.org/a", 0);
    let cluster = 7;
    let frame = body.frame(Header::new(cluster, Command::Reference).with_stream(1))?;
    let view = FrameView::validate(&frame, cluster, MAX_FRAME_SIZE)?;
    let read = ReferenceView::from_frame(&view)?;
    assert_eq!(read.tenant(), u64::MAX - 1);
    assert_eq!(read.to_body()?.tenant(), u64::MAX - 1);
    Ok(())
}

#[test]
fn test_a_home_refuses_with_a_failed_stores_outcome() -> TestResult {
    let bytes = role_bytes(&golden_role("reference_unavailable")?)?;
    let view = ReferenceView::from_frame(&frame_of(&bytes)?)?;
    let refused: Vec<u16> = view.registrations().map(|r| r.outcome()).collect();
    assert_eq!(refused, [0, 0, 17]);
    assert_eq!(view_any(&bytes)?, Ok(()));
    Ok(())
}

#[test]
fn test_a_replys_refusing_outcomes_are_7_17_and_26() -> TestResult {
    let bytes = role_bytes(&golden_role("reference_unavailable")?)?;
    let third = 2 * 48 + 12;
    for outcome in [7u64, 17, 26] {
        let frame = with_section(&bytes, 3, 0, |r| put(r, third, outcome, 2));
        assert!(ReferenceView::from_frame(&frame_of(&frame)?).is_ok());
    }
    for outcome in [1u64, 16, 18, 27] {
        let frame = with_section(&bytes, 3, 0, |r| put(r, third, outcome, 2));
        assert_eq!(
            refusal::<ReferenceView>(&frame)?,
            ProtocolErrorEnum::FieldValue {
                command: 526,
                field: "registration",
                value: 2
            }
        );
    }
    Ok(())
}

#[test]
fn test_a_torn_term_record_is_refused() -> TestResult {
    let golden_bytes = golden("reference")?;
    let mut whole = 0;
    let torn = with_section(&golden_bytes, 3, 1, |records| {
        whole = records.len();
        records.truncate(whole.saturating_sub(1));
    });
    assert!(whole > 0, "the golden request holds a term");
    assert!(
        matches!(
            refusal::<ReferenceView>(&torn)?,
            ProtocolErrorEnum::SectionLength { command: 526, section: 1, len, .. } if len == whole - 1
        ),
        "a record short of its size"
    );
    Ok(())
}

/// Stage 6.6: Reference's `mode` and its registrations' `status` (`PROTOCOL.md` §13.15), over batches laid out by hand:
/// `reply` at 0, `mode` at 1, `tenant` at 8; each registration `partition`, `group`, `iri`, `outcome`, `status`, a zero
/// byte, `generation`, `close` and `entity`.
pub mod stage_6 {
    use ultragraphdb_error_protocol::ProtocolErrorEnum;
    use ultragraphdb_protocol_wire::{
        BodyView, Command, ReferenceMode, ReferenceStatus, ReferenceView,
    };
    use ultragraphdb_tool_bdd::{Context, TestResult};

    use crate::utils::golden_bodies::frame_of;
    use crate::utils::ugs_frames::{frame, iri_terms, outcome, refused};

    /// An entity of type prefix 3, sequence 7, bucket 0x2A, big-endian: a valid system ID (§1).
    pub const ENTITY: [u8; 16] = [0, 3, 0, 0, 0, 0, 0, 0, 0, 7, 0, 0, 0x2A, 0, 0, 0];
    const IRI: &str = "https://example.org/deep_brain#Node/ORG%3Aacme";

    /// A registration's 48 bytes.
    pub fn registration(
        group: u32,
        iri: u32,
        outcome_status: (u16, u8),
        generation: u64,
        close: u64,
        entity: [u8; 16],
    ) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&4u32.to_le_bytes());
        out.extend_from_slice(&group.to_le_bytes());
        out.extend_from_slice(&iri.to_le_bytes());
        out.extend_from_slice(&outcome_status.0.to_le_bytes());
        out.extend_from_slice(&[outcome_status.1, 0]);
        out.extend_from_slice(&generation.to_le_bytes());
        out.extend_from_slice(&close.to_le_bytes());
        out.extend_from_slice(&entity);
        out
    }

    /// A batch in tenant 0 of `mode`, a reply when `reply`, of `registration` naming the IRI as term 0.
    pub fn batch(reply: bool, mode: u8, registration: Vec<u8>) -> TestResult<Vec<u8>> {
        let [records, text] = iri_terms(&[IRI]);
        frame(
            Command::Reference,
            &[(0, &[u8::from(reply), mode])],
            &[registration, records, text],
            false,
        )
    }

    fn rejected() -> ProtocolErrorEnum {
        ProtocolErrorEnum::FieldValue {
            command: 526,
            field: "registration",
            value: 0,
        }
    }

    /// A get-only request reads its mode; its registration carries no status.
    #[test]
    fn test_a_get_only_request_reads_its_mode() -> TestResult {
        let bytes = batch(false, 1, registration(u32::MAX, 0, (0, 0), 0, 0, [0; 16]))?;
        let view = ReferenceView::from_frame(&frame_of(&bytes)?)?;
        assert_eq!(view.mode(), ReferenceMode::GetOnly);
        assert_eq!(
            view.registration(0).context("a registration")?.status(),
            None
        );
        let zero = batch(false, 0, registration(u32::MAX, 0, (0, 0), 0, 0, [0; 16]))?;
        assert_eq!(
            ReferenceView::from_frame(&frame_of(&zero)?)?.mode(),
            ReferenceMode::GetOrCreate
        );
        assert_eq!(
            refused::<ReferenceView>(&batch(
                false,
                2,
                registration(u32::MAX, 0, (0, 0), 0, 0, [0; 16])
            )?)?,
            ProtocolErrorEnum::FieldValue {
                command: 526,
                field: "mode",
                value: 2
            }
        );
        Ok(())
    }

    /// An accepted get-only reply answers each status: absent with no ID, live with its generation and close, deleted
    /// and retracted with the ID and neither.
    #[test]
    fn test_a_get_only_reply_answers_each_status() -> TestResult {
        let cases = [
            (
                registration(u32::MAX, 0, (0, 1), 0, 0, [0; 16]),
                ReferenceStatus::Absent,
            ),
            (
                registration(0x2A, 0, (0, 2), 3, 30, ENTITY),
                ReferenceStatus::Live,
            ),
            (
                registration(u32::MAX, 0, (0, 3), 0, 0, ENTITY),
                ReferenceStatus::Deleted,
            ),
            (
                registration(u32::MAX, 0, (0, 4), 0, 0, ENTITY),
                ReferenceStatus::Retracted,
            ),
        ];
        for (bytes, status) in cases {
            let frame_bytes = batch(true, 1, bytes)?;
            let view = ReferenceView::from_frame(&frame_of(&frame_bytes)?)?;
            assert_eq!(
                view.registration(0).context("a registration")?.status(),
                Some(status)
            );
        }
        Ok(())
    }

    /// `status` is 0 with mode 0 and in a refused reply, 1 to 4 in an accepted get-only one; an absent entity's ID
    /// all zero; a deleted or retracted one's `generation` and `close` 0.
    #[test]
    fn test_a_status_follows_mode_and_outcome() -> TestResult {
        let broken = [
            batch(true, 1, registration(u32::MAX, 0, (0, 0), 0, 0, ENTITY))?,
            batch(true, 1, registration(u32::MAX, 0, (0, 5), 0, 0, ENTITY))?,
            batch(true, 0, registration(u32::MAX, 0, (0, 2), 0, 0, ENTITY))?,
            batch(true, 1, registration(u32::MAX, 0, (17, 1), 0, 0, [0; 16]))?,
            batch(true, 1, registration(u32::MAX, 0, (0, 1), 0, 0, ENTITY))?,
            batch(true, 1, registration(u32::MAX, 0, (0, 4), 1, 0, ENTITY))?,
            batch(true, 1, registration(u32::MAX, 0, (0, 3), 0, 30, ENTITY))?,
            batch(false, 1, registration(u32::MAX, 0, (0, 1), 0, 0, [0; 16]))?,
        ];
        for bytes in &broken {
            assert_eq!(refused::<ReferenceView>(bytes)?, rejected());
        }
        Ok(())
    }

    /// A live entity's accepted get-only registration names its ID and, with a group, the member's generation, at least
    /// 1 (§13.15): either missing is refused as `FieldValue` naming `registration`.
    #[test]
    fn test_a_live_entity_names_its_id_and_its_members_generation() -> TestResult {
        let no_id = batch(true, 1, registration(u32::MAX, 0, (0, 2), 0, 0, [0; 16]))?;
        assert_eq!(refused::<ReferenceView>(&no_id)?, rejected());
        let no_generation = batch(true, 1, registration(0x2A, 0, (0, 2), 0, 30, ENTITY))?;
        assert_eq!(refused::<ReferenceView>(&no_generation)?, rejected());
        Ok(())
    }

    /// `outcome` 30 refuses a registration with mode 0, named by its ID; with mode 1 a reply's outcome is 0, 17 or 26.
    #[test]
    fn test_outcome_30_belongs_to_mode_0() -> TestResult {
        let by_id = registration(u32::MAX, u32::MAX, (30, 0), 0, 0, ENTITY);
        assert_eq!(
            outcome::<ReferenceView>(&batch(true, 0, by_id.clone())?)?,
            None
        );
        assert_eq!(
            refused::<ReferenceView>(&batch(true, 1, by_id)?)?,
            rejected()
        );
        for refusal in [17u16, 26] {
            let refused_reply = registration(u32::MAX, 0, (refusal, 0), 0, 0, [0; 16]);
            assert_eq!(
                outcome::<ReferenceView>(&batch(true, 1, refused_reply)?)?,
                None
            );
        }
        Ok(())
    }
}
