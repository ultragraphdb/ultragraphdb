/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Named.Ty

/-!
# Named values

`SCHEMA-APG.md` definitions 5.1 and 5.2 over named types: a record's value is one value for every field,
`∏_{n ∈ dom φ} V_E(φ(n))`, a function from the record's fields; a variant's value is one field with a value for it,
`∑_{n ∈ dom φ} V_E(φ(n))`. A field is given by its position in the type's field list, which a well-formed type's
strictly increasing names make the same as giving it by name. `NTy.VAt fs i` is the values of the field at `i`.
-/

namespace ApgModel.Named

variable (T : Typeside) {L : Type}

/-- `V_E(t)` for a named type, definition 5.1: a record's value a function from its fields, a variant's a field with a
value. -/
def NTy.V (E : L → Type) : NTy T.P L → Type
  | .prim p => T.PV p
  | .lbl l => E l
  | .record fs => (i : Fin fs.length) → VAt fs i
  | .variant fs => (i : Fin fs.length) × VAt fs i
  | .list t => List (NTy.V E t)
  | .set t => Core.FinSet (NTy.V E t)
where
  /-- The values of the field at position `i`. -/
  VAt : (fs : List (Name × NTy T.P L)) → Fin fs.length → Type
    | [], i => i.elim0
    | (_, t) :: _, ⟨0, _⟩ => NTy.V E t
    | _ :: fs, ⟨i + 1, h⟩ => VAt fs ⟨i, Nat.lt_of_succ_lt_succ h⟩

variable {T}

/-- `V_h(t)` for a named type, definition 5.2: `h` at a reference, field by field on a record, the chosen field's value
on a variant. -/
def NTy.V.map {E E' : L → Type} (h : ∀ l, E l → E' l) : (t : NTy T.P L) → NTy.V T E t → NTy.V T E' t
  | .prim _, x => x
  | .lbl l, x => h l x
  | .record fs, f => fun i => mapAt fs i (f i)
  | .variant fs, ⟨i, v⟩ => ⟨i, mapAt fs i v⟩
  | .list t, xs => List.map (NTy.V.map h t) xs
  | .set t, s => Core.FinSet.map (NTy.V.map h t) s
where
  /-- `V_h` on the field at position `i`. -/
  mapAt : (fs : List (Name × NTy T.P L)) → (i : Fin fs.length) → NTy.V.VAt T E fs i → NTy.V.VAt T E' fs i
    | [], i, _ => i.elim0
    | (_, t) :: _, ⟨0, _⟩, x => NTy.V.map h t x
    | _ :: fs, ⟨i + 1, hi⟩, x => mapAt fs ⟨i, Nat.lt_of_succ_lt_succ hi⟩ x

/-- An APG on a named schema: definition 6.1 with named types. -/
structure NApg (S : NSchema T.P L) where
  /-- The elements of each label. -/
  E : L → Type
  /-- The value of each element, of its label's named type. -/
  υ : (l : L) → E l → NTy.V T E (S.σ l)

end ApgModel.Named
