# mulang

A toolchain for the **muji** programming language, written in Rust. The
language started from [muji-lang](https://github.com/UdeshyaDhungana/muji-lang);
this is a rewrite of the implementation, and the language has been trimmed
along the way — see [Not in the language](#not-in-the-language).

Compiles to macOS ARM64 assembly. `mulang program.muji` writes `program.s`,
which you then assemble and link:

```bash
cargo build --release

./target/release/mulang program.muji
clang -o program program.s
./program
```

`./test.sh` does all three for `test.muji`, and prints the generated assembly
on the way past.

## The language

### Variables (Statement)

```muji
thoos x = 43;
x = x + 1;
```

The only values are 64-bit signed integers. Comparisons and `ra` / `wa` / `!`
yield `1` or `0`. A declaration is a statement; an assignment is an
*expression*, so it has a value and `x = y = 3` works, binding to the right.

### Functions (Statement)

```muji
dhanda add(a, b) {
    de a + b;
}

bol(add(6, 9));
```

Functions are declarations, not values — there are no closures and no anonymous
functions, and a name always refers to a variable. They may be called before the
line they are declared on.

A function yields its `de` value; `de` always needs an expression after it.
Falling off the end without reaching a `de` yields `0`. Arguments past the
eighth are passed on the stack.

### Conditionals (Statements)

```muji
thoos n = 0;
yedi (n > 0) {
    bol(1);
} natra yedi (n < 0) {
    bol(-1);
} natra {
    bol(0);
}
```

`natra` is optional, and an `else if` arm is spelled `natra yedi`. Anything
non-zero is true; `0` is false.

### Loops (Statements)

```muji
jabasamma (i < 420) {
    sum = sum + i;
    i = i + 1;
}

fanfani (thoos i = 0; i < 42; i = i + 1) {
    sum = sum + i;
}
```

The `fanfani` initialiser may declare (`thoos i = 0`) or assign (`i = 0`). A
loop variable it declares is gone once the loop ends.

### Operators

| | Operators | Notes |
| --- | --- | --- |
| Prefix | `-` `!` | `!` yields `1` or `0` |
| Arithmetic | `*` `/` `%` `+` `-` | integer division, truncating |
| Comparison | `<` `<=` `>` `>=` | |
| Equality | `==` `!=` | |
| Boolean and | `ra` | |
| Boolean or | `wa` | |
| Assignment | `=` | right-associative |

Listed loosest last. Everything else is left-associative, and parentheses
override precedence.

`ra` and `wa` are boolean, not bitwise: any non-zero operand counts as true and
the result is always `1` or `0`, so `4 ra 2` is `1`. Both sides are always
evaluated — there is no short-circuiting, so `n != 0 ra 10 / n` still divides
when `n` is `0`.

Division by zero is whatever the hardware does. AArch64's `sdiv` does not trap:
`x / 0` is `0`, and `x % 0` is `x`.

### Comments

```muji
// runs from the slashes to the end of the line
```

There is no block comment form.

### Builtins

| Builtin | Takes | Does |
| --- | --- | --- |
| `bol(x)` | exactly one integer | prints it and a newline |

`abs` is recognised but not implemented yet.

### Not in the language

Decimals, strings, arrays, hash maps, functions as values, and every builtin
other than `bol`. The lexer rejects decimal and string literals by name, so
they fail with a diagnostic rather than a parse error.

## Layout

```
src/
  diagnostics.rs     spans, and the error report they render into
  syntax/            source text in, a Program out
    lexer.rs           characters to tokens
    token.rs           the lexical vocabulary; the only place keywords are spelled
    ast.rs             node types, and printing them back as source
    parser.rs          Pratt parser, with error recovery
  compiler.rs        a Program in, macOS ARM64 assembly out
  main.rs            the command line
```

Each layer is reachable through one entry point — `syntax::parse` and
`compiler::Compiler` — so the front end can gain source maps or error recovery
without anything downstream noticing.

Every node carries the span it was parsed from, so errors point at source:

```
error: `y` is not a valid identifier
 --> program.muji:2:5
  |
2 | bol(y + 1);
  |     ^
```

Parse errors are collected in one pass and reported together; the compiler
stops at the first one it hits.

## Tests

```bash
cargo test
```

Unit tests sit next to what they cover: the lexer, the parser, and diagnostics.

`tests/programs/` is the end-to-end suite, and the only thing that exercises
code generation. It is grouped into `expressions/`, `statements/`,
`control_flow/` and `functions/`. Each `NAME.muji` has a `NAME.out` holding the
exact stdout it must produce, and optionally a `NAME.exit` holding its exit code
(default `0`). `tests/programs.rs` compiles each one in its own scratch
directory under `target/muji-tests/`, assembles it with `clang`, runs it, and
compares. The artifacts are left in place, so a failing program's generated
assembly is right there to read.

Adding a case is two files and no code. **Requires `clang` on your path.**

`./test.sh` is the manual loop for a single program, `test.muji`.

## TODOs
- [x] **Unary** — `-x` → `neg x0, x0`. `!x` → `cmp x0, #0` then `cset x0, eq`.
- [x] **Control flow** — a label counter on `Compiler`, `yedi` / `natra yedi` /
      `natra`, `jabasamma`, and `fanfani`.
- [x] Scoping
- [x] Manual functions, including recursion, forward references, and implicit
      return
- [x] More than 8 arguments (the rest spill to the stack)
- [x] 8-byte immediates (`movz` / `movk`)
- [x] Remove arrays and hash maps; delete every builtin except `abs`
- [x] Boolean `ra` / `wa` instead of bitwise `and` / `orr`
- [ ] Short-circuit `ra` / `wa`
- [ ] Implement `abs`
- [ ] Function scope barrier — a function currently sees enclosing frames
- [ ] Tighten every statement is an expression constraint using LLM
