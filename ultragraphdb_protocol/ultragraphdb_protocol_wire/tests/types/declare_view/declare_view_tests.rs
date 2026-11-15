/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Declare's view (`PROTOCOL.md` §13.27), its checks in the Order line's order: unused bytes 2–7 and 48–127 zero;
//! `kind`; `drop`; `built_on`; `commit_version`; sections as section 7.2 states; for `kind` 1 at least one record,
//! each of kind 8 to 10; for `kind` 2 no record, the term table's count, the term an IRI; in a `kind` 1 reply
//! `version` and `commit_version` at least 1. Every refused frame is laid out by hand.

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{
    ABSENT, BodyView, DeclareView, Declared, DeclaredEntityType, DropKind,
};
use ultragraphdb_tool_bdd::TestResult;

use crate::types::declare::declare_tests::{
    GOLDEN_COMMIT_VERSION, golden_declarations, golden_declare, golden_drop,
};
use crate::utils::golden_bodies::{frame_of, view_any, with_command_header};
use crate::utils::system_frames::{
    DeclareFields, change_record, count, declare_frame, drop_sections, entity_type_sections, field,
    iris, reserved, role,
};
use crate::utils::term_records::{Entry, refusal, term_sections};
use crate::utils::ugs_frames::{strings, u32s};

/// The fields of a `kind` 1 request in tenant 3, built on version 11, for principal 9.
fn declaring() -> DeclareFields {
    DeclareFields {
        kind: 1,
        tenant: 3,
        built_on: 11,
        principal: 9,
        ..DeclareFields::default()
    }
}

/// The fields of a `kind` 2 request for a Drop of kind `drop` in tenant 3, for principal 9.
fn dropping(drop: u8) -> DeclareFields {
    DeclareFields {
        kind: 2,
        drop,
        tenant: 3,
        principal: 9,
        ..DeclareFields::default()
    }
}

/// The sections of one entity type, which every rule of section 7.2 passes.
fn one_type() -> Vec<Vec<u8>> {
    entity_type_sections(&["urn:a"])
}

fn refused(fields: DeclareFields, sections: &[Vec<u8>]) -> TestResult<ProtocolErrorEnum> {
    refusal::<DeclareView>(&declare_frame(fields, sections)?)
}

fn entity(name: &str) -> Declared {
    Declared::EntityType(DeclaredEntityType::new(name, None, false))
}

/// C1.59: the golden frames read back to their fields and build again.
#[test]
fn test_the_golden_declares_read_and_build_again() -> TestResult {
    let request = role("declare")?;
    let view = DeclareView::from_frame(&frame_of(&request)?)?;
    assert_eq!(
        (view.tenant(), view.built_on(), view.principal()),
        (3, 11, 9)
    );
    assert_eq!((view.version(), view.commit_version()), (0, 0));
    assert!(view.is_request());
    assert_eq!((view.drop_kind(), view.target()), (None, None));
    assert_eq!((view.len(), view.is_empty()), (3, false));
    for (index, declared) in golden_declarations().into_iter().enumerate() {
        assert_eq!(view.declaration(index), Some(declared));
    }
    assert_eq!(view.declaration(3), None);
    assert_eq!(view.to_body()?, golden_declare());

    let reply = role("declare_reply")?;
    let answered = DeclareView::from_frame(&frame_of(&reply)?)?;
    assert_eq!(
        (answered.version(), answered.commit_version()),
        (12, GOLDEN_COMMIT_VERSION)
    );
    assert!(!answered.is_request());
    assert_eq!(
        answered.to_body()?,
        golden_declare().with_version(12, GOLDEN_COMMIT_VERSION)
    );

    let drop = role("declare_drop")?;
    let dropped = DeclareView::from_frame(&frame_of(&drop)?)?;
    assert_eq!(
        (dropped.drop_kind(), dropped.target()),
        (Some(DropKind::Predicate), Some("http://ex.org/age"))
    );
    assert_eq!((dropped.len(), dropped.is_empty()), (0, true));
    assert_eq!(dropped.built_on(), 0);
    assert_eq!(dropped.to_body()?, golden_drop());
    for bytes in [request, reply, drop] {
        assert_eq!(view_any(&bytes)?, Ok(()));
    }
    Ok(())
}

