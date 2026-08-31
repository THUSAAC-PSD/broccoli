#!/usr/bin/env bash
# Prove the built worker image's g++ DEFAULT dialect is C++14, not C++17:
# std::random_shuffle exists in C++14 and was REMOVED in C++17, so it compiles
# under the default (no -std => gnu++14) and fails under -std=c++17. Self-skips
# unless docker + a prebuilt worker image are available.
set -euo pipefail
IMAGE="${BROCCOLI_WORKER_IMAGE:-}"
if ! command -v docker >/dev/null 2>&1; then echo "SKIP: docker not available"; exit 0; fi
if [ -z "$IMAGE" ]; then
  echo "SKIP: set BROCCOLI_WORKER_IMAGE to a built runtime-icpc/runtime-full image"; exit 0
fi

T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
cat > "$T/disc.cpp" <<'CPP'
#include <algorithm>
#include <vector>
int main() {
    std::vector<int> v{1, 2, 3};
    std::random_shuffle(v.begin(), v.end());   // C++14 only; removed in C++17
    return v.size() == 3 ? 0 : 1;
}
CPP

run() { docker run --rm -v "$T:/w" "$IMAGE" bash -lc "$1"; }

# Default flags (no -std) => gnu++14 => MUST compile.
if ! run 'g++ /w/disc.cpp -o /w/a.out'; then
  echo "FAIL: default g++ rejected C++14 code (random_shuffle) -- default is not gnu++14"; exit 1
fi
# Explicit -std=c++17 => MUST fail (proves the discriminator is valid, default != c++17).
if run 'g++ -std=c++17 /w/disc.cpp -o /w/b.out' 2>/dev/null; then
  echo "FAIL: -std=c++17 accepted removed-in-C++17 code -- discriminator invalid"; exit 1
fi
echo "PASS: worker g++ default dialect is C++14 (gnu++14), not C++17"
