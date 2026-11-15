/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Declare's builder (`PROTOCOL.md` §13.27) against the golden `declare.frame`, `declare_reply.frame` and
//! `declare_drop.frame` and frames laid out by hand; it refuses what its view refuses.

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, Declare, DeclareView, Declared, DeclaredEntityType,
    DeclaredKind, DeclaredPredicate, DeclaredRelation, DropKind, Header, MAX_FRAME_SIZE,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::utils::golden_bodies::{CLUSTER, frame_of};
use crate::utils::golden_roles::{golden_role, role_bytes};
use crate::utils::system_frames::{
    DeclareFields, count, declare_frame, drop_sections, entity_type_sections, field, iris,
};

/// The first version of epoch 45, the golden reply's `commit_version`.
pub const GOLDEN_COMMIT_VERSION: u64 = 45 << 22;

fn header() -> Header {
    Header::new(CLUSTER, Command::Declare).with_stream(3)
}

/// The golden declarations, as `generate_roles.py` lays them out: the single-valued property `<name>` of datatype
/// `xsd:string` with the tokenizer `term`, the entity type `<Person>`, and the relation `<knows>` of two roles.
pub fn golden_declarations() -> Vec<Declared> {
    vec![
        Declared::Predicate(
            DeclaredPredicate::new("http://ex.org/name")
                .with_kind(DeclaredKind::Property)
                .with_single(true)
                .with_datatype("http://www.w3.org/2001/XMLSchema#string")
                .with_tokenizers(vec!["term".to_string()]),
        ),
        Declared::EntityType(DeclaredEntityType::new("http://ex.org/Person", None, false)),
        Declared::Relation(DeclaredRelation::new(
            "http://ex.org/knows",
            vec![
                "http://ex.org/from".to_string(),
                "http://ex.org/to".to_string(),
            ],
        )),
    ]
}

/// The golden request of `kind` 1: tenant 3, built on version 11, principal 9.
pub fn golden_declare() -> Declare {
    Declare::declarations(3, 11, 9, golden_declarations())
}

/// The golden request of `kind` 2: principal 9's Drop of the predicate `<age>` in tenant 3.
pub fn golden_drop() -> Declare {
    Declare::drop(
        3,
        9,
        DropKind::Predicate,
        Some("http://ex.org/age".to_string()),
    )
}

fn entity(name: &str) -> Declared {
    Declared::EntityType(DeclaredEntityType::new(name, None, false))
}

fn refused(declare: &Declare) -> TestResult<ProtocolErrorEnum> {
    Ok(declare.frame(header()).err().context("a refusal")?.0)
}

/// C1.59: the builder lays out the three golden frames.
#[test]
fn test_a_declare_builds_the_golden_frames() -> TestResult {
    let request = golden_declare();
    assert_eq!(
        (request.tenant(), request.built_on(), request.principal()),
        (3, 11, 9)
    );
    assert_eq!((request.version(), request.commit_version()), (0, 0));
    assert_eq!((request.drop_kind(), request.target()), (None, None));
    assert_eq!(request.declared(), golden_declarations().as_slice());
    let golden = golden_role("declare")?;
    assert_eq!(request.frame(golden.header())?, role_bytes(&golden)?);

    let reply = golden_declare().with_version(12, GOLDEN_COMMIT_VERSION);
    assert_eq!(
        (reply.version(), reply.commit_version()),
        (12, GOLDEN_COMMIT_VERSION)
    );
    let golden_reply = golden_role("declare_reply")?;
    assert_eq!(
        reply.frame(golden_reply.header())?,
        role_bytes(&golden_reply)?
    );

    let drop = golden_drop();
    assert_eq!(
        (drop.tenant(), drop.built_on(), drop.principal()),
        (3, 0, 9)
    );
    assert_eq!(
        (drop.drop_kind(), drop.target()),
        (Some(DropKind::Predicate), Some("http://ex.org/age"))
    );
    assert!(drop.declared().is_empty());
    let golden_drop_frame = golden_role("declare_drop")?;
    assert_eq!(
        drop.frame(golden_drop_frame.header())?,
        role_bytes(&golden_drop_frame)?
    );
    Ok(())
}

