# Algebraic property graphs

* Status: first draft, 2026-11-08. Part 1 of a formal semantics for UGS (`openspec/notes/backlog.md`, item 18).

This file defines the algebraic property graph (APG) model of Shinavier, Wisnesky and Meyers, the model the formal
semantics of UGS is built on: a schema of labels whose types are records and variants over primitive types and labels,
a graph of elements each carrying one value of its label's type, the homomorphisms between such graphs, and the
schema mappings that carry a graph from one schema to another. It states the definitions in the named form CQL
implements, proves that form equivalent to the paper's binary one, and adds the results UltraGraphDB relies on.

Part 2 extends the model with collections, for UGS's set-valued attributes and roles. Part 3 maps a UGS catalog onto
the model and defines the RDF view and the GQL view as functors of one APG, the mapping contract's definition and the
test oracle the query cores are checked against.

The model is the foundation of four layers (section 10): the APG model itself; the GQL view, described by a closed
graph type with PG-Schema's semantics; keys and other constraints with PG-Keys' semantics; and the RDF view with
RDF 1.2 reifiers and SHACL shapes.

## Contents

1. Conventions
2. Primitives
3. Types
4. Schemas
5. Values
6. Algebraic property graphs
7. Homomorphisms
8. Equivalent formulations
9. Schema mappings and Δ
10. The model and the views
11. What the next parts add
12. The Lean model
13. Decisions
14. Open questions
15. Sources

## 1. Conventions

| Notation | Meaning |
|---|---|
| Paper §n | Section n of *Algebraic Property Graphs*, arXiv:1909.04881, version 3 (20 July 2022) |
| CQL | The `catdata.apg` package of CQL, read for its representation of the paper's definitions |
| `A ⇀ B` | A finite partial function from `A` to `B`, with domain `dom φ` |
| `Set^L` | Families of sets indexed by a set `L`: `X = (X(l))_{l ∈ L}`; a morphism `h : X → Y` is a family of functions `h_l : X(l) → Y(l)` |
| `A + B`, `A × B` | Disjoint union and cartesian product of sets |
| Paper, here | A proposition marked *paper* is stated in the paper; one marked *here* is this file's, with its proof sketched |

Definitions are stated so that each transcribes into a Lean 4 inductive type or structure.

## 2. Primitives

**Definition 2.1 (typeside).** A typeside is `T = (P, PV, F, ⟦·⟧)`:

- `P`, a set of primitive types;
- `PV`, a set `PV(p)` of primitive values for each `p ∈ P`;
- `F`, a set of function symbols, each with an arity `f : p₁ … pₖ → p` over `P`;
- `⟦f⟧ : PV(p₁) × … × PV(pₖ) → PV(p)`, a total function for each `f`.

`P` and `PV` are the paper's parameters (paper §3). `F` holds the paper's user-defined functions (paper §6.1), narrowed
to primitive argument and result types where the paper admits any label-free type, so a function never depends on a
schema. The paper's equations between such functions are left out here; part 3 places UGS's value checks. Every
definition below is relative to one typeside.

## 3. Types

**Definition 3.1 (field names).** `N` is a countable set of field names.

**Definition 3.2 (types).** For a set `L` of labels, `Ty(L)` is the least set closed under:

```
prim p     for p ∈ P               a primitive type
lbl l      for l ∈ L               a reference to an element labelled l
rec φ      for φ : N ⇀ Ty(L)       a record: one value for every field of dom φ
var φ      for φ : N ⇀ Ty(L)       a variant: one field of dom φ, with a value for it
```

with the abbreviations

```
1            := rec ∅                                    the unit
0            := var ∅                                    the empty type
t₁ × t₂      := rec { fst ↦ t₁, snd ↦ t₂ }
t₁ + t₂      := var { inl ↦ t₁, inr ↦ t₂ }
optional t   := var { none ↦ 1, some ↦ t }
enum c₁…cₙ   := var { c₁ ↦ 1, …, cₙ ↦ 1 }
```

`lbls(t)` is the set of labels occurring in `t`. A type is **label-free** when `lbls(t) = ∅`, and **primitive-free**
when no `prim p` occurs in it.

