// Eight parameters — every argument register x0..x7 in use at once.

dhanda eight(a: int, b: int, c: int, d: int, e: int, f: int, g: int, h: int) int {
    de a + b + c + d + e + f + g + h;
}

dhanda three(a: int, b: int, c: int) int {
    de a * 100 + b * 10 + c;
}

bol(eight(1, 2, 3, 4, 5, 6, 7, 8));
bol(three(1, 2, 3));