/// C1.37: an unused byte is refused naming its frame offset.
#[test]
fn test_unused_bytes_are_refused() -> TestResult {
    for at in [2usize, 7, 48, 127] {
        let bytes = with_command_header(&role("declare")?, |h| h[at] = 1);
        assert_eq!(refusal::<DeclareView>(&bytes)?, reserved(at));
    }
    Ok(())
}

/// C1.38: `kind` is 1 or 2.
#[test]
fn test_a_kind_outside_its_set_is_refused() -> TestResult {
    for kind in [0u8, 3, 255] {
        let fields = DeclareFields {
            kind,
            ..declaring()
        };
        assert_eq!(
            refused(fields, &one_type())?,
            field(538, "kind", kind.into())
        );
    }
    Ok(())
}

/// C1.39, C1.40: `drop` is 0 for `kind` 1 and 1 to 4 for `kind` 2.
#[test]
fn test_a_drop_that_does_not_fit_its_kind_is_refused() -> TestResult {
    let fields = DeclareFields {
        drop: 1,
        ..declaring()
    };
    assert_eq!(refused(fields, &one_type())?, field(538, "drop", 1));
    for drop in [0u8, 5, 255] {
        assert_eq!(
            refused(dropping(drop), &drop_sections(&[]))?,
            field(538, "drop", drop.into())
        );
    }
    Ok(())
}

/// C1.41, C1.42: a Drop names no version it is built on and no commit version.
#[test]
fn test_a_drop_names_no_built_on_and_no_commit_version() -> TestResult {
    let built = DeclareFields {
        built_on: 1,
        ..dropping(3)
    };
    assert_eq!(
        refused(built, &drop_sections(&[]))?,
        field(538, "built_on", 1)
    );
    for commit_version in [1u64 << 22, 7] {
        let committed = DeclareFields {
            version: 5,
            commit_version,
            ..dropping(3)
        };
        assert_eq!(
            refused(committed, &drop_sections(&[]))?,
            field(538, "commit_version", commit_version)
        );
    }
    Ok(())
}

/// C1.43: a `kind` 1 `commit_version` is the first version of an epoch, its low 22 bits zero.
#[test]
fn test_a_commit_version_inside_an_epoch_is_refused() -> TestResult {
    for commit_version in [1u64, (1 << 22) - 1, (1 << 22) + 1] {
        let fields = DeclareFields {
            version: 12,
            commit_version,
            ..declaring()
        };
        assert_eq!(
            refused(fields, &one_type())?,
            field(538, "commit_version", commit_version)
        );
    }
    Ok(())
}

/// C1.44: the header's fields are checked in the order `kind`, `drop`, `built_on`, `commit_version`.
#[test]
fn test_the_header_fields_are_checked_in_order() -> TestResult {
    let kind_first = DeclareFields {
        kind: 3,
        drop: 9,
        ..declaring()
    };
    assert_eq!(refused(kind_first, &one_type())?, field(538, "kind", 3));
    let drop_first = DeclareFields {
        drop: 7,
        built_on: 1,
        commit_version: 1,
        ..dropping(7)
    };
    assert_eq!(
        refused(drop_first, &drop_sections(&[]))?,
        field(538, "drop", 7)
    );
    let built_on_first = DeclareFields {
        built_on: 1,
        commit_version: 1,
        ..dropping(3)
    };
    assert_eq!(
        refused(built_on_first, &drop_sections(&[]))?,
        field(538, "built_on", 1)
    );
    // The header before the sections: a commit version inside an epoch, over sections that hold no record.
    let before_sections = DeclareFields {
        commit_version: 1,
        ..declaring()
    };
    assert_eq!(
        refused(before_sections, &drop_sections(&[]))?,
        field(538, "commit_version", 1)
    );
    Ok(())
}