**Proposition 3.3 (binary form; here).** Fix a total order on `N`. Translate `rec {n₁ ↦ t₁, …, nₖ ↦ tₖ}`, with
`n₁ < … < nₖ`, to `⌜t₁⌝ × (⌜t₂⌝ × ( … × (⌜tₖ⌝ × 1)))`, and `var` likewise with `+` and `0`.

1. **Values.** `V_E(t) ≅ V_E(⌜t⌝)` for every family `E`, naturally in `E` (section 5).
2. **Terms.** Each mapping term of definition 8.5 translates to the binary grammar: access to the `i`-th field in the
   order to `i − 1` second projections followed by a first, injection at the `i`-th field to `i − 1` right injections
   followed by a left, and a `case` to nested binary cases. Evaluation commutes with the translation and the
   isomorphisms of part 1.

Every definition and result of the paper, stated over its grammar `t ::= 0 | 1 | t + t | t × t | Prim p | Lbl l`,
therefore holds over `Ty(L)` up to the isomorphisms of part 1. They are isomorphisms, not equalities: a record of one
field `n ↦ t` translates to `t × 1`.

*Proof.* Part 1 by induction on `t`, using the associativity and unit isomorphisms of `×` and `+` in `Set`, which are
natural. Part 2 by induction on the term, using part 1 at each constructor.

## 4. Schemas

**Definition 4.1 (APG schema; paper §3).** An APG schema is `S = (L, σ)`: a set `L` of labels and a function
`σ : L → Ty(L)` giving each label its type. A schema is **finite** when `L` is finite.

A type may name labels, its own included: `σ(A) = A + 1` is a schema of streams (paper §4.3). A label inside a type is
a reference to an element, a leaf of the grammar, so a schema's recursion runs through element identity and never
through the unfolding of a type.

**Definition 4.2 (element kinds; paper §4.2).** A label `l` is:

- a **vertex label** when `σ(l) = 1`;
- an **edge label** when `σ(l)` is a record of exactly two fields, each `lbl a` of a vertex label `a`: the paper's
  `lbl a × lbl b`, with fields of any names;
- a **vertex property** when `σ(l)` is a record of exactly two fields, one `lbl a` of a vertex label and one of a
  label-free type, and an edge property or a meta-property likewise, with `a` an edge or a property label;
- a **meta-edge label** when `σ(l)` has a field of a label that is no vertex label (paper §4.2);
- a **hyperedge label** (here), generalising the edge, when `σ(l)` is a record whose fields include two or more labels
  or optional labels, as the paper's example of trips is (paper §4.1).

The kinds classify the schema; they add no structure to it.

## 5. Values

**Definition 5.1 (values; paper §3, equation 1).** For `E ∈ Set^L`, `V_E : Ty(L) → Set` is defined by recursion on
types:

```
V_E(prim p) = PV(p)
V_E(lbl l)  = E(l)
V_E(rec φ)  = ∏_{n ∈ dom φ} V_E(φ(n))                 one value per field
V_E(var φ)  = ∑_{n ∈ dom φ} V_E(φ(n)) = { (n, v) | n ∈ dom φ, v ∈ V_E(φ(n)) }
```

So `V_E(1)` has one value, the empty record, and `V_E(0)` is empty.

**Definition 5.2 (action on families).** A morphism `h : E → E'` of `Set^L` gives `V_h(t) : V_E(t) → V_{E'}(t)`: the
identity on `prim p`, `h_l` on `lbl l`, field by field on a record, and `(n, v) ↦ (n, V_h(φ(n))(v))` on a variant.

**Proposition 5.3 (here).** For every `t`, `V_(−)(t)` is a functor `Set^L → Set`: `V_id(t) = id`, and
`V_{h'∘h}(t) = V_{h'}(t) ∘ V_h(t)`. *Proof.* Induction on `t`.

**Definition 5.4 (value terms; paper §3.1).** For a set `E` of elements labelled by `λ : E → L`:

```
v ::= ⟨a⟩_p                     a primitive value, a ∈ PV(p)
    | elem e                    a reference to the element e ∈ E
    | { n₁ = v₁, …, nₖ = vₖ }   a record, the nᵢ distinct
    | n · v                     the variant field n with the value v
```

A value term is checked against a type:

