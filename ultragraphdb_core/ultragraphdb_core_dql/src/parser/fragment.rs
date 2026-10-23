/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Fragments: their definitions, and their expansion where they are spread.

use std::collections::HashMap;

use super::godeep::godeep;
use super::{It, check_depth, ierr, iterr};
use crate::errors::dql_error::DqlError;
use crate::state::{ITEM_LEFT_CURL, ITEM_NAME};
use crate::types::fragment_node::FragmentNode;
use crate::types::graph_query::GraphQuery;

/// What a block of [`expand_fragments`] belongs to once its children are expanded.
enum Owner {
    /// The query block being expanded.
    Root,
    /// The enclosing block, as its next child.
    Parent,
    /// The fragment of this name, whose children replace a spread among the enclosing block's children.
    Fragment(String),
}

/// A block whose children [`expand_fragments`] is expanding, with the state Go's recursion keeps in its locals.
struct Frame {
    /// The block, its children taken out.
    gq: GraphQuery,
    /// The children still to expand.
    pending: std::vec::IntoIter<GraphQuery>,
    /// The children expanded, fragment spreads replaced.
    expanded: Vec<GraphQuery>,
    /// The level of the block; a fragment's block is at the level of the block its spread sits in.
    level: usize,
    /// The levels of blocks the block spans so far: itself and its expanded children.
    height: usize,
    owner: Owner,
}

impl Frame {
    /// Starts on a block at `level`, refusing one past [`MAX_DEPTH`](super::MAX_DEPTH).
    fn new(mut gq: GraphQuery, level: usize, owner: Owner) -> Result<Self, DqlError> {
        check_depth(level)?;
        let pending = std::mem::take(&mut gq.children).into_iter();
        Ok(Self {
            expanded: Vec::with_capacity(pending.len()),
            gq,
            pending,
            level,
            height: 1,
            owner,
        })
    }

    /// Adds expanded children spanning `height` levels below the block.
    fn take(&mut self, children: impl IntoIterator<Item = GraphQuery>, height: usize) {
        self.expanded.extend(children);
        self.height = self.height.max(height + 1);
    }
}

/// Replaces each fragment spread among the children of a block at `level`, at any depth, by the fragment's
/// children, keeping their order; a fragment entered and not yet exited is a cycle. Refuses an expansion whose
/// blocks reach past [`MAX_DEPTH`](super::MAX_DEPTH), each block counted at the level it lands on; the nesting of a
/// filter or math expression is counted where its fragment is written.
///
/// Go's `expandFragments` recurses into each child and each fragment; this keeps a stack of frames instead, so
/// that the call stack stays flat however deep the expansion goes.
pub(crate) fn expand_fragments(
    gq: &mut GraphQuery,
    fmap: &mut HashMap<String, FragmentNode>,
    level: usize,
) -> Result<(), DqlError> {
    let mut frames = vec![Frame::new(std::mem::take(gq), level, Owner::Root)?];
    while let Some(frame) = frames.last_mut() {
        match frame.pending.next() {
            Some(child) if child.is_fragment() => {
                // Name of fragment being referenced.
                let name = &child.fragment;
                let node = fmap
                    .get_mut(name)
                    .ok_or_else(|| DqlError::parse(format!("Missing fragment: {name}")))?;
                if node.exited {
                    check_depth(frame.level + node.height)?;
                    frame.take(node.gq.children.iter().cloned(), node.height);
                } else if node.entered {
                    return Err(DqlError::parse(format!("Cycle detected: {}", node.name)));
                } else {
                    node.entered = true;
                    let fragment = std::mem::take(&mut node.gq);
                    let entered = Frame::new(fragment, frame.level, Owner::Fragment(name.clone()))?;
                    frames.push(entered);
                }
            }
            Some(child) => {
                let entered = Frame::new(child, frame.level + 1, Owner::Parent)?;
                frames.push(entered);
            }
            None => {
                let Some(mut done) = frames.pop() else { break };
                done.gq.children = std::mem::take(&mut done.expanded);
                let parent = frames.last_mut();
                match (done.owner, parent) {
                    // Only the root's frame has no frame below it.
                    (Owner::Root, _) | (_, None) => *gq = done.gq,
                    (Owner::Parent, Some(parent)) => parent.take([done.gq], done.height),
                    (Owner::Fragment(name), Some(parent)) => {
                        let node = fmap
                            .get_mut(&name)
                            .ok_or_else(|| DqlError::parse(format!("Missing fragment: {name}")))?;
                        // The fragment's block spans one level above its children.
                        node.height = done.height - 1;
                        node.gq = done.gq;
                        node.exited = true;
                        parent.take(node.gq.children.iter().cloned(), node.height);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Parses a fragment definition (not a reference).
pub(crate) fn get_fragment(it: &mut It<'_, '_>) -> Result<FragmentNode, DqlError> {
    let mut name = String::new();
    while it.next() {
        let item = it.item();
        match item.typ() {
            ITEM_NAME => {
                let v = item.val().trim();
                // The first nontrivial token is the fragment name; everything after it until a left curl is
                // ignored.
                if !v.is_empty() && name.is_empty() {
                    name = v.to_string();
                }
            }
            ITEM_LEFT_CURL => break,
            _ => {
                return Err(ierr(
                    item,
                    format!("Unexpected item in fragment: {} {}", item.typ(), item.val()),
                ));
            }
        }
    }
    if name.is_empty() {
        return Err(iterr(it, "Empty fragment name"));
    }
    let mut gq = GraphQuery::default();
    godeep(it, &mut gq, 1)?;
    Ok(FragmentNode {
        name,
        gq,
        entered: false,
        exited: false,
        height: 0,
    })
}
