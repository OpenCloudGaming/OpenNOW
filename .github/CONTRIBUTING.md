# Contributing to OpenNOW

Thanks for contributing.

## Project Layout

- Desktop application: `opennow-qt/` (Qt 6, C++20 and QML)
- Application services: `native/opennow-core/` (Rust)
- In-process streaming runtime: `native/opennow-streamer/` (Rust)
- Localization: `locales/` (edit only `en.json`; Crowdin owns translations)

## Local Setup

Install Qt 6.8+ with Quick, Multimedia and ShaderTools, CMake 3.24+, a C++20
toolchain, SDL3, Cargo and the platform media dependencies. See
[`opennow-qt/README.md`](../opennow-qt/README.md) for runtime and smoke-test guidance.

```bash
git clone https://github.com/OpenCloudGaming/OpenNOW.git
cd OpenNOW
cmake -S opennow-qt -B build/opennow-qt -DCMAKE_BUILD_TYPE=Debug
cmake --build build/opennow-qt
```

## Build and Checks

```bash
ctest --test-dir build/opennow-qt --output-on-failure
cargo test --manifest-path native/opennow-core/Cargo.toml
cargo test --manifest-path native/opennow-streamer/Cargo.toml --workspace
```

Node.js 22.22+ is needed only for repository localization validation, not to build or
run the desktop application. No npm dependency installation is required:

```bash
npm run locales:check
```

For local packaging after configuring and building the desired release configuration:

```bash
cmake --build build/opennow-qt --config Release --target package
```

CI builds the Qt/native stack in `qt-ci.yml`. Production-signed candidates use
`qt-release-candidate.yml`; see [`docs/qt-release-candidate.md`](../docs/qt-release-candidate.md).
The former Electron release workflows and Electron-only Nix package have been removed;
there is no replacement Nix package in this tree.

## Pull Requests

1. Create a feature branch
2. Keep commits focused and clear
3. Run the affected Qt/Rust tests and relevant formatting/lint checks; follow `AGENTS.md`
4. Open a PR with a concise summary

Attach one-off verification screenshots and recordings to the PR instead of committing
them. Keep images in the repository only when documentation or the application uses them.
