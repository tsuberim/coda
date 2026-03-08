# Tensor-First Type System — Design Document

> Status: design / pre-implementation
> Replaces: the `List(a)` / `Tensor(elem, Shape(...))` hybrid in `src/types.rs`

---

## 1. Overview

Every value type in Coda gains a **shape annotation**. A scalar `Int` is `Int` (or `Int[]`); a 1-D array is `Int[n]`; a 2-D array is `Int[m, n]`. Rank is always statically known. `List(a)` is retired — rank-1 arrays replace it.

### What changes

| Before | After |
|---|---|
| `List(Int)` | `Int[n]` |
| `Tensor(F64, Shape(m, n))` | `F64[m, n]` |
| `map(f, xs)` | `f(xs)` (auto-lifting) |
| `xs <> ys` | `concat(xs, ys)` |
| `head(xs)` | `xs[0]` |

---

## 2. Type Representation

### 2.1 Dim

```rust
pub enum Dim {
    Nat(u64),    // literal dimension — known statically
    Var(String), // unification variable — same HM machinery as type vars
}
```

Arithmetic is **literal-only**:

```rust
impl Dim {
    pub fn mul(a: &Dim, b: &Dim) -> Option<Dim> {
        match (a, b) { (Nat(m), Nat(n)) => Some(Nat(m * n)), _ => None }
    }
    pub fn add(a: &Dim, b: &Dim) -> Option<Dim> {
        match (a, b) { (Nat(m), Nat(n)) => Some(Nat(m + n)), _ => None }
    }
    pub fn sub(a: &Dim, b: &Dim) -> Option<Dim> {
        match (a, b) { (Nat(m), Nat(n)) if n <= m => Some(Nat(m - n)), _ => None }
    }
}
```

`None` means symbolic arithmetic would be required — callers emit `DimArithmeticRequiresLiterals`.

### 2.2 Shape

```rust
pub type Shape = Vec<Dim>;  // [] = scalar, length = rank
```

No open-tail (`*`). Rank is always statically known. Unknown-size arrays use a `Dim::Var` that may never concretise.

### 2.3 BaseType

```rust
pub enum BaseType {
    Int,
    F64,
    Str,
    Record(Vec<(String, Type)>, Option<String>),  // row-polymorphic
    Union(Vec<(String, Type)>, Option<String>),   // row-polymorphic
    Fun(Vec<Type>, Box<Type>),
    Task(Box<Type>, Box<Type>),
}
```

### 2.4 Type enum

```rust
pub enum Type {
    Shaped(BaseType, Shape),  // every value type: base + shape ([] = scalar)
    Var(String),              // HM type variable
}
```

All previous `Con`, `Record`, `Union`, `Nat` variants are absorbed into `Shaped(BaseType, Shape)`. Functions and Tasks are stored as `Shaped(Fun(...), [])` — they have scalar shape.

Convenience constructors:

```rust
impl Type {
    pub fn scalar(b: BaseType) -> Self { Type::Shaped(b, vec![]) }
    pub fn int()   -> Self { Type::scalar(BaseType::Int) }
    pub fn f64_()  -> Self { Type::scalar(BaseType::F64) }
    pub fn str_()  -> Self { Type::scalar(BaseType::Str) }
    pub fn unit()  -> Self { Type::scalar(BaseType::Record(vec![], None)) }
    pub fn never() -> Self { Type::scalar(BaseType::Union(vec![], None)) }
    pub fn fun(params: Vec<Type>, ret: Type) -> Self {
        Type::scalar(BaseType::Fun(params, Box::new(ret)))
    }
    pub fn shaped(b: BaseType, shape: Shape) -> Self { Type::Shaped(b, shape) }
}
```

### 2.5 Scheme

```rust
pub struct Scheme {
    pub vars:     Vec<String>,  // type variables
    pub dim_vars: Vec<String>,  // dim variables
    pub ty:       Type,
}
```

### 2.6 Substitution

```rust
pub struct Subst {
    pub types: HashMap<String, Type>,
    pub dims:  HashMap<String, Dim>,
}
```

