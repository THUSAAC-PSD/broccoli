#include <bits/stdc++.h>
using namespace std;

// Correct solution: print the smallest factor and its cofactor.
// For n = 391 this prints "17 23" -> Accepted.
int main() {
    long long n;
    if (!(cin >> n)) return 0;
    for (long long d = 2; d * d <= n; ++d) {
        if (n % d == 0) {
            cout << d << " " << n / d << "\n";
            return 0;
        }
    }
    // Input is guaranteed composite, so we never reach here. Emit something the
    // checker rejects (b = 1) so a bad test surfaces as WA, not as a crash.
    cout << n << " " << 1 << "\n";
    return 0;
}
