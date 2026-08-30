#include "testlib.h"

// Special judge for the "factor pair" problem.
//
// Input (inf):  one integer n (a composite number, 2 <= n <= 1e9).
// Output (ouf): two integers  a b  with a >= 2, b >= 2 and a * b == n.
//
// ANY valid pair in ANY order is accepted -- e.g. for n = 391 both "17 23"
// and "23 17" are correct. That non-uniqueness is exactly why an exact /
// tokens checker cannot be used here and a special judge is required.
//
// testlib argv contract (set up by broccoli): argv = checker inf ouf ans.
//   inf = the test-case input    (input.txt)
//   ouf = the contestant output  (output.txt)
//   ans = the jury answer        (answer.txt) -- unused here; we recompute.
int main(int argc, char* argv[]) {
    registerTestlibCmd(argc, argv);

    long long n = inf.readLong();          // the test input
    long long a = ouf.readLong(2, n, "a"); // out-of-range / non-int -> auto WA/PE
    long long b = ouf.readLong(2, n, "b");

    // Reject genuine extra output, but tolerate a trailing newline/spaces.
    // NOTE: do NOT use ouf.readEof() here -- it is strict and treats the
    // trailing '\n' that virtually every solution prints as junk, WA-ing a
    // correct answer. seekEof() skips trailing whitespace first, so "17 23\n"
    // passes while "17 23 99" is correctly flagged as a presentation error.
    if (!ouf.seekEof())
        quitf(_pe, "extra tokens after a b");

    if (a * b != n)
        quitf(_wa, "a*b = %lld != n = %lld", a * b, n);

    quitf(_ok, "valid factorisation %lld * %lld = %lld", a, b, n);
}
