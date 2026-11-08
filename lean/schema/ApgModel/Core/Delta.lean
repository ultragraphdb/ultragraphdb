/-
 SPDX-License-Identifier: Apache-2.0
 Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
-/
import ApgModel.Core.Model

/-!
# Schema mappings and `Δ`

`SCHEMA-APG.md` definitions 9.1 and 9.2, and propositions 9.3, 9.4 and 9.5.

A mapping `Φ : S → S'` gives each label `l` of `S` an image type `τ_l` over `S'`'s labels and a mapping term
`x : τ_l ⊢ t_l : Φ̂(σ(l))`, `Φ̂` replacing each label by its image (`Ty.subst`). `Δ_Φ` carries an APG on `S'` back to
`S`: the elements of `l` are the values of `τ_l`, and an element's value is `t_l` evaluated on it, read through
`ι_t : V_{E'}(Φ̂(t)) → V_E(t)`, the identity at labels and structural elsewhere (`iota`, inverse `iotaInv`).

- `Δ_Φ` is a functor, acting on a homomorphism by `V_h(τ_l)` (`deltaHom`, `deltaHom_id`, `deltaHom_comp`); the first
  half of proposition 9.3, resting on evaluation commuting with homomorphisms (`Tm.eval_hom`).
- Over a finite APG whose images hold no collection and only finite primitive types, every label of `Δ_Φ(G')` has
  finitely many elements (`delta_finite`), proposition 9.4.
- Under a label image `lbl l'` an element of `Δ_Φ(G')(l)` is an element of `l'` (`delta_label_image`), and under a
  variant of labels it is one element of one of them, its origin (`delta_one_origin`), proposition 9.5.

The composition of mappings, proposition 9.3's second half, is `ApgModel.Core.DeltaCompose`.
-/

namespace ApgModel.Core

/-! ## Substitution of types -/

/-- `Φ̂(t)`: each label `l` of `t` replaced by `τ l`, structural elsewhere (definition 9.1). -/
def Ty.subst {P L L' : Type} (τ : L → Ty P L') : Ty P L → Ty P L'
  | .zero => .zero
  | .one => .one
  | .sum a b => .sum (a.subst τ) (b.subst τ)
  | .prod a b => .prod (a.subst τ) (b.subst τ)
  | .prim p => .prim p
  | .lbl l => τ l
  | .list t => .list (t.subst τ)
  | .set t => .set (t.subst τ)

theorem Ty.subst_subst {P L L' L'' : Type} (τ : L → Ty P L') (τ' : L' → Ty P L'') (t : Ty P L) :
    (t.subst τ).subst τ' = t.subst (fun l => (τ l).subst τ') := by
  induction t with
  | zero | one | prim _ | lbl _ => rfl
  | sum a b iha ihb | prod a b iha ihb => simp only [Ty.subst, iha, ihb]
  | list t ih | set t ih => simp only [Ty.subst, ih]

theorem Ty.subst_lbl {P L : Type} (t : Ty P L) : t.subst .lbl = t := by
  induction t with
  | zero | one | prim _ | lbl _ => rfl
  | sum a b iha ihb | prod a b iha ihb => simp only [Ty.subst, iha, ihb]
  | list t ih | set t ih => simp only [Ty.subst, ih]

