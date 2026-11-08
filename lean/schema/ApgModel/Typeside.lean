/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/

/-!
# Typesides

`SCHEMA-APG.md` definition 2.1: a typeside `(P, PV, F, ⟦·⟧)` names the primitive types, the values of each, and the
function symbols over them with their total interpretations. Every definition of the model is relative to one
typeside, so the core's results hold for every choice of primitives; the oracle's concrete typeside, with UGS's
primitives and their lexical forms, is the executable layer's.

A function symbol's arguments are given by position, one value for each index of its argument list, so a mapping
term applying it holds one argument term per position (`ApgModel.Core.Tm.app`).
-/

namespace ApgModel

/-- A typeside, `SCHEMA-APG.md` definition 2.1: primitive types `P`, the values `PV p` of each, function symbols `F`
with an arity `dom f → cod f` over `P`, and the total interpretation of each. -/
structure Typeside where
  /-- The primitive types. -/
  P : Type
  /-- The values of each primitive type. -/
  PV : P → Type
  /-- The function symbols. -/
  F : Type
  /-- A function symbol's argument types, in order. -/
  dom : F → List P
  /-- A function symbol's result type. -/
  cod : F → P
  /-- A function symbol's interpretation, a total function of one value per argument position. -/
  interp : (f : F) → ((i : Fin (dom f).length) → PV ((dom f).get i)) → PV (cod f)

end ApgModel
