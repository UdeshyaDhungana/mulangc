// Several top-level variables read from a dhanda two calls deep. Each one
// has to land on its own slot in `_main`'s frame, after the static link slot.

thoos x: int = 1;
thoos y: int = 20;
thoos z: int = 300;

dhanda sum() int {
    de x + y + z;
}

dhanda middle() int {
    de sum();
}

bol(x);
bol(y);
bol(z);
bol(middle());
