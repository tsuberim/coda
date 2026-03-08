---
permalink: /
---
# Coda

A purely functional, Hindley-Milner typed language that feels like a scripting language.
Types are always inferred — you never have to write them.
Programs compile to native binaries via LLVM.

## Usage

```sh
coda                        # interactive REPL
coda file.coda              # interpret a file
coda -c file.coda           # compile to native binary (requires clang)
coda -c file.coda -o out    # compile with custom output path
```

## Syntax

### Comments

```
-- line comment
--- multiline
    comment ---
```

### Variables

Alphanumeric names start with a letter or `_`. Convention is **snake_case**:

```
foo   _bar   my_var
```

Symbolic names are sequences of `!@$%^&*-+=|<>?/~.:`:

```
+   >>=   @$   ~~>
```

### Literals

```
42        -- integer
3.14      -- float
`hello`   -- string (backtick-quoted)
[1, 2, 3] -- array (homogeneous, rank-1 Int[3])
[]        -- empty array (Int[0])
```

### Template strings

Backtick strings support `{expr}` interpolation, desugared to `++` calls.

```
`Hello, {name}!`
`{a} + {b} = {a + b}`
```

### Lambda

```
\x -> x
\x y z -> x + y + z
```

### Application

Arguments are always in explicit parentheses. No space before `(`.

```
f(x)
f(x, y, z)
(\x -> x)(42)
```

### Infix

Any symbolic name can be used infix with spaces around it:

```
1 + 2
a >>= f
```

Desugars to `op(lhs, rhs)`. Left-associative, no precedence (use parens).

### Blocks

Bindings scoped to a final expression:

```
(x = 1; y = x + 1; y)
```

At file level, newlines act as separators:

```
x = 1
y = x + 1
y
```

`(expr)` with no binding is just grouping.

### Records

```
point = {x: 3, y: 4}
point.x                      -- field access
{x, y: height} = point       -- destructure: x bound as x, y bound as height
```

Record types are row-polymorphic — a function accepting `{x: Int | *}` works on any record with at least an `x: Int` field.

### Tagged unions

```
-- construction
None
Some 42
Circle {r: 5}

-- elimination
when shape is
  Circle r -> `circle, r={r}`
  Rect     -> `rectangle`
  otherwise `unknown`
```

Tags with no payload carry an implicit unit payload. Unions are row-polymorphic: a value of type `[Some Int | *]` is accepted wherever `[Some Int, None | *]` is expected.

### Arrays and shaped types

Arrays are rank-polymorphic. The type `Int[3]` means a 1-D array of 3 integers; `F64[3, 4]` is a 3×4 matrix of floats. The shape is always statically known.

```
xs = [1, 2, 3]          -- Int[3]
ys = [4, 5, 6]
xs + ys                 -- Int[3]  (element-wise, same shape)
xs + 1                  -- Int[3]  (scalar broadcast)

xs[0]                   -- Int    (index reduces rank)
xs[1:3]                 -- Int[2] (slice: literal bounds)
```

Type annotations use `T[d1, d2, ...]` syntax:

```
v : Int[3]
v = [1, 2, 3]

mat : F64[3, 4]
```

Dim variables (lowercase) are generalized:

```
\x -> x + x   -- Int[n] -> Int[n]  (for any n)
```

Scalar functions are automatically **lifted** to arrays:

```
f : Int -> F64
f([1, 2, 3])    -- F64[3]
```

Broadcasting follows prefix rules: shapes must be equal, or one must be a suffix-prefix of the other; otherwise a `BroadcastFail` type error is raised.

List operations still work on rank-1 arrays:

```
len(xs)                       -- 3
0 :: xs                       -- [0, 1, 2, 3]
map(\x -> x + x, xs)         -- [2, 4, 6]
fold(\acc x -> acc + x, 0, xs)  -- 6
[1] <> [2, 3]                 -- [1, 2, 3]

-- head and tail return [None | Some val]
when head(xs) is
  Some x -> x
  None   -> 0
```

`::` is also available as `cons`; `<>` as `append`.

### Type annotations

Optional; enforced when present.

```
-- annotate before or after a binding
f : Int -> Int
f = \x -> x + 1

-- shaped types
xs : Int[3]
xs = [1, 2, 3]

mat : F64[3, 4]

-- in-block
(n : Int; n = 5; n + 1)
```

