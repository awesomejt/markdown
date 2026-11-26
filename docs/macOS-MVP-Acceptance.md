# macOS MVP aggregate acceptance (MAC-002)

The macOS MVP delivery phases are merged. The owner approved functional
aggregate closeout on 2026-11-16, with the explicit gaps below carried to
DESK-002 (D-037). No performance target or release check is waived.

| Phase | Delivered evidence |
| --- | --- |
| MAC-002A | [PR #82](https://github.com/awesomejt/markdown/pull/82): folder grants, recents and access recovery. |
| MAC-002B | [PR #83](https://github.com/awesomejt/markdown/pull/83): native workspace browser, navigation and relative links. |
| MAC-002C | [PR #84](https://github.com/awesomejt/markdown/pull/84): bounded workspace search and result navigation. |
| MAC-002D | [PR #85](https://github.com/awesomejt/markdown/pull/85): versioned preferences and native settings. |
| MAC-002E | [PR #86](https://github.com/awesomejt/markdown/pull/86): workspace watching and restoration. |
| MAC-002F | [PR #88](https://github.com/awesomejt/markdown/pull/88): R1 conformance, diagnostics and development packaging. |

The [macOS README](../apps/macos/README.md#mac-002f-local-evidence) records
native tests, artifact checks, review dispositions and measurements for these
phases. The 2026-11-15 release measurements found median warm launch to first
text of 419 ms for 1 MiB prose and 605 ms for 5 MiB prose, against the 400 ms
D-016 target; long lines measured 398 ms. The misses remain in
[open native budgets](../TODO.md#open-native-budgets). Cold launch has not been
measured. A real Finder installation, a quarantined download on a second Mac,
VoiceOver on properties, and About panel appearance remain unverified. The
unsigned, unnotarized artifact is for development only.

The owner-approved functional boundary in D-037 applies to MAC-002. The
performance and manual gaps remain obligations for DESK-002 via MAC-003; see
[macOS-Release-Evidence.md](macOS-Release-Evidence.md) for the gap register,
warm baseline after CORE-007, cold-launch procedure and manual checklist.

## Aggregate checks

On 2026-11-16, source `511fdfc` on Apple M5 Max, macOS 27.0.1,
Xcode 27.1 and Rust 1.99.1, the following passed with Cargo's bin directory
on `PATH` and `TMPDIR=/private/tmp`:

- `.build/feas004-venv/bin/python apps/macos/tools/check.py`: debug and
  release tests and smokes, document and folder relaunch, preferences,
  relocated zip and installed development disk-image verification, and
  the release core probe (zero native handles).
- `.build/feas004-venv/bin/python tools/check.py common`.

The first native attempt lacked `rustc` on `PATH`. The first common attempt
used macOS's `/var` temporary path and failed a path comparison against its
`/private/var` canonical form; the canonical temporary-directory rerun passed.
These checks do not establish the unmeasured or missed budgets above.

A fresh five-run warm-launch measurement using
`apps/macos/tools/measure.py launch --configuration release --runs 5`
reported 1 MiB prose median 399.73 ms (378.40–404.56; target missed by
individual runs), 5 MiB prose median 596.12 ms (571.23–599.23), and 1 MiB
long lines median 380.74 ms (376.38–393.25). The 5 MiB prose median
exceeds 400 ms; the 1 MiB prose median is below 400 ms but two of five runs
exceed it. D-016 requires at least five runs and their spread without specifying
a median or maximum threshold. This measurement tool reports a profile as
meeting the target only when every run is at or below 400 ms, so it reports
both prose profiles as misses. The earlier 419 ms median is a separate run,
not the result of this measurement. Cold launch remains unmeasured.
