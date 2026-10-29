// Declaration, reading an enclosing scope's variables from an inner block,
// and assignment.

thoos a: int = 10;
thoos b: int = 32;
bol(a + b);

yedi (1) {
    thoos c: int = 5;
    bol(a + b + c);
}

a = a * 2;
bol(a);

b = a + b;
bol(b);
