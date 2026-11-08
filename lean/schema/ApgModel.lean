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
import ApgModel.Named.Ty
import ApgModel.Named.Value
import ApgModel.Named.Translate
import ApgModel.Named.Term
import ApgModel.Named.TermTranslate

/-!
# Algebraic property graphs

The Lean model of `specification/schema/SCHEMA-APG.md`, planned in `openspec/notes/prepare-apg-lean.md`: a binary
core over families of elements, in which the paper's results are proved by structural induction, and, in later
waves, a named layer over it, the catalog's schema, its constraints, history, migration operations, and the RDF and
GQL views with the test oracle that runs them.

This version holds waves L1, L2 and L4. The core: the typeside (definition 2.1); types and schemas (definitions 3.2, 4.1,
12.1); finite sets; values and their functoriality (propositions 5.3, 12.2); APGs in both forms and referential
integrity (propositions 6.3, 6.6); homomorphisms, isomorphisms and the coalgebra view (propositions 7.2, 8.1); and
mapping terms with their evaluation, renaming, substitution and the laws of `map` (proposition 8.7, definition 12.4).
The named layer: named types with field names, their decidable equality and element kinds (definitions 3.1, 3.2,
4.2); named values, APGs and mapping terms; and proposition 3.3 between the layers, for values naturally in the
family and for terms with evaluation commuting with the translation.
-/