/// C1.45: the sections are a Write's, checked in section 7.2's order before any rule of the records: the term table
/// (§4.4), the names (§4.3), the args a whole number of u32s.
#[test]
fn test_the_sections_are_checked_as_a_writes_before_the_records() -> TestResult {
    let length = |section: usize, len: usize, unit: usize| ProtocolErrorEnum::SectionLength {
        command: 538,
        section,
        len,
        unit,
    };
    // Every later rule broken too: no record at all, names and args of 3 bytes.
    let mut terms_broken = drop_sections(&[]);
    terms_broken[2] = vec![0; 25];
    terms_broken[4] = vec![0; 3];
    terms_broken[1] = vec![0; 3];
    assert_eq!(refused(declaring(), &terms_broken)?, length(2, 25, 24));
    let mut names_broken = drop_sections(&[]);
    names_broken[4] = vec![0; 3];
    names_broken[1] = vec![0; 3];
    assert_eq!(refused(declaring(), &names_broken)?, length(4, 3, 4));
    let mut args_broken = drop_sections(&[]);
    args_broken[1] = vec![0; 3];
    assert_eq!(refused(declaring(), &args_broken)?, length(1, 3, 4));
    assert_eq!(refused(dropping(1), &args_broken)?, length(1, 3, 4));
    Ok(())
}

/// C1.46: a `kind` 1 Declare holds at least one record.
#[test]
fn test_a_declare_of_no_declaration_is_refused() -> TestResult {
    assert_eq!(
        refused(declaring(), &drop_sections(&[]))?,
        count(538, "declarations", 0, 1)
    );
    Ok(())
}

/// The sections of entity types `urn:a` and `urn:b` with one record of `kind`, a change over four args naming the one
/// term of the table, at `index` among them: every rule of section 7.2 passes.
fn with_a_change(kind: u8, index: usize) -> Vec<Vec<u8>> {
    let (terms, text) = term_sections(&[Entry::Text(1, "http://ex.org/s", "")]);
    let [ends, names] = strings(&["urn:a", "urn:b"]);
    let mut records = Vec::new();
    let mut args = Vec::new();
    let mut named = 0u32;
    for position in 0..3 {
        let start = u32::try_from(args.len()).unwrap_or(u32::MAX);
        if position == index {
            records.extend(change_record(kind, 0, start, 4));
            args.extend([ABSENT, 0, 0, 0]);
        } else {
            records.extend(change_record(9, 0, start, 2));
            args.extend([named, ABSENT]);
            named += 1;
        }
    }
    vec![records, u32s(&args), terms, text, ends, names]
}

/// C1.47: a `kind` 1 record of a kind outside 8 to 10 is refused naming its index. A kind section 7.2 does not hold,
/// 11, is refused by section 7.2's rule first, as the Order line has it (`notes/group-1.md`, *Found wanting*).
#[test]
fn test_a_record_that_is_no_declaration_is_refused_with_its_index() -> TestResult {
    for kind in [1u8, 7] {
        for index in [0usize, 2] {
            assert_eq!(
                refused(declaring(), &with_a_change(kind, index))?,
                field(538, "declaration", index as u64),
                "kind {kind} at {index}"
            );
        }
    }
    assert_eq!(
        refused(declaring(), &with_a_change(11, 0))?,
        field(538, "change kind", 11)
    );
    Ok(())
}

/// C1.48: one record reads, and one of each of kinds 8, 9 and 10 does, which the golden frame holds.
#[test]
fn test_one_declaration_reads() -> TestResult {
    let bytes = declare_frame(declaring(), &one_type())?;
    let view = DeclareView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!((view.len(), view.is_empty()), (1, false));
    assert_eq!(view.declaration(0), Some(entity("urn:a")));
    assert_eq!(view.declaration(1), None);
    let golden = role("declare")?;
    assert_eq!(DeclareView::from_frame(&frame_of(&golden)?)?.len(), 3);
    Ok(())
}

/// C1.49, C1.53: a `kind` 2 Declare holds no record, checked before its term table's count.
#[test]
fn test_a_drop_holding_a_record_is_refused() -> TestResult {
    assert_eq!(
        refused(dropping(3), &one_type())?,
        count(538, "declarations", 1, 0)
    );
    assert_eq!(
        refused(dropping(1), &one_type())?,
        count(538, "declarations", 1, 0)
    );
    Ok(())
}

