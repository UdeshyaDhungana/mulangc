# muji IR

Three-address IR, lowered from the AST after sema.

## Model

- Each function has unlimited virtual registers.
- Every register is assigned exactly once.
- Source variables live in frame slots, not registers (memory-slot approach). No φ nodes.
- Operands are registers only. Immediates enter only through `ldi`.
- Offsets are logical slots. Codegen converts slots to bytes.

## Program layout

```
fn <node_id>, frame_size=k, n
  ...
```

- One `fn` header per function; functions are separated by a blank line. All functions are flat, no nesting.
- `frame_size=k` is the frame size in slots; `n` is the param count.
- `r0` is the static link, `r1`…`rn` are the params. Body registers start at `r(n+1)`.
- `_main` also has `r0`, but never uses it.
- Entry block:
  ```
  ldaf rf
  str r0, rf, 0      ; static link -> slot 0
  str r1, rf, <slot> ; each param -> its slot
  ...
  ```

## Instructions

| Instruction                 | Meaning                                              |
|-----------------------------|------------------------------------------------------|
| `ldi rn, <imm>`             | `rn = imm`                                           |
| `mov ra, rb`                | `ra = rb`                                            |
| `rn <- ra op rb`            | binary op, `op` ∈ `+ - * / % == != < <= > >=`        |
| `rn <- op ra`               | unary op, `op` ∈ `- !`                               |
| `ldr rn, rm[, off]`         | `rn = mem[rm + off]` (`off` defaults to 0)           |
| `str rn, rm[, off]`         | `mem[rm + off] = rn` (`off` defaults to 0)           |
| `ldaf rn`                   | `rn` = address of current frame                      |
| `label:`                    | block label                                          |
| `goto label`                | unconditional jump                                   |
| `goif rn, l1, l2`           | jump to `l1` if `rn` ≠ 0, else `l2`                  |
| `rn <- call fn, rlink, ra, …` | call `fn` with static link `rlink` and args; result in `rn` |
| `ret rn`                    | return `rn`                                          |

## Rules

- Every block ends in `goto`, `goif` or `ret`.
- Falling off the end of a function lowers to `ldi rx, 0` then `ret rx`.
- A `call`'s result goes to the next available register. If unused, that register is simply never read.
- `&&` and `||` lower to branches. They are not ops.
- `bol` is a builtin: it lowers to an ordinary `call _debug_bol, rlink, ra`. It has no IR body; the interpreter prints `ra` natively and writes `0` to the result register.
- Static link argument depends on where the callee is defined:
  - callee nested directly inside caller → pass own frame (`ldaf`)
  - callee is a sibling → pass parent frame (caller's slot 0)
  - otherwise → walk N hops up slot 0
- Outer variables are reached by following slot 0 up the chain.

## Open items

- Syntax mixes `<-` and ARM style.
