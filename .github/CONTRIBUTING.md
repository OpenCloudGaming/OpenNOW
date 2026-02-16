# Contributing to OpenNOW

Thanks for contributing.

## Project Layout

- Active desktop client: `opennow-stable/` (Electron + React + TypeScript)

## Local Setup

```bash
git clone https://github.com/OpenCloudGaming/OpenNOW.git
cd OpenNOW/opennow-stable
npm install
npm run dev
```

## Build and Checks

```bash
npm run typecheck
npm run build
npm run dist
```

## Pull Requests

1. Create a feature branch
2. Keep commits focused and clear
3. Ensure `typecheck` and `build` pass locally
4. Open a PR with a concise summary