Two independent substitution maps. `compose` and `apply_subst` handle both.

---

## 3. Syntax

```
Int              -- scalar int (shape = [])
Int[]            -- same, explicit
F64[n]           -- 1-D F64, dim is type var n
Int[3, 4]        -- 2-D int, literal dims
F64[m, n]        -- 2-D F64, dim vars
{x: F64, y: F64}[n]   -- AoS: array of n records (row-based)
{x: F64[n], y: F64[n]} -- SoA: record of arrays (column-based, plain record syntax)
[Some F64 | None][n]  -- array of n union values
Task(F64[n], [IoErr Str | r])  -- Task returning a shaped value
```

### AST additions

```rust
// ast.rs
pub enum TypeExpr {
    // existing ...
    Shaped(Box<TypeExpr>, Vec<DimExpr>),  // T[d1, d2, ...]
}

pub enum DimExpr {
    Nat(u64),
    Var(String),
}

pub enum Expr {
    // existing ...
    Index(Box<Spanned<Expr>>, Vec<IndexArg>),  // e[i], e[i,j], e[i:j]
}

pub enum IndexArg {
    Scalar(Spanned<Expr>),                              // integer index
    Fancy(Spanned<Expr>),                               // array index (gather)
    Slice(Option<Spanned<Expr>>, Option<Spanned<Expr>>), // i:j
}
```

---

## 4. Unification

### 4.1 Shaped types

Two `Shaped(b1, s1)` and `Shaped(b2, s2)` unify iff:

1. `len(s1) == len(s2)` — otherwise `RankMismatch`
2. Each dim pair unifies: `Nat(m)` with `Nat(n)` only if `m == n`; `Var(a)` binds to any dim
3. The base types unify (row-unification for records/unions, structural for Fun/Task)

### 4.2 New TypeError variants

```rust
RankMismatch(Shape, Shape),
DimMismatch(Dim, Dim),
DimArithmeticRequiresLiterals,
RankPolymorphicInnerMismatch { expected: Shape, got: Shape },
BroadcastFail(Shape, Shape),
IndexOutOfRank { rank: usize, n_indices: usize },
SliceRequiresLiterals,
```

---

## 5. Function Lifting (Rank Polymorphism)

Lifting fires at **full application only**. Partial application never lifts.

### 5.1 Rule

```
f : a[s_inner] -> b
arg : a[s_outer ++ s_inner]
────────────────────────────
f(arg) : b[s_outer]
```

Strip `s_inner` from the **tail** of the argument's shape. The remaining prefix is `s_outer` — the lifted output shape.

For multi-arg functions, all args must agree on `s_outer` (after broadcasting, §6).

### 5.2 Algorithm

```
lift(f_ty, arg_tys):
  1. Extract Fun([p1..pk], ret) from f_ty.
  2. For each (pi, ai):
       s_inner_i = shape of pi
       s_arg_i   = shape of ai
       check tail(s_arg_i, len(s_inner_i)) matches s_inner_i → RankPolymorphicInnerMismatch if not
       s_outer_i = s_arg_i[0 .. len(s_arg_i) - len(s_inner_i)]
  3. s_outer = fold broadcast over [s_outer_0, s_outer_1, ..., s_outer_k]
  4. Unify each pi base with ai base.
  5. Result = Shaped(ret_base, s_outer ++ ret_shape)
```

### 5.3 Currying does not lift

`+(a)` where `a : Int[3]` is partial application. `+` expects `Int` (scalar) as first arg. `Int[3]` does not unify with `Int[]` (rank mismatch). **Type error.** The correct idiom is `+(a, b)` at full application.

### 5.4 Examples

```
f : Int -> F64
  f(x)  where x : Int       → F64
  f(x)  where x : Int[4]    → F64[4]
  f(x)  where x : Int[3, 4] → F64[3, 4]

g : Int[3] -> F64
  g(x)  where x : Int[3]    → F64
  g(x)  where x : Int[5, 3] → F64[5]      (lift over outer dim)
  g(x)  where x : Int[4]    → TYPE ERROR  (inner dim 3 ≠ 4)
  g(x)  where x : Int[5, 4] → TYPE ERROR  (inner dim 3 ≠ 4)

h : F64[3, 4] -> F64
  h(x)  where x : F64[3, 4]    → F64
  h(x)  where x : F64[5, 3, 4] → F64[5]
  h(x)  where x : F64[3, 5]    → TYPE ERROR
```

