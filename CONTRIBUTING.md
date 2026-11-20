# Contributing

## Commits

The history is the technical documentation of the project. Every commit
follows the Odoo convention:

```
[TAG] crate: short imperative summary in English

Body wrapped at 72 characters explaining WHY the change is needed:
the problem, the machine behaviour, the choices made. The diff already
says what changed.
```

Tags:

| Tag      | Use                                                      |
|----------|----------------------------------------------------------|
| `[ADD]`  | new crate                                                |
| `[IMP]`  | new feature or improvement inside an existing crate      |
| `[FIX]`  | bug fix                                                  |
| `[REF]`  | refactoring without behaviour change                     |
| `[REM]`  | removal of code or feature                               |
| `[REV]`  | revert                                                   |
| `[MOV]`  | files moved                                              |
| `[I18N]` | translations                                             |
| `[PERF]` | performance                                              |
| `[REL]`  | release (version bump, changelog)                        |

Several crates: factor the common prefix like the shell does,
`[IMP] mml_{laser,cnc}: ...`; a change touching the whole tree uses
`various`; repository files (workspace, docs, CI) use `repo`.

Rules:

- Summary at most 72 characters, imperative, no final dot.
- One commit = one coherent subject that builds and passes
  `cargo test --workspace` and `cargo clippy --workspace`.
- Code comments only for non-obvious traps; the why goes in the commit.
- User-facing strings go through the translation files of `mml_app`.

## Branches

`main` is always releasable. Work on `main-<subject>-<initials>` branches
created with `--no-track`, merged by pull request (rebase and merge).
