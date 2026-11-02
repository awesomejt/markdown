"""Publish and exercise the native Windows x64 shell in both configurations."""
from pathlib import Path
import os
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
PROJECT = ROOT / "apps/windows/Markdown.Windows.csproj"


def main():
    if sys.platform != "win32":
        raise SystemExit("Native Windows Python is required")
    installer = Path(os.environ.get("ProgramFiles(x86)", "C:/Program Files (x86)"))
    env = dict(os.environ)
    env["PATH"] = str(installer / "Microsoft Visual Studio/Installer") + os.pathsep + env["PATH"]
    subprocess.run(["dotnet", "restore", str(PROJECT), "--locked-mode", "-p:Platform=x64"],
                   cwd=PROJECT.parent, env=env, check=True)
    subprocess.run(["dotnet", "format", str(PROJECT), "--no-restore", "--verify-no-changes",
                    "--severity", "warn"], cwd=PROJECT.parent, env=env, check=True)
    for profile in ("Debug", "Release"):
        output = ROOT / "build/windows" / profile
        subprocess.run(["dotnet", "publish", str(PROJECT), "--no-restore", "-c", profile,
                        "-p:Platform=x64", "--nologo", "-o", str(output)],
                       cwd=PROJECT.parent, env=env, check=True)
        # No .NET runtime or globally installed App SDK is needed by this output.
        if (output / "coreclr.dll").exists():
            raise SystemExit("Expected Native AOT output, found CoreCLR")
        subprocess.run([str(output / "Markdown.Windows.exe"), "--smoke"],
                       cwd=output, env=env, check=True, timeout=60)
        print(f"{profile}: Native AOT window/commands/last-window-close smoke passed", flush=True)


if __name__ == "__main__":
    main()
