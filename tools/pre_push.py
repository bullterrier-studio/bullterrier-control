"""The pre-push hook: run the whole local gate (`uv run poe check`) when the push updates `main` or anything that is
not a branch (a tag), and skip it for any other branch.

Why: a feature branch is checked by CI when its pull request opens, so running the full gate on every push to it only
slows the work down. A direct push to `main` has no pull request in front of it, so the gate runs first, locally,
instead of leaving a red `main` for CI to find. pre-commit gives the hook the pushed ref in PRE_COMMIT_REMOTE_BRANCH
(for example `refs/heads/main`); when it is missing or is not a branch ref the gate runs: skipping is only for a
branch that is clearly not main.

A push that changes only documentation (the top-level and `docs/*.md` pages, the pictures in `docs/images/`) skips the
gate too: no code is in it, and CI still runs everything on the pushed `main`. The pushed range comes from
PRE_COMMIT_FROM_REF and PRE_COMMIT_TO_REF; when it cannot be read the gate runs.

A gate that already passed on exactly the pushed tree is not repeated. `uv run poe check` begins with
`uv run python tools/pre_push.py start` and ends with `uv run python tools/pre_push.py pass`: the second records the
tree of `HEAD` in the git folder (never committed) only if the working tree was clean and `HEAD`'s tree was the same at
both moments, so the record names contents that were checked in full. A push whose tip has that tree skips the gate;
any other tree runs it. CI checks everything again.
"""

import os
import shutil
import subprocess
import sys
from pathlib import Path, PurePosixPath

STARTED, PASSED = (
    "gate-started",
    "gate-passed",
)  # files in the git folder: the tree a running gate began on, the tree that passed


def must_run(remote_ref: str) -> bool:
    """Whether a push to `remote_ref` (as pre-commit reports it) must pass the local gate first."""
    if not remote_ref.startswith("refs/heads/"):
        return True
    return remote_ref == "refs/heads/main"


def is_documentation(path: str) -> bool:
    """Whether a changed path (relative to the repository, `/`-separated) is a page or picture that holds no code."""
    parts = PurePosixPath(path).parts
    if len(parts) == 1:
        return parts[0].endswith(".md")
    return parts[0] == "docs" and (parts[1] == "images" or (len(parts) == 2 and parts[1].endswith(".md")))


def _git(*args: str) -> str | None:
    """git's output, or None when it fails (a ref this clone lacks, not a repository)."""
    result = subprocess.run(  # noqa: S603  fixed argv; refs and paths are arguments, never a shell
        ["git", *args],  # noqa: S607  git is on the PATH of any clone
        capture_output=True,
        encoding="utf-8",
        check=False,
    )
    return result.stdout if result.returncode == 0 else None


def _record_path(name: str) -> Path | None:
    """Where the git folder keeps the record `name` (right in a worktree too), None outside a repository."""
    path = _git("rev-parse", "--git-path", name)
    return Path(path.strip()) if path else None


def _tree_of(rev: str) -> str | None:
    """The hash of the tree `rev` holds (equal for two commits with the same files), None when git cannot say."""
    tree = _git("rev-parse", f"{rev}^{{tree}}")
    return tree.strip() if tree else None


def _clean_tree() -> bool:
    """Whether the working tree has no change, staged, unstaged or untracked (ignored files do not count)."""
    return _git("status", "--porcelain") == ""


def record_gate_start() -> None:
    """Note the tree a gate begins on: that of `HEAD` if the working tree is clean, nothing if not or outside a
    repository. The first step of `uv run poe check`."""
    path = _record_path(STARTED)
    if path is not None:
        path.write_text((_tree_of("HEAD") if _clean_tree() else None) or "", encoding="utf-8")


def record_gate_pass() -> None:
    """Remember that the gate passed on the tree of `HEAD`, if it began on that very tree with a clean working tree and
    the working tree is clean now. The last step of `uv run poe check`, reached only when every step passed."""
    started, passed = _record_path(STARTED), _record_path(PASSED)
    if started is None or passed is None or not started.exists():
        print("gate: no record that a gate began here, so this pass is not remembered for the push")
        return
    began, now = started.read_text(encoding="utf-8"), _tree_of("HEAD")
    started.unlink()
    if began and began == now and _clean_tree():
        passed.write_text(began, encoding="utf-8")
        print(f"gate: passed on tree {began[:12]}; a push of exactly this tree will not repeat it")
    else:
        print("gate: the tree was not clean and unchanged from start to end, so this pass is not remembered")


def gate_passed_on(rev: str) -> bool:
    """Whether the gate passed, as `record_gate_pass` remembers it, on the tree of the commit `rev`."""
    passed = _record_path(PASSED)
    tree = _tree_of(rev)
    return bool(tree) and passed is not None and passed.exists() and passed.read_text(encoding="utf-8") == tree


def pushed_files(from_ref: str, to_ref: str) -> list[str] | None:
    """The paths the push changes, or None when git cannot say (a new branch, a ref this clone lacks)."""
    if not from_ref or not to_ref:
        return None
    diff = subprocess.run(  # noqa: S603  fixed argv; the refs come from git, as arguments, never a shell
        ["git", "diff", "--name-only", f"{from_ref}..{to_ref}"],  # noqa: S607  git is on the PATH of any clone
        capture_output=True,
        encoding="utf-8",
        check=False,
    )
    return diff.stdout.splitlines() if diff.returncode == 0 else None


def main() -> int:
    """Run the gate when the push needs it; return the exit status git should see."""
    if os.environ.get("BT_CONTROL_GATE_PASSED") == "1":  # set by `poe release`, which has just run the gate
        print("pre-push: the gate has just passed (poe release), so it is not run again")
        return 0
    remote_ref = os.environ.get("PRE_COMMIT_REMOTE_BRANCH", "")
    if not must_run(remote_ref):
        print(
            f"pre-push: {remote_ref} is not main, so the full gate is left to CI (run `uv run poe check` to see it now)"
        )
        return 0
    changed = pushed_files(os.environ.get("PRE_COMMIT_FROM_REF", ""), os.environ.get("PRE_COMMIT_TO_REF", ""))
    if changed and all(is_documentation(path) for path in changed):
        print("pre-push: only documentation changed, so the full gate is left to CI (`uv run poe check` runs it now)")
        return 0
    if gate_passed_on(os.environ.get("PRE_COMMIT_TO_REF", "")):
        print("pre-push: the gate already passed on exactly the pushed tree, so it is not repeated")
        return 0
    uv = shutil.which("uv")
    if uv is None:
        print("pre-push: uv is not on the PATH, so the gate cannot run; install uv (https://docs.astral.sh/uv/)")
        return 1
    return subprocess.run([uv, "run", "poe", "check"], check=False).returncode  # noqa: S603  fixed argv, no input


if __name__ == "__main__":
    match sys.argv[1:]:
        case ["start"]:
            record_gate_start()
        case ["pass"]:
            record_gate_pass()
        case []:
            sys.exit(main())
        case other:
            sys.exit(f"usage: pre_push.py [start | pass] (the hook gives no argument), got {other}")