---

## 6. Broadcasting

### 6.1 Rule: prefix matching

```
broadcast(s1, s2):
  s1 == s2         → s1
  s1 prefix of s2  → s2
  s2 prefix of s1  → s1
  else             → BroadcastFail(s1, s2)
```

Scalar `[]` is a prefix of everything.

### 6.2 Examples

```
Int[3, 4]  op  Int[3, 4]    → Int[3, 4]  ✓
Int[3, 4]  op  Int[3]       → Int[3, 4]  ✓  ([3] is prefix of [3,4])
Int[3, 4]  op  Int          → Int[3, 4]  ✓  (scalar broadcasts)
Int[3, 4]  op  Int[4]       → TYPE ERROR    ([4] not a prefix of [3,4])
Int[3, 4]  op  Int[3, 4, 5] → Int[3, 4, 5] ✓ ([3,4] is prefix)
```

### 6.3 Dim vars

Two dim vars `m` and `n` broadcast only if they unify (one binds to the other). No symbolic "max."

---

## 7. Row-based vs Column-based Layout

These are two **distinct types** with different semantics. The programmer chooses explicitly. No automatic conversion between them.

### 7.1 Row-based (AoS) — `{x: F64, y: F64}[n]`

An array of n record values. Each element is a complete `{x: F64, y: F64}`.

Runtime: pointer array (`Rc<Vec<Value>>`), or packed struct array for simple numeric records.

| Operation | Type |
|---|---|
| `df[i]` where `i: Int` | `{x: F64, y: F64}` |
| `df[idx]` where `idx: Int[k]` | `{x: F64, y: F64}[k]` |
| `df.x` | **type error** — not a scalar record |

### 7.2 Column-based (SoA) — `{x: F64[n], y: F64[n]}`

A plain record where each field is itself a shaped array. Already expressible with normal record syntax — no special syntax needed.

Runtime: record of arrays — each field is a separate contiguous allocation. Cache-friendly for column operations.

| Operation | Type |
|---|---|
| `df.x` | `F64[n]` |
| `df.x[i]` | `F64` |
| `df[i]` | **type error** — not an array |

### 7.3 Converting between layouts

```
-- AoS → SoA (explicit, user-written):
to_soa = \df ->
  { x: map(\r -> r.x, df)
  , y: map(\r -> r.y, df)
  }

-- SoA → AoS:
to_aos = \df -> tabulate(len(df.x), \i -> {x: df.x[i], y: df.y[i]})
```

No builtin conversion — the programmer is explicit about layout.

---

## 8. Indexing

### 8.1 Integer indexing

Each scalar `Int` index consumes one dimension from the left:

```
a[d1]          indexed by Int  →  a               (scalar)
a[d1, d2]      indexed by Int  →  a[d2]
a[d1, d2, d3]  indexed by Int  →  a[d2, d3]
```

Multi-index: `arr[i, j]` where `arr: a[m, n]` → `a`. More indices than rank → `IndexOutOfRank`.

### 8.2 Fancy indexing (gather)

```
arr : a[n],    idx : Int[k]    →  a[k]
arr : a[m, n], ri : Int[k], ci : Int[k]  →  a[k]
```

Index array shape replaces the indexed dimensions. Both index arrays in the 2-D case must have the same shape.

Mixed: `arr[i, idx]` where `arr: a[m, n]`, `i: Int`, `idx: Int[k]` → `a[k]`.

### 8.3 Slice indexing

```
arr[i:j]  where arr : a[n]
```

- `i`, `j` are literal `Int` → result is `a[j-i]` (static dim)
- `i` or `j` is a runtime `Int` → fresh dim var (unknown static size); user may annotate

### 8.4 AoS record indexing

