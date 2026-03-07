---
permalink: /
---
# Coda

A purely functional, Hindley-Milner typed language that feels like a scripting language.
Types are always inferred — you never have to write them.
Compiles to LLVM via reference-counted GC.

## Quick look

```
greet = \name -> `Hello, {name}!`

add_two = \x y -> x + y

result = add_two(1, 2)
greet(`world`)
```

## Syntax

### Variables

Alphanumeric names start with a letter or `_`. Convention is **snake_case**:

```
foo   _bar   my_var   greet_user
```

Symbolic names are sequences of `!@#$%^&*-+=|<>?/~.:`:

```
+   >>=   @$   ~~>
```

### Literals

```
42          -- integer
3.14        -- float
`hello`     -- string (backtick-quoted)
```

### Template strings

Backtick strings support `{expr}` interpolation.
Desugars to `++(part, ...)`.

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
f()
(\x -> x)(42)
```

### Infix

Any symbolic name can be used infix with spaces around it:

```
1 + 2
a >>= f
5 @my_op 6
```

Desugars to `op(lhs, rhs)`. Left-associative, no precedence yet (use parens).

### Blocks

Bindings scoped to a final expression. Two equivalent syntaxes:

**Paren + semicolons:**
```
(x = 1; y = x + 1; y)
```

**Indented (file-level and top of nested blocks):**
```
x = 1
y = x + 1
y
```

A file is itself a block.

### Parentheses for grouping

`(expr)` is just grouping — no block unless there's a `=` binding inside.

## Implementation docs

See `docs/internal/`.
