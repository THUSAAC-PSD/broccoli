// manager.cpp — interactive "guess the number" manager (interactor).
//
// Broccoli communication protocol (communication_mode = "redirect", the
// default; num_processes = 1):
//
//   stdin   : the test-case input. Here: "lo hi secret".
//   argv[1] : path of the FIFO to WRITE to the contestant (manager -> solution).
//   argv[2] : path of the FIFO to READ from the contestant (solution -> manager).
//   stdout  : the FIRST line MUST be the score as a double. The evaluator
//             clamps it to [0,1]; score >= 1.0 => Accepted, otherwise WrongAnswer.
//   stderr  : free-form; surfaced to the author as the verdict message.
//   exit    : MUST be 0. A non-zero manager exit is reported as SystemError.
//
// For num_processes = N the manager additionally receives, for each contestant
// i, argv[1+2*i] (write) and argv[2+2*i] (read).
//
// The interaction here: the manager tells the solution the search range, then
// answers each guess with HIGHER / LOWER / CORRECT, allowing up to 40 guesses.
#include <cstdio>
#include <cstdlib>

int main(int argc, char** argv) {
    if (argc < 3) {
        printf("0\n");                       // score
        fprintf(stderr, "manager: expected 2 pipe arguments\n");
        return 0;
    }

    long lo, hi, secret;
    if (scanf("%ld %ld %ld", &lo, &hi, &secret) != 3) {
        printf("0\n");
        fprintf(stderr, "manager: malformed test input\n");
        return 0;
    }

    FILE* to_sol   = fopen(argv[1], "w");    // manager -> solution
    FILE* from_sol = fopen(argv[2], "r");    // solution -> manager
    if (!to_sol || !from_sol) {
        printf("0\n");
        fprintf(stderr, "manager: failed to open communication pipes\n");
        return 0;
    }

    // Hand the solution the search range.
    fprintf(to_sol, "%ld %ld\n", lo, hi);
    fflush(to_sol);

    const int MAX_QUERIES = 40;
    int used = 0;
    double score = 0.0;

    while (used < MAX_QUERIES) {
        long guess;
        if (fscanf(from_sol, "%ld", &guess) != 1) break;  // solution closed its pipe / EOF
        used++;

        if (guess == secret) {
            fprintf(to_sol, "CORRECT\n");
            fflush(to_sol);
            score = 1.0;
            break;
        } else if (guess < secret) {
            fprintf(to_sol, "HIGHER\n");   // the secret is higher than the guess
            fflush(to_sol);
        } else {
            fprintf(to_sol, "LOWER\n");
            fflush(to_sol);
        }
    }

    fprintf(stderr, "guesses used: %d (limit %d), secret = %ld\n", used, MAX_QUERIES, secret);
    printf("%.1f\n", score);   // FIRST line of stdout is the score
    return 0;
}