/// C1.50, C1.51: the term table holds one entry for `drop` 1 or 2 and none for `drop` 3 or 4.
#[test]
fn test_a_drops_term_count_fits_its_kind() -> TestResult {
    let two = iris(&["http://ex.org/a", "http://ex.org/b"]);
    for drop in [1u8, 2] {
        assert_eq!(
            refused(dropping(drop), &drop_sections(&[]))?,
            count(538, "drop terms", 0, 1)
        );
        assert_eq!(
            refused(dropping(drop), &drop_sections(&two))?,
            count(538, "drop terms", 2, 1)
        );
    }
    for drop in [3u8, 4] {
        assert_eq!(
            refused(dropping(drop), &drop_sections(&two[..1]))?,
            count(538, "drop terms", 1, 0)
        );
    }
    Ok(())
}

/// C1.52: the one term of a Drop of kind 1 or 2 is an IRI, refused naming its kind as section 6.5 does. A triple term
/// names three entries before its own (§4.4), so a table holding one is refused by its count first.
#[test]
fn test_a_drops_term_is_an_iri() -> TestResult {
    let others = [
        (Entry::Text(2, "b1", ""), 2u64),
        (Entry::Text(3, "abc", ""), 3),
        (Entry::Text(4, "chat", "fr"), 4),
    ];
    for (entry, kind) in others {
        for drop in [1u8, 2] {
            assert_eq!(
                refused(dropping(drop), &drop_sections(&[entry]))?,
                field(538, "drop term", kind)
            );
        }
    }
    let triple = [
        Entry::Text(1, "http://ex.org/a", ""),
        Entry::Text(1, "http://ex.org/b", ""),
        Entry::Text(1, "http://ex.org/c", ""),
        Entry::Triple(0, 1, 2),
    ];
    assert_eq!(
        refused(dropping(1), &drop_sections(&triple))?,
        count(538, "drop terms", 4, 1)
    );
    Ok(())
}

/// C1.54: a `kind` 1 reply names both its `version` and its `commit_version`.
#[test]
fn test_a_reply_naming_one_version_alone_is_refused() -> TestResult {
    let no_commit = DeclareFields {
        version: 4,
        ..declaring()
    };
    assert_eq!(refused(no_commit, &one_type())?, field(538, "version", 4));
    let no_version = DeclareFields {
        commit_version: 1 << 22,
        ..declaring()
    };
    assert_eq!(refused(no_version, &one_type())?, field(538, "version", 0));
    // The records are checked before the reply's versions.
    assert_eq!(
        refused(no_commit, &drop_sections(&[]))?,
        count(538, "declarations", 0, 1)
    );
    Ok(())
}

/// C1.55: `built_on` holds any version, 0 for a tenant holding none.
#[test]
fn test_any_built_on_reads() -> TestResult {
    for built_on in [0, u64::MAX] {
        let fields = DeclareFields {
            built_on,
            ..declaring()
        };
        let bytes = declare_frame(fields, &one_type())?;
        assert_eq!(
            DeclareView::from_frame(&frame_of(&bytes)?)?.built_on(),
            built_on
        );
    }
    Ok(())
}

/// C1.56: a `kind` 2 reply that changed nothing holds the bytes of its request, `version` 0; one that built a version
/// names it, with `commit_version` 0.
#[test]
fn test_a_drops_reply_reads_with_or_without_a_version() -> TestResult {
    for version in [0u64, 5] {
        let fields = DeclareFields {
            version,
            ..dropping(4)
        };
        let bytes = declare_frame(fields, &drop_sections(&[]))?;
        let view = DeclareView::from_frame(&frame_of(&bytes)?)?;
        assert_eq!((view.version(), view.commit_version()), (version, 0));
        assert_eq!(view.is_request(), version == 0);
        assert_eq!(
            (view.drop_kind(), view.target()),
            (Some(DropKind::Tenant), None)
        );
        assert_eq!(view_any(&bytes)?, Ok(()));
    }
    Ok(())
}

