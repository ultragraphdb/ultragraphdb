/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{BucketGroup, SystemId};
use ultragraphdb_protocol_wire::{BodyBuilder, Reference};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_roles::{golden_role, role_bytes};

pub const O: &str = "http://ex.org/o";
pub const BLANK: &str = "https://w3id.org/ultragraphdb/ns/namespace/0/.well-known/genid/00a001-0123456789abcdef0123456789";

/// The system ID laid out from section 1's table: type prefix, sequence, bucket, three zero bytes, big-endian.
pub fn sid(prefix: u16, sequence: u64, bucket: u32) -> TestResult<SystemId> {
    let mut bytes = prefix.to_be_bytes().to_vec();
    bytes.extend_from_slice(&sequence.to_be_bytes());
    bytes.extend_from_slice(&bucket.to_be_bytes()[1..]);
    bytes.extend_from_slice(&[0; 3]);
    Ok(SystemId::decode(&bytes)?)
}

/// The golden request: `<o>` by IRI for group 0x2A at generation 4, a predicate by ID, a blank node by skolem IRI
/// for group 0x10.
pub fn golden_request() -> TestResult<Reference> {
    let mut reference = Reference::request();
    assert_eq!(
        reference.register_iri(3, Some(BucketGroup::new(0x2A)?), O, 4),
        0
    );
    assert_eq!(reference.register_entity(5, None, sid(1, 9, 0x40)?, 0), 1);
    assert_eq!(
        reference.register_iri(8, Some(BucketGroup::new(0x10)?), BLANK, 0),
        2
    );
    Ok(reference)
}

#[test]
fn test_the_golden_request_is_built_byte_for_byte() -> TestResult {
    let reference = golden_request()?;
    assert!(!reference.is_reply());
    assert_eq!(reference.registrations().len(), 3);
    assert_eq!(reference.terms().len(), 2);
    assert_eq!(reference.iri(0), Some(O));
    assert_eq!(reference.iri(1), None);
    assert_eq!(reference.iri(2), Some(BLANK));
    assert_eq!(reference.iri(3), None);
    let golden = golden_role("reference")?;
    assert_eq!(reference.frame(golden.header())?, role_bytes(&golden)?);
    Ok(())
}

#[test]
fn test_the_golden_reply_is_built_byte_for_byte() -> TestResult {
    let mut reply = golden_request()?.into_reply();
    assert!(reply.is_reply());
    assert!(reply.accept(0, sid(1, 77, 0xA80)?, 5, 41));
    assert!(reply.accept(1, sid(1, 9, 0x40)?, 0, 41));
    assert!(reply.refuse(2, 26));
    assert!(!reply.accept(3, sid(1, 1, 1)?, 1, 1));
    assert!(!reply.refuse(3, 7));
    assert_eq!(reply.command_header()[0], 1);
    let golden = golden_role("reference_reply")?;
    assert_eq!(reply.frame(golden.header())?, role_bytes(&golden)?);
    Ok(())
}

#[test]
fn test_one_iri_named_twice_is_interned_once() -> TestResult {
    let mut reference = Reference::request();
    let _first: usize = reference.register_iri(3, None, O, 0);
    let _second: usize = reference.register_iri(4, Some(BucketGroup::new(1)?), O, 2);
    assert_eq!(reference.terms().len(), 1);
    assert_eq!(reference.registrations()[1].iri(), Some(0));
    assert_eq!(Reference::request().command_header(), [0; 128]);
    Ok(())
}
