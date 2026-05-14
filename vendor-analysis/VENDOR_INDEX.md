# Vendor Analysis Index

```json
{
  "source_directory": "/Users/jayian/Downloads/vendor",
  "output_directory": "/Volumes/Projects/OpenNOW/vendor-analysis",
  "detected_app": "GeForce NOW gfn_mall bundled web application",
  "detected_frameworks": [
    "Webpack",
    "Angular",
    "Angular Material",
    "RxJS",
    "Zone.js"
  ],
  "source_file_count": 13,
  "source_total_bytes": 11364723,
  "documents": {
    "VENDOR_REPORT.md": "Primary AI-parsable report: inventory, hashes, manifest, domains, endpoints, webpack/runtime/module summaries.",
    "VENDOR_FEATURES.md": "Concise feature-oriented keyword hits and matching short strings grouped by semantic domain.",
    "VENDOR_CSS.md": "CSS selectors, media queries, keyframes, and custom properties.",
    "VENDOR_STRINGS_*.md": "Exhaustive deduplicated string literal catalogs split by source file; may include long minified fragments.",
    "vendor_extraction.json": "Full structured JSON extraction backing all Markdown documents."
  }
}
```

## Source Files
| file | bytes | chunk_ids | modules | sha256 |
| --- | ---: | --- | ---: | --- |
| 598.df12ac4ff70f2238.js | 58340 | 598 | 2 | `64910993aaeb1f29bc13a35e440faf2fdd579ab2b00e4306470eef4f5071ba8a` |
| 626.e6f5b98204eade60.js | 810744 | 626 | 26 | `5f054941a5eff503daa0fe1920543b6b59ac5189061ba669255d5450ead454f4` |
| 667.8b84dcfc99bf931e.js | 527307 | 667 | 1 | `7a86c83c52054227e241dc49e01ef6f9c673cff89e9a0f1c5ba6980896ab8212` |
| 689.981ba519cbc17084.js | 340040 | 689 | 1 | `27a6ef075a9d416b2cbc802ceae1a741b69707987d6e1dd3c64365b707a1b7fc` |
| 77.1882f9ae7a439de5.js | 11980 | 77 | 3 | `5b6192e2e4ee2baa98014a284fed6c80f189138c612cca5263dcba92d99df854` |
| 862.5162a13553ab0766.js | 20428 | 862 | 3 | `493f720665736db23765cfb70a1754662d286e554de905b210ec031417171408` |
| 923.4d491d740bdd7d37.js | 2473900 | 923 | 116 | `e0bfd1728dfe0be6e2f34a2d6abe38e61afaf4155a100259733b1deef929e8ff` |
| main.81a2d6165286131d.js | 3753637 | 792 | 487 | `4246982c4c436dbbeef7d02e08a0fbcb836ce69e5ce91270d98657284e305967` |
| manifest.webmanifest | 2658 |  | 0 | `3873e6aa71c7078df7b2c8afd0d135a8bf4f54693242288a23057b4ea1291faf` |
| polyfills.0119af3bec0a10f7.js | 65628 | 461 | 18 | `19ebd667b5f7d20412a07929ced0690b3951851a22bf18440a2f3322acecb5a9` |
| runtime.de302b1b971bfb57.js | 5955 |  | 0 | `be82356b4064287a34ce7c8bc1d6f9481c6c86effd963fcec07b16123ea85c65` |
| styles.b163082243582a97.css | 338751 |  | 0 | `59670fc08c65a377f8b974a073556c578013e6b62a27ca4f28ca2ca0d88be3e2` |
| vendor.0087fab5da9f1091.js | 2955355 | 502 | 412 | `587a3c7f48d5a47731f6be7707982f09db572bb92291d3b57d4973589056e83d` |

## Detected Domains
```json
{
  "api.gdn.nvidia.com": [
    "main.81a2d6165286131d.js"
  ],
  "bit.ly": [
    "vendor.0087fab5da9f1091.js"
  ],
  "events.telemetry.data-uat.nvidia.com": [
    "vendor.0087fab5da9f1091.js"
  ],
  "events.telemetry.data.nvidia.com": [
    "main.81a2d6165286131d.js",
    "vendor.0087fab5da9f1091.js"
  ],
  "feedbacks.telemetry.data-uat.nvidia.com": [
    "vendor.0087fab5da9f1091.js"
  ],
  "feedbacks.telemetry.data.nvidia.com": [
    "vendor.0087fab5da9f1091.js"
  ],
  "g.co": [
    "vendor.0087fab5da9f1091.js"
  ],
  "github.com": [
    "vendor.0087fab5da9f1091.js"
  ],
  "localhost:4318": [
    "vendor.0087fab5da9f1091.js"
  ],
  "localhost:9411": [
    "vendor.0087fab5da9f1091.js"
  ],
  "mock-box-art-url": [
    "main.81a2d6165286131d.js"
  ],
  "mock-key-art-url": [
    "main.81a2d6165286131d.js"
  ],
  "npms.io": [
    "vendor.0087fab5da9f1091.js"
  ],
  "nvfile": [
    "main.81a2d6165286131d.js"
  ],
  "nvidia.custhelp.com": [
    "923.4d491d740bdd7d37.js"
  ],
  "prod.cloudmatchbeta.nvidiagrid.net": [
    "main.81a2d6165286131d.js"
  ],
  "prod.otel.kaizen.nvidia.com": [
    "vendor.0087fab5da9f1091.js"
  ],
  "samsung.com": [
    "main.81a2d6165286131d.js"
  ],
  "steamcommunity.com": [
    "main.81a2d6165286131d.js"
  ],
  "telemetry.gfe.nvidia.com": [
    "main.81a2d6165286131d.js"
  ],
  "tizen.org": [
    "main.81a2d6165286131d.js"
  ],
  "wide-art-url": [
    "main.81a2d6165286131d.js"
  ],
  "www.nvidia.com": [
    "923.4d491d740bdd7d37.js",
    "main.81a2d6165286131d.js"
  ],
  "www.w3.org": [
    "626.e6f5b98204eade60.js",
    "main.81a2d6165286131d.js",
    "vendor.0087fab5da9f1091.js"
  ]
}
```
