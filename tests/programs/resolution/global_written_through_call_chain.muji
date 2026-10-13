// A top-level variable assigned by a dhanda deep in a call chain. The write
// has to go through the static link into `_main`'s frame, so the change is
// visible to every later read.

thoos counter: int = 0;

dhanda bump() {
    counter = counter + 5;
}

dhanda twice() {
    bump();
    bump();
}

twice();
bol(counter);
twice();
bol(counter);

bol(counter);
