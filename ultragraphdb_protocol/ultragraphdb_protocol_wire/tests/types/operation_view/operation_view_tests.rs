/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolErrorEnum;
use ultragraphdb_protocol_wire::{BodyView, OperationView, RequestTermView};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::types::operation::operation_tests::{counter, upsert};
use crate::utils::golden_bodies::{frame_of, golden, put, with_command_header, with_section};

#[test]
fn test_the_golden_operations_read_their_values() -> TestResult {
    let bytes = golden("operation")?.bytes()?;
    let view = OperationView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!(
        (view.kind(), view.delta(), view.properties(), view.links()),
        (2, 0, 1, 1)
    );
    assert_eq!(
        view.terms().get(1),
        Some(RequestTermView::Literal {
            lexical: "x",
            datatype: None
        })
    );
    assert_eq!(view.to_body()?, upsert()?);
    let counter_bytes = golden("operation_counter")?.bytes()?;
    let counter_view = OperationView::from_frame(&frame_of(&counter_bytes)?)?;
    assert_eq!((counter_view.kind(), counter_view.delta()), (4, -5));
    assert_eq!(counter_view.to_body()?, counter());
    Ok(())
}

fn header_refused(name: &str, change: impl FnOnce(&mut [u8])) -> TestResult<ProtocolErrorEnum> {
    let bytes = with_command_header(&golden(name)?.bytes()?, change);
    Ok(OperationView::from_frame(&frame_of(&bytes)?)
        .err()
        .context("a refusal")?
        .0)
}

#[test]
fn test_command_header_fields_fit_the_kind() -> TestResult {
    // PROTOCOL.md §7.3: kinds 1 to 6; 7 is the first number no kind has.
    assert_eq!(
        header_refused("operation", |h| h[0] = 7)?,
        ProtocolErrorEnum::FieldValue {
            command: 9,
            field: "kind",
            value: 7
        }
    );
    assert_eq!(
        header_refused("operation", |h| h[0] = 0)?,
        ProtocolErrorEnum::FieldValue {
            command: 9,
            field: "kind",
            value: 0
        }
    );
    assert_eq!(
        header_refused("operation", |h| put(h, 8, 1, 8))?,
        ProtocolErrorEnum::FieldValue {
            command: 9,
            field: "delta",
            value: 1
        }
    );
    assert_eq!(
        header_refused("operation", |h| h[0] = 1)?,
        ProtocolErrorEnum::FieldValue {
            command: 9,
            field: "links",
            value: 1
        }
    );
    assert_eq!(
        header_refused("operation_counter", |h| put(h, 16, 1, 4))?,
        ProtocolErrorEnum::FieldValue {
            command: 9,
            field: "properties",
            value: 1
        }
    );
    // Compare-and-set, kind 3, takes no properties: the golden upsert's one property is refused before its link.
    assert_eq!(
        header_refused("operation", |h| h[0] = 3)?,
        ProtocolErrorEnum::FieldValue {
            command: 9,
            field: "properties",
            value: 1
        }
    );
    // A negative delta is a delta too: the counter's -5 under insert-if-absent, as its u64 bits 2^64 - 5.
    assert_eq!(
        header_refused("operation_counter", |h| h[0] = 1)?,
        ProtocolErrorEnum::FieldValue {
            command: 9,
            field: "delta",
            value: 18_446_744_073_709_551_611
        }
    );
    assert_eq!(
        header_refused("operation_counter", |h| put(h, 20, 2, 4))?,
        ProtocolErrorEnum::FieldValue {
            command: 9,
            field: "links",
            value: 2
        }
    );
    Ok(())
}

