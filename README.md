# Coda

[![CI](https://github.com/tsuberim/coda/actions/workflows/ci.yml/badge.svg)](https://github.com/tsuberim/coda/actions/workflows/ci.yml)

A purely functional, Hindley-Milner typed language that feels like a scripting language. Types are always inferred — you never have to write them.

## Quick look

```
-- tagged unions
describe = \shape ->
  when shape is
    Circle r -> `circle, radius {r}`
    Rect     -> `rectangle`
    otherwise `unknown`

-- records
point = {x: 3, y: 4}
dist  = point.x + point.y

-- IO via Task monad
name <- read_line
print(`Hello, {name}!`)
```

## Features

- **Full type inference** — Hindley-Milner with row-polymorphic records and unions
- **Tagged unions** — `Tag payload`, `when x is Tag y -> ...`, open/closed rows
- **Records** — `{x: 1, y: 2}`, `.field` access, structural subtyping
- **Lists** — `[1, 2, 3]`, `cons`, `head`, `tail`, `map`, `fold`, `append`, `len`
- **Type annotations** — optional, enforced: `f : Int -> Int`, `xs : List(Int)`
- **Modules** — `import \`file.coda\``, cached by canonical path
- **Task monad** — `ok`, `>>=`, `fail`, `catch`; `x <- task` bind syntax; row-polymorphic error types
- **REPL** — persistent history, colored output, `:env`, `:clear`
- **Interpreter** — tree-walking eval

## Build & run

```sh
cargo build
cargo test

cargo run                        # REPL
cargo run -- file.coda           # interpret a file
```

## Syntax reference

See [`docs/CLAUDE.md`](docs/CLAUDE.md).
