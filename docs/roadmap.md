# Roadmap

Roughly in order. Each item builds on the previous.

## 1. Reference-counting GC

Currently the compiler leaks. Add `rc: int32_t` to `CodaVal`, with `retain`/`release` in the runtime. Insert retain/release calls during codegen via liveness analysis: retain on capture/store, release at last use. No cycle collector needed — immutable values can't form cycles, and `fix` creates fresh closures on each call rather than a static heap cycle.

## 2. Tail call optimization

Recursive `fix` uses the C stack and will overflow on deep recursion. Detect self-tail-calls in codegen and emit LLVM `musttail` or convert to a loop. Enough to make `factorial`, `fib`, and list traversals stack-safe.

## 3. Compiled Tasks / IO

Tasks are currently interpreter-only. In compiled code, represent `Task ok err` as a closure `() -> CodaVal*` that either returns a value or calls a runtime error handler. Wire up `ok`, `>>=`, `fail`, `catch`, `print`, `read_line` as compiled builtins. This unlocks compiling IO programs end-to-end.

## 4. Compiled imports / modules

`import \`path\`` needs to work in compiled mode. Options:
- **Whole-program**: resolve all imports at compile time, inline everything into one `.ll` file. Simple, no linking complexity.
- **Separate compilation**: compile each module to an object file, link. Requires a stable ABI for `CodaVal*` across modules (already have it).

Start with whole-program.

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
