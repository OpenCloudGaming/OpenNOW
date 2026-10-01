# Library service fixture

Run from the repository root with a Swift 5-compatible compiler and Python 3:

```sh
python3 ios/OpenNOWiOS/OpenNOWiOSTests/run_library_service_tests.py
```

Set `SWIFTC` if the compiler is not on `PATH`. To compare an older source revision, export `OpenNOWStore.swift` with `git show` and pass its path with `--source`.

The runner extracts the actual `fetchLibraryGames`, page request, panel adapter, game parser, metadata enrichment and registry-retry loops and parsers, and catalog types from `OpenNOWStore.swift`. It compiles them together with the fixture using the app's Swift 5 language mode. The in-process HTTP boundary records requests and returns JSON responses; it checks the endpoint, account token, VPC and account-owned library filter. Metadata HTTP requests, logging, the cancellation classifier, account models and platform networking are stubbed. No credentials or network access are needed.

The 96-game fixture spans three pages, including an overlapping app. Assertions compare exact ordered app IDs and selected launch variants. Other cases cover empty pages, terminal cursors, malformed page metadata and apps, numeric values masquerading as JSON Booleans, GraphQL partial errors, HTTP and transport failures, cursor cycles, oversized responses and page counts, and cancellation before a request, during page loading, during optional enrichment and during registry subchunk fallback. The real enrichment and retry loops must stop after cancellation and retain the complete library when optional metadata fails normally. An incomplete traversal must throw rather than return a partial snapshot to the store.

The production traversal permits at most 100 pages of 100 apps, 4,096-byte continuation cursors and 8 MiB per page. These are local resource bounds, not server protocol guarantees. Reaching a limit fails the refresh; it does not publish a truncated library. The existing store stages its publication after a successful fetch and current-account check, and the new cancellation check prevents a cancelled fetch from publishing.

This runner does not build the iOS target, execute SwiftUI or persistent account state, validate real GraphQL responses, or prove the reporter's installed build. Xcode tests and a live account comparison remain necessary for target acceptance.