`df[i]` where `df : {x: F64, y: F64}[n]`, `i : Int` → `{x: F64, y: F64}`.

Mechanically: the array's first dim is consumed, revealing the record base type.

---

## 9. View Operations

All zero-copy (or copy-on-write via `Rc`). Static = output shape computed from input type alone.

| Operation | Type | Static? |
|---|---|---|
| `transpose(a)` | `F64[m, n] → F64[n, m]` | Yes |
| `transpose_nd(a)` | `a[d1..dk] → a[dk..d1]` | Yes (rank fixed) |
| `flatten(a)` | `F64[Nat(m), Nat(n)] → F64[Nat(m*n)]` | Only when both dims are `Nat` |
| `unsqueeze(a)` | `a[d1..dk] → a[1, d1..dk]` | Yes |
| `squeeze(a)` | `a[1, d1..dk] → a[d1..dk]` | Yes (first dim must be `Nat(1)`) |
| `concat(a, b)` | `a[Nat(m)] → a[Nat(n)] → a[Nat(m+n)]` | Only when both dims are `Nat` |

`reshape` is **deferred** — requires type-level arithmetic (`m*n = p*q`) not supported in v1.

`transpose` on rank ≠ 2 is a type error; use `transpose_nd`.

`flatten` / `concat` with var dims → `DimArithmeticRequiresLiterals`.

---

## 10. Builtins

### 10.1 Arithmetic — unchanged signatures, lifted automatically

`+`, `-`, `*` stay `Int Int -> Int`. Lifting (§5) handles shaped args. Broadcasting (§6) resolves mixed shapes. No `add_tensor` / `scale_tensor` needed.

### 10.2 Removed

`::`, `head`, `tail`, `map`, `list_of`, `list_init`, `<>`, `append`, `cons`.

| Old | New |
|---|---|
| `map(f, xs)` | `f(xs)` — lifting |
| `xs <> ys` | `concat(xs, ys)` — literal dims only |
| `head(xs)` | `xs[0]` |
| `list_of(n, x)` | `fill(n, x)` |
| `list_init(n, f)` | `tabulate(n, f)` |

### 10.3 `fold`

`fold : ∀a b n. (b -> a -> b) -> b -> a[n] -> b`

Folds along the **first** dimension. On `F64[m, n]` each element is `F64[n]` (a row).

To fold all elements of a 2-D array: `fold(f, init, flatten(mat))` (requires literal dims).

### 10.4 `len`

`len : ∀a shape. a[d, shape...] -> Int`

Returns the size of the first dimension at runtime.

### 10.5 New builtins

```
fill     : ∀a.   Int -> a -> a[k]           -- k is fresh dim var (runtime size)
tabulate : ∀a.   Int -> (Int -> a) -> a[k]  -- k is fresh dim var
slice    : ∀a n. a[n] -> Int -> Int -> a[k]  -- k fresh if non-literal bounds
zeros    : ∀m n. Int -> Int -> F64[m, n]     -- with annotation to pin dims
ones     : ∀m n. Int -> Int -> F64[m, n]
matmul   : ∀m k n. F64[m, k] -> F64[k, n] -> F64[m, n]
```

### 10.6 `==`

`== : a -> a -> Bool` — via lifting, `a[s] == a[s]` → `Bool[s]` (elementwise). Shapes must match after broadcasting.

### 10.7 `debug`

`debug : ∀a s. a[s] -> a[s]` — prints value and shape to stderr, passes through.

---

## 11. Runtime Representation

### 11.1 Numeric arrays

`Int[d1..dk]`, `F64[d1..dk]`: flat contiguous allocation in row-major order + `Vec<usize>` shape.

```rust
pub enum Value {
    Int(i64),
    Float(f64),
    Str(String),
    IntArray   { data: Rc<Vec<i64>>,   shape: Vec<usize> },
    FloatArray { data: Rc<Vec<f64>>,   shape: Vec<usize> },
    StrArray   { data: Rc<Vec<String>>, shape: Vec<usize> },
    // AoS record array:
    RecordArray { rows: Rc<Vec<Value>>, shape: Vec<usize> },
    // Union array:
    UnionArray  { elems: Rc<Vec<Value>>, shape: Vec<usize> },
    // Scalars:
    Record(Vec<(String, Value)>),
    Tag(String, Box<Value>),
    Closure { ... },
    Builtin { ... },
    Task(Rc<dyn Fn() -> Result<Value, Value>>),
}
```