/// A Drop of each kind against the frame laid out by hand: `drop` its number, the term table the target of kinds 1
/// and 2 alone, `built_on` and `commit_version` 0, and in a reply the `version`.
#[test]
fn test_a_drop_of_each_kind_is_laid_out_by_its_fields() -> TestResult {
    let target = "http://ex.org/g";
    for (kind, number, named) in [
        (DropKind::Predicate, 1u8, true),
        (DropKind::Graph, 2, true),
        (DropKind::TenantData, 3, false),
        (DropKind::Tenant, 4, false),
    ] {
        for version in [0u64, 5] {
            let entries = if named { iris(&[target]) } else { Vec::new() };
            let expected = declare_frame(
                DeclareFields {
                    kind: 2,
                    drop: number,
                    tenant: 6,
                    principal: 4,
                    version,
                    ..DeclareFields::default()
                },
                &drop_sections(&entries),
            )?;
            let request = Declare::drop(6, 4, kind, named.then(|| target.to_string()));
            let built = if version == 0 {
                request
            } else {
                request.with_version(version, 0)
            };
            assert_eq!(built.frame(header())?, expected, "{kind:?}");
        }
    }
    Ok(())
}

/// C1.55 at the builder: entity types against the frame laid out by hand, whatever `built_on` is.
#[test]
fn test_declarations_are_laid_out_as_a_writes_records() -> TestResult {
    for built_on in [0, u64::MAX] {
        let expected = declare_frame(
            DeclareFields {
                kind: 1,
                tenant: 8,
                built_on,
                principal: 2,
                ..DeclareFields::default()
            },
            &entity_type_sections(&["urn:a", "urn:b"]),
        )?;
        let built = Declare::declarations(8, built_on, 2, vec![entity("urn:a"), entity("urn:b")]);
        assert_eq!(built.frame(header())?, expected);
    }
    Ok(())
}

/// C1.46 at the builder: a Declare of no declaration is refused, and no frame is built.
#[test]
fn test_the_builder_refuses_no_declaration() -> TestResult {
    let empty = Declare::declarations(3, 11, 9, Vec::new());
    assert_eq!(refused(&empty)?, count(538, "declarations", 0, 1));
    Ok(())
}

/// C1.50, C1.51 at the builder: a Drop of kind 1 or 2 names its target, and one of kind 3 or 4 names none.
#[test]
fn test_the_builder_refuses_a_drop_whose_target_does_not_fit_its_kind() -> TestResult {
    for kind in [DropKind::Predicate, DropKind::Graph] {
        let unnamed = Declare::drop(3, 9, kind, None);
        assert_eq!(refused(&unnamed)?, count(538, "drop terms", 0, 1));
    }
    for kind in [DropKind::TenantData, DropKind::Tenant] {
        let named = Declare::drop(3, 9, kind, Some("http://ex.org/age".to_string()));
        assert_eq!(refused(&named)?, count(538, "drop terms", 1, 0));
    }
    Ok(())
}

/// C1.42, C1.43, C1.54 at the builder: a reply's `version` and `commit_version` are checked as its view checks them.
#[test]
fn test_the_builder_refuses_a_reply_whose_versions_break_a_rule() -> TestResult {
    for commit_version in [1u64, (1 << 22) - 1, (1 << 22) + 1] {
        let reply = golden_declare().with_version(12, commit_version);
        assert_eq!(
            refused(&reply)?,
            field(538, "commit_version", commit_version)
        );
    }
    assert_eq!(
        refused(&golden_declare().with_version(4, 0))?,
        field(538, "version", 4)
    );
    assert_eq!(
        refused(&golden_declare().with_version(0, 1 << 22))?,
        field(538, "version", 0)
    );
    assert_eq!(
        refused(&golden_drop().with_version(5, 1 << 22))?,
        field(538, "commit_version", 1 << 22)
    );
    Ok(())
}

/// C1.58: declarations filling a frame of 1,048,576 bytes are built and read; twice as many are a frame past 1 MiB.
/// Each entity type takes 48 bytes: a record of 16, two args of 4, an end of 4 and a name of 20 bytes.
#[test]
fn test_declarations_filling_a_frame_are_built_and_read() -> TestResult {
    let name = |index: usize| format!("urn:type:{index:011}");
    let fitting = (MAX_FRAME_SIZE - 4_096) / 48;
    let types: Vec<Declared> = (0..fitting).map(|index| entity(&name(index))).collect();
    let bytes = Declare::declarations(1, 0, 1, types).frame(header())?;
    assert!(bytes.len() > MAX_FRAME_SIZE - 8_192 && bytes.len() <= MAX_FRAME_SIZE);
    let view = DeclareView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(view.len(), fitting);
    assert_eq!(
        view.declaration(fitting - 1),
        Some(entity(&name(fitting - 1)))
    );
    assert_eq!(view.declaration(fitting), None);

    let over: Vec<Declared> = (0..2 * fitting).map(|index| entity(&name(index))).collect();
    let refusal = refused(&Declare::declarations(1, 0, 1, over))?;
    assert!(
        matches!(
            refusal,
            ProtocolErrorEnum::FrameTooLarge {
                limit: MAX_FRAME_SIZE,
                ..
            }
        ),
        "{refusal:?}"
    );
    Ok(())
}
