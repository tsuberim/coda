---
permalink: /
---
# Coda

A purely functional, Hindley-Milner typed language that feels like a scripting language.
Types are always inferred — you never have to write them.

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
`hello`   -- string (backtick-quoted)
[1, 2, 3] -- list (homogeneous, any element type)
[]        -- empty list
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

### Lists

```
xs = [1, 2, 3]
len(xs)                     -- 3
cons(0, xs)                 -- [0, 1, 2, 3]
map(\x -> x + x, xs)       -- [2, 4, 6]
fold(\acc x -> acc + x, 0, xs)  -- 6
append([1], [2, 3])         -- [1, 2, 3]

-- head and tail return [None | Some val]
when head(xs) is
  Some x -> x
  None   -> 0
```

All elements must have the same type (inferred). The type is `List(a)`.

### Type annotations

Optional; enforced when present.

```
-- annotate before or after a binding
f : Int -> Int
f = \x -> x + 1

-- parameterized types use Con(arg, ...) syntax
xs : List(Int)
xs = [1, 2, 3]

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
- Row-polymorphic records: `{field: T | r}`
- Row-polymorphic unions: `[Tag T | r]`
- Open union construction: `Some 5 : [Some Int | *]`
- Closed union from `when` without `otherwise`

## Task monad

Coda has no mutable state or implicit effects. IO is represented as `Task ok err` — a suspended computation that either succeeds with a value of type `ok` or fails with a value of type `err`.

### Primitives

```
ok(v)          -- wrap v in a successful Task  : a -> Task a e
then(t, f)     -- sequence: run t, pass result to f  : Task a e -> (a -> Task b e) -> Task b e
fail(e)        -- always-failing Task  : e -> Task a e
```

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

### IO builtins

| Name        | Type                              | Description            |
|-------------|-----------------------------------|------------------------|
| `print`     | `Str -> Task {} [IoErr Str \| r]` | Print a line to stdout |
| `read_line` | `Task Str [IoErr Str \| r]`       | Read a line from stdin |

## Builtins

| Name     | Type                                    | Description              |
|----------|-----------------------------------------|--------------------------|
| `++`     | `Str Str -> Str`                        | String concatenation     |
| `+`      | `Int Int -> Int`                        | Integer addition         |
| `cons`   | `a -> List(a) -> List(a)`               | Prepend element          |
| `head`   | `List(a) -> [None \| Some a]`           | First element            |
| `tail`   | `List(a) -> [None \| Some List(a)]`     | Rest of list             |
| `len`    | `List(a) -> Int`                        | Length                   |
| `map`    | `(a -> b) -> List(a) -> List(b)`        | Transform elements       |
| `fold`   | `(b -> a -> b) -> b -> List(a) -> b`    | Left fold                |
| `append`    | `List(a) -> List(a) -> List(a)`         | Concatenate two lists    |
| `list_of`   | `Int -> a -> List(a)`                   | N copies of a value      |
| `list_init` | `Int -> (Int -> a) -> List(a)`          | N items via index fn     |

## Implementation docs

See `docs/internal/`.
