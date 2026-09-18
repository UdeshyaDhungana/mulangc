# mulang

A toolchain for the **muji** programming language, written in Rust. The
language is unchanged from [muji-lang](https://github.com/UdeshyaDhungana/muji-lang);
this is a rewrite of the implementation.

Compiles to macOS ARM64 assembly. `mulang program.muji` writes `program.s`,
which you then assemble and link:

```bash
cargo build --release

./target/release/mulang program.muji
clang -o program program.s
./program
```

`./test.sh` does all three for `test.muji`.

## The language

### Variables (Statement)

```muji
thoos x = 43;
```

Types: integer, decimal, string, boolean.

### Functions (Yet to implement manual functions) (Statement)

```muji
thoos add = dhanda(x, y) {
    de x + y;
};
add(6, 9);
```

Functions are values: pass them, return them, call them where they stand.

```muji
thoos adder = dhanda(x) {
    de dhanda(y) { de x + y; };
};
adder(2)(3);    // 5
```

A function yields its `de` value, or its last statement's value if it
has no `de`.

### Booleans and conditionals (Statements)

```muji
thoos yes = ho;
thoos no = haina;

thoos n = 0;
yedi (n > 0) {
    bol(1);
} nabhae (n < 0) {
    bol(-1);
} natra {
    bol(0);
}
```

Both `nabhae` and `natra` are optional. Only `haina` and
`khali_muji` are false — `0` and `""` are both true.

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

The `fanfani` initialiser may declare (`thoos i = 0`) or assign
(`i = 0`). A loop variable it declares is gone once the loop ends.

### Comments

```muji
// runs from the slashes to the end of the line
```

There is no block comment form.

### Builtins

| Builtin | Takes | Does |
| --- | --- | --- |
| `bol(…)` | anything | prints its arguments and a newline |
| `abs(x)` | integer, decimal | absolute value |

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
error: division by zero
 --> program.muji:2:11
  |
2 | bol(x / 0);
  |           ^^^^^
```

## Tests

```bash
cargo test
```

Unit tests sit next to what they cover: the lexer, the parser, and diagnostics.

`tests/programs/` is the end-to-end suite, and the only thing that exercises
code generation. Each `NAME.muji` has a `NAME.out` holding the exact stdout it
must produce, and optionally a `NAME.exit` holding its exit code (default `0`).
`tests/programs.rs` compiles each one in its own scratch directory under
`target/muji-tests/`, assembles it with `clang`, runs it, and compares. The
artifacts are left in place, so a failing program's generated assembly is right
there to read.

Adding a case is two files and no code. **Requires `clang` on your path.**

`./test.sh` is the manual loop for a single program, `test.muji`.

## TODOs
- [x] **Unary** — `-x` → `neg x0, x0`. `!x` → `cmp x0, #0` then `cset x0, eq`.
- [x] **Booleans** — `ho`/`haina` → `mov x0, #1` / `mov x0, #0`.
- [x] **Then control flow — this is the next real step:**
- [x] **A label counter** on `Compiler`, plus an `emit_label` helper. Everything below needs unique labels (`.Lif0`, `.Lelse0`, `.Lend0`).
- [x] **`yedi`** — `cmp x0, #0`, `b.eq .Lelse`, body, `b .Lend`. New instructions: `b` (unconditional), `b.eq`/`b.ne` (conditional), `cbz`/`cbnz` (branch if register is zero / nonzero — shorter than `cmp`+`b.eq`).
- [x] **`nabhae` chain + `natra`** — just more labels, no new concepts.
- [x] **`jabasamma`** — the only new idea is a *backward* branch: label at the top, condition, body, `b` back.
- [x] **`fanfani`** — initialiser, then the same loop shape, with the update emitted before the backward branch.
- [X] Scoping
- [X] Manual functions
- [X] Remove: Arrays, hashmaps
- [X] Delete all builtins except abs
- [ ] Tighten every statement is an expression constraint using LLM
