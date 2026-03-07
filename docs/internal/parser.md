# Parser

Char-level recursive-descent parser using `chumsky 0.9`.

## AST (`src/ast.rs`)

```
Expr = Var(String)
     | Lam(Vec<String>, Expr)
     | App(Expr, Vec<Expr>)
     | Lit(Int | Str)
     | Block(Vec<BlockItem>, Expr)
     | Record(Vec<(String, Expr)>)
     | Field(Expr, String)
     | Tag(String, Option<Expr>)
     | When(Expr, Vec<(String, Option<String>, Expr)>, Option<Expr>)
     | Import(String)

BlockItem = Bind(String, Expr)
          | Ann(String, TypeExpr)
          | MonadicBind(String, Expr)   -- transient; desugared before leaving the parser

TypeExpr = Var(String)
         | Con(String)
         | Fun(Vec<TypeExpr>, TypeExpr)
         | Record(Vec<(String, TypeExpr)>, Option<String>)
         | Union(Vec<(String, Option<TypeExpr>)>, Option<String>)
```

Desugars at parse time:
- `a + b` → `App(Var("+"), [a, b])`
- `` `hi {x}` `` → `App(Var("++"), [Lit("hi "), x])`
- `None` → `Tag("None", None)` (payload filled with unit at type-check time)
- `{name, age: y} = e` → `Bind("#0", e); Bind("name", Field(Var("#0"), "name")); Bind("y", Field(Var("#0"), "age"))` (`#N` is impossible in user syntax)
- `x <- e` in a block/file → `then(e, \x -> rest)` via right-to-left fold in `desugar_block`; `_` uses a fresh `#N` tmp

## Key design decisions

**Atom padding:** atoms strip *leading* whitespace/comments only. Trailing whitespace
is left for the infix `ws1` check. `chumsky`'s `repeated()` backtracks on failure, so
`ws1` before a symbolic name is safe — if `sym_name` fails the consumed whitespace is restored.

**`=` vs `==`:** the binding `=` is parsed by consuming the full greedy symbolic token
and checking it equals exactly `"="`. `==` is never confused with two `=` signs.

**Application vs infix:** application `f(x)` requires no whitespace before `(`.
Infix `a + b` requires `ws1` on both sides. These two rules make the grammar unambiguous.

**Empty blocks:** `block_body` with no bindings returns the inner expression directly
(grouping, no `Block` node).

**Comments as whitespace:** `ws1()` consumes whitespace *and* comments so that
`-- comment` at the start of a line is never mistaken for an infix `--` operator.

**Type annotations in type `Fun` parser:** multi-arg atoms (`Int Int -> Int`) are
chained with `hws1()` (horizontal whitespace only) to prevent consuming the newline
before the next binding and treating its name as a type argument.

**Tag payload disambiguation:** in type position, `[Some Int, None]` is unambiguous
because a `,` or `]` following the tag name (no horizontal whitespace) means no
payload, while whitespace + non-whitespace char triggers payload parsing.
In expression position the same rule applies but also rejects uppercase first char,
preventing `[Foo Bar]` from being parsed as `Foo` with payload `Bar` (both tags).

## Entry points

- `expr_parser()` — single expression, used inside blocks and as the REPL expression arm.
- `file_parser()` — full file as a top-level block (newline or `;` separators, final expression last).
- `repl_parser()` — one of: `Items(Vec<BlockItem>)`, expression, or `Nop` (empty/comment).
- `type_expr_parser()` — surface type syntax for annotations.

All three share `item_parser(ep, ident)` for parsing bindings, annotations, and record destructures.
`file_parser` and `repl_parser` use `expr_parser().boxed()` as the expression sub-parser.

## Reserved symbols

`->` (lambda/function arrow), `=` (binding), and `<-` (monadic bind) are reserved and rejected by `sym_name()`.
Everything else in `!@$%^&*-+=|<>?/~.:` is valid as a symbolic name.

Reserved words: `when`, `is`, `otherwise`, `import`.
