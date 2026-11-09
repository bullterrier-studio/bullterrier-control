"""The pre-push hook: run the local gate when the push updates `main` or anything that is not a branch (a tag), and skip
it for any other branch. Two tiers: a push to `main` runs `uv run poe check`; a tag, or a ref this hook cannot classify,
runs `uv run poe full`, which also runs the gate proofs (the tests marked `gate`, each proving that a gate fails on a
known-bad input). `check` runs the proofs only when the change touches what they test (`proofs`, below).

Why: a feature branch is checked by CI when its pull request opens, so running the full gate on every push to it only
slows the work down. A direct push to `main` has no pull request in front of it, so the gate runs first, locally,
instead of leaving a red `main` for CI to find. pre-commit gives the hook the pushed ref in PRE_COMMIT_REMOTE_BRANCH
(e.g. `refs/heads/main`); when it is missing or is not a branch ref the gate runs: skipping is only for a
branch that is clearly not main.

A push that changes only documentation (the top-level and `docs/*.md` pages, the pictures in `docs/images/`) skips the
gate too: no code is in it, and CI still runs everything on the pushed `main`. The pushed range comes from
PRE_COMMIT_FROM_REF and PRE_COMMIT_TO_REF; when it cannot be read the gate runs.

A gate that already passed on exactly the pushed tree is not repeated. `uv run poe check` (and `full`) begins with
`uv run python tools/pre_push.py start` and ends with `uv run python tools/pre_push.py pass TIER`: the second
records the tree of `HEAD` and the tier in the git folder (never committed) only if the working tree was clean and
`HEAD`'s tree was the same at both moments, so the record names contents that were checked in full. A push whose tip
has that tree skips the gate if the pass was of the tier the push needs or a higher one (`full` covers `check`, never
the other way round); any other tree runs it. CI checks everything again.

`pre_push.py proofs` is the last gate step of `check`: it runs `poe gate-tests` when a file the change touches matches
`[tool.gate-proofs] triggers` in pyproject.toml (compared with the merge base with `origin/main`, working tree and
untracked files included), and when that base cannot be read (no `origin/main`, a shallow clone): skipping is only for a
change that clearly leaves the proofs' inputs alone.

One gate runs per checkout: `start` also records the PID of the `poe` process running the gate, and refuses to begin
while a live `poe` process holds that record (two gates in one checkout share its build folder and double the memory
they take). A record left by a gate that died names a dead process, or one that is no longer `poe`, and is ignored.
Where `ps` is missing (Windows) nothing is recorded and nothing is refused.
"""

import fnmatch
import os
import re
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path, PurePosixPath

STARTED, PASSED, RUNNING = (
    "gate-started",
    "gate-passed",
    "gate-running",
)  # files in the git folder: the tree a running gate began on, the tree that passed, the PID of the `poe` running one
POE = re.compile(r"\bpoe(thepoet)?\b")
TIERS = ("check", "full")  # the gates a pass can be of, lowest first: a `full` pass covers a push that needs `check`


def must_run(remote_ref: str) -> bool:
    """Whether a push to `remote_ref` (as pre-commit reports it) must pass the local gate first."""
    if not remote_ref.startswith("refs/heads/"):
        return True
    return remote_ref == "refs/heads/main"


def tier_for(remote_ref: str) -> str:
    """The gate a push to `remote_ref` needs: `check` for `main`, `full` for a tag or a ref it cannot classify."""
    return "check" if remote_ref == "refs/heads/main" else "full"


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


def _process(pid: int) -> tuple[int, str] | None:
    """(parent PID, command line) of process `pid`, None when it is not running."""
    row = subprocess.run(  # noqa: S603  fixed argv; the PID is a number
        ["ps", "-o", "ppid=,command=", "-p", str(pid)],  # noqa: S607  ps is a system tool
        capture_output=True,
        encoding="utf-8",
        check=False,
    ).stdout.split(None, 1)
    return (int(row[0]), row[1]) if len(row) == 2 else None


def _gate_owner() -> int | None:
    """The PID of the `poe` process running this step: the nearest ancestor whose command line names poe."""
    pid = os.getppid()
    while (found := _process(pid)) is not None:
        if POE.search(found[1]):
            return pid
        pid = found[0]
    return None


def record_gate_start() -> None:
    """Note the tree a gate begins on: that of `HEAD` if the working tree is clean, nothing if not or outside a
    repository, and the PID of the `poe` running the gate. Exit with a message when another gate is running here.
    The first step of `uv run poe check` and `full`."""
    path, running = _record_path(STARTED), _record_path(RUNNING)
    if running is not None and shutil.which("ps"):
        holder = int(running.read_text(encoding="utf-8")) if running.exists() else None
        found = _process(holder) if holder else None
        if found is not None and POE.search(found[1]):
            sys.exit(f"gate: another gate is already running in this checkout (pid {holder}); wait for it or stop it")
        owner = _gate_owner()
        if owner is not None:
            running.write_text(str(owner), encoding="utf-8")
    if path is not None:
        path.write_text((_tree_of("HEAD") if _clean_tree() else None) or "", encoding="utf-8")


