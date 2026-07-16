// wrong.cpp - an INCORRECT contestant solution (WrongAnswer).
//
// It always guesses the low bound, so it never converges and blows the
// manager's 40-guess budget. The manager then reports score 0.0 => WrongAnswer.
#include <cstdio>
#include <cstring>

int main() {
    long lo, hi;
    if (scanf("%ld %ld", &lo, &hi) != 2) return 0;

    char resp[16];
    for (;;) {
        printf("%ld\n", lo);
        fflush(stdout);
        if (scanf("%15s", resp) != 1) break;
        if (strcmp(resp, "CORRECT") == 0) break;
    }
    return 0;
}