- `⟨a⟩_p : prim p` when `a ∈ PV(p)`;
- `elem e : lbl l` when `λ(e) = l`;
- `{ n₁ = v₁, …, nₖ = vₖ } : rec φ` when `dom φ = {n₁, …, nₖ}` and `vᵢ : φ(nᵢ)` for each `i`;
- `n · v : var φ` when `n ∈ dom φ` and `v : φ(n)`.

`v : t` holds exactly when `v` denotes a member of `V_E(t)`, with `E(l) = λ⁻¹(l)`. A variant term is checked, not
inferred: `n · v` checks against every variant type whose field `n` admits `v`, which is why the paper annotates its
injections with the other summand.

**Definition 5.5 (references).** `refs(v)`, the elements a value term references, is the set of `e` with `elem e`
occurring in `v`. It is finite, since a term is.

## 6. Algebraic property graphs

**Definition 6.1 (APG; paper §3).** An APG on a schema `S = (L, σ)` is `G = (E, υ)`:

- `E ∈ Set^L`, the elements of each label;
- `υ_l : E(l) → V_E(σ(l))` for each `l ∈ L`, the value of each element.

**Definition 6.2 (single-set form; paper §3.1).** Equivalently, `G = (E, λ, υ)`: a set `E` of elements, a labelling
`λ : E → L`, and `υ`, sending each element to a value term, such that the type each value is checked against is its
label's:

```
υ(e) : σ(λ(e))      for every e ∈ E
```

which is the paper's commuting square `τ ∘ υ = σ ∘ λ`, with `τ` the type of a value.

**Proposition 6.3 (paper §3.1).** The two forms are in bijection: `E(l) := λ⁻¹(l)` and `υ_l := υ|E(l)` one way, and
`E := ⨆_{l ∈ L} E(l)` with `λ` and `υ` read off the summands the other.

**Definition 6.4.** An APG is **finite** when `E` is finite. A finite APG is a finite set of labelled elements, each
carrying one finite value term: the shape a store holds.

**Definition 6.5 (element graph).** The element graph of `G` has the elements of `G` as its vertices and an arc from
`e` to `e'` for each occurrence of `elem e'` in `υ(e)`, named by the path of field names leading to the occurrence.

**Proposition 6.6 (referential integrity; here).** Every arc of the element graph ends at an element of `G`, and that
element carries the label the path's type names. *Proof.* `V_E(lbl l) = E(l)`, so a reference typed `lbl l` is an
element of `E(l)`.

The element graph may be cyclic, as a stream that returns to an earlier element is (paper §4.3): a cycle is a
reference to an existing element, never an infinite value.

**Example 6.7.** People, organisations, a membership with its own attributes, and provenance on the membership and on
a person:

```
L = { Person, Organization, Source, MemberOf, AssertedBy }

σ(Person)       = rec { name ↦ prim String }
σ(Organization) = rec { label ↦ prim String }
σ(Source)       = rec { label ↦ prim String }
σ(MemberOf)     = rec { member     ↦ lbl Person,
                        collection ↦ lbl Organization,
                        role       ↦ enum employee contractor,
                        confidence ↦ optional (prim Float64) }
σ(AssertedBy)   = rec { statement  ↦ var { membership ↦ lbl MemberOf, person ↦ lbl Person },
                        asserter   ↦ lbl Source }
```

An APG on it:

```
E(Person) = { ada }   E(Organization) = { acme }   E(Source) = { hr }
E(MemberOf) = { m1 }  E(AssertedBy) = { a1, a2 }

υ(ada)  = { name = ⟨"Ada"⟩ }
υ(acme) = { label = ⟨"Acme"⟩ }
υ(hr)   = { label = ⟨"HR system"⟩ }
υ(m1)   = { member = elem ada, collection = elem acme, role = employee · {}, confidence = some · ⟨0.98⟩ }
υ(a1)   = { statement = membership · elem m1, asserter = elem hr }
υ(a2)   = { statement = person · elem ada,    asserter = elem hr }
```

`MemberOf` is a hyperedge label with attributes of its own; `m1` has identity, so a second membership between Ada and
Acme is a second element; `AssertedBy`'s `statement` field is a role whose type is a union of labels; and `AssertedBy`
is a meta-edge label, so `a1` is a relation over a relation, an element whose value references another relation's
element.

## 7. Homomorphisms

**Definition 7.1 (homomorphism; paper §5.2).** A homomorphism `h : G → G'` of APGs on one schema `S` is a morphism
`h : E → E'` of `Set^L` that commutes with the values:

