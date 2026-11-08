/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{Literal, NamedNode};
use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_terms::{Datatype, Hash128, TermError, TermView, Text};
use ultragraphdb_tool_bdd::TestResult;

fn id(sequence: u64) -> TestResult<SystemId> {
    Ok(SystemId::new(
        TypePrefix::RESOURCE,
        Bucket::new(7)?,
        Sequence::new(sequence)?,
    ))
}

#[test]
fn test_an_id_is_no_literal() -> TestResult {
    let refused = TermView::Id(id(5)?).to_literal(
        |_| Ok::<_, TermError>(NamedNode::new_unchecked("http://e/x")),
        |_| Ok(String::new()),
    );
    assert_eq!(refused, Err(TermError::not_a_literal(id(5)?)));
    Ok(())
}

#[test]
fn test_a_resolver_failure_is_returned() -> TestResult {
    #[derive(Debug, PartialEq)]
    struct Missing(u64);
    impl From<TermError> for Missing {
        fn from(_: TermError) -> Self {
            Missing(0)
        }
    }
    let view = TermView::Typed {
        datatype: Datatype::Id(id(9)?),
        lexical: Text::Inline("x"),
    };
    assert_eq!(
        view.to_literal(
            |id| Err(Missing(id.sequence().get())),
            |_| Ok(String::new())
        ),
        Err(Missing(9))
    );
    let hashed = TermView::String(Text::Hashed(Hash128::of(b"gone")));
    assert_eq!(
        hashed.to_literal(|_| Err(Missing(1)), |_| Err(Missing(2))),
        Err(Missing(2))
    );
    Ok(())
}

/// A datatype a Result names by its IRI's hash reads as the IRI that hash names, refused when that is no IRI.
#[test]
fn test_a_datatype_by_hash_is_the_iri_its_hash_names() -> TestResult {
    let iri = "http://e/celsius";
    let view = TermView::Typed {
        datatype: Datatype::Hashed(Hash128::of(iri.as_bytes())),
        lexical: Text::Inline("21.5"),
    };
    assert_eq!(
        view.to_literal(
            |id| Err(TermError::not_a_literal(id)),
            |_| Ok(iri.to_string())
        )?,
        Literal::new_typed_literal("21.5", NamedNode::new(iri)?)
    );
    let no_iri = "no IRI";
    assert_eq!(
        view.to_literal(
            |id| Err(TermError::not_a_literal(id)),
            |_| Ok(no_iri.to_string())
        ),
        Err(TermError::malformed(
            no_iri.as_bytes().to_vec(),
            "a datatype that is no IRI"
        ))
    );
    Ok(())
}
