#include <iostream>
#include <vector>
int main() {
    std::vector<int> v(80'000'000, 1);
    int a, b;
    std::cin >> a >> b;
    std::cout << a + b + v[0] - 1 << "\n";
    return 0;
}
