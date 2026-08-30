// solution.cpp - a CORRECT contestant solution (Accepted).
//
// Contestant I/O contract (communication_mode = "redirect"):
//   stdin  : connected to the manager (read the manager's messages here).
//   stdout : connected to the manager (write your guesses here). FLUSH after
//            every write, or the manager will block forever waiting for input.
//   argv[1]: this contestant's 0-based process index (only relevant when
//            num_processes > 1).
//
// Strategy: read the range, then binary-search for the secret.
#include <cstdio>
#include <cstring>

int main() {
    long lo, hi;
    if (scanf("%ld %ld", &lo, &hi) != 2) return 0;

    char resp[16];
    while (lo <= hi) {
        long mid = lo + (hi - lo) / 2;
        printf("%ld\n", mid);
        fflush(stdout);                       // critical: flush every guess

        if (scanf("%15s", resp) != 1) break;  // manager closed the pipe
        if (strcmp(resp, "CORRECT") == 0) break;
        if (strcmp(resp, "HIGHER") == 0) lo = mid + 1;   // secret > mid
        else                              hi = mid - 1;   // "LOWER": secret < mid
    }
    return 0;
}
