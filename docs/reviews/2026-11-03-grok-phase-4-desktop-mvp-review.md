# Phase 4 desktop basic MVPs review (Grok)

- **Date:** 2026-11-03
- **Evidence run:** 2026-11-04 UTC, on the same baseline. `develop` had not moved.
- **Reviewer:** Grok (independent review)
- **Branch:** `review/phase-4`, based on `develop` at `9e1566e`
  (`9e1566eda1a292643c1711bd13fd37180d514777`, merge of PR #64, LNX-001E).
  After `git fetch origin`, the remote has `main`, `develop`, and this
  branch. There is no `review/mac-001`, `review/win-001`, or `review/lnx-001`.
- **`main`:** `dc84d37` (`dc84d370b0dde869b98a83f2d2723170dc3f6514`), the
  `m-core-fix` merge (PR #28, 2026-10-29). `develop` is 171 commits ahead of
  `main`.
- **Scope:** Phase 4 in `docs/Implementation-Plan.md` (lines 266–282):
  WIN-002, MAC-002, LNX-002, and DESK-002. The planned children are 002A
  through 002F (lines 181–188). Walking-skeleton work (Phase 3: WIN-001,
  MAC-001, LNX-001, DESK-001) is the gate in front of Phase 4. It is not
  counted as Phase 4 delivery.

## Verdict

**Not ready. Phase 4 has not started, and it is not eligible to start.**
Do not merge this branch into `main`. Do not treat the desktop walking
skeletons as the basic MVPs.

The four Phase 4 tasks are `pending` in `project.yaml` revision 757, with
`updated_at` still equal to their creation time (2026-10-24) and no passes.
The 002A–002F children the plan says to add at DESK-001 closeout do not
exist. There is no Phase 4 implementation pull request. `gh pr list --state open`
is empty. The newest merged PR on `develop` is #64 (LNX-001E).

Two predecessors the plan requires are still open: the Linux skeleton
(LNX-001F through LNX-001I, and the LNX-001 aggregate) and DESK-001.
HARD-001, which the graph places before DESK-002, is also `pending`.

## What Phase 4 requires

From `docs/Implementation-Plan.md` lines 266–282, Phase 4 adds folder
workspaces, native persistent access and recents, browsing, workspace
search, preferences, robust file watching, state restoration, packaging,
and platform performance tuning.

Exit criteria:

- A user can install and run each app, open a file or a folder, navigate
  and search, read representative documents offline, quit and relaunch, and
  recover an unavailable file or permission.
- Named baseline hardware meets the accepted launch, parse, memory, layout,
  scroll, and search budgets.
- Capability deviations are documented and surfaced in the app.

The children, and the prerequisites the plan names for them:

| Suffix | Acceptance the plan names | Direct prerequisites |
| --- | --- | --- |
| 002A | Folder capability, access recovery, recents; denied, revoked, and moved-folder checks | DESK-001 |
| 002B | Native workspace browser and navigation; large-folder and keyboard checks | own 002A, CORE-005D |
| 002C | Workspace search UI; bounded indexing, cancellation, result navigation | own 002B, CORE-005B, CORE-005B2, CORE-005C |
| 002D | Persistent preferences and native settings; migration and relaunch checks | own 002A |
| 002E | Workspace watching and restoration; change storms, access loss, session recovery | own 002C, own 002D |
| 002F | Development packaging and complete R1 conformance; install/run evidence and host budgets | own 002E |

DESK-002 still requires all three 002 aggregates and HARD-001
(`docs/Implementation-Plan.md` lines 190–194). The live edges in
`project.yaml` match that graph: WIN-001, MAC-001, and LNX-001 precede
DESK-001; DESK-001 precedes each 002 and HARD-001; the three 002s and
HARD-001 precede DESK-002.

## What is actually on the baseline

| Task | State | Evidence |
| --- | --- | --- |
| WIN-001 (001A–001I and the aggregate) | completed | `project.yaml` WIN-001 `completed_at` 2026-11-03; PRs #53–#62 merged |
| MAC-001 (001A–001I and the aggregate) | completed | `project.yaml` MAC-001 `completed_at` 2026-10-31; PRs #39–#48 merged |
| LNX-001A–LNX-001E and LNX-001D1 | completed | PRs #49–#52, #63, #64 merged. LNX-001E is the baseline HEAD |
| LNX-001F, LNX-001G, LNX-001H, LNX-001I, LNX-001 | pending | `project.yaml` LNX-001F at lines 1269–1290; aggregate at lines 372–388 |
| DESK-001 | pending | lines 389–403. No closeout, so 002A–002F were never materialized |
| WIN-002, MAC-002, LNX-002 | pending | lines 404–451. Descriptions are still the original one-line MVP blurbs |
| HARD-001 | pending | lines 1849–1870. Required before DESK-002 |
| DESK-002 | pending | lines 452–466 |
| CORE-005B, CORE-005B2, CORE-005C, CORE-005D | completed | Shared workspace search, its C ABI, the 2 MiB content guard, and wider invisible-filename warnings. These are library prerequisites for 002B and 002C, not the UI |

`desktop-apps` is the active phase (`project.yaml` lines 45–56).
`portable-core` is completed. No task key matching `*-002A` through
`*-002F` is present.

The shared library underneath a future 002C is present and is what the
locked tests below exercise:

- `md_workspace_classify_path` at `core/crates/markdown-ffi/src/workspace.rs`
  line 124
- `md_workspace_find_names` at line 202
- `md_workspace_content_search` at line 490
- Default content budget `MD_CONTENT_DEFAULT_MAX_FILE_BYTES` (2 MiB) and
  `MD_CONTENT_DEFAULT_MAX_TOTAL_BYTES` (256 MiB) at
  `core/crates/markdown-ffi/src/abi.rs` lines 196–198

No host calls those functions. macOS and Windows only check the two workspace
struct sizes (`apps/macos/Sources/MarkdownCore/CoreRuntime.swift` lines 80–81;
`apps/windows/Core/CoreRuntime.cs` lines 323–324). The Linux adapter's FFI
import block ends at `md_snapshot_release`
(`apps/linux/core/src/ffi.rs` lines 287–331) and contains no `md_workspace_*`
symbol. A search of `apps/linux` for `md_workspace_` returns nothing.

## Exit criteria against the tree

| Criterion | Result on this baseline |
| --- | --- |
| Install and run each app | macOS and Windows have unsigned development artifacts from 001I (PRs #47 and #61). Linux has no 001I artifact. None of the three is an R1 install (002F). This review did not launch any app. |
| Open a file | Present on all three hosts as the 001C single-file flow. |
| Open a folder and browse it | Absent. Each host rejects a directory as a document. See finding C1. |
| Navigate and search a workspace | Absent. Document find exists on macOS (MAC-001F) and Windows (WIN-001F). Linux find is LNX-001F and is pending. Workspace file-name and content search have no UI on any host. |
| Read representative documents offline | Single-document reading exists on all three hosts through the current skeleton slice. Linux stops at tables, task lists, and images (LNX-001E). |
| Quit, relaunch, and recover access | Single-document restoration exists on macOS (MAC-001H) and Windows (WIN-001H). Linux restoration is LNX-001H and is pending. Folder grants, recents, and moved-folder recovery are not implemented. |
| Named-hardware budgets | Skeleton budgets are recorded for macOS in `TODO.md` lines 593–621. No Phase 4 budget has been measured, and this review measured none. |
| Deviations documented and surfaced | CAP-02, CAP-03, and the folder half of CAP-05 in `docs/Platform-Capability-Evaluation.md` still say the next check is WIN-002, MAC-002, or LNX-002, with Authoritative evidence. That is the correct status for work that has not run. |

## Evidence

Commands were run on Linux x86_64, Rust 1.99.1 (`58cf8f9c3`, 2026-10-13),
at `9e1566e`. `LD_LIBRARY_PATH` was unset. GTK 4, the .NET SDK, and Swift
are not installed (`pkg-config --modversion gtk4` fails; `dotnet` and
`swift` are absent). Windows and macOS apps were not built and were not
launched. Unrun native checks are not passes.

| Check | Result |
| --- | --- |
| `git fetch origin`; `git branch -a` | `main`, `develop`, and `review/phase-4` only |
| `gh pr list --state open` | no open pull requests |
| `gh pr list --head review/phase-4 --state all` | no pull request for this branch |
| `gh pr list --state all --limit 30` | newest is #64, LNX-001E, merged 2026-11-03. No PR title mentions WIN-002, MAC-002, LNX-002, or DESK-002 |
| `git rev-list --count origin/main..9e1566e` | 171 |
| `project.yaml` revision 757, tasks parsed with PyYAML | the status table above |
| `cargo test --locked --workspace` | 215 passed, 0 failed, debug, default features. Suites: markdown-core lib 5, autolinks 10, budgets 3, conformance 4, content_budgets 3, hostile 4, metadata 20, model_v1 20, parser 27, search 30, search_memory 1, session 28, workspace 23; markdown-ffi lib 35 (including the workspace ABI tests); c_abi 2. `v2.rs` ran 0 tests under default features |
| `cargo fmt --check --all` | clean |
| `markdownlint-cli2` 0.18.1 on `docs/**/*.md` | 0 errors |
| `apps/linux`, `apps/windows`, `apps/macos` builds | not run. GTK 4, the .NET SDK, and Swift are missing on this machine |

`--all-features` and release-mode tests were not run. `tools/check.py all`
was not run: the host apps it drives cannot be built here. The Rust result
shows the shared workspace library still builds and its tests pass. It does
not show a desktop MVP.

Files read for the host boundary: `docs/Implementation-Plan.md`,
`docs/Requirements.md` (R1 and R2), `docs/Delivery-Workflow.md` (milestone
cuts), `docs/Platform-Capability-Evaluation.md` (CAP-02, CAP-03, CAP-05),
`TODO.md` (Desktop basic MVPs, Open native budgets), `HANDOFF.md`,
`project.yaml` task and relationship records, `apps/macos/README.md`,
`apps/macos/Sources/Markdown/MarkdownApp.swift`,
`apps/macos/Sources/MarkdownDocument/DocumentAccess.swift`,
`apps/macos/Sources/MarkdownRender/DocumentRenderer.swift`,
`apps/macos/Sources/Markdown/ReaderPreferences.swift`,
`apps/windows/DocumentPicker.cs`, `apps/windows/Document/DocumentAccess.cs`,
`apps/windows/ReaderPreferences.cs`, `apps/windows/Restoration.cs`,
`apps/windows/Core/CoreRuntime.cs`, `apps/linux/src/files.rs`,
`apps/linux/src/document/access.rs`, `apps/linux/core/src/ffi.rs`, and
`core/crates/markdown-ffi/src/workspace.rs`.

## Findings

### B1 — Blocker: Phase 4 is not implemented

WIN-002, MAC-002, LNX-002, and DESK-002 are `pending`
(`project.yaml` lines 404–466). Each `updated_at` is the creation timestamp
`2026-10-24T23:34:34Z` (DESK-002 a second later). There is no execution pass,
no 002A–002F task, no feature branch, and no implementation pull request.
A repository search for those child keys finds them only in the plan, the
roadmap, and comments that defer work to a later phase.

The exit criteria at `docs/Implementation-Plan.md` lines 274–282 are unmet
on every host. Folder open, workspace browse, workspace search, recents with
move and permission recovery, workspace watching, and R1 packaging have no
implementation to review.

**Recommendation:** Keep the four tasks pending. The next execution task
remains LNX-001F, which `HANDOFF.md` already names.

### B2 — Blocker: the plan's predecessors are open, so Phase 4 is not eligible

The plan says all WIN/MAC/LNX-001 aggregates precede DESK-001, DESK-001
materializes the 002 children, and DESK-002 waits for those aggregates plus
HARD-001 (`docs/Implementation-Plan.md` lines 171–173 and 190–194). The
`project.yaml` edges match.

Still open:

- LNX-001F document find, theme, and text controls (lines 1269–1290)
- LNX-001G large-document layout, selection, and accessibility
- LNX-001H external refresh, anchors, and reading restoration
- LNX-001I development artifact, license inventory, and relocated launch
- LNX-001 aggregate (lines 372–388)
- DESK-001 (lines 389–403)
- HARD-001 fuzzing, sanitizers, and a dependency advisory scan (lines 1849–1870)

MAC-001 and WIN-001 are completed, including their aggregates. That does
not open 002A. 002A's prerequisite is DESK-001, and DESK-001 waits for
LNX-001 as well as the other two skeletons.

**Recommendation:** Finish the Linux skeleton and its aggregate, then run
DESK-001. Create 002A–002F there, one PR per child, as the plan describes.
Schedule HARD-001 before any DESK-002 claim.

### C1 — Concern: the skeleton's file, settings, and watch surfaces are single-document, and they refuse a folder

A reader can mistake 001F and 001H behavior for 002A, 002D, and 002E. The
code draws the boundary explicitly, and the boundary is the skeleton:

- macOS open panel sets `canChooseDirectories = false`
  (`apps/macos/Sources/Markdown/MarkdownApp.swift` line 177). A directory
  granted any other way fails as `notRegularFile(isDirectory: true)` with
  "Folders can’t be opened as a document."
  (`apps/macos/Sources/MarkdownDocument/DocumentAccess.swift` lines 77 and
  182).
- Relative links to other files stay inert. The tooltip is "Links to other
  files are not available yet" when the core reports
  `MD_TARGET_REQUIRES_CAPABILITY`
  (`apps/macos/Sources/MarkdownRender/DocumentRenderer.swift` lines 633–637).
  `apps/macos/README.md` line 66 states that folder grants, recents, and
  security-scoped bookmarks remain MAC-002A work.
- Windows `GetOpenFileNameW` sets `OFN_FILEMUSTEXIST` and
  `OFN_DONTADDTORECENT` (`apps/windows/DocumentPicker.cs` lines 84–89).
  A directory opened with `FILE_FLAG_BACKUP_SEMANTICS` returns
  `DocumentError.NotRegularFile`
  (`apps/windows/Document/DocumentAccess.cs` lines 416–418 and 436–438).
- Linux `FileChooserNative` uses `FileChooserAction::Open`
  (`apps/linux/src/files.rs` lines 183–186). A non-regular file, including
  a directory, is `DocumentError::NotRegularFile` with "Choose a regular
  text file, rather than a folder or device."
  (`apps/linux/src/document/access.rs` lines 82 and 152–153).
- Preferences that do exist are appearance and text size from 001F:
  `ReaderPreferences` in
  `apps/macos/Sources/Markdown/ReaderPreferences.swift` lines 19–21, and
  theme plus `textScale` in `apps/windows/ReaderPreferences.cs` lines
  154–157. There is no settings migration, no per-workspace precedence,
  and no Linux preferences type (LNX-001F is pending).
- Relaunch state that does exist is one window's file, anchors, text scale,
  and outline visibility (`apps/windows/Restoration.cs` lines 10–22). It is
  not a recents list and not a folder bookmark. Linux has no equivalent yet.

This is the right shape for Phase 3. It is not Phase 4 acceptance.

**Recommendation:** When 002A is cut, add folder-grant, denied-subfolder,
revoked-grant, and moved-folder tests on the named OS, and keep the
single-file rejection tests so a directory is never parsed as a document.
Revisit `OFN_DONTADDTORECENT` only inside that task, if shell recents are
the Windows identity chosen at DESK-001.

### C2 — Concern: R1 and R2 both claim recents and workspace search

`docs/Requirements.md` puts recent files and workspaces, with move and
permission recovery, in R1.1 (lines 35–36). R1.4 (lines 64–66) allows
workspace file-name and content search to slip to "the second desktop
increment" if it would delay the single-document MVP. R2 (lines 77–79 and
86) lists recent documents, virtualized large-folder browsing, workspace
full-text search, and preferences again.

The implementation plan puts folder access, recents, browsing, workspace
search, preferences, and workspace watching into 002A–002E, and leaves R2
polish to DESK-003. DESK-001 is the closeout that is supposed to
materialize those children and choose the deepening order. Until that
reconciliation is written down, an implementer can build the same behavior
twice or drop it from both increments.

**Recommendation:** At DESK-001, record one owner decision: 002A owns
recents and folder identity, 002C owns workspace search, and DESK-003 owns
the R2 polish (virtualization, styles, focus, print) on top of those.
Point the R2 bullets that repeat R1.1 and R1.4 at that decision.

### C3 — Concern: the finished Windows and macOS skeletons have no D-030 review branch

`docs/Delivery-Workflow.md` lines 61–68 cuts a milestone review per host at
the end of 001D–001I, on a `review/<milestone>` branch, with a PR into
`main`. MAC-001 and WIN-001 are completed on `develop`. `main` is still
`m-core-fix`. No `review/mac-001` or `review/win-001` branch exists. This
`review/phase-4` branch is a later gate. Merging it would land 171 commits,
including an unfinished Linux skeleton, under a Phase 4 label.

**Recommendation:** Leave this branch unmerged. When the owner wants a
D-030 packet, cut it from the skeleton milestone (Windows and macOS can be
cut now; Linux after LNX-001I), not from Phase 4.

## Risks carried into Phase 4

These are already recorded on the shared core and the skeletons. They are
not new defects, and they become host bugs if 002C or 002E ignores them.

- Content search is bounded per call: 2 MiB per file and 256 MiB total by
  default (`abi.rs` lines 196–198). Parallel indexing multiplies that.
  Hosts should call `md_workspace_content_search` with a concurrency limit
  rather than re-parse every file through `md_session_*`. The session
  dense-delimiter case remains the larger ceiling for a host that does the
  latter.
- macOS budgets left open in `TODO.md` lines 593–621 were not remeasured
  here: prose 5 MiB warm launch 533–573 ms against a 400 ms target, cold
  launch unmeasured, first VoiceOver line query on 5 MiB at 3.6 s with the
  footprint staying near 1 GiB, and the external-refresh footprint rising
  to 887–1,043 MiB. DESK-001 is the comparison point. 002F should not
  accept packaging until those are fixed or explicitly accepted.
- CAP-02 and CAP-03 are Authoritative, not Measured. A Phase 4 claim needs
  a named machine, a denied nested item, and a moved or revoked grant.
- HARD-001 (coverage-guided fuzzing, sanitizers, advisory scan) has not
  run. DESK-002 is defined to wait for it.

## Manual checks

None of the Phase 4 manual checks can be run. There is no folder workspace,
no recents list, and no workspace search UI on any host. This machine also
cannot build the Windows, macOS, or Linux apps, so the existing skeleton
UIs were not exercised either.

## Recommendations, in order

1. Leave `main` at `m-core-fix`. This branch records the Phase 4 gap. It is
   not a D-030 milestone packet (finding C3).
2. Continue the Linux skeleton at LNX-001F, then LNX-001G, LNX-001H,
   LNX-001I, and the LNX-001 aggregate.
3. Cut the Windows and macOS skeleton milestone reviews when the owner
   wants `develop` considered for `main`. Do that on their own
   `review/<milestone>` branches.
4. Run DESK-001. Materialize 002A–002F per host, and write down the R1/R2
   split in finding C2. Carry the open macOS budgets in `TODO.md` lines
   593–621 into that closeout.
5. Implement each 002 child as its own PR with native evidence on the named
   OS. Shared CORE-005B, CORE-005B2, CORE-005C, and CORE-005D can be
   consumed as they stand. The locked workspace tests passed on this
   baseline (23 Rust tests, plus the FFI workspace tests inside the
   markdown-ffi suite of 35).
6. Run HARD-001 before DESK-002. Keep CAP-02, CAP-03, and folder watching
   at Authoritative until a named host measures them.

## Owner decisions

- This branch should not be merged to `main`. Phase 4 is not a milestone
  under review.
- At DESK-001, confirm whether recents and workspace search ship in the
  002 increment (the plan) or wait for DESK-003 (the R2 list). The plan and
  R1.4 can be read together only after that sentence exists.
- Whether the open macOS budgets in `TODO.md` lines 593–621 are accepted
  as-is before 002F, or must be fixed first.
