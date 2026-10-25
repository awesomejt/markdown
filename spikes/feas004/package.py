#!/usr/bin/env python3
"""Assemble local development bundles; never signs, installs, or publishes."""
from pathlib import Path
import plistlib
import shutil

root = Path(__file__).resolve().parents[2]
binary = root / "spikes/feas004/.build/release/MacRenderingProbe"
for name, mode, swiftui in [("MarkdownProbeTK1", "tk1", False), ("MarkdownProbeTK2", "tk2", False),
                           ("MarkdownProbeBlocks", "blocks", False), ("MarkdownProbeSwiftUI", "tk2", True)]:
    contents = root / "build/feas004" / f"{name}.app" / "Contents"
    (contents / "MacOS").mkdir(parents=True, exist_ok=True)
    (contents / "Resources").mkdir(exist_ok=True)
    shutil.copy2(binary, contents / "MacOS/MacRenderingProbe")
    shutil.copy2(root / "fixtures/markdown/foundation/gfm-tour.md", contents / "Resources/gfm-tour.md")
    shutil.copy2(root / "fixtures/licenses/markdown-viewer-APACHE-2.0.txt", contents / "Resources/LICENSE-fixture.txt")
    info = {"CFBundleExecutable": "MacRenderingProbe", "CFBundleIdentifier": f"dev.markdown.feas004.{mode}{'swiftui' if swiftui else ''}",
            "CFBundleName": name, "CFBundlePackageType": "APPL", "CFBundleVersion": "1",
            "NSHighResolutionCapable": True, "LSMinimumSystemVersion": "13.0",
            "ProbeMode": mode, "ProbeSwiftUI": swiftui,
            "FixtureOriginCommit": "0ab59c0f7bf91730bf9683b47a5eb2658f331b00"}
    (contents / "Info.plist").write_bytes(plistlib.dumps(info))
    print(contents.parent.relative_to(root))
