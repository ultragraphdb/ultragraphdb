/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Named.Value

/-!
# Proposition 3.3: the binary form

`SCHEMA-APG.md` proposition 3.3, part 1 for values: `V_E(t) ≅ V_E(⌜t⌝)` for every named type `t` and family `E`,
naturally in `E`. `toBin` sends a record's function from fields to the right-nested pairs of its binary form and a
variant's chosen field to the injections reaching it; `ofBin` reads them back. They are mutually inverse
(`ofBin_toBin`, `toBin_ofBin`), and `toBin` commutes with the action of a morphism of families (`toBin_map`). The
isomorphisms are those of associativity and unit of `×` and `+`, built field by field.

An APG on a named schema is an APG on its binary schema with every value translated (`NApg.toCore`, `NApg.ofCore`,
inverse both ways), and a family of functions is a homomorphism of the named APGs exactly when it is one of their
binary forms (`NApg.comm_iff`): every result of the core holds of the named layer through them. Part 2, for mapping
terms, is `ApgModel.Named.TermTranslate`.
-/

namespace ApgModel.Named

open Core

variable {T : Typeside} {L : Type} {E E' : L → Type}

/-- Proposition 3.3's isomorphism forth: a named value as a value of the binary form. -/
def toBin : (t : NTy T.P L) → NTy.V T E t → Core.V T E t.bin
  | .prim _, x => x
  | .lbl _, x => x
  | .record fs, f => toBinRec fs f
  | .variant fs, ⟨i, v⟩ => toBinVar fs i v
  | .list t, xs => List.map (toBin t) xs
  | .set t, s => FinSet.map (toBin t) s
where
  /-- A record's fields as right-nested pairs. -/
  toBinRec : (fs : List (Name × NTy T.P L)) → ((i : Fin fs.length) → NTy.V.VAt T E fs i) →
      Core.V T E (NTy.bin.binRec fs)
    | [], _ => ()
    | (_, t) :: fs, f => (toBin t (f ⟨0, Nat.zero_lt_succ _⟩), toBinRec fs (fun i => f i.succ))
  /-- A variant's chosen field as the injections reaching it. -/
  toBinVar : (fs : List (Name × NTy T.P L)) → (i : Fin fs.length) → NTy.V.VAt T E fs i →
      Core.V T E (NTy.bin.binVar fs)
    | [], i, _ => i.elim0
    | (_, t) :: _, ⟨0, _⟩, v => Sum.inl (toBin t v)
    | _ :: fs, ⟨i + 1, h⟩, v => Sum.inr (toBinVar fs ⟨i, Nat.lt_of_succ_lt_succ h⟩ v)

/-- Proposition 3.3's isomorphism back: a value of the binary form as a named value. -/
def ofBin : (t : NTy T.P L) → Core.V T E t.bin → NTy.V T E t
  | .prim _, x => x
  | .lbl _, x => x
  | .record fs, x => ofBinRec fs x
  | .variant fs, x => ofBinVar fs x
  | .list t, xs => List.map (ofBin t) xs
  | .set t, s => FinSet.map (ofBin t) s
where
  /-- Right-nested pairs as a record's function from fields. -/
  ofBinRec : (fs : List (Name × NTy T.P L)) → Core.V T E (NTy.bin.binRec fs) →
      (i : Fin fs.length) → NTy.V.VAt T E fs i
    | [], _, i => i.elim0
    | (_, t) :: _, (x, _), ⟨0, _⟩ => ofBin t x
    | _ :: fs, (_, rest), ⟨i + 1, h⟩ => ofBinRec fs rest ⟨i, Nat.lt_of_succ_lt_succ h⟩
  /-- Injections as a variant's chosen field. -/
  ofBinVar : (fs : List (Name × NTy T.P L)) → Core.V T E (NTy.bin.binVar fs) →
      (i : Fin fs.length) × NTy.V.VAt T E fs i
    | [], x => nomatch x
    | (_, t) :: _, Sum.inl x => ⟨⟨0, Nat.zero_lt_succ _⟩, ofBin t x⟩
    | _ :: fs, Sum.inr y => ⟨(ofBinVar fs y).1.succ, (ofBinVar fs y).2⟩

/-! ## The two round trips -/

/-- Proposition 3.3, part 1: back after forth is the identity. -/
theorem ofBin_toBin : (t : NTy T.P L) → (x : NTy.V T E t) → ofBin t (toBin t x) = x
  | .prim _, _ => rfl
  | .lbl _, _ => rfl
  | .record fs, f => funext (fields fs f)
  | .variant fs, ⟨i, v⟩ => alts fs i v
  | .list t, xs => by
    show List.map (ofBin t) (List.map (toBin t) xs) = xs
    rw [List.map_map]
    exact (List.map_congr_left (fun y _ => ofBin_toBin t y)).trans (List.map_id' xs)
  | .set t, s => by
    show FinSet.map (ofBin t) (FinSet.map (toBin t) s) = s
    rw [FinSet.map_map]
    exact (FinSet.map_congr (fun y => ofBin_toBin t y) s).trans (FinSet.map_id' s)
where
  fields : (fs : List (Name × NTy T.P L)) → (f : (i : Fin fs.length) → NTy.V.VAt T E fs i) →
      (i : Fin fs.length) → ofBin.ofBinRec fs (toBin.toBinRec fs f) i = f i
    | [], _, i => i.elim0
    | (_, t) :: _, f, ⟨0, _⟩ => ofBin_toBin t _
    | _ :: fs, f, ⟨i + 1, h⟩ => fields fs (fun i => f i.succ) ⟨i, Nat.lt_of_succ_lt_succ h⟩
  alts : (fs : List (Name × NTy T.P L)) → (i : Fin fs.length) → (v : NTy.V.VAt T E fs i) →
      ofBin.ofBinVar fs (toBin.toBinVar fs i v) = ⟨i, v⟩
    | [], i, _ => i.elim0
    | (n, t) :: tail, ⟨0, h⟩, v =>
      congrArg (Sigma.mk (β := NTy.V.VAt T E ((n, t) :: tail)) ⟨0, h⟩) (ofBin_toBin t v)
    | hd :: fs, ⟨i + 1, h⟩, v =>
      congrArg (fun p : (j : Fin fs.length) × NTy.V.VAt T E fs j =>
          (⟨p.1.succ, p.2⟩ : (j : Fin (hd :: fs).length) × NTy.V.VAt T E (hd :: fs) j))
        (alts fs ⟨i, Nat.lt_of_succ_lt_succ h⟩ v)

/-- Proposition 3.3, part 1: forth after back is the identity. -/
theorem toBin_ofBin : (t : NTy T.P L) → (y : Core.V T E t.bin) → toBin t (ofBin t y) = y
  | .prim _, _ => rfl
  | .lbl _, _ => rfl
  | .record fs, y => fields fs y
  | .variant fs, y => alts fs y
  | .list t, ys => by
    show List.map (toBin t) (List.map (ofBin t) ys) = ys
    rw [List.map_map]
    exact (List.map_congr_left (fun y _ => toBin_ofBin t y)).trans (List.map_id' ys)
  | .set t, s => by
    show FinSet.map (toBin t) (FinSet.map (ofBin t) s) = s
    rw [FinSet.map_map]
    exact (FinSet.map_congr (fun y => toBin_ofBin t y) s).trans (FinSet.map_id' s)
where
  fields : (fs : List (Name × NTy T.P L)) → (y : Core.V T E (NTy.bin.binRec fs)) →
      toBin.toBinRec fs (ofBin.ofBinRec fs y) = y
    | [], () => rfl
    | (_, t) :: fs, (x, rest) => by
      show (toBin t (ofBin t x), toBin.toBinRec fs (ofBin.ofBinRec fs rest)) = (x, rest)
      rw [toBin_ofBin t x, fields fs rest]
  alts : (fs : List (Name × NTy T.P L)) → (y : Core.V T E (NTy.bin.binVar fs)) →
      toBin.toBinVar fs (ofBin.ofBinVar fs y).1 (ofBin.ofBinVar fs y).2 = y
    | [], y => nomatch y
    | (_, t) :: _, Sum.inl x => by
      show Sum.inl (toBin t (ofBin t x)) = _
      rw [toBin_ofBin t x]
    | _ :: fs, Sum.inr y => by
      show Sum.inr (toBin.toBinVar fs (ofBin.ofBinVar fs y).1 (ofBin.ofBinVar fs y).2) = _
      rw [alts fs y]

/-! ## Naturality in the family -/

/-- Proposition 3.3, part 1: the isomorphism is natural in `E`, `⌜V_h(t)⌝ = V_h(⌜t⌝) ∘ ⌜·⌝`. -/
theorem toBin_map (h : ∀ l, E l → E' l) : (t : NTy T.P L) → (x : NTy.V T E t) →
    toBin t (NTy.V.map h t x) = Core.V.map h t.bin (toBin t x)
  | .prim _, _ => rfl
  | .lbl _, _ => rfl
  | .record fs, f => fields fs f
  | .variant fs, ⟨i, v⟩ => alts fs i v
  | .list t, xs => by
    show List.map (toBin t) (List.map (NTy.V.map h t) xs) = List.map (Core.V.map h t.bin) (List.map (toBin t) xs)
    rw [List.map_map, List.map_map]
    exact List.map_congr_left (fun y _ => toBin_map h t y)
  | .set t, s => by
    show FinSet.map (toBin t) (FinSet.map (NTy.V.map h t) s) =
      FinSet.map (Core.V.map h t.bin) (FinSet.map (toBin t) s)
    rw [FinSet.map_map, FinSet.map_map]
    exact FinSet.map_congr (fun y => toBin_map h t y) s
where
  fields : (fs : List (Name × NTy T.P L)) → (f : (i : Fin fs.length) → NTy.V.VAt T E fs i) →
      toBin.toBinRec fs (fun i => NTy.V.map.mapAt h fs i (f i)) =
        Core.V.map h (NTy.bin.binRec fs) (toBin.toBinRec fs f)
    | [], _ => rfl
    | (_, t) :: fs, f => by
      show (toBin t (NTy.V.map h t (f ⟨0, _⟩)), toBin.toBinRec fs (fun i => NTy.V.map.mapAt h fs i (f i.succ))) =
        (Core.V.map h t.bin (toBin t (f ⟨0, _⟩)), Core.V.map h (NTy.bin.binRec fs) (toBin.toBinRec fs (fun i => f i.succ)))
      rw [toBin_map h t, fields fs (fun i => f i.succ)]
  alts : (fs : List (Name × NTy T.P L)) → (i : Fin fs.length) → (v : NTy.V.VAt T E fs i) →
      toBin.toBinVar fs i (NTy.V.map.mapAt h fs i v) = Core.V.map h (NTy.bin.binVar fs) (toBin.toBinVar fs i v)
    | [], i, _ => i.elim0
    | (_, t) :: _, ⟨0, _⟩, v => by
      show Sum.inl (toBin t (NTy.V.map h t v)) = Sum.inl (Core.V.map h t.bin (toBin t v))
      rw [toBin_map h t v]
    | _ :: fs, ⟨i + 1, hi⟩, v => by
      show Sum.inr (toBin.toBinVar fs ⟨i, _⟩ (NTy.V.map.mapAt h fs ⟨i, _⟩ v)) =
        Sum.inr (Core.V.map h (NTy.bin.binVar fs) (toBin.toBinVar fs ⟨i, _⟩ v))
      rw [alts fs ⟨i, Nat.lt_of_succ_lt_succ hi⟩ v]

/-- `toBin` is injective, being invertible. -/
theorem toBin_injective (t : NTy T.P L) {x y : NTy.V T E t} (h : toBin t x = toBin t y) : x = y := by
  rw [← ofBin_toBin t x, h, ofBin_toBin t y]

/-! ## Named APGs and their binary forms -/

variable {S : NSchema T.P L}

/-- A named APG as an APG on the binary schema, every value translated. -/
def NApg.toCore (G : NApg S) : Core.Apg S.bin := ⟨G.E, fun l e => toBin (S.σ l) (G.υ l e)⟩

/-- An APG on the binary schema as a named APG, every value read back. -/
def NApg.ofCore (G : Core.Apg S.bin) : NApg S := ⟨G.E, fun l e => ofBin (S.σ l) (G.υ l e)⟩

theorem NApg.ofCore_toCore (G : NApg S) : NApg.ofCore G.toCore = G := by
  obtain ⟨E, υ⟩ := G
  show NApg.mk E (fun l e => ofBin (S.σ l) (toBin (S.σ l) (υ l e))) = NApg.mk E υ
  congr
  funext l e
  exact ofBin_toBin (S.σ l) (υ l e)

theorem NApg.toCore_ofCore (G : Core.Apg S.bin) : (NApg.ofCore G).toCore = G := by
  obtain ⟨E, υ⟩ := G
  show Core.Apg.mk E (fun l e => toBin (S.σ l) (ofBin (S.σ l) (υ l e))) = Core.Apg.mk E υ
  congr
  funext l e
  exact toBin_ofBin (S.σ l) (υ l e)

/-- A family of functions commutes with the named values exactly when it commutes with their binary forms: a
homomorphism of named APGs is a homomorphism of their binary forms, which carries the core's results to the named
layer. -/
theorem NApg.comm_iff (G G' : NApg S) (h : ∀ l, G.E l → G'.E l) (l : L) (e : G.E l) :
    G'.υ l (h l e) = NTy.V.map h (S.σ l) (G.υ l e) ↔
      G'.toCore.υ l (h l e) = Core.V.map h (S.σ l).bin (G.toCore.υ l e) := by
  show _ ↔ toBin (S.σ l) (G'.υ l (h l e)) = Core.V.map h (S.σ l).bin (toBin (S.σ l) (G.υ l e))
  rw [← toBin_map]
  exact ⟨fun w => w ▸ rfl, toBin_injective (S.σ l)⟩

end ApgModel.Named
