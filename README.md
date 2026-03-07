# Coda

[![CI](https://github.com/tsuberim/coda/actions/workflows/ci.yml/badge.svg)](https://github.com/tsuberim/coda/actions/workflows/ci.yml)

A purely functional, Hindley-Milner typed language that feels like a scripting language. Types are always inferred — you never have to write them. Compiles to LLVM with reference-counted GC.

> Early stage. Parser is done; type inference and codegen are next.

## Quick look

```
greet = \name -> `Hello, {name}!`

add_two = \x y -> x + y

result = add_two(1, 2)
greet(`world`)
```

## Syntax

See [`docs/CLAUDE.md`](docs/CLAUDE.md) for the full reference.

## Build

```sh
cargo build
cargo test
```

## Usage

```sh
cargo run -- parse path/to/file.coda
```
