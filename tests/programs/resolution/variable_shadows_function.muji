// A local variable with the same name as a top-level dhanda. The call
// happens before the declaration, and the reads after it, so the name has
// to mean the function once and the variable afterwards.

dhanda size() int {
    de 7;
}

dhanda shadow() {
    bol(size());

    thoos size: int = 100;
    bol(size);

    size = size + 5;
    bol(size);
}

shadow();
