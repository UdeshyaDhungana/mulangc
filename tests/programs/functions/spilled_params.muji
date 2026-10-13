// More than eight parameters. x0..x7 carry the first eight; the rest have to
// travel on the stack.

dhanda nine(a: int, b: int, c: int, d: int, e: int, f: int, g: int, h: int, i: int) int {
    de a + b + c + d + e + f + g + h + i;
}

dhanda ninth(a: int, b: int, c: int, d: int, e: int, f: int, g: int, h: int, i: int) int {
    de i;
}

dhanda twelve(a: int, b: int, c: int, d: int, e: int, f: int, g: int, h: int, i: int, j: int, k: int, l: int) int {
    de a + b + c + d + e + f + g + h + i + j + k + l;
}

// Only the spilled parameters are read, and their weights differ, so a
// reversed or shifted spill order shows up as a different number.
dhanda spilled_only(a: int, b: int, c: int, d: int, e: int, f: int, g: int, h: int, i: int, j: int, k: int) int {
    de i * 100 + j * 10 + k;
}

bol(nine(1, 2, 3, 4, 5, 6, 7, 8, 9));
bol(ninth(1, 2, 3, 4, 5, 6, 7, 8, 99));
bol(twelve(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12));
bol(spilled_only(0, 0, 0, 0, 0, 0, 0, 0, 4, 5, 6));
