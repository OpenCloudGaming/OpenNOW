# README design images

These images are exports of editable artboards in the **OpenNOW Socket** Paper file,
not screenshots of the running application. The three source artboards were updated
to follow the current Qt desktop and console layouts, retaining illustrative catalog data.

| Asset | Paper page | Artboard | Node |
| --- | --- | --- | --- |
| `desktop-home.webp` | [Desktop (1440)](https://app.paper.design/file/01M11SPTRPMYQB9S9AX948A6WH/4-0) | Desktop · 01 Home · Current Qt | `8N5-0` |
| `desktop-library.webp` | [Desktop (1440)](https://app.paper.design/file/01M11SPTRPMYQB9S9AX948A6WH/4-0) | Desktop · 02 Library · Current Qt | `81Q-0` |
| `console-home.webp` | [Console (1920)](https://app.paper.design/file/01M11SPTRPMYQB9S9AX948A6WH/3-0) | V3 · 01 Home · Current Qt | `1GJ-0` |

Layout reference: Qt nightly `1.0.0-nightly.225.1`, built from `dev` commit
[`502cff45`](https://github.com/OpenCloudGaming/OpenNOW/commit/502cff452f5b94d9fc89a78e6b4026a6b317e590)
in [CI run 34203221079](https://github.com/OpenCloudGaming/OpenNOW/actions/runs/34203221079).
The packaged Qt application was rendered with an isolated mock core and sample
catalog, without signing in or starting gameplay. Layout and control details were
also checked against the desktop Home, Library, Sidebar, console Home, GameTile,
and NavPill QML components. Other Paper artboards were not part of this refresh.

To refresh, export the listed artboards as PNG at 1×, then encode as WebP at quality
90 with Pillow's `method=6`. Preserve the original dimensions (1440 × 900 for
desktop; 1920 × 1080 for console). Do not add account data or measured-performance
claims to these design previews. Game artwork belongs to its respective owners.
