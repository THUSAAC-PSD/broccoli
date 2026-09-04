#include <bits/stdc++.h>
using namespace std;

// A DIFFERENT but equally-correct solution: same factors, swapped order.
// For n = 391 this prints "23 17". An exact / tokens checker would mark this
// WrongAnswer against "17 23"; the special judge accepts it. This file exists
// purely to demonstrate that non-unique answers pass.
int main() {
    long long n;
    if (!(cin >> n)) return 0;
    for (long long d = 2; d * d <= n; ++d) {
        if (n % d == 0) {
            cout << n / d << " " << d << "\n"; // swapped vs solution.cpp
            return 0;
        }
    }
    cout << 1 << " " << n << "\n";
    return 0;
}
