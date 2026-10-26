/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{GraphNameRef, NamedNodeRef, NamedOrBlankNodeRef, TermRef};
use ultragraphdb_core_catalog::Catalog;
use ultragraphdb_core_mapping::{Cardinality, PredicateDeclaration};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_store::{ListCache, Store};

use crate::errors::changeset_error::ChangesetError;
use crate::types::changeset_write::{ChangesetWrite, kind_of};

/// The statement a change names, resolved: its graph, subject and predicate UIDs and its object's encoding.
pub(crate) struct Resolved {
    pub graph: u64,
    pub subject: u64,
    pub predicate: u64,
    pub object: EncodedTerm,
}

impl<S, C> ChangesetWrite<'_, S, C>
where
    S: Store,
    C: ListCache<Value = CachedList>,
{
    /// The UIDs and encoding of a statement's terms; a literal that cannot be encoded is refused naming `predicate`.
    pub(crate) fn resolve(
        &mut self,
        graph: GraphNameRef<'_>,
        subject: NamedOrBlankNodeRef<'_>,
        predicate: NamedNodeRef<'_>,
        object: TermRef<'_>,
    ) -> Result<Resolved, ChangesetError> {
        Ok(Resolved {
            graph: self.graph(graph)?,
            subject: self.node(subject)?,
            predicate: self.iri(predicate)?,
            object: self.term(predicate, object)?,
        })
    }

    /// The UIDs and encoding of a statement's terms when the store holds every one, allocating none: a statement
    /// naming a term it does not hold is not held.
    pub(crate) fn find(
        &mut self,
        graph: GraphNameRef<'_>,
        subject: NamedOrBlankNodeRef<'_>,
        predicate: NamedNodeRef<'_>,
        object: TermRef<'_>,
    ) -> Result<Option<Resolved>, ChangesetError> {
        let (Some(graph), Some(subject), Some(uid), Some(object)) = (
            self.find_graph(graph)?,
            self.find_term(predicate, subject.into())?
                .and_then(|e| e.as_uid()),
            self.find_iri(predicate)?,
            self.find_term(predicate, object)?,
        ) else {
            return Ok(None);
        };
        Ok(Some(Resolved {
            graph,
            subject,
            predicate: uid,
            object,
        }))
    }

    /// Inserts a triple, held once: checked against the catalog, replacing the subject's other objects of a
    /// single-valued predicate. The subject's list is read only when the predicate is single-valued, counted, or
    /// carries statement IDs, whose postings a blind insert would overwrite.
    pub(crate) fn insert(
        &mut self,
        graph: GraphNameRef<'_>,
        subject: NamedOrBlankNodeRef<'_>,
        predicate: NamedNodeRef<'_>,
        object: TermRef<'_>,
    ) -> Result<(), ChangesetError> {
        let r = self.resolve(graph, subject, predicate, object)?;
        let declaration = self.checked(&r, predicate, object)?;
        let reads = declaration.cardinality() == Cardinality::Single
            || declaration.count_index()
            || self.catalog.has_statements(r.predicate);
        if reads && self.held(&r, &declaration)?.is_some() {
            return Ok(());
        }
        self.add(&r, &declaration, object, &[])
    }

    /// The declaration a statement is written under, the statement checked against it: its object's kind and
    /// datatype, and a unique value no other subject holds in the graph.
    pub(crate) fn checked(
        &mut self,
        r: &Resolved,
        predicate: NamedNodeRef<'_>,
        object: TermRef<'_>,
    ) -> Result<PredicateDeclaration, ChangesetError> {
        let declaration = self.declaration(r.predicate, predicate)?;
        Catalog::check_object(&declaration, object)?;
        if declaration.unique() {
            let holder = self
                .write
                .subjects(r.graph, r.predicate, &r.object)?
                .into_iter()
                .find(|h| *h != r.subject);
            if let Some(holder) = holder {
                let holder = self.decode(&EncodedTerm::uid(holder))?;
                let value = self.decode(&r.object)?;
                return Err(ChangesetError::Unique(
                    predicate.as_str(),
                    value.to_string(),
                    holder.to_string(),
                ));
            }
        }
        Ok(declaration)
    }

    /// The statement IDs of the triple when the subject holds it, after removing the subject's other objects when
    /// the predicate is single-valued: a read of the subject's list.
    pub(crate) fn held(
        &mut self,
        r: &Resolved,
        declaration: &PredicateDeclaration,
    ) -> Result<Option<Vec<u64>>, ChangesetError> {
        let held = self.write.objects(r.graph, r.subject, r.predicate)?;
        if declaration.cardinality() == Cardinality::Single {
            for (other, _) in held.iter().filter(|(o, _)| *o != r.object) {
                self.remove(r.graph, r.subject, r.predicate, declaration, other)?;
            }
        }
        Ok(held
            .into_iter()
            .find(|(o, _)| *o == r.object)
            .map(|(_, ids)| ids))
    }

    /// Adds a triple the subject does not hold, its posting carrying `statement_ids`, with its index tokens and
    /// counts.
    pub(crate) fn add(
        &mut self,
        r: &Resolved,
        declaration: &PredicateDeclaration,
        object: TermRef<'_>,
        statement_ids: &[u64],
    ) -> Result<(), ChangesetError> {
        let counts = if declaration.count_index() {
            Some(self.counts(r.graph, r.subject, r.predicate, &r.object)?)
        } else {
            None
        };
        self.write.insert(
            r.graph,
            r.subject,
            r.predicate,
            &r.object,
            statement_ids,
            &kind_of(object),
        );
        if let Some((objects, subjects)) = counts {
            self.move_counts(
                r.graph,
                r.subject,
                r.predicate,
                &r.object,
                (objects, objects + 1),
                (subjects, subjects + 1),
            );
        }
        if let TermRef::Literal(literal) = object {
            for (tokenizer, token) in self.tokens(declaration, literal)? {
                self.index(r.graph, r.subject, r.predicate, tokenizer, token, true);
            }
        }
        Ok(())
    }

    /// Removes `object` of the subject's predicate when held: its statement records, its counts, and each index
    /// token no remaining literal of the subject's predicate produces. Whether it was held.
    pub(crate) fn remove(
        &mut self,
        graph: u64,
        subject: u64,
        predicate: u64,
        declaration: &PredicateDeclaration,
        object: &EncodedTerm,
    ) -> Result<bool, ChangesetError> {
        // Only a held object is decoded: a literal no statement holds may have no dictionary entry.
        if !self
            .write
            .objects(graph, subject, predicate)?
            .iter()
            .any(|(o, _)| o == object)
        {
            return Ok(false);
        }
        let term = self.decode(object)?;
        let counts = if declaration.count_index() {
            Some(self.counts(graph, subject, predicate, object)?)
        } else {
            None
        };
        let Some(ids) =
            self.write
                .delete(graph, subject, predicate, object, &kind_of(term.as_ref()))?
        else {
            return Ok(false);
        };
        ids.into_iter().for_each(|id| {
            self.records.insert((graph, id), None);
        });
        if let Some((objects, subjects)) = counts {
            self.move_counts(
                graph,
                subject,
                predicate,
                object,
                (objects, objects - 1),
                (subjects, subjects - 1),
            );
        }
        if let oxrdf::Term::Literal(literal) = &term
            && !declaration.tokenizers().is_empty()
        {
            let mut stale = self.tokens(declaration, literal.as_ref())?;
            for (remaining, _) in self.write.objects(graph, subject, predicate)? {
                if let oxrdf::Term::Literal(kept) = self.decode(&remaining)? {
                    for token in self.tokens(declaration, kept.as_ref())? {
                        stale.remove(&token);
                    }
                }
            }
            for (tokenizer, token) in stale {
                self.index(graph, subject, predicate, tokenizer, token, false);
            }
        }
        Ok(true)
    }

    /// Deletes a triple and every statement that holds it.
    pub(crate) fn delete(
        &mut self,
        graph: GraphNameRef<'_>,
        subject: NamedOrBlankNodeRef<'_>,
        predicate: NamedNodeRef<'_>,
        object: TermRef<'_>,
    ) -> Result<(), ChangesetError> {
        let Some(r) = self.find(graph, subject, predicate, object)? else {
            return Ok(());
        };
        let declaration = self.behaviour(r.predicate, predicate);
        self.remove(r.graph, r.subject, r.predicate, &declaration, &r.object)?;
        Ok(())
    }

    /// Deletes every object of the subject's predicate in the graph.
    pub(crate) fn delete_pattern(
        &mut self,
        graph: GraphNameRef<'_>,
        subject: NamedOrBlankNodeRef<'_>,
        predicate: NamedNodeRef<'_>,
    ) -> Result<(), ChangesetError> {
        let (Some(graph), Some(subject), Some(uid)) = (
            self.find_graph(graph)?,
            self.find_term(predicate, subject.into())?
                .and_then(|e| e.as_uid()),
            self.find_iri(predicate)?,
        ) else {
            return Ok(());
        };
        let declaration = self.behaviour(uid, predicate);
        for (object, _) in self.write.objects(graph, subject, uid)? {
            self.remove(graph, subject, uid, &declaration, &object)?;
        }
        Ok(())
    }
}
