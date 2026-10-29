// Functions with no `de` anywhere. Control has to reach an epilogue,
// return to the caller, and let the program carry on.

dhanda shout(a: int) {
    bol(a);
}

dhanda shout_twice(a: int) {
    bol(a);
    bol(a);
}

// A function with no `de`, called from inside one that has a `de`.
dhanda caller(n: int) int {
    shout(n);
    shout(n + 1);
    de n * 10;
}

shout(7);
shout_twice(8);
bol(caller(1));
bol(99);
