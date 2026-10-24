# Instructions for AI Coding Assistants

Read this file before working in the repository. Repository-local instructions
are authoritative for this project.

## Required first reads

1. `README.md`
2. `MEMORY.md`
3. `project.yaml` through the state inspection command in
   `AGENT_WORKFLOW.md`
4. `TODO.md`
5. `docs/Requirements.md` and `docs/Repository-Strategy.md`
6. The documents directly related to the selected task
7. `docs/Architecture.md` and `docs/Implementation-Plan.md` for design,
   sequencing, dependency, or platform work

Also inspect `git status` before editing. Preserve user changes and keep
unrelated changes out of the task.

## Sources of truth

- `project.yaml`: readable block-YAML operational phase, task, dependency,
  blocker, lease, and pass state. Mutate it through
  `python3 tools/project_state.py`; do not hand-edit it during an active pass.
- `TODO.md`: readable roadmap and task descriptions. It is not the live status
  database; update it when the roadmap or acceptance criteria change.
- `MEMORY.md`: short durable context, current posture, risks, and handoffs.
- `agents/Decisions.md`: accepted architecture and product decisions.
- `docs/`: product, review, design, validation, and implementation contracts.

When sources disagree, stop the affected work, record the discrepancy, and
resolve it before changing implementation.

## Working rules

- Work on one dependency-ready task at a time unless the project state raises
  the concurrency limit.
- Use the pass lifecycle in `AGENT_WORKFLOW.md`; recover stale work before
  selecting new work.
- Keep changes focused, reviewable, and covered by proportionate validation.
- Do not claim cross-platform behavior without evidence on the named platform.
- Treat `markdown-viewer` as a read-only reference unless the task explicitly
  includes that repository.
- Copy fixtures or contracts with their origin commit and license/provenance;
  never depend on `../markdown-viewer` at build or runtime.
- Keep FFI interfaces narrow, value-oriented, versioned, and free of UI types.
- Avoid introducing a second implementation language without an accepted
  decision record.
- Never commit credentials, signing identities, provisioning profiles,
  transcripts, private runtime state, or user document contents.
- Ask before production changes, publishing, signing, external communication,
  destructive operations, or expanding to another repository.

## Architecture guardrails

- Native means platform-owned windows, navigation, commands, accessibility,
  settings, file access, and document presentation. A general-purpose WebView
  is not the default document renderer.
- Rust is the leading candidate for portable semantic work, but the feasibility
  gate may select Rust, C++, or intentionally platform-specific components.
- Each target OS owns its lifecycle, presentation, file access, accessibility,
  packaging, and performance tuning. Do not introduce a shared UI toolkit.
- C++ is a documented fallback, not an additional default layer.
- Parser output is a versioned semantic document model. Platform UI must not
  depend on parser-internal pointers, arenas, or lifetimes.
- Filesystem permissions and security-scoped access stay in platform adapters;
  the core receives capabilities, not unrestricted paths.
- Raw HTML is untrusted. Links, images, and generated SVG require explicit
  policy and sanitization.
- Platform deviations are permitted only when recorded in the capability
  matrix with user impact, rationale, and fallback behavior.

## Completion contract

A task is complete only when:

1. Its acceptance criteria are met.
2. Relevant tests, lint, build, or manual checks have run and are recorded.
3. Design and user documentation match the behavior.
4. `MEMORY.md` and `agents/Decisions.md` reflect durable changes when needed.
5. The active pass is finished in `project.yaml` with evidence and no orphaned
   working state.

Report the concrete outcome, checks run, blocking issues, residual risk, and
the next dependency-ready task. Do not push unless the owner asks.
