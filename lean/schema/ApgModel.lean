/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Typeside
import ApgModel.Core.Ty
import ApgModel.Core.FinSet
import ApgModel.Core.Value
import ApgModel.Core.Apg
import ApgModel.Core.Hom
import ApgModel.Core.Term
import ApgModel.Core.Model
import ApgModel.Core.Delta
import ApgModel.Core.DeltaCompose
import ApgModel.Named.Ty
import ApgModel.Named.Value
import ApgModel.Named.Translate
import ApgModel.Named.Term
import ApgModel.Named.TermTranslate
import ApgModel.Named.Apg
import ApgModel.Named.Collections
import ApgModel.Examples
import ApgModel.Catalog.Schema
import ApgModel.Catalog.Constraints
import ApgModel.Oracle.Typeside
import ApgModel.Oracle.Lexical
import ApgModel.Oracle.NQuads
import ApgModel.Oracle.Parse
import ApgModel.Oracle.Aviation
import ApgModel.Oracle.Gen5
import ApgModel.Oracle.Check
import ApgModel.Views.Rdf
import ApgModel.Views.RdfInverse
import ApgModel.Views.RdfRoundTrip
import ApgModel.Views.RdfShacl
import ApgModel.Views.Gql
import ApgModel.History
import ApgModel.Migration
import ApgModel.Theorems

/-!
# Algebraic property graphs

The Lean model of `specification/schema/SCHEMA-APG.md`, planned in `openspec/notes/prepare-apg-lean.md`: a binary
core over families of elements, in which the paper's results are proved by structural induction, and, in later
waves, a named layer over it, the catalog's schema, its constraints, history, migration operations, and the RDF and
GQL views with the test oracle that runs them.

This version holds waves L1 to L10; `ApgModel.Theorems` tables every statement with its Lean names and the comparison
with the paper's Coq development. The core: the typeside (definition 2.1); types and schemas (definitions 3.2, 4.1,
12.1); finite sets; values and their functoriality (propositions 5.3, 12.2); APGs in both forms and referential
integrity (propositions 6.3, 6.6); homomorphisms, isomorphisms and the coalgebra view (propositions 7.2, 8.1); and
mapping terms with their evaluation, renaming, substitution and the laws of `map` (proposition 8.7, definition 12.4);
models of the APG theory in generator form (proposition 8.4); and schema mappings with `Δ`, its functoriality, the
identity and composite mappings, finiteness and one origin (propositions 9.3, 9.4, 9.5). The named layer: named types
with field names, their decidable equality and element kinds (definitions 3.1, 3.2, 4.2); named values, APGs and mapping
terms; and proposition 3.3 between the layers, for values naturally in the family and for terms with evaluation
commuting with the translation. The executable layer: value terms checked by their denotation, complete for well-formed
types, with their references (definitions 5.4, 5.5); finite APGs over numbered elements, their validity and their
embedding into the core; role labels, symmetric navigation and unnesting (definitions 12.5 to 12.7); and examples 6.7
and 9.6, checked by the kernel. The catalog: its schema `S(K)` and the prefix-range type test (proposition 13.1); the
state constraints as violation queries, each sound and complete, and the transition constraints as predicates on
consecutive states (section 14). The views: the RDF view `R(G)` from templates natural in their references, monotone
under homomorphisms preserving names (definition 17.1, proposition 17.2); its inverse `R⁻¹`, giving back every element
of a valid APG written as the store writes it under its IRI, triple or pair (proposition 17.3); the SHACL shapes holding
on it (corollary 17.4). The oracle: its typeside, the lexical forms and N-Quads of `R(G)` with their parser, the
aviation schema's and a Gen5 subset's instance sets, and the checks every set passes before `apg_oracle` writes it.
History: the naming, injective where defined (section 15.1); the union of graphs (proposition 15.1); states, their
slices and commits, each commit keeping an invariant under which the live slice, every believed slice and the state are
closed under references (proposition 15.2). Migration: selection with its inclusion (definition 16.1), key lookup
(definition 16.2), extended mappings (definition 16.3), the fragment and its origins (proposition 16.4), coproducts
(definition 16.5) and history through a migration (proposition 16.6). The GQL view: `P(G)` from templates and its closed
graph type (definition 18.1), conformance under PG-Schema's strict, closed semantics and of its projection under MGQL's
`graphConformsSchema` (proposition 18.2), monotone under homomorphisms preserving names (proposition 18.3), determined
by the RDF view (proposition 18.4).
-/
