# OpenNOW

Electron client source now lives in `opennow-stable/`.

## Development

```bash
cd opennow-stable
npm install
npm run dev
```

## Build

```bash
cd opennow-stable
npm run build
npm run dist
```

## Notes

- Legacy Tauri/Rust build files have been removed.
- CI builds/releases are handled by:
  - `.github/workflows/opennow-stable-build.yml`
  - `.github/workflows/opennow-stable-release.yml`
