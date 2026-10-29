# Mulang

A program is a list of statements, run top to bottom, starting at the first
line of the file.

```mu
thoos x: int = 6;
bol(x * 7);
```

## Types

`int` is a 64-bit signed integer. `bool` is true or false.

## Variables

```mu
thoos x: int = 43;
x = x + 1;
```

`thoos` declares a variable with a type. Assignment is an expression, so
`x = y = 3` works.

## Functions

```mu
dhanda add(a: int, b: int) int {
    de a + b;
}

dhanda greet(n: int) {
    bol(n);
}

bol(add(3, 4));
greet(1);
```

`dhanda` declares a function, and `de` returns from it. The optional return
type follows the parameters. A
function with a return type must reach a `de` on every path.

Functions may be declared inside other functions and blocks, where they are
visible only from their declaration onwards. A top-level function may be called
before the line that declares it.

## Conditionals

```mu
yedi (n > 0) {
    bol(1);
} natra yedi (n < 0) {
    bol(-1);
} natra {
    bol(0);
}
```

`yedi` is if, `natra` is else.

## Loops

```mu
jabasamma (i < 10) {
    i = i + 1;
}

fanfani (thoos i: int = 0; i < 10; i = i + 1) {
    bol(i);
}
```

`jabasamma` is while, `fanfani` is for.

## Operators

| | Operators |
| --- | --- |
| Prefix | `-` `!` |
| Arithmetic | `*` `/` `%` `+` `-` |
| Comparison | `<` `<=` `>` `>=` |
| Equality | `==` `!=` |
| And | `ra` |
| Or | `wa` |
| Assignment | `=` |

Tightest first. Parentheses group.

## Comments

```mu
// to the end of the line
```

## Builtins

| Builtin | Does |
| --- | --- |
| `bol(x)` | prints an `int` |
| `bolbool(x)` | prints a `bool` |

## How the compiler works

`src/main.rs` drives four stages, each a separate module. There is no LLVM or other
compiler backend — the arm64 assembly is emitted directly, and `clang` only
assembles and links it.

| Stage | Module | Produces |
| --- | --- | --- |
| Lex and parse | `syntax/lexer.rs`, `syntax/parser.rs` | an AST (`syntax/ast.rs`) |
| Semantic analysis | `syntax/sema.rs` | name resolution and frame layout |
| Lowering | `ir/lowering.rs` | three-address IR (`ir/_ir_specs.md`) |
| Code generation | `codegen/macos_arm64.rs` | macOS arm64 assembly |

Semantic analysis resolves every identifier to its declaration and decides where
each variable lives — which frame, at which slot. It is also where undefined
names, redeclarations, type mismatches, wrong argument counts, and functions
that fall off the end without returning are reported. Because functions may nest
and see their enclosing scope, each frame carries a **static link** to its
parent, and sema records the frame depth that resolving a name has to walk.

Lowering flattens the AST into three-address instructions over unlimited virtual
registers, each assigned exactly once. Variables stay in frame slots rather than
registers, so there are no phi nodes. Code generation scales slots to bytes,
allocates real registers, and handles the calling convention: eight arguments in
registers, the rest on a 16-byte-aligned stack.

Errors from any stage go through `diagnostics.rs`, which prints the message with
the offending line and a caret span.

### A program through the pipeline

```mu
dhanda factorial(n: int) int {
    yedi (n <= 1) {
        de 1;
    }
    de n * factorial(n - 1);
}

bol(factorial(5));
```

`mulang --irtext factorial.mu` stops after lowering and writes the IR:

```
fn factorial_0, frame_size=2, 1
  ldaf r2
  str r0, r2, 0
  str r1, r2, 1
  ldaf r3
  ldr r4, r3, 1
  ldi r6, 1
  r5 <- r4 <= r6
  goif r5, yedi_3_then, yedi_3_else
yedi_3_then:
  ldi r7, 1
  ret r7
  goto yedi_3_end
yedi_3_else:
yedi_3_end:
  ldaf r8
  ldr r9, r8, 1
  ...
  r16 <- call factorial_0, r0, r14
  r10 <- r9 * r16
  ret r10
```

`mulang factorial.mu` runs the same pipeline to the end and writes
`factorial.s`. The prologue saves the frame record and opens the frame, and the
`yedi` becomes a compare and a conditional branch:

```asm
factorial_0:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    ...
    cmp x1, x2
    cset x0, le
    cbz x0, yedi_3_else
```

Assembling and running it prints `120`.

## Building and running

```bash
cargo build
cargo run -- program.mu      # writes program.s
cargo run -- --irtext program.mu   # writes program.irmu instead
```

The native backend emits macOS arm64 assembly, so assembling and running a
program needs `clang`:

```bash
clang -g program.s -o program && ./program
```

`./test.sh` does all of that for `test.mu`, and `./test.sh --ir` prints the
IR text instead.

## Tests

```bash
cargo test
```

`tests/programs.rs` compiles, assembles, and runs every `.mu` file under
`tests/programs`, comparing stdout against the `.out` file beside it. To add a
case, drop in a matching `.mu` and `.out` pair — it is picked up automatically.
