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

## 8. Char type

`'a'` literals, `char_code : Char -> Int`, `code_char : Int -> Char`. Needed for proper string processing. Adds `Char` to the runtime value tag and type system; strings can be iterated as `List(Char)` without changing their internal rep.

## 9. Named recursion (without `fix`)

Allow `f = \x -> ... f(x) ...` at file level. Currently requires `fix`. The compiler can detect this pattern and emit a labeled loop or a forward-declared function pointer, avoiding the `fix` overhead entirely.

## ✓ 10. Improved error messages

Spans added to every `Expr` node (`Spanned<T> = (T, Range<usize>)`). `infer_inner` threads spans through and errors carry `(TypeError, Span)`. `InferError::render` uses `ariadne` to produce underlined diagnostics with file/line/column context.

## 11. Debug intrinsic

`debug : a -> a` — prints the value to stderr and returns it unchanged. Threads through pipelines without disrupting types. Small addition; used constantly during development. Compiler emits a no-op in release mode (`-O`).

## 12. Env / process builtins

`get_env : Str -> Task [Some Str | None] []`, `args : Task (List Str) []`, `exit : Int -> Task {} []`. Needed for any real CLI program. Runtime-only additions; no type system changes.

## 13. File I/O

`read_file : Str -> Task Str [IoErr Str | r]`, `write_file : Str Str -> Task {} [IoErr Str | r]`, `append_file`. Backed by the `Task` monad — pure programs stay pure, IO is explicit. Runtime additions only.

## 14. Operator sections

`(+ 1)` desugars to `\x -> x + 1`; `(x +)` to `\y -> x + y`. Natural in functional style, eliminates many short lambdas. Parser change only — no type system or runtime impact.

## 15. Pipe operator

`x |> f` desugars to `f(x)`. Chains transformations left-to-right without nesting. Left-associative: `x |> f |> g` → `g(f(x))`. Parser-level desugaring.

## 16. Float type

`42.0`, `+.`, `-.`, `*.`, `/.`. Separate from `Int` — no implicit coercion. Adds `Float` to the runtime value tag and type system.

## 17. Numeric precision types

Full set of native numeric primitives: `I8`, `I16`, `I32`, `I64`, `U8`, `U16`, `U32`, `U64`, `F32`, `F64`. Current `Int` and `Float` become aliases for `I64` and `F64`. No implicit coercion between widths — explicit `to_f64`, `to_i32`, etc. LLVM already has native support for all of these; the main work is in the type system and runtime value tag. Prerequisite for tensors and GPU.

## 18. Type aliases

`type Point = {x: I64, y: I64}`. Purely a naming convenience — no new types at runtime, just substituted in the type checker. Improves error messages and annotations significantly. Parameterised aliases: `type Pair(a, b) = {fst: a, snd: b}`.

## 19. Newtype / opaque types

`type UserId = opaque I64`. Same runtime rep as the wrapped type but distinct in the type system — `UserId` and `I64` don't unify. Zero-cost abstraction for domain modelling. `wrap` and `unwrap` are the only operations; both only available in the defining module.

## 20. Map / Dict

`Map(k, v)`: `map_empty`, `map_insert`, `map_lookup`, `map_delete`, `map_fold`. Backed by a hash map in the runtime. Essential data structure; currently hand-rolled with lists. `map_lookup` returns `[Some v | None]`.

## 21. FFI

`foreign "c" sin(F64) -> F64` — declare an external C symbol with a Coda type signature. The type annotation is the entire contract; no wrappers generated. Lets Coda call any C library directly. Needed to unlock the C ecosystem without reimplementing everything in the runtime.

## 22. HTTP client

`http_get : Str -> Task Str [HttpErr Str | r]`, `http_post`. Backed by `libcurl` via FFI or a minimal runtime implementation. Wraps responses in `Task`; network errors surface as typed failures. Opens up scripting and data-fetching use cases.

## 23. Typeclasses / traits

`class Show a where show : a -> Str`. Principled ad-hoc polymorphism — the clean solution to `==`, `show`, `compare`, and numeric ops across types. Requires dictionary passing or monomorphisation in codegen. Large feature; unlocks a proper standard library.

## 24. Lazy / infinite streams

`Stream(a)`: deferred cons cells — `stream_from`, `take`, `drop`, `zip`, `stream_map`. Complements the strict `List` type. Enables infinite sequences, generators, and pipeline processing without materialising the whole list. Backed by thunks (zero-arg closures) in the runtime.

## 25. Async IO / event loop

Non-blocking IO via epoll/kqueue under the `Task` monad. `parallel` (item 26) handles CPU parallelism; this handles IO concurrency — many tasks in flight, one thread. `Task` already models effects; the scheduler is the only new piece. Needed for HTTP servers, multiplexed IO, and high-concurrency scripts.

## 26. Mutable references (optional)

`Ref(a)` — `new_ref(v)`, `read_ref(r)`, `write_ref(r, v)`. Breaks pure semantics but needed for efficient imperative algorithms. Wrap in `Task` to keep effects explicit: `write_ref : Ref a -> a -> Task {} []`.

## 27. Linter and formatter

`coda fmt` — opinionated, zero-config formatter: canonical indentation, spacing, trailing commas, import ordering. One style, no arguments. `coda lint` — static checks beyond type errors: unused bindings, redundant `otherwise`, shadowed variables, suspicious patterns. Both operate on the existing AST; formatter pretty-prints it, linter walks it for known anti-patterns. Like `gofmt` — the community converges on one style because there is only one style.

## 28. LSP

Language server implementing the Language Server Protocol. Hover shows inferred types, go-to-definition navigates to binding sites, inline errors mirror the compiler's ariadne output. The type information is already computed — LSP is mostly plumbing. Unlocks VS Code, Neovim, Zed, etc.

## 29. Package manager

`coda add https://example.com/lib.coda#sha256:...` — manage a project's pinned URL imports in a `coda.lock` file. Extends URL imports (item 5) with a CLI for adding, updating, and auditing dependencies. No central registry required; any HTTPS URL is a package.

## 30. WASM target

Emit WebAssembly instead of native LLVM IR. Same frontend and type system; different codegen backend. Enables running Coda in browsers, edge runtimes, and sandboxed environments. Pure functions map cleanly; `Task`-based IO bridges to host imports.

## 31. Tensor types with dimension checking

`Tensor(elem, dims)` where `dims` is a type-level shape, e.g. `Tensor(Float, [3, 4])`. Dimensions tracked as phantom nat literals in the type system — mismatched shapes are caught at compile time, not runtime. Operations like `matmul` carry dimension constraints (`[m, k] × [k, n] → [m, n]`) enforced by unification. Requires extending HM with a lightweight kind for nat literals.

## 32. Multi-threading

Spawn parallel tasks with `parallel : List(Task a e) -> Task (List a) e`. Pure, immutable values are safe to share across threads with no locks — RC is the only hazard, replaced with atomic RC for shared values. The runtime manages a thread pool; the type system ensures no mutable state crosses thread boundaries. Builds naturally on the `Task` monad: threads are just concurrent effects.

## 33. GPU acceleration

Lower `Tensor` operations to GPU kernels. Pure tensor expressions are effect-free and trivially parallelisable — the compiler schedules them onto the GPU automatically. `gpu_map`, `gpu_matmul`, and friends emit LLVM NVPTX/AMDGPU IR or call into a runtime that dispatches via Metal/CUDA/WebGPU. Wrapped in `Task` where data transfer is involved.