```
υ'_l ∘ h_l  =  V_h(σ(l)) ∘ υ_l        for every l ∈ L
```

It sends each element to an element of the same label, and its image's value is the element's value with every
reference mapped by `h`; primitive values are preserved exactly. CQL checks the same two conditions on its transforms,
the labels agreeing and the data agreeing through the element map; its transforms also carry a label map between two
schemas, and definition 7.1 is the case of the identity label map.

**Proposition 7.2 (paper §5.1).** APGs on `S` and their homomorphisms form a category, `S-APG`, with componentwise
identities and composition.

**Definition 7.3.** An isomorphism is a homomorphism whose components are bijections. Two APGs are isomorphic exactly
when they differ by a renaming of their elements, which is how part 3 compares two APGs whose element names differ.

## 8. Equivalent formulations

### 8.1 Coalgebras

**Proposition 8.1 (paper §5.1).** A schema `S` determines the functor `F_S : Set^L → Set^L`,
`F_S(E)(l) = V_E(σ(l))` and `F_S(h)_l = V_h(σ(l))`. An APG on `S` is exactly an `F_S`-coalgebra `υ : E → F_S(E)`, and
a homomorphism exactly a coalgebra morphism, so `S-APG = F_S-Coalg`. `F_S` is a polynomial functor (paper §8).

### 8.2 Models of the APG theory

**Definition 8.2 (APG theory; paper §5.2).** `C_S` is the free category with finite products and finite coproducts,
terminal and initial objects included, generated by the objects `lbl l` for `l ∈ L` and `prim p` for `p ∈ P`, the
arrows `f : prim p₁ × … × prim pₖ → prim p` for the typeside's functions, and an arrow `δ_l : lbl l → σ(l)` for each
label (paper footnote 12, after Cockett and Seely). Its objects are exactly the types of `Ty(L)`, in binary form.

**Definition 8.3 (model).** A model of `C_S` is a functor `M : C_S → Set` that preserves finite products and
coproducts and sends `prim p` to `PV(p)` and `f` to `⟦f⟧`. A morphism of models is a natural transformation `η` with
`η_{prim p} = id`, `η_{t × u} = η_t × η_u` and `η_{t + u} = η_t + η_u`.

**Proposition 8.4 (paper §5.2).** `G = (E, υ)` gives the model `M(t) = V_E(t)`, `M(δ_l) = υ_l`, and a model `M` gives
the APG `E(l) = M(lbl l)`, `υ_l = M(δ_l)`. The category of models is equivalent to `S-APG`.

**Distributivity (decided 2026-11-08).** `C_S` is taken **distributive**: the canonical maps
`t × u + t × v → t × (u + v)` and `0 → t × 0` are isomorphisms. The paper allows the choice (paper §5.2), writing the
second map as `t → t + 0`, which is an isomorphism in every category with an initial object; `0 → t × 0` is the one
distributivity requires.

- Models are unchanged, since `Set` is distributive (paper §5.2).
- The mapping terms of section 8.3 need it: a `case` whose branches read the argument `x` is an arrow
  `τ × (A + B) → C`, which factors through `τ × A + τ × B` only when the first canonical map is invertible.
- More schema mappings exist (section 9).
- The paper notes that distributivity complicates term rewriting (paper §5.2); section 14 asks which procedure decides
  equality.

### 8.3 Mapping terms

**Definition 8.5 (mapping terms).** An arrow of `C_S` out of a type `τ` is written as a term in one variable `x : τ`:

```
s ::= x                                        the argument
    | ⟨a⟩_p                                    a primitive value
    | f(s₁, …, sₖ)                             a typeside function
    | { n₁ = s₁, …, nₖ = sₖ }                  a record
    | s . n                                    a record's field
    | n · s                                    a variant injection
    | case s of { n₁ y₁ ⇒ s₁ | … | nₖ yₖ ⇒ sₖ }   one branch per field of the variant
    | absurd s                                 out of the empty type
    | deref s                                  an element's value: s : lbl l gives deref s : σ(l)
```

