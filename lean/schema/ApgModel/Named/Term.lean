/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Named.Translate

/-!
# Named mapping terms

`SCHEMA-APG.md` definition 8.5 over named types, the grammar the specification writes: a record `{ n₁ = s₁, … }`, a
field `s . n`, an injection `n · s`, and a `case` with one branch per field of the variant, each branch binding the
field's value beside the context; `absurd s` is the `case` of the empty variant, which has no branches. A field is
given by its position in the type's field list, which a well-formed type's increasing names make the same as its
name (`NTy.fieldIndex` finds it).

`NTy.fieldAt fs i` is the type of the field at position `i`; `VAt.out` and `VAt.into` pass between a record's or
variant's field values, `NTy.VAt fs i`, and the values of that type, by the same recursion, so no cast enters a
definition. `ApgModel.Named.TermTranslate` translates these terms to the core's and proves that evaluation commutes
with the translation, proposition 3.3's part 2.
-/

namespace ApgModel.Named

variable {T : Typeside} {L : Type}

/-- The type of the field at position `i`. -/
def NTy.fieldAt {P L : Type} : (fs : List (Name × NTy P L)) → Fin fs.length → NTy P L
  | [], i => i.elim0
  | (_, t) :: _, ⟨0, _⟩ => t
  | _ :: fs, ⟨i + 1, h⟩ => fieldAt fs ⟨i, Nat.lt_of_succ_lt_succ h⟩

/-- The position of the field named `n`, the first if two share it. -/
def NTy.fieldIndex {P L : Type} (fs : List (Name × NTy P L)) (n : Name) : Option (Fin fs.length) :=
  match fs.findIdx? (fun f => f.1 == n) with
  | some i => if h : i < fs.length then some ⟨i, h⟩ else none
  | none => none

/-- A field's value as a value of its type. -/
def NTy.V.VAt.out {E : L → Type} : (fs : List (Name × NTy T.P L)) → (i : Fin fs.length) →
    NTy.V.VAt T E fs i → NTy.V T E (NTy.fieldAt fs i)
  | [], i, _ => i.elim0
  | (_, _) :: _, ⟨0, _⟩, x => x
  | _ :: fs, ⟨i + 1, h⟩, x => out fs ⟨i, Nat.lt_of_succ_lt_succ h⟩ x

/-- A value of a field's type as the field's value. -/
def NTy.V.VAt.into {E : L → Type} : (fs : List (Name × NTy T.P L)) → (i : Fin fs.length) →
    NTy.V T E (NTy.fieldAt fs i) → NTy.V.VAt T E fs i
  | [], i, _ => i.elim0
  | (_, _) :: _, ⟨0, _⟩, x => x
  | _ :: fs, ⟨i + 1, h⟩, x => into fs ⟨i, Nat.lt_of_succ_lt_succ h⟩ x

/-- A variable of type `t` in a named context, by its de Bruijn index. -/
inductive NVar {P L : Type} : List (NTy P L) → NTy P L → Type
  /-- The innermost variable. -/
  | here {Γ : List (NTy P L)} {t : NTy P L} : NVar (t :: Γ) t
  /-- A variable under one more binder. -/
  | there {Γ : List (NTy P L)} {t u : NTy P L} : NVar Γ t → NVar (u :: Γ) t

/-- The named mapping terms of `S` in the context `Γ` of type `t`, definitions 8.5 and 12.4. -/
inductive NTm (S : NSchema T.P L) : List (NTy T.P L) → NTy T.P L → Type
  /-- A variable. -/
  | var {Γ t} : NVar Γ t → NTm S Γ t
  /-- A primitive value. -/
  | lit {Γ} (p : T.P) (a : T.PV p) : NTm S Γ (.prim p)
  /-- A typeside function, one argument term per position. -/
  | app {Γ} (f : T.F) (args : (i : Fin (T.dom f).length) → NTm S Γ (.prim ((T.dom f).get i))) :
      NTm S Γ (.prim (T.cod f))
  /-- A record, `{ n₁ = s₁, …, nₖ = sₖ }`: one term for every field. -/
  | record {Γ} (fs : List (Name × NTy T.P L)) (args : (i : Fin fs.length) → NTm S Γ (NTy.fieldAt fs i)) :
      NTm S Γ (.record fs)
  /-- A record's field, `s . n`. -/
  | field {Γ} (fs : List (Name × NTy T.P L)) (i : Fin fs.length) : NTm S Γ (.record fs) →
      NTm S Γ (NTy.fieldAt fs i)
  /-- A variant injection, `n · s`. -/
  | inj {Γ} (fs : List (Name × NTy T.P L)) (i : Fin fs.length) : NTm S Γ (NTy.fieldAt fs i) →
      NTm S Γ (.variant fs)
  /-- A case on a variant, one branch per field, each binding the field's value beside the context. -/
  | case {Γ c} (fs : List (Name × NTy T.P L)) : NTm S Γ (.variant fs) →
      ((i : Fin fs.length) → NTm S (NTy.fieldAt fs i :: Γ) c) → NTm S Γ c
  /-- An element's value, the generator `δ_l`. -/
  | deref {Γ} (l : L) : NTm S Γ (.lbl l) → NTm S Γ (S.σ l)
  /-- The empty list. -/
  | nil {Γ a} : NTm S Γ (.list a)
  /-- A list with one more member in front. -/
  | cons {Γ a} : NTm S Γ a → NTm S Γ (.list a) → NTm S Γ (.list a)
  /-- Two lists one after the other. -/
  | append {Γ a} : NTm S Γ (.list a) → NTm S Γ (.list a) → NTm S Γ (.list a)
  /-- `map(s, y ⇒ u)` over a list. -/
  | map {Γ a b} : NTm S Γ (.list a) → NTm S [a] b → NTm S Γ (.list b)
  /-- `filter(s, y ⇒ q)` over a list. -/
  | filter {Γ a} : NTm S Γ (.list a) → NTm S [a] NTy.bool → NTm S Γ (.list a)
  /-- The empty set. -/
  | empty {Γ a} : NTm S Γ (.set a)
  /-- A set with one more member. -/
  | insert {Γ a} : NTm S Γ a → NTm S Γ (.set a) → NTm S Γ (.set a)
  /-- The union of two sets. -/
  | union {Γ a} : NTm S Γ (.set a) → NTm S Γ (.set a) → NTm S Γ (.set a)
  /-- `map(s, y ⇒ u)` over a set. -/
  | mapSet {Γ a b} : NTm S Γ (.set a) → NTm S [a] b → NTm S Γ (.set b)
  /-- `filter(s, y ⇒ q)` over a set. -/
  | filterSet {Γ a} : NTm S Γ (.set a) → NTm S [a] NTy.bool → NTm S Γ (.set a)