def record_gate_pass(tier: str = "check") -> None:
    """Remember that the gate of `tier` passed on the tree of `HEAD`, if it began on that very tree with a clean working
    tree and the working tree is clean now. The last step of `uv run poe check` and `full`, reached only when every
    step passed."""
    started, passed = _record_path(STARTED), _record_path(PASSED)
    if started is None or passed is None or not started.exists():
        print("gate: no record that a gate began here, so this pass is not remembered for the push")
        return
    began, now = started.read_text(encoding="utf-8"), _tree_of("HEAD")
    started.unlink()
    if (running := _record_path(RUNNING)) is not None:
        running.unlink(missing_ok=True)
    if began and began == now and _clean_tree():
        passed.write_text(f"{began} {tier}", encoding="utf-8")
        print(f"gate: {tier} passed on tree {began[:12]}; a push of exactly this tree will not repeat it")
    else:
        print("gate: the tree was not clean and unchanged from start to end, so this pass is not remembered")


def gate_passed_on(rev: str, tier: str = "check") -> bool:
    """Whether a gate of `tier` or a higher one passed, as `record_gate_pass` remembers it, on the tree of `rev`."""
    passed = _record_path(PASSED)
    tree = _tree_of(rev)
    if not tree or passed is None or not passed.exists():
        return False
    recorded_tree, _, recorded_tier = passed.read_text(encoding="utf-8").partition(" ")
    return recorded_tree == tree and recorded_tier in TIERS and TIERS.index(recorded_tier) >= TIERS.index(tier)


def changed_since_base() -> list[str] | None:
    """The paths the working tree differs in from the merge base with `origin/main` (committed, staged, unstaged and
    untracked), None when the base cannot be read."""
    base = _git("merge-base", "HEAD", "origin/main")
    if not base:
        return None
    tracked = _git("diff", "--name-only", base.strip())
    untracked = _git("ls-files", "--others", "--exclude-standard")
    if tracked is None or untracked is None:
        return None
    return [*tracked.splitlines(), *untracked.splitlines()]


def proofs_needed(changed: list[str] | None, triggers: list[str]) -> bool:
    """Whether the gate proofs must run for a change of the `changed` paths: when the paths are unknown (None) or any
    one matches a trigger glob (`*` also crosses `/`)."""
    return changed is None or any(fnmatch.fnmatchcase(path, glob) for path in changed for glob in triggers)


def run_proofs() -> int:
    """`poe gate-tests` if the change touches what the proofs test; the last gate step of `check`."""
    config = tomllib.loads(Path("pyproject.toml").read_text(encoding="utf-8"))
    if not proofs_needed(changed_since_base(), config["tool"]["gate-proofs"]["triggers"]):
        print("proofs: the change touches nothing the gate proofs test, so they are left to `poe full` and CI")
        return 0
    print("proofs: the change touches what the gate proofs test (or its base is unknown), so they run", flush=True)
    return subprocess.run([sys.executable, "-m", "poethepoet", "gate-tests"], check=False).returncode


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
        print(f"pre-push: {remote_ref} is not main, so the gate is left to CI (run `uv run poe check` to see it now)")
        return 0
    changed = pushed_files(os.environ.get("PRE_COMMIT_FROM_REF", ""), os.environ.get("PRE_COMMIT_TO_REF", ""))
    if changed and all(is_documentation(path) for path in changed):
        print("pre-push: only documentation changed, so the gate is left to CI (`uv run poe check` runs it now)")
        return 0
    tier = tier_for(remote_ref)
    if gate_passed_on(os.environ.get("PRE_COMMIT_TO_REF", ""), tier):
        print(f"pre-push: `{tier}` (or more) already passed on exactly the pushed tree, so it is not repeated")
        return 0
    uv = shutil.which("uv")
    if uv is None:
        print("pre-push: uv is not on the PATH, so the gate cannot run; install uv (https://docs.astral.sh/uv/)")
        return 1
    return subprocess.run([uv, "run", "poe", tier], check=False).returncode  # noqa: S603  fixed argv, no input


if __name__ == "__main__":
    match sys.argv[1:]:
        case ["start"]:
            record_gate_start()
        case ["pass", tier] if tier in TIERS:
            record_gate_pass(tier)
        case ["proofs"]:
            sys.exit(run_proofs())
        case []:
            sys.exit(main())
        case other:
            sys.exit(f"usage: pre_push.py [start | pass TIER | proofs] (the hook gives no argument), got {other}")
