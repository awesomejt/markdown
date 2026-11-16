#!/usr/bin/env python3
"""Package the release app as a local, unsigned development zip and disk image, and verify
the zip relocated and the disk image installed, launched and uninstalled."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import subprocess
import tempfile
import time

from build import REPO_ROOT, build
from package_core import cargo_homes, prepare_core

ARTIFACTS = REPO_ROOT / "build" / "macos" / "artifacts"
BUNDLE_ID = "io.github.awesomejt.markdown"
SYSTEM_PREFIXES = ("/usr/lib/", "/System/Library/")
LSREGISTER = ("/System/Library/Frameworks/CoreServices.framework/Frameworks/"
              "LaunchServices.framework/Support/lsregister")


def smoke_environment() -> dict:
    return {k: v for k, v in os.environ.items() if not k.startswith("DYLD_")}


def run_smoke(executable: Path, cwd: Path, env: dict) -> None:
    """Launch a bundle executable with --smoke-test and require its success marker."""
    try:
        result = subprocess.run([str(executable), "--smoke-test"], cwd=cwd, env=env, text=True,
                                capture_output=True, timeout=30)
    except subprocess.TimeoutExpired as error:
        # TimeoutExpired can carry bytes even when text=True was requested.
        for output in (error.stdout, error.stderr):
            if output:
                print(output.decode(errors="replace") if isinstance(output, bytes) else output,
                      end="", flush=True)
        raise
    print(result.stdout, end="", flush=True)
    if result.stderr:
        print(result.stderr, end="", flush=True)
    result.check_returncode()
    if "shell-smoke: pass" not in result.stdout:
        raise SystemExit("app exited without completing its smoke")


def check_version(executable: Path, env: dict, configuration: str) -> str:
    """Require `--version` to name the app, the semantic contract and the core package."""
    line = subprocess.check_output([str(executable), "--version"], env=env, text=True,
                                   timeout=30).strip()
    pattern = (r"Markdown \d+\.\d+\.\d+ \(\d+\); Semantic contract: model-v2 \(ABI v1, anchor v1\); "
               rf"Core package: {configuration} [0-9a-f]{{12}}; Source: [0-9a-f]{{12}}(-dirty)?")
    if not re.fullmatch(pattern, line):
        raise SystemExit(f"unexpected --version output: {line}")
    return line


def package(app: Path) -> tuple[Path, Path]:
    """Zip an assembled bundle with ditto, make its disk image, and record both beside them."""
    info = json.loads((app / "Contents/Resources/BuildInfo.json").read_text())
    if info["configuration"] != "release":
        raise SystemExit("the development artifact is the release bundle")
    with (app / "Contents/Info.plist").open("rb") as file:
        version = plistlib.load(file)["CFBundleShortVersionString"]
    name = (f"Markdown-{version}-{info['source_commit'][:12]}"
            f"{'-dirty' if info['source_dirty'] else ''}-macos-arm64")
    ARTIFACTS.mkdir(parents=True, exist_ok=True)
    archive = ARTIFACTS / f"{name}.zip"
    image = ARTIFACTS / f"{name}.dmg"
    # Only this tool's own output is replaced.
    archive.unlink(missing_ok=True)
    image.unlink(missing_ok=True)
    with tempfile.TemporaryDirectory(prefix="markdown-package-") as directory:
        volume = Path(directory) / "volume"
        volume.mkdir()
        staged = volume / "Markdown.app"
        subprocess.run(["ditto", str(app), str(staged)], check=True)
        # The debug map names the build's object and source paths. strip
        # reapplies the linker's ad hoc signature; the local build keeps it.
        subprocess.run(["strip", "-S", str(staged / "Contents/MacOS/Markdown")], check=True)
        subprocess.run(["ditto", "-c", "-k", "--keepParent", str(staged), str(archive)],
                       check=True)
        # The disk image is the usual macOS install: drag the app onto the
        # Applications link beside it.
        (volume / "Applications").symlink_to("/Applications")
        hdiutil(["create", "-volname", "Markdown", "-srcfolder", str(volume), "-fs", "HFS+",
                 "-format", "UDZO", "-imagekey", "zlib-level=9", "-quiet", str(image)])
    def digest(path: Path) -> dict:
        return {"file": path.name, "bytes": path.stat().st_size,
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
    zipped = digest(archive)
    record = dict(info, artifact=archive.name, bytes=zipped["bytes"], sha256=zipped["sha256"],
                  disk_image=digest(image), bundle_version=version)
    (ARTIFACTS / f"{name}.json").write_text(json.dumps(record, indent=2) + "\n")
    return archive, image


def hdiutil(arguments: list[str], attempts: int = 3) -> str:
    """Run hdiutil, retrying the transient "Resource busy" failures disk images can hit."""
    for attempt in range(attempts):
        result = subprocess.run(["hdiutil", *arguments], text=True, capture_output=True)
        if result.returncode == 0:
            return result.stdout
        if attempt == attempts - 1 or "busy" not in (result.stderr + result.stdout).lower():
            raise SystemExit(f"hdiutil {arguments[0]} failed: {result.stderr.strip()}")
        time.sleep(2)
    raise AssertionError("unreachable")


def private_prefixes() -> set[str]:
    return {str(REPO_ROOT), str(Path.home()), *cargo_homes()}


def verify(archive: Path, *, env: dict) -> None:
    """Extract the zip outside the checkout and check what a copy to another folder needs."""
    with tempfile.TemporaryDirectory(prefix="markdown-artifact-") as directory:
        root = Path(directory).resolve()
        extracted = root / "extracted"
        subprocess.run(["ditto", "-x", "-k", str(archive), str(extracted)], check=True)
        if [path.name for path in extracted.iterdir()] != ["Markdown.app"]:
            raise SystemExit("the artifact must hold exactly Markdown.app")
        app = extracted / "Markdown.app"
        packages = check_bundle(app)
        check_version(app / "Contents/MacOS/Markdown", env, "release")
        launch(app, root, env)
        print(f"artifact: {archive.name} verified relocated ({packages} "
              "linked packages inventoried, direct and Launch Services launches)", flush=True)


def verify_image(image: Path, archive: Path, *, env: dict) -> None:
    """Install from the disk image as Finder does, into a stand-in Applications folder,
    then launch the installed copy and uninstall it."""
    with tempfile.TemporaryDirectory(prefix="markdown-install-") as directory:
        root = Path(directory).resolve()
        mount = root / "mount"
        mount.mkdir()
        applications = root / "Applications"
        applications.mkdir()
        hdiutil(["attach", "-readonly", "-nobrowse", "-noautoopen", "-noverify",
                 "-mountpoint", str(mount), str(image)])
        try:
            entries = sorted(path.name for path in mount.iterdir() if not path.name.startswith("."))
            if entries != ["Applications", "Markdown.app"]:
                raise SystemExit(f"the disk image must hold Markdown.app and Applications, not {entries}")
            if os.readlink(mount / "Applications") != "/Applications":
                raise SystemExit("the disk image's Applications link must lead to /Applications")
            # Dragging to Applications copies the bundle; ditto copies the same way.
            subprocess.run(["ditto", str(mount / "Markdown.app"),
                            str(applications / "Markdown.app")], check=True)
        finally:
            try:
                hdiutil(["detach", str(mount), "-quiet"])
            except SystemExit:
                hdiutil(["detach", str(mount), "-force", "-quiet"])
        app = applications / "Markdown.app"
        # The installed bundle is byte for byte the zip's.
        with tempfile.TemporaryDirectory(prefix="markdown-compare-") as other:
            subprocess.run(["ditto", "-x", "-k", str(archive), other], check=True)
            if bundle_digest(Path(other) / "Markdown.app") != bundle_digest(app):
                raise SystemExit("the disk image's bundle differs from the zip's")
        check_bundle(app)
        version = check_version(app / "Contents/MacOS/Markdown", env, "release")
        launch(app, root, env)
        # Uninstalling is removing the bundle; launch() already unregistered it.
        subprocess.run(["rm", "-rf", str(app)], check=True)
        if app.exists():
            raise SystemExit("the installed app could not be removed")
        print(f"artifact: {image.name} installed, launched (direct and Launch Services) and "
              f"uninstalled; {version}", flush=True)


def bundle_digest(app: Path) -> str:
    """One SHA-256 over every file's relative path, mode and contents."""
    digest = hashlib.sha256()
    for path in sorted(app.rglob("*")):
        if path.is_file() and not path.is_symlink():
            digest.update(str(path.relative_to(app)).encode() + b"\0")
            digest.update(oct(path.stat().st_mode & 0o777).encode() + b"\0")
            digest.update(path.read_bytes())
    return digest.hexdigest()


