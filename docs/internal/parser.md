# Parser

Char-level recursive-descent parser using `chumsky 0.9`.

## AST (`src/ast.rs`)

```
Expr = Var(String)
     | Lam(Vec<String>, Expr)
     | App(Expr, Vec<Expr>)
     | Lit(Int | Float | Str)
     | Block(Vec<(String, Expr)>, Expr)
```

No `Infix` or `Template` nodes — both desugar at parse time:
- `a + b` → `App(Var("+"), [a, b])`
- `` `hi {x}` `` → `App(Var("++"), [Lit("hi "), x])`

## Key design decisions

**Atom padding:** atoms strip *leading* whitespace only. Trailing whitespace is left
for the infix `ws1` check. `chumsky`'s `repeated()` backtracks on failure, so `ws1`
before a symbolic name is safe — if `sym_name` fails, `ws1`'s consumed whitespace
is restored.

**`=` vs `==`:** the binding `=` is parsed by consuming the full greedy symbolic
token and checking it equals exactly `"="`. This means `==` is never confused with
two adjacent `=` signs.

**Application vs infix:** application `f(x)` requires no whitespace before `(`.
Infix `a + b` requires at least one whitespace character on both sides of the operator.
These two rules make the grammar unambiguous.

**Empty blocks:** `block_body` with no bindings returns the inner expression directly
(grouping), not a `Block` node.

## Entry points

- `expr_parser()` — parses a single expression.
- `file_parser()` — parses a full file as a top-level block (newline or `;` separated
  bindings, final expression last).

## Reserved symbols

`->` (lambda arrow) and `=` (binding) are reserved and rejected by `sym_name()`.
Everything else in `!@#$%^&*-+=|<>?/~.:` is fair game.
