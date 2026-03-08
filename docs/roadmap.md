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

## 5. URL imports with hash pinning

`import \`https://example.com/math.coda#sha256:abc123...\`` — fetch remote modules over HTTPS, verify the SHA-256 hash of the content before compiling. Hash mismatch is a hard compile error. Cached by hash on disk (e.g. `~/.cache/coda/`), so subsequent builds are offline and deterministic. No hash = fetch allowed but warned. Gives reproducible dependency pinning without a package manager.

## 6. Integer comparison and division

Add `<`, `>`, `<=`, `>=`, `/`, `%` builtins. Needed for non-trivial programs. Straightforward additions to both the interpreter and codegen.

## 7. Strings: length, slice, parse

`str_len`, `str_slice`, `int_to_str`, `str_to_int`. Needed for real programs. Runtime-only additions.

## 8. Named recursion (without `fix`)

Allow `f = \x -> ... f(x) ...` at file level. Currently requires `fix`. The compiler can detect this pattern and emit a labeled loop or a forward-declared function pointer, avoiding the `fix` overhead entirely.

## ✓ 9. Improved error messages

Spans added to every `Expr` node (`Spanned<T> = (T, Range<usize>)`). `infer_inner` threads spans through and errors carry `(TypeError, Span)`. `InferError::render` uses `ariadne` to produce underlined diagnostics with file/line/column context.

## 10. Float type

`42.0`, `+.`, `-.`, `*.`, `/.`. Separate from `Int` — no implicit coercion. Adds `Float` to the runtime value tag and type system.

## 11. Numeric precision types

Full set of native numeric primitives: `I8`, `I16`, `I32`, `I64`, `U8`, `U16`, `U32`, `U64`, `F32`, `F64`. Current `Int` and `Float` become aliases for `I64` and `F64`. No implicit coercion between widths — explicit `to_f64`, `to_i32`, etc. LLVM already has native support for all of these; the main work is in the type system and runtime value tag. Prerequisite for tensors and GPU.

## 12. Mutable references (optional)

`Ref(a)` — `new_ref(v)`, `read_ref(r)`, `write_ref(r, v)`. Breaks pure semantics but needed for efficient imperative algorithms. Wrap in `Task` to keep effects explicit: `write_ref : Ref a -> a -> Task {} []`.

## 13. Tensor types with dimension checking

`Tensor(elem, dims)` where `dims` is a type-level shape, e.g. `Tensor(Float, [3, 4])`. Dimensions tracked as phantom nat literals in the type system — mismatched shapes are caught at compile time, not runtime. Operations like `matmul` carry dimension constraints (`[m, k] × [k, n] → [m, n]`) enforced by unification. Requires extending HM with a lightweight kind for nat literals.

## 14. Multi-threading

Spawn parallel tasks with `parallel : List(Task a e) -> Task (List a) e`. Pure, immutable values are safe to share across threads with no locks — RC is the only hazard, replaced with atomic RC for shared values. The runtime manages a thread pool; the type system ensures no mutable state crosses thread boundaries. Builds naturally on the `Task` monad: threads are just concurrent effects.

## 15. Linter and formatter

`coda fmt` — opinionated, zero-config formatter: canonical indentation, spacing, trailing commas, import ordering. One style, no arguments. `coda lint` — static checks beyond type errors: unused bindings, redundant `otherwise`, shadowed variables, suspicious patterns. Both operate on the existing AST; formatter pretty-prints it, linter walks it for known anti-patterns. Like `gofmt` — the community converges on one style because there is only one style.

## 16. GPU acceleration

Lower `Tensor` operations to GPU kernels. Pure tensor expressions are effect-free and trivially parallelisable — the compiler schedules them onto the GPU automatically. `gpu_map`, `gpu_matmul`, and friends emit LLVM NVPTX/AMDGPU IR or call into a runtime that dispatches via Metal/CUDA/WebGPU. Wrapped in `Task` where data transfer is involved.