#[test]
fn test_args_are_counted_and_in_range() -> TestResult {
    fn refused(change: impl FnOnce(&mut Vec<u8>)) -> TestResult<ProtocolErrorEnum> {
        let bytes = with_section(&golden("operation")?.bytes()?, 3, 0, change);
        Ok(OperationView::from_frame(&frame_of(&bytes)?)
            .err()
            .context("a refusal")?
            .0)
    }
    // graph, predicate, value, one property (2), one link (3): 8 args.
    assert_eq!(
        refused(|a| a.truncate(28))?,
        ProtocolErrorEnum::CountMismatch {
            command: 9,
            field: "args",
            count: 7,
            expected: 8
        }
    );
    assert_eq!(
        refused(|a| put(a, 28, 2, 4))?,
        ProtocolErrorEnum::FieldValue {
            command: 9,
            field: "link direction",
            value: 2
        }
    );
    assert_eq!(
        refused(|a| put(a, 4, 6, 4))?,
        ProtocolErrorEnum::IndexOutOfRange {
            command: 9,
            section: 0,
            at: 1,
            index: 6,
            limit: 6
        }
    );
    assert_eq!(
        refused(|a| put(a, 4, u32::MAX.into(), 4))?,
        ProtocolErrorEnum::IndexOutOfRange {
            command: 9,
            section: 0,
            at: 1,
            index: u32::MAX,
            limit: 6
        }
    );
    Ok(())
}

/// An upsert-and-link of two properties and three links reads back its counts and, from its args, each property's pair
/// and each link: args 3 + 2k and 4 + 2k are property k, the links follow in threes (`PROTOCOL.md` §7.3).
#[test]
fn test_every_property_and_link_reads_from_its_args() -> TestResult {
    use ultragraphdb_protocol_wire::{BodyBuilder, CommitOperation, Link, Operation, TermTable};

    let mut t = TermTable::new();
    let [id, x, p1, v1, p2, v2, l1, n1, l2, n2, l3, n3] = [
        "id", "x", "p1", "v1", "p2", "v2", "l1", "n1", "l2", "n2", "l3", "n3",
    ]
    .map(|name| t.iri(&format!("http://ex.org/{name}")));
    let operation = CommitOperation::UpsertAndLink {
        graph: None,
        predicate: id,
        value: x,
        properties: vec![(p1, v1), (p2, v2)],
        links: vec![
            Link::to_node(l1, n1),
            Link::from_node(l2, n2),
            Link::to_node(l3, n3),
        ],
    };
    let bytes = Operation::new(t, operation.clone()).frame(golden("operation")?.header())?;
    let view = OperationView::from_frame(&frame_of(&bytes)?)?;
    assert_eq!((view.kind(), view.properties(), view.links()), (2, 2, 3));
    assert_eq!(view.operation(), operation);
    Ok(())
}

/// Stage 6: Operation kinds 5, delete an instance, and 6, retract an instance (`PROTOCOL.md` §7.3), over Operations
/// laid out by hand: `kind` at 0, `delta` at 8, `properties` at 16, `links` at 20; the args, then the term table.
mod stage_6 {
    use ultragraphdb_error_protocol::ProtocolErrorEnum;
    use ultragraphdb_protocol_wire::{
        ABSENT, BodyBuilder, BodyView, Command, CommitOperation, Header, Operation, OperationView,
        RequestTermView, TermTable,
    };
    use ultragraphdb_tool_bdd::{Context, TestResult};

    use crate::utils::golden_bodies::{CLUSTER, frame_of};
    use crate::utils::ugs_frames::{frame, iri_terms, outcome, refused, u32s};

    const INSTANCE: &str = "https://example.org/deep_brain#Node/ORG%3Aacme";
    const GRAPH: &str = "https://example.org/g";

    /// An Operation of `kind` and the header fields `fields` past it, its `args` over the IRIs `iris`.
    fn operation(fields: &[(usize, &[u8])], args: &[u32], iris: &[&str]) -> TestResult<Vec<u8>> {
        let [records, text] = iri_terms(iris);
        frame(
            Command::Operation,
            fields,
            &[u32s(args), records, text],
            false,
        )
    }

    fn field_value(field: &'static str, value: u64) -> ProtocolErrorEnum {
        ProtocolErrorEnum::FieldValue {
            command: 9,
            field,
            value,
        }
    }

