"""The cargo tools the gate runs that rustup does not install (cargo-shear, cargo-deny), at the versions it expects.

Their versions are written once, in `[workspace.metadata.tools]` of Cargo.toml. `check` (in `poe check`, before the
first of them runs) fails naming the install command when one is missing or at another version; `install` (in
`poe setup`) installs what `check` would report, with cargo-binstall when it is installed (a download) and with
`cargo install --locked` otherwise (a build from source); `specs` prints the pins as `tool@version,...`, the line CI
gives taiki-e/install-action (prebuilt, checksum-verified downloads).

Interface:
    pinned(root)             -> tool name -> version, from root/Cargo.toml
    problems(pins)           -> one sentence per tool that is missing or at another version (empty when all are right)
    main(argv)               -> exit code: 0 all right (or installed, or printed), 1 a problem `check` reports or a
                                failed install
"""

import argparse
import re
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
VERSION = re.compile(r"\b(\d+\.\d+\.\d+)\b")


def pinned(root: Path) -> dict[str, str]:
    """The tools of `[workspace.metadata.tools]` in `root`/Cargo.toml, each with the version the gate expects."""
    manifest = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    return dict(manifest["workspace"]["metadata"]["tools"])


def installed_version(tool: str) -> str | None:
    """The version of `tool` (a program such as `cargo-deny`) on the PATH, or None if it is not there."""
    program = shutil.which(tool)
    if program is None:
        return None
    result = subprocess.run([program, "--version"], capture_output=True, encoding="utf-8", check=False)  # noqa: S603  a program found on PATH by its fixed name, with a fixed argument
    found = VERSION.search(result.stdout + result.stderr)
    return found.group(1) if found else None


def install_command(tool: str, version: str) -> list[str]:
    """The command that installs `tool` at `version`: cargo-binstall's download if it is installed, else a build."""
    if shutil.which("cargo-binstall"):
        return ["cargo", "binstall", "--no-confirm", "--locked", f"{tool}@{version}"]
    return ["cargo", "install", "--locked", f"{tool}@{version}"]


def problems(pins: dict[str, str]) -> list[str]:
    """One sentence per tool of `pins` that is missing or installed at another version, naming the fix."""
    found = []
    for tool, version in pins.items():
        actual = installed_version(tool)
        if actual != version:
            state = "is not installed" if actual is None else f"is {actual}"
            fix = " ".join(install_command(tool, version))
            found.append(f"{tool} {state}, the gate needs {version}: run `{fix}` (or `uv run poe setup`)")
    return found


def main(argv: list[str] | None = None) -> int:
    """`check` the tools against Cargo.toml, `install` the ones that are missing or at another version, or print their
    `specs`."""
    parser = argparse.ArgumentParser(description="Check, install or list the cargo tools the gate runs.")
    parser.add_argument("action", choices=["check", "install", "specs"])
    parser.add_argument("--root", type=Path, default=ROOT, help="the repository (default: this script's)")
    arguments = parser.parse_args(argv)
    pins = pinned(arguments.root)
    if arguments.action == "specs":
        print(",".join(f"{tool}@{version}" for tool, version in pins.items()))
        return 0
    if arguments.action == "check":
        found = problems(pins)
        for problem in found:
            print(f"rust-tools: {problem}", file=sys.stderr)
        return 1 if found else 0
    for tool, version in pins.items():
        if installed_version(tool) != version:
            command = install_command(tool, version)
            print(f"rust-tools: {' '.join(command)}")
            if subprocess.run(command, check=False).returncode != 0:  # noqa: S603  cargo with a pinned tool name and version from Cargo.toml
                return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
