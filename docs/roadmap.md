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

## ✓ 8. Improved error messages

Spans added to every `Expr` node (`Spanned<T> = (T, Range<usize>)`). `infer_inner` threads spans through and errors carry `(TypeError, Span)`. `InferError::render` uses `ariadne` to produce underlined diagnostics with file/line/column context.

## 9. Float type

`42.0`, `+.`, `-.`, `*.`, `/.`. Separate from `Int` — no implicit coercion. Adds `Float` to the runtime value tag and type system.

## 10. Mutable references (optional)

`Ref(a)` — `new_ref(v)`, `read_ref(r)`, `write_ref(r, v)`. Breaks pure semantics but needed for efficient imperative algorithms. Wrap in `Task` to keep effects explicit: `write_ref : Ref a -> a -> Task {} []`.

## 11. Tensor types with dimension checking

`Tensor(elem, dims)` where `dims` is a type-level shape, e.g. `Tensor(Float, [3, 4])`. Dimensions tracked as phantom nat literals in the type system — mismatched shapes are caught at compile time, not runtime. Operations like `matmul` carry dimension constraints (`[m, k] × [k, n] → [m, n]`) enforced by unification. Requires extending HM with a lightweight kind for nat literals.

## 12. Multi-threading

Spawn parallel tasks with `parallel : List(Task a e) -> Task (List a) e`. Pure, immutable values are safe to share across threads with no locks — RC is the only hazard, replaced with atomic RC for shared values. The runtime manages a thread pool; the type system ensures no mutable state crosses thread boundaries. Builds naturally on the `Task` monad: threads are just concurrent effects.

## 13. GPU acceleration

Lower `Tensor` operations to GPU kernels. Pure tensor expressions are effect-free and trivially parallelisable — the compiler schedules them onto the GPU automatically. `gpu_map`, `gpu_matmul`, and friends emit LLVM NVPTX/AMDGPU IR or call into a runtime that dispatches via Metal/CUDA/WebGPU. Wrapped in `Task` where data transfer is involved.