def check_bundle(app: Path) -> int:
    """Check a packaged bundle's identity, linkage, paths, signature and licenses;
    return how many packages its inventory lists."""
    contents = app / "Contents"
    executable = contents / "MacOS/Markdown"
    subprocess.run(["plutil", "-lint", str(contents / "Info.plist")], check=True)
    with (contents / "Info.plist").open("rb") as file:
        plist = plistlib.load(file)
    if plist["CFBundleIdentifier"] != BUNDLE_ID or plist["CFBundleExecutable"] != "Markdown":
        raise SystemExit("unexpected bundle identity")
    if not os.access(executable, os.X_OK):
        raise SystemExit("the bundle executable lost its execute permission")

    archs = subprocess.check_output(["lipo", "-archs", str(executable)], text=True).split()
    if archs != ["arm64"]:
        raise SystemExit(f"unexpected architectures {archs}")
    build_version = subprocess.check_output(["vtool", "-show-build", str(executable)], text=True)
    if "platform MACOS" not in build_version or "minos 27.0" not in build_version:
        raise SystemExit("the executable must target macOS 27.0")

    # Only the operating system's libraries, found by absolute path.
    libraries = subprocess.check_output(["otool", "-L", str(executable)],
                                        text=True).splitlines()[1:]
    for line in libraries:
        path = line.split(" (")[0].strip()
        if not path.startswith(SYSTEM_PREFIXES):
            raise SystemExit(f"non-system library dependency {path}")
    commands = subprocess.check_output(["otool", "-l", str(executable)], text=True).splitlines()
    for index, line in enumerate(commands):
        if line.strip() == "cmd LC_RPATH":
            path = commands[index + 2].split()[1]
            if not path.startswith("@"):
                raise SystemExit(f"absolute run path {path}")

    # No build-machine path anywhere in the bundle.
    prefixes = [prefix.encode() for prefix in private_prefixes()]
    for path in app.rglob("*"):
        if path.is_file() and any(prefix in path.read_bytes() for prefix in prefixes):
            raise SystemExit(f"{path.relative_to(app)} names a build-machine path")

    # Unsigned: only the linker's ad hoc signature, no identity or bundle seal.
    signature = subprocess.run(["codesign", "-dv", str(app)], text=True,
                               capture_output=True).stderr
    if "Authority=" in signature or (contents / "_CodeSignature").exists():
        raise SystemExit("the development artifact must not be signed")

    licenses = contents / "Resources/Licenses"
    if (licenses / "LICENSE").read_bytes() != (REPO_ROOT / "LICENSE").read_bytes():
        raise SystemExit("the bundled LICENSE differs from the repository's")
    inventory = json.loads((licenses / "inventory.json").read_text())
    notices = (licenses / "THIRD-PARTY-NOTICES.txt").read_text()
    names = {package["name"] for package in inventory["packages"]}
    if not {"markdown-core", "markdown-ffi", "pulldown-cmark"} <= names:
        raise SystemExit("the inventory is missing the core packages")
    for package in inventory["packages"]:
        heading = f"\n{package['name']} {package['version']}\nLicense: {package['license']}\n"
        if heading not in notices:
            raise SystemExit(f"no notice for {package['name']} {package['version']}")
        if package["source"] != "workspace" and not package["files"]:
            raise SystemExit(f"no license text for {package['name']}")
        for name in package["files"]:
            if f"----- {name} -----" not in notices:
                raise SystemExit(f"{package['name']}: {name} missing from the notices")
    return len(inventory["packages"])


