# mulang

A toolchain for the **muji** programming language, written in Rust. The
language is unchanged from [muji-lang](https://github.com/UdeshyaDhungana/muji-lang);
this is a rewrite of the implementation.

```bash
cargo build --release

./target/release/mulang                     # interactive session
./target/release/mulang examples/sum.muji   # run a program
```

## The language

### Variables

```muji
thoos_muji x = 43;
```

Types: integer, decimal, string, boolean, array, hashmap.

### Functions

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

### Booleans and conditionals

```muji
thoos_muji yes = sacho_muji;
thoos_muji no = jhut_muji;

yedi_muji (n > 0) {
    bhan_muji("Positive");
} nabhae_muji (n < 0) {
    bhan_muji("Negative");
} nabhae_chikne {
    bhan_muji("Zero!");
}
```

Both `nabhae_muji` and `nabhae_chikne` are optional. Only `jhut_muji` and
`khali_muji` are false — `0`, `""` and `[]` are all true.

### Loops

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

### Arrays and hashmaps

```muji
thoos_muji values = [1, "newstring", 69.69];
values[0] = 99;

thoos_muji countryCodes = { "NP": "+977", "IN": "+91" };
countryCodes["US"] = "+1";
```

Hashmap keys are strings; values can be anything. Reading a key that was never
set gives `khali_muji`. Arrays and hashmaps are shared handles, so a function
that mutates one it was passed mutates the caller's.

### Comments

```muji
$ opens and closes with a dollar sign $
```

### Builtins

| Builtin | Takes | Does |
| --- | --- | --- |
| `bhan_muji(…)` | anything | prints its arguments and a newline |
| `lambai_muji(x)` | string, array, hashmap | length |
| `khaad_muji(array, value)` | array, value | appends in place |
| `udaa_muji(array[, index])` | array, optional index | removes and returns that element, or the last |
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
  runtime/           a Program in, a Value out
    value.rs           what a running program manipulates
    scope.rs           bindings, and the chain of enclosing scopes
    operations.rs      what the operators mean
    builtins.rs        the builtin table
    interpreter.rs     the tree walk
  repl.rs            the interactive session
  main.rs            the command line
```

Each layer is reachable only through one entry point — `syntax::parse`,
`runtime::Interpreter` — so a compiler back end can sit beside the interpreter
and consume the same `Program`, and the front end can gain source maps or
error recovery without anything downstream noticing.

Every node carries the span it was parsed from, so errors point at source:

```
error: index 7 is outside this array of 3 element(s)
 --> program.muji:2:11
  |
2 | bhan_muji(x[7]);
  |           ^^^^
```

## Tests

```bash
cargo test
```

Unit tests sit next to what they cover; `tests/programs.rs` runs whole programs,
including every file in `examples/`, and checks what they print.

## TODOs
- [x] **Unary** — `-x` → `neg x0, x0`. `!x` → `cmp x0, #0` then `cset x0, eq`.
- [x] **Booleans** — `sacho_muji`/`jhut_muji` → `mov x0, #1` / `mov x0, #0`.
- [x] **Then control flow — this is the next real step:**
- [x] **A label counter** on `Compiler`, plus an `emit_label` helper. Everything below needs unique labels (`.Lif0`, `.Lelse0`, `.Lend0`).
- [x] **`yedi_muji`** — `cmp x0, #0`, `b.eq .Lelse`, body, `b .Lend`. New instructions: `b` (unconditional), `b.eq`/`b.ne` (conditional), `cbz`/`cbnz` (branch if register is zero / nonzero — shorter than `cmp`+`b.eq`).
- [x] **`nabhae_muji` chain + `nabhae_chikne`** — just more labels, no new concepts.
- [x] **`jaba_samma_muji`** — the only new idea is a *backward* branch: label at the top, condition, body, `b` back.
- [x] **`ghuma_muji`** — initialiser, then the same loop shape, with the update emitted before the backward branch.
- [ ] Scoping
- [ ] Manual functions
- [ ] Remove: Arrays, hashmaps
- [ ] Delete all builtins except abs
- [ ] Tighten every statement is an expression constraint using LLM
