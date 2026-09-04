#!/usr/bin/env sh
# testlib.h is third-party (github.com/MikeMirzayanov/testlib) and is NOT
# vendored here. Fetch the canonical copy next to checker.cpp so the two can be
# uploaded together as the problem's checker source.
set -e
dir="$(cd "$(dirname "$0")" && pwd)"
curl -fsSL https://raw.githubusercontent.com/MikeMirzayanov/testlib/master/testlib.h \
  -o "$dir/testlib.h"
echo "Saved $dir/testlib.h"
