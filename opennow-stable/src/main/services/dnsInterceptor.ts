import dns from "node:dns";

const originalLookup = dns.lookup;

const fallbackResolver = new dns.Resolver();
fallbackResolver.setServers(["1.1.1.1", "8.8.8.8"]);

function isNvidiaHostname(hostname: string): boolean {
  if (!hostname) return false;
  const lower = hostname.toLowerCase();
  if (lower.startsWith("unit.") || lower.startsWith("np-test.") || lower.endsWith(".test") || lower.includes("example")) return false;
  return lower.endsWith(".nvidiagrid.net") || lower.endsWith(".nvidia.com");
}

export function initDnsInterceptor(): void {
  const isTestEnvironment =
    process.env.NODE_ENV === "test" ||
    Boolean(process.env.NODE_TEST_CONTEXT) ||
    process.argv.some((a) => a.includes("test")) ||
    process.execArgv.some((a) => a.includes("test"));

  if (isTestEnvironment) {
    return;
  }
  // @ts-ignore
  dns.lookup = function (
    hostname: string,
    options: any,
    callback: (err: NodeJS.ErrnoException | null, address: string | any[], family?: number) => void
  ) {
    let actualOptions = options;
    let actualCallback = callback;

    if (typeof options === "function") {
      actualCallback = options;
      actualOptions = {};
    } else if (typeof options === "number") {
      actualOptions = { family: options };
    } else {
      actualOptions = options ?? {};
    }

    if (isNvidiaHostname(hostname)) {
      const family = actualOptions.family;
      if (family === 6) {
        // Fall back to original for IPv6 lookup
        originalLookup(hostname, actualOptions, actualCallback);
        return;
      }

      // Try original lookup first to respect local network configurations/hosts file
      originalLookup(hostname, actualOptions, (err, address, resolvedFamily) => {
        if (err && (err.code === "ENOTFOUND" || err.code === "EAI_AGAIN")) {
          // If native lookup failed, attempt resolving via Cloudflare/Google public DNS
          fallbackResolver.resolve4(hostname, (fallbackErr, addresses) => {
            if (!fallbackErr && addresses && addresses.length > 0) {
              const ip = addresses[0];
              console.log(`[DNS Interceptor] Fallback resolved ${hostname} -> ${ip}`);
              if (actualOptions.all) {
                actualCallback(null, [{ address: ip, family: 4 }], 4);
              } else {
                actualCallback(null, ip, 4);
              }
            } else {
              // Return original error if fallback also failed
              actualCallback(err, address, resolvedFamily);
            }
          });
        } else {
          // Normal success or other errors
          actualCallback(err, address, resolvedFamily);
        }
      });
      return;
    }

    originalLookup(hostname, actualOptions, actualCallback);
  };

  // Preserve util.promisify(dns.lookup) compatibility — copy the custom promisify
  // symbol from the original so callers get the correct { address, family } shape.
  const promisifyCustom = Symbol.for("nodejs.util.promisify.custom");
  if (promisifyCustom in originalLookup) {
    // @ts-ignore
    dns.lookup[promisifyCustom] = originalLookup[promisifyCustom as keyof typeof originalLookup];
  }

  console.log("[DNS Interceptor] Interceptor initialized successfully with safe fallback resolver.");
}