View operations reuse `Rc` — no copy unless the data is actually mutated (immutable, so never).

### 11.2 Row-based records (AoS)

`{x: F64, y: F64}[n]` → `Value::RecordArray { rows: Rc<Vec<Value>>, shape: [n] }` where each element of `rows` is `Value::Record(...)`. Pointer array — not cache-optimal but correct.

### 11.3 Column-based records (SoA)

`{x: F64[n], y: F64[n]}` → `Value::Record([("x", FloatArray{...}), ("y", FloatArray{...})])`. Just a record whose fields are arrays. No special representation.

### 11.4 Union arrays

`[Some F64 | None][n]` → `Value::UnionArray { elems: Rc<Vec<Value>>, shape: [n] }`. Shape does **not** distribute into payloads — each element is a `Value::Tag(...)`. Pointer array.

---

## 12. Implementation Plan

### Step 1 — `Dim`, `Shape`, extended `Subst`

`src/types.rs`: add `Dim`, extend `Subst` with `dims: HashMap<String, Dim>`. Implement `apply_dim_subst`, extend `apply_subst`, `compose`, `ftv` → `fdv` for dim vars.

### Step 2 — New `Type` / `BaseType` enums

`src/types.rs`: replace `Type` enum with `Shaped(BaseType, Shape)` + `Var(String)`. Update all constructors, `Display`, `pretty`, `normalize_inner`, `apply_subst`.

### Step 3 — Unification

`src/types.rs`: rewrite `unify` for new enum. Add `unify_dim`. Add new `TypeError` variants.

### Step 4 — AST additions

`src/ast.rs`: add `TypeExpr::Shaped`, `DimExpr`. Add `Expr::Index`, `IndexArg`.

### Step 5 — Parser

`src/parser.rs`: extend type-atom parser for `T[d1, d2, ...]`. Add `e[...]` indexing in expression position. Disambiguate from union `[...]`.

### Step 6 — `type_expr_to_type`

`src/types.rs`: handle `TypeExpr::Shaped`. No automatic record normalisation — SoA and AoS are distinct types.

### Step 7 — Lifting in `Expr::App`

`src/types.rs`: implement the lifting algorithm in `infer_inner` for `Expr::App`. This is the core of the new system.

### Step 8 — Broadcasting

`src/types.rs`: implement `broadcast(s1, s2) -> Result<Shape, TypeError>`. Use in lifting step 3.

### Step 9 — Indexing inference

`src/types.rs`: implement indexing rules for `Expr::Index` — scalar index (consume dim), fancy index (replace dim), slice (literal or fresh var).

### Step 10 — Builtin schemes

`src/types.rs` (`std_type_env`): rewrite all schemes using new representation. Remove List-based builtins. Add `fold`, `len`, `concat`, `fill`, `tabulate`, `transpose`, `flatten`, `unsqueeze`, `squeeze`, `matmul`.

### Step 11 — `eval.rs` runtime

`src/eval.rs`: extend `Value`, implement `Expr::Index` eval, add new builtins. Update `Display`/`pretty`.

### Step 12 — Array literals

`src/types.rs`, `src/eval.rs`: `[1, 2, 3]` infers as `Int[3]` (literal dim from list length). Update `infer_inner` and `eval_inner` for `Expr::List`.

### Step 13 — Tests

`tests/type_tests.rs`: shaped types, lifting, broadcasting, indexing, error cases.

### Step 14 — Docs

`docs/CLAUDE.md`: remove `List` section, add shaped-type syntax, update builtins table.

---

## 13. Edge Cases

**`f: Int[3] -> F64` on `Int[4]`** — inner `[3]` vs tail `[4]`: `Nat(3) ≠ Nat(4)` → `RankPolymorphicInnerMismatch`. ✓

