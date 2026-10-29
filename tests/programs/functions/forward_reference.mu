// A function that calls one defined further down the file, and two that
// call each other. Neither can work without the definitions being visible
// before the bodies are compiled.

dhanda is_even(n: int) int {
    yedi (n == 0) {
        de 1;
    }
    de is_odd(n - 1);
}

dhanda is_odd(n: int) int {
    yedi (n == 0) {
        de 0;
    }
    de is_even(n - 1);
}

bol(is_even(10));
bol(is_odd(10));
bol(is_even(7));
