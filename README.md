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
