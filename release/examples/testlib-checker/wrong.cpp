#include <bits/stdc++.h>
using namespace std;

// Incorrect solution: prints "2 n". Both numbers are in range, so the checker
// reads them fine, but 2 * n != n -> the checker calls quitf(_wa, ...) ->
// WrongAnswer. Demonstrates a value-level rejection (not a format error).
int main() {
    long long n;
    if (!(cin >> n)) return 0;
    cout << 2 << " " << n << "\n";
    return 0;
}
