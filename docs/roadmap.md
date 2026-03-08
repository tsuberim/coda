# Roadmap

Roughly in order. Each item builds on the previous.

## ✓ 1. Reference-counting GC

`rc: int32_t` in `CodaVal`, `retain`/`release` in the runtime. Retain on capture/store, release at last use via owned-set liveness in codegen. No cycle collector needed — immutable values can't form cycles.

## ✓ 2. Tail call optimization

`fix`-based recursion uses a trampoline: `fix_shim` loops instead of recursing; `coda_fix_tail_call` bounces when inside a trampoline frame. `tail: bool` threaded through codegen marks genuine tail positions.

## ✓ 3. Compiled Tasks / IO

`Task ok err` represented as a zero-arg closure returning `[Ok val | Err e]`. `ok`, `>>=`, `fail`, `catch`, `print`, `read_line` wired as compiled builtins. `coda_run_task` called from `coda_main` when the top-level type is `Task`.

## ✓ 4. Compiled imports / modules

Whole-program: each `import \`path\`` is compiled into a `@coda_module_N()` function inlined into the same `.ll` file. Deduplication via a path cache in `Compiler`; cycle detection via an in-progress set. Paths resolved with `canonicalize` (relative to CWD), matching interpreter behaviour.

## 5. Integer comparison and division

Add `<`, `>`, `<=`, `>=`, `/`, `%` builtins. Needed for non-trivial programs. Straightforward additions to both the interpreter and codegen.

## 6. Strings: length, slice, parse

`str_len`, `str_slice`, `int_to_str`, `str_to_int`. Needed for real programs. Runtime-only additions.

## 7. Named recursion (without `fix`)

Allow `f = \x -> ... f(x) ...` at file level. Currently requires `fix`. The compiler can detect this pattern and emit a labeled loop or a forward-declared function pointer, avoiding the `fix` overhead entirely.

## 8. Improved error messages

Type errors currently print raw type expressions. Add source spans to the AST (from chumsky), thread them through inference, and use `ariadne` (already a dep) to render underlined error messages pointing at the source.

## 9. Float type

`42.0`, `+.`, `-.`, `*.`, `/.`. Separate from `Int` — no implicit coercion. Adds `Float` to the runtime value tag and type system.

## 10. Mutable references (optional)

`Ref(a)` — `new_ref(v)`, `read_ref(r)`, `write_ref(r, v)`. Breaks pure semantics but needed for efficient imperative algorithms. Wrap in `Task` to keep effects explicit: `write_ref : Ref a -> a -> Task {} []`.
