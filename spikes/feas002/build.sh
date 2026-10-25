#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
work_dir="${repo_root}/build/feas002"
cmark_dir="${work_dir}/cmark-gfm"
cmark_build="${work_dir}/cmark-build"
cmark_tag="0.29.0.gfm.13"
cmark_commit="587a12bb54d95ac37241377e6ddc93ea0e45439b"

mkdir -p "${work_dir}"
if [[ ! -d "${cmark_dir}/.git" ]]; then
  git clone --depth 1 --branch "${cmark_tag}" \
    https://github.com/github/cmark-gfm.git "${cmark_dir}"
fi
if [[ "$(git -C "${cmark_dir}" rev-parse HEAD)" != "${cmark_commit}" ]]; then
  echo "cmark-gfm checkout does not match ${cmark_tag} (${cmark_commit})" >&2
  exit 1
fi

cmake -S "${cmark_dir}" -B "${cmark_build}" \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMARK_TESTS=OFF \
  -DCMAKE_POLICY_VERSION_MINIMUM=3.5
cmake --build "${cmark_build}" --parallel

cc -O3 -DNDEBUG -std=c99 \
  -I "${cmark_dir}/src" \
  -I "${cmark_dir}/extensions" \
  -I "${cmark_build}/src" \
  -I "${cmark_build}/extensions" \
  "${repo_root}/spikes/feas002/cmark_probe.c" \
  -Wl,--start-group \
  "${cmark_build}/src/libcmark-gfm.a" \
  "${cmark_build}/extensions/libcmark-gfm-extensions.a" \
  -Wl,--end-group \
  -o "${work_dir}/cmark-probe"

cargo build --locked --release \
  --manifest-path "${repo_root}/spikes/feas002/Cargo.toml"

