# Shared files

These files come from the template and are meant to be the same in every project made from it. `copier update` brings a
change to them (the answers are in `.copier-answers.yml`); a change to one is made in the template, not here. The tables say
what may differ.

## Identical (apart from the project's name)

| File | Why it is shared |
|---|---|
| `tools/check_rust_test_layout.py`, `tests/tools/test_check_rust_test_layout.py` | the one-binary test layout gate; per-project data is only `[tool.rust-tests.own-binaries]` |
| `tools/run_capped.py`, `tests/tools/test_run_capped.py` | the memory cap every task that runs tests goes through; the cap is `[tool.run-capped] cap-mb` |
| `tools/rust_tools.py`, `tests/tools/test_rust_tools.py` | installs and checks the cargo tools pinned in `[workspace.metadata.tools]` |
| `tools/check_commit_message.py`, `tests/tools/test_check_commit_message.py` | Conventional Commits check; limits in `[tool.commit-message]` |
| `tools/check_one_config_home.py`, `tests/tools/test_one_config_home.py` | one home for tool settings |
| `tools/check_dists.py`, `tests/tools/test_check_dists.py` | the build gate's distribution checks |
| `tools/check_release.py`, `tools/release.py` and their tests | the release command and the tag check |
| `rustfmt.toml`, `.editorconfig`, `.python-version`, `.gitmessage` | formatting and commit template |
| `Cargo.toml` `[workspace.lints]` and `[workspace.metadata.tools]` blocks, `[workspace.package]` keys | the Rust lint set and the tool pins |
| `pyproject.toml` poe tasks `rust-fmt`, `rust-lint`, `rust-doc`, `rust-tools`, `rust-shear`, `rust-deny`, `rust-test`, `rust-test-layout`, `gates`, `fast`, `proofs`, `check`, `full`, `gate-tests`, `setup`, `fix`; tables `[tool.ruff]`, `[tool.mypy]`, `[tool.pytest.ini_options]`, `[tool.run-capped]`, `[tool.rust-tests.own-binaries]`, `[tool.commit-message]` | the gate |
| `.github/workflows/{ci,lint,test,security,pre-commit,min-deps,release}.yml`, `.github/dependabot.yml` | same action SHAs everywhere; lint and test list the project's own gates only if it has extra ones |
| `tests/tools/` gate proofs: `test_rust_gates.py` rows for the shared gates, `test_workflow_gate.py`, `test_lint_and_type_gate.py`, `test_security_gate.py`, `test_warning_gate.py`, `test_dead_code_gate.py`, `test_dependency_gate.py`, `test_project_wiring.py`, `test_pre_push.py` | every gate has a known-bad input |

## Different on purpose

| File | What differs |
|---|---|
| `deny.toml` | licence allow-list and the wrappers of the pyo3/numpy bans follow the project's dependencies |
| `Cargo.toml` `[workspace.dependencies]`, `members` | the project's crates and crates.io dependencies |
| `pyproject.toml` `[project]`, `[tool.maturin]`, `[tool.importlinter]`, `[tool.vulture]`, `[tool.deptry]` | the package, its layers and exemptions |
| `rust-wasm` task | a project that builds more than the core for the browser extends it |
| `tools/check_installed.py` | a project may check more of the installed build; the template only names the artifact's file |
| `build.yml` smoke step | runs the project's own command on the installed build |
| Domain gates and scaffolds (a `tools/` script and its `poe` task) | each project adds the checks and generators of its own kind of thing |
| `pyproject.toml` `[tool.project-wiring] system-commands` | the system tools those gates run that are not dev dependencies (`node`, say); absent when there are none |
| `tools/pre_push.py` env var | `BT_CONTROL_GATE_PASSED` is named after the project (`poe release` sets it) |