variable {S : NSchema T.P L}

/-- `absurd s`, out of the empty variant: the case with no branches. -/
def NTm.absurd {Γ : List (NTy T.P L)} {c : NTy T.P L} (s : NTm S Γ (.variant [])) : NTm S Γ c :=
  .case [] s (fun i => i.elim0)

/-- An environment of a named context. -/
def NEnv (T : Typeside) {L : Type} (E : L → Type) (Γ : List (NTy T.P L)) : Type := ∀ t, NVar Γ t → NTy.V T E t

/-- The environment of the empty context. -/
def NEnv.nil {E : L → Type} : NEnv T E [] := fun _ v => nomatch v

/-- An environment extended by a value for a new innermost variable. -/
def NEnv.cons {E : L → Type} {a : NTy T.P L} {Γ : List (NTy T.P L)} (x : NTy.V T E a) (ρ : NEnv T E Γ) :
    NEnv T E (a :: Γ)
  | _, .here => x
  | _, .there v => ρ _ v

/-- The environment of a mapping term's one argument. -/
def NEnv.one {E : L → Type} {a : NTy T.P L} (x : NTy.V T E a) : NEnv T E [a] := NEnv.cons x NEnv.nil

/-- A named Boolean as a `Bool`: `false` the first member, `true` the second. -/
def nasBool {E : L → Type} (b : NTy.V T E NTy.bool) : Bool := b.1.val == 1

/-- `⟦s⟧_G` for a named term, definition 8.6. -/
def NTm.eval (G : NApg S) : {Γ : List (NTy T.P L)} → {t : NTy T.P L} → NTm S Γ t → NEnv T G.E Γ → NTy.V T G.E t
  | _, _, .var v, ρ => ρ _ v
  | _, _, .lit _ a, _ => a
  | _, _, .app f args, ρ => T.interp f (fun i => (args i).eval G ρ)
  | _, _, .record fs args, ρ => fun i => NTy.V.VAt.into fs i ((args i).eval G ρ)
  | _, _, .field fs i s, ρ => NTy.V.VAt.out fs i ((s.eval G ρ) i)
  | _, _, .inj fs i s, ρ => ⟨i, NTy.V.VAt.into fs i (s.eval G ρ)⟩
  | _, _, .case fs s br, ρ =>
    match s.eval G ρ with
    | ⟨i, v⟩ => (br i).eval G (NEnv.cons (NTy.V.VAt.out fs i v) ρ)
  | _, _, .deref l s, ρ => G.υ l (s.eval G ρ)
  | _, _, .nil, _ => List.nil
  | _, _, .cons x xs, ρ => List.cons (x.eval G ρ) (xs.eval G ρ)
  | _, _, .append xs ys, ρ => List.append (xs.eval G ρ) (ys.eval G ρ)
  | _, _, .map s u, ρ => List.map (fun y => u.eval G (NEnv.one y)) (s.eval G ρ)
  | _, _, .filter s q, ρ => List.filter (fun y => nasBool (q.eval G (NEnv.one y))) (s.eval G ρ)
  | _, _, .empty, _ => Core.FinSet.empty
  | _, _, .insert x s, ρ => Core.FinSet.insert (x.eval G ρ) (s.eval G ρ)
  | _, _, .union s s', ρ => Core.FinSet.union (s.eval G ρ) (s'.eval G ρ)
  | _, _, .mapSet s u, ρ => Core.FinSet.map (fun y => u.eval G (NEnv.one y)) (s.eval G ρ)
  | _, _, .filterSet s q, ρ => Core.FinSet.filter (fun y => nasBool (q.eval G (NEnv.one y))) (s.eval G ρ)

end ApgModel.Named
