/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{
    BodyBuilder, DeclaredEntityType, DeclaredKind, DeclaredPredicate, DeclaredRelation, Describe,
};
use ultragraphdb_tool_bdd::TestResult;

use crate::utils::golden_bodies::{DESCRIBE_SCHEMA_ID, golden};

fn ex(name: &str) -> String {
    format!("http://ex.org/{name}")
}

pub fn golden_answer() -> Describe {
    Describe::new(0, 42)
        .with_labels(vec![ex("Person")])
        .with_relationship_types(vec![ex("knows")])
        .with_property_keys(vec![ex("name")])
        .with_predicates(vec![
            DeclaredPredicate::new(ex("knows")).with_kind(DeclaredKind::Edge),
            DeclaredPredicate::new(ex("name"))
                .with_kind(DeclaredKind::Property)
                .with_tokenizers(vec!["term".to_string()]),
        ])
        .with_entity_types(vec![(
            DeclaredEntityType::new(ex("Person"), None, false),
            // PROTOCOL.md section 4.6: a declaration's `extra` is 0; a record names no place in the type hierarchy.
            0,
        )])
}

#[test]
fn test_the_request_and_the_answer_build_the_golden_frames() -> TestResult {
    assert_eq!(
        Describe::request(0).frame(golden("describe")?.header())?,
        golden("describe")?.bytes()?
    );
    assert_eq!(Describe::request(0), Describe::new(0, 0));
    let answer = golden_answer();
    assert_eq!(
        answer.frame(golden("describe_reply")?.header())?,
        golden("describe_reply")?.bytes()?
    );
    assert_eq!((answer.tenant(), answer.read_version()), (0, 42));
    assert_eq!(answer.labels(), &[ex("Person")]);
    assert_eq!(answer.predicates().len(), 2);
    assert_eq!(answer.entity_types()[0].1, 0);
    assert!(answer.relations().is_empty());
    Ok(())
}

#[test]
fn test_the_schema_id_follows_the_declarations_alone() {
    let answer = golden_answer();
    assert_eq!(answer.schema_id(), DESCRIBE_SCHEMA_ID);
    let other_data = Describe::new(7, 99)
        .with_labels(vec![ex("Other")])
        .with_predicates(answer.predicates().to_vec())
        .with_entity_types(answer.entity_types().to_vec());
    assert_eq!(other_data.schema_id(), DESCRIBE_SCHEMA_ID);
    let declared =
        golden_answer().with_relations(vec![DeclaredRelation::new(ex("R"), vec![ex("r")])]);
    assert_ne!(declared.schema_id(), DESCRIBE_SCHEMA_ID);
    let typed = golden_answer().with_predicates(vec![
        DeclaredPredicate::new(ex("name"))
            .with_kind(DeclaredKind::Property)
            .with_datatype("http://www.w3.org/2001/XMLSchema#string")
            .with_tokenizers(vec!["term".to_string()]),
        DeclaredPredicate::new(ex("knows")).with_kind(DeclaredKind::Edge),
    ]);
    assert_ne!(typed.schema_id(), DESCRIBE_SCHEMA_ID);
}

/// Stage 6.6: Describe's builder writes nine sections and its entries, and its schema id covers the entries
/// (`PROTOCOL.md` §8.3, §11.7), against answers laid out by hand.
mod stage_6 {
    use ultragraphdb_protocol_wire::{
        BodyBuilder, Command, Describe, DescribeEntry, EntryKind, FLAG_LAST, Header, RetiredItem,
        RetiredName, xxh3_64,
    };
    use ultragraphdb_tool_bdd::TestResult;

    use crate::utils::golden_bodies::{CLUSTER, body_data, join_sections};
    use crate::utils::ugs_frames::{describe_frame, entry_sections, frame, retired, strings};

    fn header() -> Header {
        Header::new(CLUSTER, Command::Describe)
            .with_stream(3)
            .with_flags(FLAG_LAST)
    }

    /// A Type entry, a source and a retired name renamed to the Type's name, in kind order.
    pub fn entries() -> Vec<DescribeEntry> {
        vec![
            DescribeEntry::new(EntryKind::Type, "gen5.Org", vec![1, 2, 3]),
            DescribeEntry::new(EntryKind::Source, "gen5", b"namespace gen5 {}".to_vec()),
            DescribeEntry::retired(
                "gen5.Company",
                RetiredName::renamed(RetiredItem::Type, "gen5.Org", 3),
            ),
        ]
    }

    /// The entries' names in entry order, then the retired name's current name, already interned at 0 (§4.3).
    fn entry_rows() -> (Vec<&'static str>, [Vec<u8>; 2]) {
        let renamed = retired(1, 1, 0, 3, 0);
        (
            vec!["gen5.Org", "gen5", "gen5.Company"],
            entry_sections(&[
                (3, 0, &[1, 2, 3]),
                (19, 1, b"namespace gen5 {}"),
                (20, 2, &renamed),
            ]),
        )
    }

    /// The request: nine empty sections, its body 72 bytes.
    #[test]
    fn test_a_request_builds_nine_empty_sections() -> TestResult {
        let expected = frame(
            Command::Describe,
            &[],
            &[
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
            ],
            true,
        )?;
        assert_eq!(body_data(&expected).len(), 72);
        assert_eq!(Describe::request(0).frame(header())?, expected);
        Ok(())
    }

    /// An answer of entries builds them in sections 7 and 8, a retired name's current name by its string index.
    #[test]
    fn test_an_answer_builds_its_entries() -> TestResult {
        let answer = Describe::new(9, 42).with_entries(entries());
        assert_eq!(answer.entries(), entries().as_slice());
        let (names, rows) = entry_rows();
        assert_eq!(answer.frame(header())?, describe_frame(&names, rows, true)?);
        Ok(())
    }

    /// The schema id hashes the body of the declarations and entries alone: sections 2 to 4 empty, the strings the
    /// entries' names then the retired names' current names (§11.7), XXH3-64 of the nine sections laid out by hand.
    #[test]
    fn test_the_schema_id_covers_the_entries() -> TestResult {
        let answer = Describe::new(5, 42)
            .with_labels(vec!["http://ex.org/Person".to_string()])
            .with_entries(entries());
        let (names, [records, bytes]) = entry_rows();
        let [ends, text] = strings(&names);
        let body = join_sections(
            &[],
            &[
                ends,
                text,
                vec![],
                vec![],
                vec![],
                vec![],
                vec![],
                records,
                bytes,
            ],
        );
        assert_eq!(answer.schema_id(), xxh3_64(&body));
        assert_ne!(answer.schema_id(), Describe::new(5, 42).schema_id());
        Ok(())
    }
}