variable {T : Typeside} {L L' : Type}

/-! ## Mappings and `Δ` -/

/-- A schema mapping `Φ : S → S'`, definition 9.1: an image type and a mapping term for every label of `S`. -/
structure Mapping (S : Schema T.P L) (S' : Schema T.P L') where
  /-- The label's image, `τ_l`. -/
  τ : L → Ty T.P L'
  /-- The label's term, `x : τ_l ⊢ t_l : Φ̂(σ(l))`. -/
  t : (l : L) → Tm S' [τ l] ((S.σ l).subst τ)

/-- `ι_t : V_{E'}(Φ̂(t)) → V_E(t)`, `E(l) = V_{E'}(τ_l)`: the identity at labels, structural elsewhere. -/
def iota (τ : L → Ty T.P L') (E' : L' → Type) : (t : Ty T.P L) → V T E' (t.subst τ) → V T (fun l => V T E' (τ l)) t
  | .zero, x => nomatch x
  | .one, x => x
  | .sum a _, Sum.inl x => Sum.inl (iota τ E' a x)
  | .sum _ b, Sum.inr y => Sum.inr (iota τ E' b y)
  | .prod a b, (x, y) => (iota τ E' a x, iota τ E' b y)
  | .prim _, x => x
  | .lbl _, x => x
  | .list t, xs => List.map (iota τ E' t) xs
  | .set t, s => FinSet.map (iota τ E' t) s

/-- The inverse of `ι_t`. -/
def iotaInv (τ : L → Ty T.P L') (E' : L' → Type) : (t : Ty T.P L) → V T (fun l => V T E' (τ l)) t → V T E' (t.subst τ)
  | .zero, x => nomatch x
  | .one, x => x
  | .sum a _, Sum.inl x => Sum.inl (iotaInv τ E' a x)
  | .sum _ b, Sum.inr y => Sum.inr (iotaInv τ E' b y)
  | .prod a b, (x, y) => (iotaInv τ E' a x, iotaInv τ E' b y)
  | .prim _, x => x
  | .lbl _, x => x
  | .list t, xs => List.map (iotaInv τ E' t) xs
  | .set t, s => FinSet.map (iotaInv τ E' t) s

theorem iotaInv_iota (τ : L → Ty T.P L') (E' : L' → Type) (t : Ty T.P L) (x : V T E' (t.subst τ)) :
    iotaInv τ E' t (iota τ E' t x) = x := by
  induction t with
  | zero => exact nomatch x
  | one | prim _ | lbl _ => rfl
  | sum a b iha ihb =>
    cases x with
    | inl x => exact congrArg Sum.inl (iha x)
    | inr y => exact congrArg Sum.inr (ihb y)
  | prod a b iha ihb =>
    obtain ⟨x, y⟩ := x
    exact Prod.ext (iha x) (ihb y)
  | list t ih =>
    show List.map (iotaInv τ E' t) (List.map (iota τ E' t) x) = x
    rw [List.map_map]
    exact (List.map_congr_left (fun y _ => ih y)).trans (List.map_id' x)
  | set t ih =>
    show FinSet.map (iotaInv τ E' t) (FinSet.map (iota τ E' t) x) = x
    rw [FinSet.map_map]
    exact (FinSet.map_congr ih x).trans (FinSet.map_id' x)

theorem iota_iotaInv (τ : L → Ty T.P L') (E' : L' → Type) (t : Ty T.P L) (y : V T (fun l => V T E' (τ l)) t) :
    iota τ E' t (iotaInv τ E' t y) = y := by
  induction t with
  | zero => exact nomatch y
  | one | prim _ | lbl _ => rfl
  | sum a b iha ihb =>
    cases y with
    | inl x => exact congrArg Sum.inl (iha x)
    | inr y => exact congrArg Sum.inr (ihb y)
  | prod a b iha ihb =>
    obtain ⟨x, y⟩ := y
    exact Prod.ext (iha x) (ihb y)
  | list t ih =>
    show List.map (iota τ E' t) (List.map (iotaInv τ E' t) y) = y
    rw [List.map_map]
    exact (List.map_congr_left (fun x _ => ih x)).trans (List.map_id' y)
  | set t ih =>
    show FinSet.map (iota τ E' t) (FinSet.map (iotaInv τ E' t) y) = y
    rw [FinSet.map_map]
    exact (FinSet.map_congr ih y).trans (FinSet.map_id' y)

/-- `ι` is natural: a morphism of `E'` acts on `Φ̂(t)` as its image `V_h(τ_l)` acts on `t`. -/
theorem iota_map (τ : L → Ty T.P L') {E₁ E₂ : L' → Type} (h : ∀ l, E₁ l → E₂ l) (t : Ty T.P L)
    (x : V T E₁ (t.subst τ)) :
    iota τ E₂ t (V.map h (t.subst τ) x) = V.map (fun l => V.map h (τ l)) t (iota τ E₁ t x) := by
  induction t with
  | zero => exact nomatch x
  | one | prim _ | lbl _ => rfl
  | sum a b iha ihb =>
    cases x with
    | inl x => exact congrArg Sum.inl (iha x)
    | inr y => exact congrArg Sum.inr (ihb y)
  | prod a b iha ihb =>
    obtain ⟨x, y⟩ := x
    exact Prod.ext (iha x) (ihb y)
  | list t ih =>
    show List.map (iota τ E₂ t) (List.map (V.map h (t.subst τ)) x) =
      List.map (V.map (fun l => V.map h (τ l)) t) (List.map (iota τ E₁ t) x)
    exact list_map_comm ih x
  | set t ih =>
    show FinSet.map (iota τ E₂ t) (FinSet.map (V.map h (t.subst τ)) x) =
      FinSet.map (V.map (fun l => V.map h (τ l)) t) (FinSet.map (iota τ E₁ t) x)
    exact FinSet.map_comm ih x

variable {S : Schema T.P L} {S' : Schema T.P L'}

/-- `Δ_Φ(G')`, definition 9.2: the elements of `l` are the values of `τ_l`, each with `t_l`'s value through `ι`. -/
def Mapping.delta (Φ : Mapping S S') (G' : Apg S') : Apg S where
  E l := V T G'.E (Φ.τ l)
  υ l v := iota Φ.τ G'.E (S.σ l) ((Φ.t l).eval G' (Env.one v))

theorem Env.map_one {E E' : L' → Type} (h : ∀ l, E l → E' l) {a : Ty T.P L'} (x : V T E a) :
    Env.map h (Env.one x) = Env.one (V.map h a x) := by
  funext t v
  cases v with
  | here => rfl
  | there v => exact nomatch v

/-- `Δ_Φ(h)`, definition 9.2 on a homomorphism: `V_h(τ_l)` at each label. -/
def Mapping.deltaHom (Φ : Mapping S S') {G₁ G₂ : Apg S'} (h : Hom G₁ G₂) : Hom (Φ.delta G₁) (Φ.delta G₂) where
  h l v := V.map h.h (Φ.τ l) v
  comm l v := by
    show iota Φ.τ G₂.E (S.σ l) ((Φ.t l).eval G₂ (Env.one (V.map h.h (Φ.τ l) v))) =
      V.map (fun l => V.map h.h (Φ.τ l)) (S.σ l) (iota Φ.τ G₁.E (S.σ l) ((Φ.t l).eval G₁ (Env.one v)))
    rw [← Env.map_one, ← Tm.eval_hom h, iota_map]

/-- Proposition 9.3: `Δ_Φ` preserves identities. -/
theorem Mapping.deltaHom_id (Φ : Mapping S S') (G' : Apg S') : Φ.deltaHom (Hom.id G') = Hom.id (Φ.delta G') :=
  Hom.ext (funext fun l => funext fun v => V.map_id (Φ.τ l) v)

/-- Proposition 9.3: `Δ_Φ` preserves composition. -/
theorem Mapping.deltaHom_comp (Φ : Mapping S S') {G₁ G₂ G₃ : Apg S'} (f : Hom G₁ G₂) (g : Hom G₂ G₃) :
    Φ.deltaHom (f.comp g) = (Φ.deltaHom f).comp (Φ.deltaHom g) :=
  Hom.ext (funext fun l => funext fun v => V.map_comp f.h g.h (Φ.τ l) v)

/-! ## Proposition 9.5: one origin -/

/-- A variant of labels in binary form, `lbl l₁ + (lbl l₂ + ( … + 0))`. -/
def labelSum {P L : Type} : List L → Ty P L
  | [] => .zero
  | l :: ls => .sum (.lbl l) (labelSum ls)

/-- The values of a variant of labels are the elements of its labels, each with the position of its label. -/
def labelSumBij (E : L' → Type) : (ls : List L') → Bij (V T E (labelSum ls)) ((i : Fin ls.length) × E (ls.get i))
  | [] => ⟨(fun x => nomatch x), (fun p => p.1.elim0), (fun x => nomatch x), (fun p => p.1.elim0)⟩
  | l :: ls =>
    let b := labelSumBij E ls
    { toFun := fun
        | Sum.inl x => ⟨⟨0, Nat.zero_lt_succ _⟩, x⟩
        | Sum.inr y => ⟨(b.toFun y).1.succ, (b.toFun y).2⟩
      invFun := fun
        | ⟨⟨0, _⟩, x⟩ => Sum.inl x
        | ⟨⟨i + 1, h⟩, x⟩ => Sum.inr (b.invFun ⟨⟨i, Nat.lt_of_succ_lt_succ h⟩, x⟩)
      left := fun
        | Sum.inl _ => rfl
        | Sum.inr y => by
          show Sum.inr (b.invFun ⟨(b.toFun y).1, (b.toFun y).2⟩) = Sum.inr y
          rw [b.left y]
      right := fun
        | ⟨⟨0, _⟩, _⟩ => rfl
        | ⟨⟨i + 1, h⟩, x⟩ => by
          show (⟨(b.toFun (b.invFun ⟨⟨i, _⟩, x⟩)).1.succ, (b.toFun (b.invFun ⟨⟨i, _⟩, x⟩)).2⟩ :
            (j : Fin (l :: ls).length) × E ((l :: ls).get j)) = ⟨⟨i + 1, h⟩, x⟩
          rw [b.right]
          rfl }

/-- Proposition 9.5, a label image: under `τ_l = lbl l'`, the elements of `Δ_Φ(G')(l)` are those of `G'(l')`. -/
theorem Mapping.delta_label_image (Φ : Mapping S S') (G' : Apg S') {l : L} {l' : L'} (h : Φ.τ l = .lbl l') :
    (Φ.delta G').E l = G'.E l' := by
  show V T G'.E (Φ.τ l) = G'.E l'
  rw [h]
  rfl

/-- Proposition 9.5, a variant of labels: under `τ_l = var { n₁ ↦ lbl l'₁, … }`, each element of `Δ_Φ(G')(l)` is
one element of one `G'(l'ᵢ)`, its origin, and `|Δ_Φ(G')(l)| = |G'(l'₁)| + … + |G'(l'ₖ)|`. -/
def Mapping.delta_one_origin (Φ : Mapping S S') (G' : Apg S') {l : L} {ls : List L'} (h : Φ.τ l = labelSum ls) :
    Bij ((Φ.delta G').E l) ((i : Fin ls.length) × G'.E (ls.get i)) := by
  show Bij (V T G'.E (Φ.τ l)) _
  rw [h]
  exact labelSumBij G'.E ls

/-! ## Proposition 9.4: finiteness -/

/-- A type is finite when a list enumerates it. -/
def Enumerable (α : Type) : Prop := ∃ l : List α, ∀ a, a ∈ l

/-- Every primitive type occurring in `t` satisfies `Q`. -/
def Ty.PrimsSatisfy {P L : Type} (Q : P → Prop) : Ty P L → Prop
  | .zero | .one | .lbl _ => True
  | .prim p => Q p
  | .sum a b | .prod a b => a.PrimsSatisfy Q ∧ b.PrimsSatisfy Q
  | .list t | .set t => t.PrimsSatisfy Q

/-- The pairs of two lists. -/
def prodList {α β : Type} : List α → List β → List (α × β)
  | [], _ => []
  | a :: as, bs => bs.map (Prod.mk a) ++ prodList as bs

theorem mem_prodList {α β : Type} {a : α} {b : β} : {as : List α} → {bs : List β} → a ∈ as → b ∈ bs →
    (a, b) ∈ prodList as bs
  | _ :: _, _, ha, hb => by
    simp only [prodList, List.mem_append, List.mem_map]
    rcases List.mem_cons.mp ha with rfl | ha
    · exact Or.inl ⟨b, hb, rfl⟩
    · exact Or.inr (mem_prodList ha hb)

/-- Over finite element sets, a type without collections whose primitive types are finite has finitely many values. -/
theorem V_enumerable {E : L → Type} (hE : ∀ l, Enumerable (E l)) (t : Ty T.P L)
    (hp : t.PrimsSatisfy fun p => Enumerable (T.PV p)) (hc : t.CollectionFree) : Enumerable (V T E t) := by
  induction t with
  | zero => exact ⟨[], fun x => nomatch x⟩
  | one => exact ⟨[()], fun x => List.mem_singleton.mpr rfl⟩
  | prim p => exact hp
  | lbl l => exact hE l
  | sum a b iha ihb =>
    obtain ⟨la, hla⟩ := iha hp.1 hc.1
    obtain ⟨lb, hlb⟩ := ihb hp.2 hc.2
    refine ⟨la.map Sum.inl ++ lb.map Sum.inr, fun x => ?_⟩
    cases x with
    | inl x => exact List.mem_append_left _ (List.mem_map_of_mem _ (hla x))
    | inr y => exact List.mem_append_right _ (List.mem_map_of_mem _ (hlb y))
  | prod a b iha ihb =>
    obtain ⟨la, hla⟩ := iha hp.1 hc.1
    obtain ⟨lb, hlb⟩ := ihb hp.2 hc.2
    exact ⟨prodList la lb, fun ⟨x, y⟩ => mem_prodList (hla x) (hlb y)⟩
  | list _ _ => exact hc.elim
  | set _ _ => exact hc.elim

/-- Proposition 9.4: if `G'` is finite and every image `τ_l` holds no collection and only primitive types with
finitely many values, every label of `Δ_Φ(G')` has finitely many elements. -/
theorem Mapping.delta_finite (Φ : Mapping S S') (G' : Apg S') (hG : ∀ l', Enumerable (G'.E l'))
    (hp : ∀ l, (Φ.τ l).PrimsSatisfy fun p => Enumerable (T.PV p)) (hc : ∀ l, (Φ.τ l).CollectionFree) (l : L) :
    Enumerable ((Φ.delta G').E l) :=
  V_enumerable hG (Φ.τ l) (hp l) (hc l)

end ApgModel.Core
