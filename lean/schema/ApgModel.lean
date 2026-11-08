/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Typeside
import ApgModel.Core.Ty
import ApgModel.Core.FinSet
import ApgModel.Core.Value

/-!
# Algebraic property graphs

The Lean model of `specification/schema/SCHEMA-APG.md`, planned in `openspec/notes/prepare-apg-lean.md`: a binary
core over families of elements, in which the paper's results are proved by structural induction, and, in later
waves, a named layer over it, the catalog's schema, its constraints, history, migration operations, and the RDF and
GQL views with the test oracle that runs them.

This version holds the typeside (definition 2.1), the core's types and schemas (definitions 3.2, 4.1, 12.1), finite
sets, and values with their functoriality (definitions 5.1, 5.2; propositions 5.3, 12.2).
-/