def launch(app: Path, root: Path, env: dict) -> None:
    """Run the smoke from `app`: directly, then through Launch Services as Finder does."""
    executable = app / "Contents/MacOS/Markdown"
    run_smoke(executable, root, env)
    output = root / "launch-services.out"
    try:
        subprocess.run(["open", "-n", "-W", "--stdout", str(output), "--stderr",
                        str(root / "launch-services.err"), "-a", str(app),
                        "--args", "--smoke-test"], check=True, timeout=60)
    except subprocess.TimeoutExpired:
        # launchd started the app, not open: stop it before its folder goes.
        subprocess.run(["pkill", "-f", str(executable)], check=False)
        raise
    finally:
        # open registered this temporary copy and its document types.
        subprocess.run([LSREGISTER, "-u", str(app)], check=False)
    # open does not report the app's exit status; the smoke prints its
    # marker last, just before Quit.
    lines = output.read_text().splitlines()
    if not lines or not lines[-1].startswith("shell-smoke: pass"):
        print((root / "launch-services.err").read_text(), end="", flush=True)
        raise SystemExit("the Launch Services launch did not complete its smoke")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--no-verify", action="store_true",
                        help="package without the relocated launch checks")
    arguments = parser.parse_args()
    prepare_core("release")
    archive, image = package(build("release", prepared=True))
    if not arguments.no_verify:
        verify(archive, env=smoke_environment())
        verify_image(image, archive, env=smoke_environment())
    print(archive)
    print(image)
