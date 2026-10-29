// Every arm of a yedi / nabhae / natra chain, plus a
// bare yedi with no alternative.

thoos positive: int = 5;
thoos zero: int = 0;
thoos negative: int = -5;

yedi (positive > 0) {
    bol(1);
} natra yedi (positive < 0) {
    bol(2);
} natra {
    bol(3);
}

yedi (negative > 0) {
    bol(1);
} natra yedi (negative < 0) {
    bol(2);
} natra {
    bol(3);
}

yedi (zero > 0) {
    bol(1);
} natra yedi (zero < 0) {
    bol(2);
} natra {
    bol(3);
}

yedi (positive == 5) {
    bol(4);
}

yedi(positive == -1) {
    bol(1);
} natra {
    bol(5);
}
