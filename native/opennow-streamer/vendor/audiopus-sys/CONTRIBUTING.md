# Contributing

Everyone is welcome to get involved, may it be a pull request, suggestion, bug
report, or a textual improvement! : )

The language applied in this repository is British English.

## Contributions

Contributions to `audiopus_sys` should be first discussed up via an issue and then
implemented via pull request.
Issues display development-plans or required brainstorming, feel free to ask,
suggest, and discuss!
The `master`-branch contains the latest release.

## Comments & Documentation Style

- Comments are placed the lines before the related code line, not on the same
line.

- Write full sentences in British English.

- `unsafe` must always be reasoned and their soundness must be proven via a
comment.

- Use Rust intra-doc-links paths to refer Rust items in documentation:
`[name](crate::module::struct::method)`.

- If code ends up difficult, try to simplify it, if unavoidable, explain code
with comments. Prefer explicit variable naming instead of abbreviations.

## Commit Style

Write full sentences in British English.

Commits should describe the action being peformed.

Example:
- *Fix deadlock for events.*
- *Correct grammar in `command`-example.*

## Pull Request Checklist

- Make sure to open an issue prior working on a problem or ask on existing
issue be assigned.

- If a pull requests breaks the current API, use the `breaking-changes`-branch,
otherwise `stable-changes`.

- Commits shall be as small as possible, compile, and pass all tests.

- Make sure your code is formatted with `rustfmt` and free of lints,
run `cargo fmt` and `cargo clippy`.

- If you fixed a bug, add a test for that bug. Unit tests belong inside the
same file's `mod` named `tests`, integrational tests belong inside the
`tests`-folder.

- Last but not least, make sure your planned pull request merges cleanly,
if it does not, rebase your changes.

If you have any questions left, please reach out via the issue system : )
