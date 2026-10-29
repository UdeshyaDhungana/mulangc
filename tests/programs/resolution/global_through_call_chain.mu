// A top-level variable read by a dhanda that is only reached through a chain
// of calls. `a` finds `x` by following its static link to `_main`'s frame,
// where slot 0 is the static link slot and `x` sits after it.

thoos x: int = 3;

dhanda a() int {
    de x;
}

dhanda b() int {
    de a();
}

bol(b());