/// C1.57: the least `kind` 1 reply, version 1 at the first version of epoch 1.
#[test]
fn test_the_least_reply_reads() -> TestResult {
    let fields = DeclareFields {
        version: 1,
        commit_version: 1 << 22,
        ..declaring()
    };
    let bytes = declare_frame(fields, &one_type())?;
    let view = DeclareView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!((view.version(), view.commit_version()), (1, 1 << 22));
    assert!(!view.is_request());
    Ok(())
}

/// Each Drop kind reads with the target its kind names.
#[test]
fn test_a_drop_of_each_kind_reads() -> TestResult {
    let kinds = [
        (1u8, DropKind::Predicate),
        (2, DropKind::Graph),
        (3, DropKind::TenantData),
        (4, DropKind::Tenant),
    ];
    for (number, kind) in kinds {
        let named = number <= 2;
        let entries = if named {
            iris(&["http://ex.org/t"])
        } else {
            Vec::new()
        };
        let bytes = declare_frame(dropping(number), &drop_sections(&entries))?;
        let view = DeclareView::from_frame(&frame_of(&bytes)?)?;
        assert_eq!(view.drop_kind(), Some(kind));
        assert_eq!(view.target(), named.then_some("http://ex.org/t"));
        assert_eq!(view.tenant(), 3);
        assert_eq!(view.principal(), 9);
    }
    Ok(())
}

/// A frame of another command is no Declare.
#[test]
fn test_a_frame_of_another_command_is_refused() -> TestResult {
    assert_eq!(
        refusal::<DeclareView>(&role("defaults")?)?,
        ProtocolErrorEnum::WrongCommand {
            expected: 538,
            found: 539
        }
    );
    Ok(())
}

/// Phase 3: a `kind` 1 body may hold terms no declaration names, and a `kind` 2 body names no record names: the
/// sections are a Write's, which states no rule against either, and the view reads the declarations and the target
/// alone.
#[test]
fn test_terms_and_names_no_record_names_are_read_past() -> TestResult {
    let mut with_a_term = one_type();
    let (terms, text) = term_sections(&[Entry::Text(1, "http://ex.org/unnamed", "")]);
    with_a_term[2] = terms;
    with_a_term[3] = text;
    let bytes = declare_frame(declaring(), &with_a_term)?;
    let view = DeclareView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!((view.len(), view.target()), (1, None));
    assert_eq!(view.declaration(0), Some(entity("urn:a")));

    let mut with_a_name = drop_sections(&iris(&["http://ex.org/p"]));
    let [ends, names] = strings(&["urn:unused"]);
    with_a_name[4] = ends;
    with_a_name[5] = names;
    let bytes_2 = declare_frame(dropping(1), &with_a_name)?;
    let view_2 = DeclareView::from_frame(&frame_of(&bytes_2)?)?;
    assert_eq!(
        (view_2.len(), view_2.target()),
        (0, Some("http://ex.org/p"))
    );
    Ok(())
}

/// Phase 3: the records' own rules (`PROTOCOL.md` §4.6) reach through the body before the Declare's: an arg no
/// record consumes, a flag its kind does not hold, and a name past the string list.
#[test]
fn test_the_records_refusals_reach_through_the_body() -> TestResult {
    let mut stray_arg = drop_sections(&[]);
    stray_arg[1] = u32s(&[0]);
    assert_eq!(refused(dropping(3), &stray_arg)?, count(538, "args", 1, 0));
    let mut flagged = one_type();
    flagged[0][1] = 2;
    let refusal = refused(declaring(), &flagged)?;
    assert!(
        matches!(
            refusal,
            ProtocolErrorEnum::FieldValue {
                command: 538,
                field: "flags",
                ..
            }
        ),
        "{refusal:?}"
    );
    let mut unnamed = one_type();
    unnamed[1] = u32s(&[5, ABSENT]);
    let refusal_2 = refused(declaring(), &unnamed)?;
    assert!(
        matches!(
            refusal_2,
            ProtocolErrorEnum::IndexOutOfRange {
                command: 538,
                section: 1,
                ..
            }
        ),
        "{refusal_2:?}"
    );
    Ok(())
}
