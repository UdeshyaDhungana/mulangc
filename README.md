# muji

A program is a list of statements, run top to bottom, starting at the first
line of the file.

```muji
thoos x: int = 6;
bol(x * 7);
```

## Types

`int` is a 64-bit signed integer. `bool` is true or false.

## Variables

```muji
thoos x: int = 43;
x = x + 1;
```

`thoos` declares a variable with a type. Assignment is an expression, so
`x = y = 3` works.

## Functions

```muji
dhanda add(a: int, b: int) int {
    de a + b;
}

dhanda greet(n: int) {
    bol(n);
}

bol(add(6, 9));
greet(1);
```

`dhanda` declares a function, and `de` returns from it. The optional return
type follows the parameters. A
function with a return type must reach a `de` on every path.

Functions may be declared inside other functions and blocks, where they are
visible only from their declaration onwards. A top-level function may be called
before the line that declares it.

## Conditionals

```muji
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

```muji
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

```muji
// to the end of the line
```

## Builtins

| Builtin | Does |
| --- | --- |
| `bol(x)` | prints an `int` |
| `bolbool(x)` | prints a `bool` |
