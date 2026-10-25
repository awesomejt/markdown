#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
probe="${repo_root}/spikes/feas005/target/release/linux-native-rendering-probe"

python3 "${repo_root}/tools/generate_performance_fixtures.py"
cargo build --locked --release \
  --manifest-path "${repo_root}/spikes/feas005/Cargo.toml"

for profile in prose-1mib prose-5mib delimiter-storm-1mib; do
  for trial in 1 2 3; do
    echo "profile: ${profile}"
    echo "trial: ${trial}"
    timeout 30s "${probe}" \
      "${repo_root}/build/fixtures/performance/${profile}.md" --automated
  done
done

# A single 1 MiB line is an intentional failure case. Exit 124 means the
# end-scroll did not return control to the GTK main loop within ten seconds.
echo "profile: long-lines-1mib"
echo "trial: 1"
set +e
timeout 10s "${probe}" \
  "${repo_root}/build/fixtures/performance/long-lines-1mib.md" --automated
long_line_exit=$?
set -e
echo "long_line_exit: ${long_line_exit}"
test "${long_line_exit}" -eq 124

