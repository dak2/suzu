# Suzu core design

Frozen. Change it only at an end-of-milestone review, never mid-implementation.

## Pillars

1. Simple syntax and fast compilation, like Go
2. Memory safety without a GC, like Rust
3. Runs without a runtime

## Premises

- No garbage collector
- Borrows are second-class (never stored, always end within a nested scope); owned values are first-class
- AOT to native code via Cranelift, with or without an OS
- Targets: OS kernels, language VMs, and low-level tools

## Policies

Every feature must satisfy all five.

1. **Explicit, minimal syntax.** Nothing happens that the code doesn't say. One way to write each thing
2. **Simple memory safety.** Rust-level safety with little to learn
3. **Fast compilation.** Designed in from the start, not optimized later
4. **Simple concurrency.** Structured, with data races rejected at compile time
5. **No hidden costs.** No allocation, runtime work, or control flow the code doesn't show

Policies 1 and 3 stay separate so that "it doesn't slow compilation" is never enough to admit a feature.

## Core rules

1. **Move by default.** `Int`, `Float`, `Bool`, and `Unit` copy; everything else moves unless it `derives(Copy)`
2. **Borrows are second-class.** They exist only as arguments: unmarked (shared) or `mut` (exclusive). They cannot be stored. A method may return part of `self` as a borrow that lives until the end of the caller's statement (a projection)
3. **Checking is function-local.** Borrows always nest inside calls, so there are no lifetime annotations
4. **Signatures are explicit, bodies are inferred.**
5. **No long-lived references.** Use projections, non-escaping blocks, or arena handles. `for` is the main way to iterate

Moves are written with `take`, and `mut` is written at the call site too.

## Syntax

- Braces, newline-terminated statements, `//` comments
- Calls always use parentheses; blocks are passed inside them
- One kind of variable, `var`
- Deterministic parsing: no syntax that needs type information or more than two tokens of lookahead

## What you cannot do

| Not possible | Instead |
|---|---|
| Store a reference | `clone`, or keep an arena handle |
| Data structures holding references (graphs, caches) | `Arena` + generational `Handle` |
| External iterators | `for`, or a cursor plus projections |
| Global mutable state | Pass it down with `mut` |
| Escaping closures | Structs that `take` their captures, or capture-free `fn` values |
| Return a borrow of one of two arguments | Return which one, and let the caller choose |
| Classes and inheritance | Structs with `impl` methods, protocols, or structs of `fn` values |
| Lifetimes, macros, overloading, union types, implicit conversions, exceptions, unwinding, dynamic dispatch, GC | — |

## Guarantees

- **Compile time:** no use-after-free, double free, data race, null, or use after move
- **Run time:** stale handles return `none`; out-of-bounds, overflow, and division by zero panic (abort)
- **Not covered:** consistency of state shared across processes, validity of external input, deadlock
