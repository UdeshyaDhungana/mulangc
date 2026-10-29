// Locals declared inside a function body, a local declared inside a
// nested block, and assignment to a parameter.

dhanda compute(a: int, b: int) int {
    thoos sum: int = a + b;
    thoos doubled: int = sum * 2;
    yedi (doubled > 10) {
        thoos extra: int = 100;
        de doubled + extra;
    }
    de doubled;
}

dhanda mutate(n: int) int {
    n = n * 3;
    de n;
}

bol(compute(1, 2));
bol(compute(4, 5));
bol(mutate(5));
