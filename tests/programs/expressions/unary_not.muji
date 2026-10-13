// `!` turns non-zero into 0 and 0 into 1.

dhanda no() int {
    de 0;
}

bol(!1);
bol(!0);
bol(!!1);
bol(!!0);

// Binds tighter than `ra` and `wa`.
bol(!1 wa 1);
bol(!0 ra 0);
bol(!1 ra 1);

// Parentheses put it back on the outside.
bol(!(1 ra 0));
bol(!(0 wa 0));
bol(!(1 < 2));

// On a variable, on a call's result, and as a condition.
thoos flag: int = 0;
bol(!flag);
bol(!no());

yedi (!0) {
    bol(10);
} natra {
    bol(20);
}

thoos done: int = 0;
jabasamma (!done) {
    bol(77);
    done = 1;
}
