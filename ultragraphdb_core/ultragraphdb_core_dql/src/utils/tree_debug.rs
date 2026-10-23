/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! `Debug` for the query trees, written from a stack rather than by recursion, in the layout `#[derive(Debug)]`
//! gives: `Name { field: value, .. }`, and with `{:#?}` one field per line, indented four spaces a level.

use std::fmt::{self, Debug};

/// A field of a tree node as `Debug` writes it.
pub(crate) enum Field<'t, T> {
    /// A field that holds no node of the tree, already formatted.
    Value(&'static str, String),
    /// The node's children.
    Children(&'static str, &'t [T]),
}

/// A field's value formatted as `Debug` writes it at `level` levels of indentation.
pub(crate) fn value(v: &impl Debug, pretty: bool, level: usize) -> String {
    if !pretty {
        return format!("{v:?}");
    }
    format!("{v:#?}").replace('\n', &format!("\n{}", indent(level)))
}

fn indent(level: usize) -> String {
    "    ".repeat(level)
}

/// Text still to write, or a node to expand at a level of indentation.
enum Piece<'t, T> {
    Text(String),
    Node(&'t T, usize),
}

/// Writes `root` and its descendants. `fields` gives a node's fields in order, each value formatted for the level
/// of indentation it is given.
pub(crate) fn write_tree<'t, T>(
    f: &mut fmt::Formatter<'_>,
    root: &'t T,
    name: &str,
    fields: impl Fn(&'t T, bool, usize) -> Vec<Field<'t, T>>,
) -> fmt::Result {
    let pretty = f.alternate();
    let mut stack = vec![Piece::Node(root, 0)];
    while let Some(piece) = stack.pop() {
        let (node, level) = match piece {
            Piece::Text(text) => {
                f.write_str(&text)?;
                continue;
            }
            Piece::Node(node, level) => (node, level),
        };
        // The node's pieces in order, pushed onto the stack reversed.
        let mut out = vec![Piece::Text(if pretty {
            format!("{name} {{\n")
        } else {
            format!("{name} {{ ")
        })];
        for (i, field) in fields(node, pretty, level + 1).into_iter().enumerate() {
            let field_name = match &field {
                Field::Value(n, _) | Field::Children(n, _) => *n,
            };
            out.push(Piece::Text(if pretty {
                format!("{}{field_name}: ", indent(level + 1))
            } else if i == 0 {
                format!("{field_name}: ")
            } else {
                format!(", {field_name}: ")
            }));
            match field {
                Field::Value(_, text) => out.push(Piece::Text(text)),
                Field::Children(_, children) => list(&mut out, children, pretty, level + 1),
            }
            if pretty {
                out.push(Piece::Text(",\n".to_string()));
            }
        }
        out.push(Piece::Text(if pretty {
            format!("{}}}", indent(level))
        } else {
            " }".to_string()
        }));
        stack.extend(out.into_iter().rev());
    }
    Ok(())
}

/// The pieces of a list of nodes at `level`: `[a, b]`, or one per line with `{:#?}`.
fn list<'t, T>(out: &mut Vec<Piece<'t, T>>, nodes: &'t [T], pretty: bool, level: usize) {
    if nodes.is_empty() {
        out.push(Piece::Text("[]".to_string()));
        return;
    }
    out.push(Piece::Text(if pretty { "[\n" } else { "[" }.to_string()));
    for (i, node) in nodes.iter().enumerate() {
        if pretty {
            out.push(Piece::Text(indent(level + 1)));
        } else if i > 0 {
            out.push(Piece::Text(", ".to_string()));
        }
        out.push(Piece::Node(node, level + 1));
        if pretty {
            out.push(Piece::Text(",\n".to_string()));
        }
    }
    out.push(Piece::Text(if pretty {
        format!("{}]", indent(level))
    } else {
        "]".to_string()
    }));
}