Annotating an already-bound name unifies the annotation with its existing type — error if incompatible.

### Modules

A file evaluates to its last expression. The convention is to end with a record of exported names.

```
-- math.coda
double = \n -> n + n
{double: double}

-- main.coda
math = import `math.coda`
math.double(21)
```

`import` is a keyword — the path must be a plain backtick string (no interpolation). The file is read, parsed, type-checked, and evaluated once; the result is cached by canonical path. Running `coda file.coda` uses the same path.

## Type system

- Hindley-Milner with let-polymorphism
- Rank-polymorphic shaped types: `Int[n]`, `F64[m, n]`, scalar = `Int` (zero dims)
- Automatic lifting of scalar functions to shaped types; broadcasting for multi-arg ops
- Row-polymorphic records: `{field: T | r}`
- Row-polymorphic unions: `[Tag T | r]`
- Open union construction: `Some 5 : [Some Int | *]`
- Closed union from `when` without `otherwise`

## Task monad

Coda has no mutable state or implicit effects. IO is represented as `Task ok err` — a suspended computation that either succeeds with a value of type `ok` or fails with a value of type `err`.

### Primitives

```
ok(v)            -- wrap v in a successful Task  : a -> Task a e
t >>= f          -- sequence: run t, pass result to f  : Task a e -> (a -> Task b e) -> Task b e
fail(e)          -- always-failing Task  : e -> Task a e
catch(t, f)      -- recover from failure: run t; on error call f  : Task a e -> (e -> Task a f) -> Task a f
```

`>>=` is also available as `then`.

### Monadic bind syntax

Inside any block or file, `x <- expr` is sugar for `then`:

```
(
  line <- read_line
  print(`You typed: {line}`)
)
```

Desugars right-to-left: `then(read_line, \line -> print(...))`.

`_` discards the result: `_ <- some_task`.

### Error accumulation

The error type is a row-polymorphic union. Each `<-` step unifies the error row, so if a block can fail in multiple ways the type reflects all of them:

```
Task Str [IoErr Str, NetworkErr Str | r]
```

### Error recovery

```
safe = catch(might_fail, \err ->
  when err is
    NotFound -> ok(`default`)
)
```

The handler receives the error value and returns a new Task. The result error type `f` is independent of the original `e`, so recovery can change the error type entirely.

### IO builtins

| Name        | Type                              | Description            |
|-------------|-----------------------------------|------------------------|
| `print`     | `Str -> Task {} [IoErr Str \| r]` | Print a line to stdout |
| `read_line` | `Task Str [IoErr Str \| r]`       | Read a line from stdin |

## Builtins

| Name     | Type                                    | Description              |
|----------|-----------------------------------------|--------------------------|
| `++`     | `Str Str -> Str`                        | String concatenation     |
| `+`      | `Int Int -> Int`                        | Integer addition (lifted to arrays) |
| `-`      | `Int Int -> Int`                        | Integer subtraction (lifted) |
| `*`      | `Int Int -> Int`                        | Integer multiplication (lifted) |
| `==`     | `a -> a -> [False \| True]`             | Structural equality      |
| `fix`    | `(a -> a) -> a`                         | Fixed-point (recursion)  |
| `::` / `cons`   | `a -> List(a) -> List(a)`        | Prepend element          |
| `head`   | `List(a) -> [None \| Some a]`           | First element            |
| `tail`   | `List(a) -> [None \| Some List(a)]`     | Rest of list             |
| `len`    | `List(a) -> Int`                        | Length                   |
| `map`    | `(a -> b) -> List(a) -> List(b)`        | Transform elements       |
| `fold`   | `(b -> a -> b) -> b -> List(a) -> b`    | Left fold                |
| `<>` / `append` | `List(a) -> List(a) -> List(a)` | Concatenate two lists    |
| `list_of`   | `Int -> a -> List(a)`                | N copies of a value      |
| `list_init` | `Int -> (Int -> a) -> List(a)`       | N items via index fn     |
| `ones`   | `Int Int -> F64[m, n]`                  | m×n matrix of 1.0        |
| `zeros`  | `Int Int -> F64[m, n]`                  | m×n matrix of 0.0        |
| `matmul` | `F64[m, k] F64[k, n] -> F64[m, n]`     | Matrix multiplication    |

## Implementation docs

See `docs/internal/`.
