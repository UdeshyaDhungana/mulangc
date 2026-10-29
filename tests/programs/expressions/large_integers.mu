// Integer literals wider than one 16-bit halfword, which `mov` cannot encode.

bol(0);
bol(65535);
bol(65536);
bol(100000);
bol(4294967296);
bol(9223372036854775807);
bol(-9223372036854775807);
