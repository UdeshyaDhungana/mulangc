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
thoos_muji x = 43;
```

Types: integer, decimal, string, boolean.

### Functions (Yet to implement manual functions) (Statement)

```muji
thoos_muji add = kaam_gar_muji(x, y) {
    patha_muji x + y;
};
add(6, 9);
```

Functions are values: pass them, return them, call them where they stand.

```muji
thoos_muji adder = kaam_gar_muji(x) {
    patha_muji kaam_gar_muji(y) { patha_muji x + y; };
};
adder(2)(3);    $ 5 $
```

A function yields its `patha_muji` value, or its last statement's value if it
has no `patha_muji`.

### Booleans and conditionals (Statements)

```muji
thoos_muji yes = sacho_muji;
thoos_muji no = jhut_muji;

thoos_muji n = 0;
yedi_muji (n > 0) {
    bhan_muji(1);
} nabhae_muji (n < 0) {
    bhan_muji(-1);
} nabhae_chikne {
    bhan_muji(0);
}
```

Both `nabhae_muji` and `nabhae_chikne` are optional. Only `jhut_muji` and
`khali_muji` are false — `0` and `""` are both true.

### Loops (Statements)

```muji
jaba_samma_muji (i < 420) {
    sum = sum + i;
    i = i + 1;
}

ghuma_muji (thoos_muji i = 0; i < 42; i = i + 1) {
    sum = sum + i;
}
```

The `ghuma_muji` initialiser may declare (`thoos_muji i = 0`) or assign
(`i = 0`). A loop variable it declares is gone once the loop ends.

### Comments

```muji
$ opens and closes with a dollar sign $
```

### Builtins

| Builtin | Takes | Does |
| --- | --- | --- |
| `bhan_muji(…)` | anything | prints its arguments and a newline |
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
2 | bhan_muji(x / 0);
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
- [x] **Booleans** — `sacho_muji`/`jhut_muji` → `mov x0, #1` / `mov x0, #0`.
- [x] **Then control flow — this is the next real step:**
- [x] **A label counter** on `Compiler`, plus an `emit_label` helper. Everything below needs unique labels (`.Lif0`, `.Lelse0`, `.Lend0`).
- [x] **`yedi_muji`** — `cmp x0, #0`, `b.eq .Lelse`, body, `b .Lend`. New instructions: `b` (unconditional), `b.eq`/`b.ne` (conditional), `cbz`/`cbnz` (branch if register is zero / nonzero — shorter than `cmp`+`b.eq`).
- [x] **`nabhae_muji` chain + `nabhae_chikne`** — just more labels, no new concepts.
- [x] **`jaba_samma_muji`** — the only new idea is a *backward* branch: label at the top, condition, body, `b` back.
- [x] **`ghuma_muji`** — initialiser, then the same loop shape, with the update emitted before the backward branch.
- [X] Scoping
- [X] Manual functions
- [X] Remove: Arrays, hashmaps
- [X] Delete all builtins except abs
- [ ] Tighten every statement is an expression constraint using LLM