typed by these rules: `x : τ`; `⟨a⟩_p : prim p`; `f(s₁, …, sₖ) : p` for `f : p₁ … pₖ → p` and `sᵢ : prim pᵢ`;
`{ n₁ = s₁, …, nₖ = sₖ } : rec φ` when `dom φ = {n₁, …, nₖ}` and `sᵢ : φ(nᵢ)`; `s . n : φ(n)` for `s : rec φ`;
`n · s : var φ` for `s : φ(n)`; a `case` on `s : var φ` has a branch per field of `dom φ`, each binding
`yᵢ : φ(nᵢ)` and every branch of one type, and its branches may also read `x` (the distributivity of section 8.2);
`absurd s : t` for `s : 0`; and `deref s : σ(l)` for `s : lbl l`, the generator `δ_l`. Two terms are equal when the
laws of a distributive category with finite products and coproducts, β and η for records and variants, prove them
equal.

**Definition 8.6 (evaluation).** For an APG `G`, a term `x : τ ⊢ s : t` denotes `⟦s⟧_G : V_E(τ) → V_E(t)`, by
recursion on `s`, with `⟦deref s⟧_G = υ_l ∘ ⟦s⟧_G`.

**Proposition 8.7 (here).** Evaluation terminates on every APG, cyclic ones included. *Proof.* `⟦·⟧` recurses on the
term, which is finite; `deref` returns an element's stored value and evaluates nothing further.

## 9. Schema mappings and Δ

**Definition 9.1 (schema mapping; paper §7.1).** A mapping `Φ : S → S'` gives each label `l ∈ L`:

- a type `τ_l ∈ Ty(L')`, the label's image;
- a mapping term `x : τ_l ⊢ t_l : Φ̂(σ(l))`,