    /// wire-protocol, "A retract operation is read and built": kind 6 naming the default graph and the instance's IRI,
    /// built and read back, the view answering kind 6, the graph `ABSENT` and the IRI.
    #[test]
    fn test_a_retract_operation_is_built_and_read_back() -> TestResult {
        let mut terms = TermTable::new();
        let instance = terms.iri(INSTANCE);
        let retract = Operation::new(
            terms,
            CommitOperation::RetractInstance {
                graph: None,
                instance,
            },
        );
        let bytes = retract.frame(Header::new(CLUSTER, Command::Operation).with_stream(3))?;
        assert_eq!(bytes, operation(&[(0, &[6])], &[ABSENT, 0], &[INSTANCE])?);
        let view = OperationView::from_frame(&frame_of(&bytes)?)?;
        assert_eq!(view.kind(), 6);
        assert_eq!(
            view.operation(),
            CommitOperation::RetractInstance {
                graph: None,
                instance: 0
            }
        );
        assert_eq!(
            view.terms().get(0).context("the instance")?,
            RequestTermView::Iri(INSTANCE)
        );
        assert_eq!(view.to_body()?, retract);
        Ok(())
    }

    /// Kind 5 deletes the instance a named graph's term names: graph term 0, instance term 1.
    #[test]
    fn test_a_delete_operation_names_its_graph_and_instance() -> TestResult {
        let bytes = operation(&[(0, &[5])], &[0, 1], &[GRAPH, INSTANCE])?;
        let view = OperationView::from_frame(&frame_of(&bytes)?)?;
        assert_eq!(
            view.operation(),
            CommitOperation::DeleteInstance {
                graph: Some(0),
                instance: 1
            }
        );
        Ok(())
    }

    /// wire-protocol, "A delete operation with a third arg is refused": `CountMismatch` naming `args`; one arg alike.
    #[test]
    fn test_a_delete_or_retract_takes_two_args() -> TestResult {
        assert_eq!(
            refused::<OperationView>(&operation(&[(0, &[5])], &[ABSENT, 0, 0], &[INSTANCE])?)?,
            ProtocolErrorEnum::CountMismatch {
                command: 9,
                field: "args",
                count: 3,
                expected: 2
            }
        );
        assert_eq!(
            refused::<OperationView>(&operation(&[(0, &[6])], &[ABSENT], &[INSTANCE])?)?,
            ProtocolErrorEnum::CountMismatch {
                command: 9,
                field: "args",
                count: 1,
                expected: 2
            }
        );
        Ok(())
    }

    /// A kind above 6 is refused as `FieldValue` naming `kind`; kinds 5 and 6 take no `delta`, `properties` or `links`;
    /// the instance is a term, never `ABSENT`.
    #[test]
    fn test_a_delete_or_retract_takes_no_other_field() -> TestResult {
        let args = [ABSENT, 0];
        assert_eq!(
            refused::<OperationView>(&operation(&[(0, &[7])], &args, &[INSTANCE])?)?,
            field_value("kind", 7)
        );
        assert_eq!(
            refused::<OperationView>(&operation(
                &[(0, &[5]), (8, &1i64.to_le_bytes())],
                &args,
                &[INSTANCE]
            )?)?,
            field_value("delta", 1)
        );
        assert_eq!(
            refused::<OperationView>(&operation(
                &[(0, &[6]), (16, &1u32.to_le_bytes())],
                &args,
                &[INSTANCE]
            )?)?,
            field_value("properties", 1)
        );
        assert_eq!(
            refused::<OperationView>(&operation(
                &[(0, &[5]), (20, &1u32.to_le_bytes())],
                &args,
                &[INSTANCE]
            )?)?,
            field_value("links", 1)
        );
        assert_eq!(
            refused::<OperationView>(&operation(&[(0, &[5])], &[ABSENT, ABSENT], &[INSTANCE])?)?,
            ProtocolErrorEnum::IndexOutOfRange {
                command: 9,
                section: 0,
                at: 1,
                index: ABSENT,
                limit: 1
            }
        );
        assert_eq!(
            outcome::<OperationView>(&operation(&[(0, &[6])], &args, &[INSTANCE])?)?,
            None
        );
        Ok(())
    }
}
