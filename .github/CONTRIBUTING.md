# Contributing to GFNClient

Thank you for your interest in contributing to GFNClient! This document provides guidelines and instructions for contributing.

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) (latest)
- Platform-specific dependencies for Tauri

### Development Setup

```bash
# Clone the repository
git clone https://github.com/zortos293/GFNClient.git
cd GFNClient

# Install dependencies
bun install

# Run in development mode
bun tauri dev
```

## How to Contribute

### Reporting Bugs

1. Check if the bug has already been reported in [Issues](https://github.com/zortos293/GFNClient/issues)
2. If not, create a new issue using the **Bug Report** template
3. Provide as much detail as possible, including:
   - Steps to reproduce
   - Expected vs actual behavior
   - Platform and version information
   - Logs and screenshots

### Suggesting Features

1. Check if the feature has already been requested in [Issues](https://github.com/zortos293/GFNClient/issues)
2. Create a new issue using the **Feature Request** template
3. Clearly describe the use case and proposed solution

### Submitting Pull Requests

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/your-feature-name`
3. Make your changes
4. Test your changes locally on your platform
5. Commit with clear, descriptive messages
6. Push to your fork
7. Open a Pull Request using the PR template

## Code Guidelines

### Rust (Backend)

- Follow standard Rust conventions
- Use `cargo fmt` before committing
- Run `cargo clippy` and address warnings
- Add appropriate error handling

### TypeScript (Frontend)

- Use TypeScript strict mode
- Follow existing code patterns
- Use meaningful variable and function names

### Commit Messages

Use clear, descriptive commit messages:

```
type: short description

Longer description if needed

Fixes #123
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`

## Platform-Specific Notes

### macOS
- H.265/HEVC and Opus Stereo features are macOS-only due to VideoToolbox/CoreAudio
- Fullscreen uses Tauri's window API instead of browser API

### Windows/Linux
- H.265 codec option is disabled (WebRTC limitation)
- Test with appropriate GPU drivers

## Questions?

- Open a **Question** issue
- Check existing issues and discussions

Thank you for contributing!