where `Φ̂ : Ty(L) → Ty(L')` replaces each `lbl l₀` by `τ_{l₀}` and is the identity on primitive types and structural
on records and variants. This is the paper's functor `C_S → C_{S'}` preserving products, coproducts and primitive
types, given by its values on the generators, `lbl l ↦ τ_l` and `δ_l ↦ t_l`; CQL writes it the same way, a type and a
term per label, and checks the term against `Φ̂(σ(l))`. The identity mapping has `τ_l = lbl l` and `t_l = deref x`.

**Definition 9.2 (Δ; paper §7.1).** For an APG `G' = (E', υ')` on `S'`, `Δ_Φ(G') = (E, υ)` on `S` is

```
E(l)   = V_{E'}(τ_l)
υ_l(v) = ι_{σ(l)}( ⟦t_l⟧_{G'}(v) )
```

where `ι_t : V_{E'}(Φ̂(t)) → V_E(t)` is the bijection that is the identity at labels, since
`V_{E'}(Φ̂(lbl l₀)) = V_{E'}(τ_{l₀}) = E(l₀)`, and is structural elsewhere. On a homomorphism,
`Δ_Φ(h)_l = V_h(τ_l)`.

An element of `Δ_Φ(G')(l)` is a value of `τ_l` over `G'`; its identity is that value, tagged by `l` in the single-set
form of definition 6.2, since two labels with one image hold equal values. CQL tags it the same way.

**Proposition 9.3 (paper §7.1).** `Δ_Φ` is a functor `S'-APG → S-APG`, and mappings compose as functors of the APG
theories, so `S ↦ S-APG` and `Φ ↦ Δ_Φ` form a functor `Schema^op → Cat`. The migrations `Δ` expresses include
dropping labels, duplicating labels, and permuting and computing fields (paper §7.1). The paper adds joins of labels
given equality functions; under definition 9.2 an image that is a record of labels yields every combination, and
keeping the matching ones is a selection, which `Δ` alone does not perform (after proposition 9.5).

**Proposition 9.4 (finiteness; here).** If `G'` is finite and every primitive type occurring in every `τ_l` has
finitely many values, `Δ_Φ(G')` is finite. *Proof.* `V_{E'}(t)` is finite for such a `t` over finite `E'`, by
induction on `t`. A primitive type with infinitely many values in `τ_l` may make `E(l)` infinite; it need not, as when
it sits in a record beside a label with no elements.

**Proposition 9.5 (one origin; here).** If every `τ_l` is a label `lbl l'` or a variant of labels
`var { n₁ ↦ lbl l'₁, …, nₖ ↦ lbl l'ₖ }`, then each element of `Δ_Φ(G')(l)` is one element `e'` of `G'`, its
**origin**: `e'` itself under a label image, `(nᵢ, e')` under a variant. So `|Δ_Φ(G')(l)| = |E'(l')|`, or
`|E'(l'₁)| + … + |E'(l'ₖ)|`. *Proof.* By definition 5.1, `V_{E'}(lbl l') = E'(l')` and
`V_{E'}(var {nᵢ ↦ lbl l'ᵢ}) = ∑ᵢ E'(l'ᵢ)`.

A record of labels as an image, `lbl a × lbl b`, makes every pair of an `a` and a `b` an element: a join, whose size
is the product of theirs. Proposition 9.5 is the core of `SCHEMA-EVOLUTION.md`'s non-merging fragment: a target label
fed by a union of source labels is a `Δ` along a variant of labels, each image with one origin. The fragment adds two
steps `Δ` does not take: a `where`, a selection, which is an equalizer of homomorphisms; and a role assigned by a key
lookup, which searches a label's elements by key and is defined by the key's semantics (section 10). Part 3 defines
both.

**Example 9.6.** A target schema renames `MemberOf` of example 6.7 to `Membership` and drops its `confidence`. Its
labels are example 6.7's with `Membership` for `MemberOf`, and its types differ in two:

```
σ'(Membership) = rec { member ↦ lbl Person, collection ↦ lbl Organization, role ↦ enum employee contractor }
σ'(AssertedBy) = rec { statement ↦ var { membership ↦ lbl Membership, person ↦ lbl Person },
                       asserter  ↦ lbl Source }
```

The mapping runs from the target to the source, since `Δ` carries source data along it:

```
τ_Membership = lbl MemberOf
t_Membership = { member     = (deref x) . member,
                 collection = (deref x) . collection,
                 role       = (deref x) . role }
```

with the identity mapping on every other label, which type-checks for `AssertedBy` because `Φ̂(σ'(AssertedBy))` is
example 6.7's `σ(AssertedBy)`. `Δ` gives one `Membership` per `MemberOf`, `m1`'s image carrying Ada, Acme and the role,
and every `AssertedBy` whose statement names `m1` names its image.

## 10. The model and the views

Decided 2026-11-08, after comparing the APG model with PG-Schema, the LDBC proposal for property graph schemas:

| Layer | Formalism | Carries |
|---|---|---|
| Foundation | The APG model of this file | Every instance's identity, n-ary relations, relations over relations, UQL's result types, and migrations as `Δ` (`SCHEMA-EVOLUTION.md`) |
| GQL view | A closed graph type of ISO GQL, read with PG-Schema's STRICT, closed semantics | The generated graph type and its conformance, which MGQL checks for the GQL core |
| Constraints | PG-Keys for keys, uniqueness, presence and single values; PG-Schema's proposed `COUNT` form for participation bounds; UGS's own rules for acyclicity, no-overlap, limits and bitemporal statements | Constraints on the APGs of a schema, defined in part 3 |
| RDF view | RDF 1.2 with reifiers, published with SHACL shapes | Part 3's construction; the shapes cross-checked against the common core of SHACL, ShEx and PG-Schema |

**Why APG is the foundation.**

- PG-Schema's property graph maps each edge to an ordered pair of nodes (PG-Schema, definition 3.1): an edge joins two
  nodes and never ends at an edge. UGS's n-ary relations and relations over relations, DeepBrain's provenance on every
  statement among them, are nodes there, the encoding the GQL view performs; as the foundation, that encoding would
  become the definition.
- PG-Schema types structurally: an element conforms to every type whose label set equals its own and whose record
  admits its properties (PG-Schema, definition 4.2), so it has several types or none. UGS types nominally, one concrete
  type per instance recorded in its ID's prefix, as an APG labels each element once.
- PG-Schema's property values are atomic. Records, variants and optional values are the APG model's types, and UQL's
  typed trees, nested records with `Option` and tagged unions for `on Type`, are its values.
- PG-Schema has no schema mappings or migration. The APG model has `Δ` (section 9).
- PG-Schema carries what the APG model lacks, keys and constraints with a formal semantics, and is LDBC's input to the
  next edition of ISO GQL. It therefore describes the GQL view and lends its keys.

**The GQL view's graph type.** A node of PG-Schema's closed type conforms only when its label set equals the type's
exactly (PG-Schema, section 4.2), so a closed supertype admits no node of a subtype, which carries the subtype's label
as well. The generated graph type therefore takes one node type per concrete type, with the exact label set of the
type and every supertype, and one edge type per edge-encoded relation, role edge and navigation edge, its endpoints
listing the node types of every concrete type its role admits. A supertype is a label, never a node type of its own.

**Keys.** A UGS key is a PG-Keys `IDENTIFIER` key, exclusive, mandatory and singleton, whose scope is the key's type
tree in one graph; `@unique` is an `EXCLUSIVE` key; a required attribute or role is `MANDATORY` and a single-valued one
`SINGLETON`. Each is checked as the emptiness of its violation query, as PG-Keys checks it (PG-Keys, section 4.5). A
role assigned by key lookup in a migration reaches at most one element because the key is exclusive.

## 11. What the next parts add

| UGS construct | Where it is modelled |
|---|---|
| Entity type, attribute, optional attribute, default | A label whose type is a record of primitive fields, `optional` for an optional one; a default is part 3's |
| Enum | `enum`, a variant of units |
| Relation instance with identity, n-ary, with attributes | A hyperedge label: a record of its roles' labels and its attributes |
| Union-typed role, a role typed by a supertype | A variant over the concrete labels it admits |
| Relation over a relation | A field typed by a relation's label, a meta-edge |
| `@multi` | Identity: two elements may carry equal values |
| Interval, measure, record attribute | A record of primitives |
| Set attribute, set-valued role, `@symmetric` relation | Part 2: a collection constructor, or reification as elements; a symmetric relation's participants are a set of two |
| List attribute | Part 2, with the inductive types of paper §6.2 |
| Keys, `@unique`, cardinality bounds, checks | Part 3: none is equational, so each is a constraint on the APGs of a schema |
| Single inheritance | Part 3: each element carries one label, its concrete type's; a supertype is the variant of its concrete subtypes |
| Derived closures and folds | Part 3: a transitive closure is no arrow of `C_S`, so it is a query over the APG |
| Bitemporal versions, `asserted` | Part 3 |
| Named graphs, shared graphs, bridges | Part 3: a graph is an APG on its schema, and a bridge a field typed by a shared graph's label |
| `@open` data and `Resource` | Part 3: the label `Resource`, whose value holds undeclared attributes, with part 2's collections |
| Value entity, `@independent` | Part 3: a label keyed by its value |
| A node's labels in the GQL view | Section 10: the exact label set of the concrete type and every supertype |
| An edge-encoded relation without a reifier | Part 3: its element's identity is its triple, which a relation without `@multi` keeps unique |

## 12. The Lean model

Decided 2026-11-08: the Lean model of this file sits in `lean/`, beside `PlacementModel` and `RoundModel`, under the
repository's toolchain, Lean 4.14, without Mathlib, and takes a binary core with a named layer over it.

A spike of 2026-11-08 under Lean 4.14 measured the two forms of definition 3.2:

| Under Lean 4.14 | Binary form, `0`, `1`, `+`, `×` | Named form, records and variants over field lists |
|---|---|---|
| The type declares | As a plain inductive type | As a type nested through `List (Name × Ty)` |
| Recursion over types | Structural | Mutual, over a type and its field lists |
| `V_E` computes by `rfl` | Yes | Yes |
| `deriving DecidableEq` | Yes | Unimplemented for nested types; written by hand |
| One record, one term | By construction | Only under an invariant: two orders of one record's fields are two terms |

The forms trade as follows:

| | Named form alone | Binary form alone | Binary core, named layer |
|---|---|---|---|
| Fidelity to definition 3.2 and to UGS | Direct | Fields become positions | Direct, at the layer part 3 reads |
| Field names for RDF predicates and GQL property keys | Present | A side table, a named layer under another name | Present |
| Equality and decidability | By hand, with a field-order invariant in every proof | Derived; structural equality | Derived on the core; by hand on the layer alone |
| Proofs of functoriality, `Δ` and proposition 8.4 | Mutual induction over field lists throughout | Structural induction | Structural, on the core |
| Executable as a test oracle, as MGQL is | Yes | Yes | Yes |
| Added work | List machinery in every theorem | Names rebuilt when part 3 needs them | Proposition 3.3 proved once |

The model takes a **binary core with a named layer over it**:

- The core states the paper's definitions in its binary form and proves sections 5 to 9 by structural induction. The
  paper references a Coq development of its results (paper §7), which this file has not read; the core's statements
  are to be compared against it where it exists.
- The named layer holds the types of definition 3.2, with a record's or variant's field names strictly increasing in
  the total order proposition 3.3 fixes, so one finite map is one term, and its decidable equality written by hand.
- Proposition 3.3 is proved between them, values and terms both, which carries every core result to the named layer.
  Part 3 reads the named layer, where each field's name gives its predicate and property key.
- A view that needs a declaration order keeps it beside the type, outside the record.

The translation theorem is the model's one substantial proof: it must hold naturally in `E`, and for terms as well as
values, or the core's results do not transfer.

## 13. Decisions

| Question | Decision |
|---|---|
| Foundation of UGS's semantics | The APG model of this file, decided 2026-11-08 after the comparison with PG-Schema (section 10) |
| GQL view | A closed graph type with PG-Schema's STRICT, closed semantics, one node type per concrete type with its exact label set (section 10) |
| Constraint semantics | PG-Keys for keys, uniqueness, presence and single values; PG-Schema's proposed `COUNT` for participation bounds; UGS's own for the rest (section 10) |
| RDF view | RDF 1.2 with reifiers and SHACL shapes, cross-checked against the common core of SHACL, ShEx and PG-Schema (section 10) |
| Distributivity | `C_S` is distributive (section 8.2) |
| Lean model | In `lean/`, a binary core with a named layer, proposition 3.3 proved between them (section 12) |

## 14. Open questions

- With `C_S` distributive and without function types, which procedure decides equality of mapping terms, as checking
  a migration against a declared equation needs? Scherer's procedure decides the lambda calculus with sums and the
  empty type, function types included: does the distributive theory without them embed conservatively in it, so that
  the procedure decides it, or does UltraGraphDB need a normaliser of its own, and at what cost for a catalog's size?
- Are a label's images restricted to types whose primitive types have finitely many values (proposition 9.4)?
- Which of the products, coproducts, equalizers and coequalizers of APGs that CQL's `apg` package builds belong to the
  model? Part 3 needs equalizers for a migration's `where`; colimits would integrate two schemas.

## 15. Sources

- J. Shinavier, R. Wisnesky, J. G. Meyers, *Algebraic Property Graphs*, arXiv:1909.04881v3, 2022: the schema and the
  APG (§3), the single-set form (§3.1), the element kinds (§4.2), coalgebras and models (§5), user-defined functions
  and further types (§6), schema mappings and `Δ` (§7.1), polynomial functors (§8). Its Coq proofs are referenced at
  `http://categoricaldata.net/APG.v`.
- J. R. B. Cockett, R. A. G. Seely, *Finite Sum-Product Logic*, Theory and Applications of Categories 8(5), 2001: the
  free category with finite sums and products.
- G. Scherer, *Deciding Equivalence with Sums and the Empty Type*, POPL 2017.
- D. I. Spivak, *Functorial Data Migration*, arXiv:1009.1166: `Δ`, `Σ` and `Π` over schemas presented as categories.
- T. Grust, *Monad Comprehensions: A Versatile Representation for Queries*, 2003: the collection types part 2 draws on
  (paper §6.2).
- CQL, `src/catdata/apg`: the named records and variants of definition 3.2, the transform conditions of definition
  7.1, and the per-label type and term of definition 9.1.
- R. Angles et al., *PG-Schema: Schemas for Property Graphs*, SIGMOD 2023 (PACMMOD 1(2)), arXiv:2211.10962: the
  property graph (definition 3.1), formal graph types and conformance (section 4.2), inheritance (section 4.1).
- R. Angles et al., *PG-Keys: Keys for Property Graphs*, SIGMOD 2021: `EXCLUSIVE`, `MANDATORY`, `SINGLETON` and
  `IDENTIFIER`, and validation as the emptiness of a query (section 4.5).
- S. Ahmetaj et al., *Common Foundations for SHACL, ShEx, and PG-Schema*, arXiv:2502.01295, 2025.
- A. Thimmaiah, T.-N. Lin, M. Gligoric, *MGQL: An Executable, Small-Step Semantics of GQL*, OOPSLA 2026: the closed
  graph schemas the GQL view's conformance is checked against.
