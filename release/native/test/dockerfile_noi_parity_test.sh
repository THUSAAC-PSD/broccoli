#!/usr/bin/env bash
# Structural assertions on Dockerfile.worker: the runtime base is NOI Linux 2.0
# (Ubuntu 20.04, gcc/g++ 9.3.0, glibc 2.31), worker + broccoli-compare are
# musl-static, and version drift is asserted at build time. Pure text checks.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
df="$here/../../../Dockerfile.worker"

fail=0
check()  { grep -Eq -- "$1" "$df" || { echo "MISSING: $1"; fail=1; }; }
refute() { grep -Eq -- "$1" "$df" && { echo "PRESENT (must be gone): $1"; fail=1; }; return 0; }

# --- base swap: runtime base is digest-pinned ubuntu:20.04 (not debian) ---
check '^ARG RUNTIME_IMAGE=ubuntu:20\.04@sha256:[0-9a-f]{64}'
refute 'ARG DEBIAN_IMAGE=debian:bookworm'

# --- musl worker + broccoli-compare ---
check 'ARG MUSL_TARGET=x86_64-unknown-linux-musl'
check 'rustup target add .*\$\{?MUSL_TARGET\}?'
check 'musl-tools'
check 'cargo chef cook .*--target .*\$\{?MUSL_TARGET\}?'
check 'cargo build .*--target .*\$\{?MUSL_TARGET\}?'
check 'target/\$\{?MUSL_TARGET\}?/\$\{?CARGO_BUILD_PROFILE\}?/worker'
# static-link assert baked into the builder (one RUN, both binaries)
check 'statically linked'

# --- focal deltas ---
refute 'tini=0\.19\.0-1\+b3'                        # Debian binNMU pin gone
check 'archive\.ubuntu\.com'                        # focal classic mirror host (absent from bookworm deb822)
check 'old-releases\.ubuntu\.com'                   # EOL-mirror fallback

# --- gcc 9.3.0 parity ---
check 'ARG GCC_VERSION=9\.3\.0'
# The installable 9.3.0 is the focal/main GA revision (frozen pocket); the former
# -updates pin 9.3.0-17ubuntu1~20.04 was a phantom (focal-updates floated to
# 9.4.0 and dropped it), which wedged the build. Pin the GA revision instead.
check 'ARG GCC_APT_VERSION=9\.3\.0-10ubuntu2'
check 'g\+\+-9=\$\{?GCC_APT_VERSION\}?'
# gcc-9-base (exact-depended) and libasan5 (hard-depended by libgcc-9-dev) must be
# pinned to the same rev, else apt resolves them to the 9.4.0 candidate and aborts
# with "held broken packages".
check 'gcc-9-base=\$\{?GCC_APT_VERSION\}?'
check 'libasan5=\$\{?GCC_APT_VERSION\}?'
check 'update-alternatives --install /usr/bin/g\+\+ g\+\+ /usr/bin/g\+\+-9'
check 'gcc --version.*\$\{?GCC_VERSION\}?'          # baked version assert

# --- non-interactive apt (tzdata postinst prompt would wedge BuildKit) ---
check 'ENV DEBIAN_FRONTEND=noninteractive'

# --- kotlin from tarball, not focal apt (which lacks it) ---
check 'ARG KOTLIN_VERSION='
refute '^[[:space:]]+kotlin \\\\?$'                 # no bare `kotlin` apt package line

[ "$fail" -eq 0 ] && echo "PASS: Dockerfile.worker is NOI-parity" || { echo "FAIL"; exit 1; }
