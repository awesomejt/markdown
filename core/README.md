# Portable core workspace

This is the production Rust workspace established by CORE-001A. It deliberately
contains no semantic-model, parser, session or C ABI implementation yet:
CORE-002A, CORE-003A, CORE-004A and CORE-006A own those contracts separately.
Feasibility crates under `spikes/` are not workspace members or dependencies.

`markdown-core` is a safe-Rust semantic crate. A future C-facade crate owns any
necessary unsafe FFI code without weakening this crate's `forbid(unsafe_code)`.
Platform windows, rendering, filesystem permissions and packaging stay in their
native application trees. Linux consumes the same public facade as other hosts.

Run the root [development checks](../docs/Development.md). The production graph
has no third-party dependencies yet; the [dependency policy](../docs/Dependencies.md)
checks resolved dependencies when future phases introduce them. No crates are
publishable until explicitly authorized.