**`f: Int[3] -> F64` on `Int[5, 3]`** — tail `[3]` matches, outer `[5]`, result `F64[5]`. ✓

**`+(a)` where `a: Int[3]`** — partial application. `Int[3]` must unify with `Int` (rank `[1]` vs `[]`) → type error. ✓

**`+(a, b)` where `a: Int[3,4]`, `b: Int[3]`** — full application, lifting. Outer shapes `[3,4]` and `[3]`. `broadcast([3,4],[3])` = `[3,4]`. Result `Int[3,4]`. ✓

**`{x: F64[3], y: F64[4]}` — different field shapes** — valid record. Field access works: `.x : F64[3]`, `.y : F64[4]`. `df[i]` is a type error (it's not an array). ✓

**`{x: F64, y: F64}[n]` vs `{x: F64[n], y: F64[n]}`** — two distinct types, no conversion. ✓

**`a[3] == a[3]`** — via lifting: `== : a -> a -> Bool`, outer shape `[3]`, result `Bool[3]`. ✓

**`a[3] == a[4]`** — broadcast `[3]` vs `[4]`: neither prefix → `BroadcastFail`. ✓

**`fold(f, init, mat)` where `mat: F64[m, n]`** — `fold` type: `(b -> a -> b) -> b -> a[d] -> b`. Instantiate `a := F64[n]`, `d := m`. Each step passes `f` a row `F64[n]`. Result type `b`. ✓

**`transpose(a)` where `a: F64[n]`** — `transpose` expects rank 2. Rank `[1]` ≠ rank `[2]` → `RankMismatch`. ✓

**`flatten(a)` where `a: F64[m, n]`, `m`/`n` are vars** — `Dim::mul(Var, Var) = None` → `DimArithmeticRequiresLiterals`. ✓

**`[Some F64 | None][n]`** — union shape does NOT distribute to payloads. Stored as `UnionArray`. `when xs[0] is ...` works; `when xs is ...` fails ("not a union"). ✓

**`Int[0]`** — valid. Fold returns accumulator. Indexing `arr[0]` is runtime OOB. ✓

**`when` on shaped union** — `when xs is ...` where `xs: [Some F64 | None][n]` → type error ("has type `[...][3]`, not a union"). User must index first: `when xs[i] is ...`. ✓

**`Task F64[n] [IoErr Str]`** — fully valid; `Task` params can be shaped. ✓

**`fix` with shaped types** — `fix : (a -> a) -> a`. `a` can be `Int[n] -> Int[n]`; dim var `n` is part of the scheme. Works without change. ✓

**`fill(n, x)` return dim** — fresh dim var, never concretised statically. Callers must treat size as abstract or annotate: `fill(5, 0.0) : F64[5]` pins the dim via annotation. ✓

**`concat(a, b)` with var dims** — `Dim::add(Var, Var) = None` → `DimArithmeticRequiresLiterals`. ✓

**Slices with runtime bounds** — `arr[i:j]` where `i`, `j` are runtime `Int` → result dim is a fresh var. User may annotate to name it. ✓

---

## 14. Open Questions

**Q1: Reshape** — deferred. Requires type-level `m*n = p*q` constraint. Future: a runtime-checked `reshape` with opaque output type.

**Q2: Rank-polymorphic user functions** — can a user write one function that accepts any rank? Not in v1. Workaround: write per-rank overloads.

**Q3: `fill` / `tabulate` return type** — fresh unresolved dim var is sound but limits shape reasoning at call sites. Consider requiring a type annotation, or requiring a literal `n`.

**Q4: Partial application lifting** — deferred. May be revisited if the APL-style lifting-at-partial-application use case proves important.

**Q5: Multi-axis `permute`** — `permute(a, [2, 0, 1])` would require compile-time literal permutation list. Deferred.

**Q6: Packed AoS layout** — `RecordArray` is currently a pointer-array. For numeric-only records (all fields `F64` or `Int`), a packed struct layout is possible and GPU-friendly. Future codegen optimisation.
