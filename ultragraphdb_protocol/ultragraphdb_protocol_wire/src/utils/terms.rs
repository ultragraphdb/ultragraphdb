/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! RDF terms in requests: N-Triples text, as oxrdf writes and parses it, one string per term. A view checks a term's
//! UTF-8; its syntax is checked where it is parsed.

use std::str::FromStr;

use oxrdf::{GraphName, NamedNode, NamedOrBlankNode, Term};
use ultragraphdb_error_protocol::ProtocolError;

/// The term `text`, string `at` of section `section`, refused as [`ProtocolError::BadTerm`].
pub(crate) fn parse_term(
    command: u16,
    section: usize,
    at: usize,
    text: &str,
) -> Result<Term, ProtocolError> {
    Term::from_str(text).map_err(|_| ProtocolError::BadTerm(command, section, at))
}

/// An IRI or a blank node.
pub(crate) fn parse_subject(
    command: u16,
    section: usize,
    at: usize,
    text: &str,
) -> Result<NamedOrBlankNode, ProtocolError> {
    match parse_term(command, section, at, text)? {
        Term::NamedNode(node) => Ok(node.into()),
        Term::BlankNode(node) => Ok(node.into()),
        _ => Err(ProtocolError::BadTerm(command, section, at)),
    }
}

/// An IRI.
pub(crate) fn parse_named(
    command: u16,
    section: usize,
    at: usize,
    text: &str,
) -> Result<NamedNode, ProtocolError> {
    match parse_term(command, section, at, text)? {
        Term::NamedNode(node) => Ok(node),
        _ => Err(ProtocolError::BadTerm(command, section, at)),
    }
}

/// A graph's name: the default graph when absent, else an IRI or a blank node.
pub(crate) fn parse_graph(
    command: u16,
    section: usize,
    at: usize,
    text: Option<&str>,
) -> Result<GraphName, ProtocolError> {
    match text {
        None => Ok(GraphName::DefaultGraph),
        Some(text) => match parse_subject(command, section, at, text)? {
            NamedOrBlankNode::NamedNode(node) => Ok(node.into()),
            NamedOrBlankNode::BlankNode(node) => Ok(node.into()),
        },
    }
}

/// An IRI given as its plain text, without angle brackets, as a declaration and the data dictionary name them.
pub(crate) fn parse_iri(
    command: u16,
    section: usize,
    at: usize,
    text: &str,
) -> Result<NamedNode, ProtocolError> {
    NamedNode::new(text).map_err(|_| ProtocolError::BadTerm(command, section, at))
}

/// A graph's N-Triples text, `None` for the default graph.
pub(crate) fn graph_text(graph: &GraphName) -> Option<String> {
    match graph {
        GraphName::DefaultGraph => None,
        GraphName::NamedNode(node) => Some(node.to_string()),
        GraphName::BlankNode(node) => Some(node.to_string()),
    }
}
