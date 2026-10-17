# Method and reproduction

## Analysis boundary

REA 6.4.0 was installed from the `rea-agents` npm package. Its documented registration is a stdio MCP running `rea mcp`. The official registry entry `io.github.morluto/rea`, version 6.4.0, names that package and `https://github.com/morluto/rea` as its repository. Ghidra 12.1.4 and the installed 64-bit OpenJDK 21 were validated by REA's provider diagnostics.

The downloaded analyzer archive was `ghidra_12.1.4_PUBLIC_20260921.zip`, SHA-256 `ddac49f903da9d5bac833e5cc79395098b9c33cfd3279be5f31bd00387d2d4db`. Its computed digest matched the digest in the GitHub release metadata from `NationalSecurityAgency/ghidra`. That checks the distribution identity; it is not a separate security audit of Ghidra or REA.

The MCP initialized and advertised 170 tools. The connected server opened the supplied streamer slice and returned a Ghidra-backed procedure search. Subsequent subsystem analysis used independent local stdio MCP sessions so one worker could not change another worker's active target. No uploaded executable was run.

REA's `inspect_macho` utility requires macOS and was unavailable on this Linux host. Its Ghidra provider also rejected a universal Mach-O because it could not enforce the selected architecture. These failures were not treated as evidence that the target lacked a feature. The inventory tool instead extracted exact architecture slices, and Ghidra analyzed those thin images.

## Byte identity

`tools/research/gfn_inventory.py` parses little-endian 64-bit Mach-O headers and big-endian FAT/FAT64 containers. It records original-file SHA-256, architecture-slice SHA-256, container offsets, Mach-O UUIDs, sections, linked libraries, and symbol tables. It also saves printable ASCII runs with thin-file offsets, translated virtual addresses, and original-container offsets.

All 52 original-file hashes and all 56 slice hashes were checked against the supplied bytes. Every extracted slice was compared byte-for-byte with its original container range. The inventory parser's tests exercise thin files, FAT/FAT64 offsets, malformed ranges, invalid load-command sizes, and non-Mach-O input.

Printable runs are a discovery aid. They can come from executable bytes, symbol tables, unrelated dependencies, or unused code. The generated configuration catalog narrows its source to `libBifrost2.dylib` x86_64 `__cstring` and exact `x-nv-` field-name literals. It does not recover field values, defaults, or runtime usage.

## Repeat the inventory

Use the supplied archives, not a different release. Extract them into separate directories named `archive` and `macos` to reproduce the paths in [the manifest](artifacts.md). Exclude AppleDouble metadata from analysis. Do not launch extracted applications.

```sh
python3 tools/research/gfn_inventory.py /absolute/path/archive /absolute/path/macos \
    --output /absolute/path/gfn-inventory
python3 -m unittest discover -s tools/research -p 'test_gfn_inventory.py' -v
```

The output must be outside both inputs. The generated `inventory.json` contains the detailed mappings; `artifacts.md` and `configuration-fields.md` are the Markdown references. The `slices/` directory contains private analysis inputs and symbol/string exports. Do not commit or publicly upload that directory.

## Repeat the native MCP queries

Install the pinned analysis tools independently of the application's runtime:

```sh
npm install --global rea-agents@6.4.0
export GHIDRA_INSTALL_DIR=/absolute/path/ghidra_12.1.4_PUBLIC
export JAVA_HOME=/absolute/path/jdk-21
export REA_ANALYSIS_PROVIDER=ghidra
export REA_GHIDRA_STARTUP_TIMEOUT_MS=1200000
export GHIDRA_HEADLESS_MAXMEM=4G
```

`tools/research/rea_batch.py` is a Python-standard-library MCP client. It checks the target SHA-256 before opening the file, initializes stdio MCP, permits only the listed read-only query operations, saves each complete response, and closes the target. Existing output directories are rejected. A query error or deadline ends the run with an error rather than producing a success claim.

| Plan | Thin target | Required SHA-256 |
| --- | --- | --- |
| `queries/transport.json` | `archive/libBifrost2.dylib/x86_64` | `e145b333045dd66652ffcc6e75a2d301bff54c74064b1193e57afe9d498971e9` |
| `queries/video.json` | `archive/libGeronimo.dylib/x86_64` | `f564aa1910f1a366973ce3c0f0e6261122833bc41c530a669ec846a51d50d15a` |
| `queries/audio.json` | `archive/libGsAudioWebRTC.dylib/x86_64` | `53858ed4421f6299a998e1957fbf33a07c14dd16e72f860f63a9919b81cbe36e` |
| `queries/providers.json` | `macos/MacOS/GeForceNOW/x86_64` | `2c7fe76d605315df74fbbc112578a883a6ae69c3f6a002558170d53d390c8b4c` |

Select the exact thin target and its hash from the manifest. For Bifrost's transport queries:

```sh
python3 tools/research/rea_batch.py \
    --target /absolute/path/gfn-inventory/slices/archive/libBifrost2.dylib/x86_64 \
    --sha256 e145b333045dd66652ffcc6e75a2d301bff54c74064b1193e57afe9d498971e9 \
    --plan docs/gfn-macos-re/queries/transport.json \
    --output /absolute/path/new-transport-evidence
```

The default deadline is 1,500 seconds per MCP request. Ghidra import occurs on the first native query and can take minutes. Run large libraries one at a time on memory-constrained machines. Do not reduce the deadline and then interpret missing output as absence of a feature.

REA results include `evidence_id`, artifact digest, provider, analysis-profile digest, addresses, and limitations. An MCP response can carry a `structuredContent` object or JSON in its text content. Preserve the complete response. A transport-size error may mean that a complete observation remains retained in REA; inspect or export that retained record instead of repeating the analysis or silently dropping limitations.

The batch runner was exercised against the supplied streamer slice: it initialized MCP, completed a Ghidra procedure search, saved the result, and closed the binary successfully. The subsystem plans also contain queries that succeeded during the independent sessions. They were checked for supported read-only operations and machine-independent arguments; this is not a claim that every plan was separately replayed from a fresh import after writing the documents.

`tools/research/verify_gfn_research.py` checks local document links, footnote definitions, read-only plan shape, and whether each cited REA record exists in caller-supplied private evidence directories. It rejects leftover scratch paths. Citation retention is a provenance check, not a proof of semantic interpretation or runtime correctness.

## What the evidence does not establish

Ghidra's recovered types and object offsets are analysis artifacts, not original declarations. A direct call establishes a static relationship; unresolved virtual calls remain unresolved. A decompiled function with an incomplete body cannot establish the behavior outside that body. A function name or bundled third-party implementation does not prove that a live session selects it.

This study does not establish account credentials, service-side policy, allowed subscription profiles, wire values not traced to serializers, macOS runtime permissions, optical HDR output, actual latency, or successful playback under packet loss. Those questions require additional artifacts or controlled, authorized runtime observations.
