int main() {
    volatile long x = 0;
    while (true) {
        x += 1;
    }
}
