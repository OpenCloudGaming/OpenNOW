import type {
  PersistentStorageLocation,
  PersistentStorageLocationsResult,
  PersistentStorageResetResult,
} from "@shared/gfn";

const GFN_PAYWALL_API_BASE_URL = "https://api-prod.nvidia.com/gfn-paywall-api/api/v2";
const GFN_STATUS_COMPONENTS_URL = "https://status.geforcenow.com/api/v2/components.json";
const DEFAULT_PAYWALL_LOCALE = "en_US";
const FALLBACK_FETCH_TIMEOUT_MS = 10_000;

interface ResetPersistentStorageInput {
  idToken: string;
  idTokenAlternates?: string[];
  storageRegion?: string | null;
}

interface FetchPersistentStorageLocationsInput {
  idToken: string;
  idTokenAlternates?: string[];
  vpcId?: string | null;
  locale?: string;
  currentRegionCode?: string | null;
  currentRegionName?: string | null;
}

interface PaywallResponseWithMessage {
  message?: unknown;
  errors?: {
    errorMessage?: unknown;
  };
}

interface PaywallProductsResponse extends PaywallResponseWithMessage {
  status?: unknown;
  products?: PaywallProduct[];
}

interface PaywallProduct {
  id?: unknown;
  productId?: unknown;
  productType?: unknown;
  add_on?: PaywallProduct[];
  regions?: PaywallStorageRegion[];
}

interface PaywallStorageRegion {
  metroRegion?: unknown;
  metroRegionName?: unknown;
  isAvailable?: unknown;
  isRecommendedRegion?: unknown;
}

interface StatuspageComponentsResponse {
  components?: StatuspageComponent[];
}

interface StatuspageComponent {
  id?: unknown;
  name?: unknown;
  status?: unknown;
  position?: unknown;
  group_id?: unknown;
  group?: unknown;
}

interface StorageStatusLocation {
  name: string;
  codes: string[];
  isAvailable?: boolean;
}

interface PaywallFailure {
  status: number;
  message: string;
  payload: unknown;
}

const FALLBACK_STORAGE_STATUS_LOCATIONS: StorageStatusLocation[] = [
  { name: "Northern California (USA)", codes: ["NP-SJC6-04", "NP-SJC6-06"] },
  { name: "Southern California (USA)", codes: ["NP-LAX-03"] },
  { name: "Oregon (USA)", codes: ["NP-PDX-01"] },
  { name: "Arizona (USA)", codes: ["NP-PHX-02"] },
  { name: "Texas (USA)", codes: ["NP-DAL-04", "NP-DAL-05", "NP-DAL-06"] },
  { name: "Illinois (USA)", codes: ["NP-CHI-04", "NP-CHI-05"] },
  { name: "Florida (USA)", codes: ["NP-MIA-03", "NP-MIA-04"] },
  { name: "Georgia (USA)", codes: ["NP-ATL-03", "NP-ATL-04"] },
  { name: "Virginia (USA)", codes: ["NP-ASH-04"] },
  { name: "New Jersey (USA)", codes: ["NP-NWK-03", "NP-NWK-04"] },
  { name: "Quebec (Canada)", codes: ["NP-MON-02"] },
  { name: "United Kingdom 1", codes: ["NP-LON-07", "NP-LON-08"] },
  { name: "Sweden", codes: ["NP-STH-03", "NP-STH-04"] },
  { name: "Netherlands North", codes: ["NP-AMS-06", "NP-AMS-08"] },
  { name: "Germany", codes: ["NP-FRK-06", "NP-FRK-07", "NP-FRK-08"] },
  { name: "France 1", codes: ["NP-PAR-05", "NP-PAR-06", "NP-PAR-07"] },
  { name: "Poland", codes: ["NP-WAW-01"] },
  { name: "Bulgaria", codes: ["NP-SOF-02"] },
  { name: "India", codes: ["NP-BOM-01"] },
  { name: "Japan", codes: ["NP-TYO-01"] },
];

function normalizeStorageRegion(storageRegion: string | null | undefined): string | null {
  if (typeof storageRegion !== "string") {
    return null;
  }

  const trimmed = storageRegion.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function buildResetStorageUrl(storageRegion: string | null): string {
  const regionQueryValue = storageRegion ?? "null";
  return `${GFN_PAYWALL_API_BASE_URL}/reset/storage?storageRegion=${encodeURIComponent(regionQueryValue)}`;
}

function buildProductsUrl(input: Pick<FetchPersistentStorageLocationsInput, "locale" | "vpcId">): string {
  const url = new URL(`${GFN_PAYWALL_API_BASE_URL}/products`);
  url.searchParams.set("locale", input.locale?.trim() || DEFAULT_PAYWALL_LOCALE);
  if (input.vpcId?.trim()) {
    url.searchParams.set("vpcId", input.vpcId.trim());
  }
  return url.toString();
}

function buildPaywallHeaders(idToken: string): Record<string, string> {
  return {
    Accept: "application/json",
    "Content-Type": "application/json",
    idToken,
  };
}

function parseMessage(payload: unknown): string | undefined {
  if (!payload || typeof payload !== "object") {
    return undefined;
  }

  const response = payload as PaywallResponseWithMessage;
  const statuspageMessage = (payload as { error?: { message?: unknown } }).error?.message;
  const message = response.message ?? response.errors?.errorMessage ?? statuspageMessage;
  return typeof message === "string" && message.trim().length > 0 ? message : undefined;
}

async function readPaywallJson(response: Response): Promise<unknown> {
  const text = await response.text();
  if (!text.trim()) {
    return null;
  }

  try {
    return JSON.parse(text) as unknown;
  } catch {
    return text;
  }
}

function asString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined;
}

function normalizeRegionCode(value: string | null | undefined): string | undefined {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined;
}

function normalizeRegionName(value: string | null | undefined): string | undefined {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : undefined;
}

function normalizeStatusComponentName(value: string): string {
  return value
    .replace(/\s*\[[^\]]+\]\s*/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

function normalizeStatusCode(value: string): string | undefined {
  const normalized = normalizeStatusComponentName(value);
  return /^NPA?-[A-Z0-9]+(?:-[A-Z0-9]+)*$/.test(normalized) ? normalized : undefined;
}

function isCurrentLocationName(name: string, currentRegionName: string | undefined): boolean {
  return Boolean(currentRegionName && name.toLowerCase() === currentRegionName.toLowerCase());
}

function tokenCandidates(idToken: string, alternates: string[] | undefined): string[] {
  return [...new Set([idToken, ...(alternates ?? [])].map((token) => token.trim()).filter(Boolean))];
}

function isAuthFailure(failure: PaywallFailure): boolean {
  return (
    failure.status === 401 ||
    failure.status === 403 ||
    /idtoken|unauthori[sz]ed|forbidden|token/i.test(failure.message)
  );
}

function isRegionFailure(failure: PaywallFailure): boolean {
  return /region|storageRegion|products not found/i.test(failure.message);
}

function paywallFailureMessage(failure: PaywallFailure | null, fallback: string): string {
  if (!failure) {
    return fallback;
  }
  if (/starfleet idtoken was invalid/i.test(failure.message)) {
    return "NVIDIA rejected the saved session token for this storage API. Please log in again, then retry.";
  }
  return failure.message;
}

function isStorageProductCandidate(product: PaywallProduct): boolean {
  const productType = asString(product.productType)?.toUpperCase();
  return productType === "STORAGE" || productType === "PAID" || Array.isArray(product.regions);
}

function collectStorageProductCandidates(products: PaywallProduct[]): PaywallProduct[] {
  const candidates: PaywallProduct[] = [];

  for (const product of products) {
    if (isStorageProductCandidate(product)) {
      candidates.push(product);
    }

    if (Array.isArray(product.add_on)) {
      for (const addon of product.add_on) {
        if (isStorageProductCandidate(addon)) {
          candidates.push(addon);
        }
      }
    }
  }

  return candidates;
}

function chooseStorageRegions(products: PaywallProduct[]): PaywallStorageRegion[] {
  const candidates = collectStorageProductCandidates(products);
  const selected = candidates.find((product) => Array.isArray(product.regions) && product.regions.length > 0);
  return selected?.regions ?? [];
}

function toLocation(
  region: PaywallStorageRegion,
  currentRegionCode: string | undefined,
): PersistentStorageLocation | null {
  const code = asString(region.metroRegion);
  if (!code) {
    return null;
  }

  const name = asString(region.metroRegionName) ?? code;
  return {
    code,
    name,
    isAvailable: region.isAvailable !== false,
    isCurrent: currentRegionCode ? code === currentRegionCode : undefined,
    isRecommended: region.isRecommendedRegion === true,
  };
}

function dedupeLocations(locations: PersistentStorageLocation[]): PersistentStorageLocation[] {
  const seen = new Set<string>();
  const deduped: PersistentStorageLocation[] = [];

  for (const location of locations) {
    if (seen.has(location.code)) {
      continue;
    }
    seen.add(location.code);
    deduped.push(location);
  }

  return deduped;
}

function dedupeCodes(codes: string[]): string[] {
  const seen = new Set<string>();
  const deduped: string[] = [];

  for (const code of codes) {
    const normalized = normalizeRegionCode(code);
    if (!normalized || seen.has(normalized)) {
      continue;
    }
    seen.add(normalized);
    deduped.push(normalized);
  }

  return deduped;
}

function ensureCurrentLocation(
  locations: PersistentStorageLocation[],
  currentRegionCode: string | undefined,
  currentRegionName: string | undefined,
): PersistentStorageLocation[] {
  if (!currentRegionCode || locations.some((location) => location.code === currentRegionCode)) {
    return locations;
  }

  return [
    {
      code: currentRegionCode,
      name: currentRegionName ?? currentRegionCode,
      isAvailable: true,
      isCurrent: true,
    },
    ...locations,
  ];
}

function statusLocationsToPersistentLocations(
  statusLocations: StorageStatusLocation[],
  currentRegionCode: string | undefined,
  currentRegionName: string | undefined,
): PersistentStorageLocation[] {
  const locations = statusLocations
    .map<PersistentStorageLocation | null>((location) => {
      const codes = dedupeCodes(location.codes);
      if (codes.length === 0) {
        return null;
      }

      const currentByName = isCurrentLocationName(location.name, currentRegionName);
      const currentCode = currentByName && currentRegionCode ? currentRegionCode : undefined;
      const code = currentCode ?? codes[0];

      return {
        code,
        name: location.name,
        isAvailable: location.isAvailable !== false,
        isCurrent: currentRegionCode ? code === currentRegionCode || currentByName : currentByName || undefined,
      };
    })
    .filter((location): location is PersistentStorageLocation => Boolean(location));

  return ensureCurrentLocation(dedupeLocations(locations), currentRegionCode, currentRegionName);
}

function fallbackPersistentStorageLocations(
  currentRegionCode: string | undefined,
  currentRegionName: string | undefined,
): PersistentStorageLocation[] {
  return statusLocationsToPersistentLocations(
    FALLBACK_STORAGE_STATUS_LOCATIONS,
    currentRegionCode,
    currentRegionName,
  );
}

async function fetchWithTimeout(url: string, options: RequestInit = {}): Promise<Response> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), FALLBACK_FETCH_TIMEOUT_MS);
  try {
    return await fetch(url, {
      ...options,
      signal: controller.signal,
    });
  } finally {
    clearTimeout(timeout);
  }
}

function parseStatuspageStorageLocations(payload: unknown): StorageStatusLocation[] {
  const response = payload as StatuspageComponentsResponse;
  const components = Array.isArray(response.components) ? response.components : [];
  const groups = new Map<string, StatuspageComponent>();

  for (const component of components) {
    const id = asString(component.id);
    if (id && component.group === true) {
      groups.set(id, component);
    }
  }

  const storageLocations: StorageStatusLocation[] = [];
  const sortedGroups = [...groups.values()].sort((a, b) => {
    const left = typeof a.position === "number" ? a.position : Number.MAX_SAFE_INTEGER;
    const right = typeof b.position === "number" ? b.position : Number.MAX_SAFE_INTEGER;
    return left - right;
  });

  for (const group of sortedGroups) {
    const groupId = asString(group.id);
    const groupName = asString(group.name);
    if (!groupId || !groupName) {
      continue;
    }

    const children = components
      .filter((component) => asString(component.group_id) === groupId)
      .sort((a, b) => {
        const left = typeof a.position === "number" ? a.position : Number.MAX_SAFE_INTEGER;
        const right = typeof b.position === "number" ? b.position : Number.MAX_SAFE_INTEGER;
        return left - right;
      });
    const storageChild = children.find((child) => asString(child.name) === "Cloud Storage");
    if (!storageChild) {
      continue;
    }

    const codes = dedupeCodes(
      children
        .map((child) => asString(child.name))
        .map((name) => name ? normalizeStatusCode(name) : undefined)
        .filter((code): code is string => Boolean(code)),
    );
    if (codes.length === 0) {
      continue;
    }

    storageLocations.push({
      name: normalizeStatusComponentName(groupName),
      codes,
      isAvailable: storageChild.status !== "major_outage",
    });
  }

  return storageLocations;
}

async function fetchStatuspageStorageLocations(
  currentRegionCode: string | undefined,
  currentRegionName: string | undefined,
): Promise<PersistentStorageLocation[]> {
  try {
    const response = await fetchWithTimeout(GFN_STATUS_COMPONENTS_URL, {
      headers: { Accept: "application/json" },
    });
    if (!response.ok) {
      return fallbackPersistentStorageLocations(currentRegionCode, currentRegionName);
    }

    const payload = await readPaywallJson(response);
    const statusLocations = parseStatuspageStorageLocations(payload);
    if (statusLocations.length === 0) {
      return fallbackPersistentStorageLocations(currentRegionCode, currentRegionName);
    }

    return statusLocationsToPersistentLocations(statusLocations, currentRegionCode, currentRegionName);
  } catch {
    return fallbackPersistentStorageLocations(currentRegionCode, currentRegionName);
  }
}

function getFallbackRegionCandidates(storageRegion: string | null): Array<string | null> {
  if (!storageRegion) {
    return [null];
  }

  const normalizedRegion = normalizeRegionCode(storageRegion);
  if (!normalizedRegion) {
    return [storageRegion];
  }

  const location = FALLBACK_STORAGE_STATUS_LOCATIONS.find((candidate) =>
    candidate.codes.some((code) => code === normalizedRegion),
  );
  if (!location) {
    return [normalizedRegion];
  }

  return dedupeCodes([normalizedRegion, ...location.codes]);
}

export async function fetchPersistentStorageLocations(
  input: FetchPersistentStorageLocationsInput,
): Promise<PersistentStorageLocationsResult> {
  const currentRegionCode = normalizeRegionCode(input.currentRegionCode);
  const currentRegionName = normalizeRegionName(input.currentRegionName);
  let lastFailure: PaywallFailure | null = null;

  for (const idToken of tokenCandidates(input.idToken, input.idTokenAlternates)) {
    const response = await fetch(buildProductsUrl(input), {
      headers: buildPaywallHeaders(idToken),
    });

    const payload = await readPaywallJson(response);
    if (!response.ok) {
      lastFailure = {
        status: response.status,
        message: parseMessage(payload) ?? `Persistent storage locations failed with status ${response.status}`,
        payload,
      };
      if (isAuthFailure(lastFailure)) {
        continue;
      }
      break;
    }

    const productsResponse = payload as PaywallProductsResponse;
    if (productsResponse.status === "failure") {
      lastFailure = {
        status: response.status,
        message: parseMessage(productsResponse) ?? "Persistent storage locations request failed",
        payload,
      };
      if (isAuthFailure(lastFailure)) {
        continue;
      }
      break;
    }

    const products = Array.isArray(productsResponse.products) ? productsResponse.products : [];
    const locations = ensureCurrentLocation(
      dedupeLocations(
        chooseStorageRegions(products)
          .map((region) => toLocation(region, currentRegionCode))
          .filter((location): location is PersistentStorageLocation => Boolean(location)),
      ),
      currentRegionCode,
      currentRegionName,
    );

    if (locations.length > 0) {
      return {
        locations,
        currentRegionCode,
        currentRegionName,
      };
    }
  }

  if (lastFailure) {
    console.warn("Persistent storage locations fell back to status page:", lastFailure.message);
  }

  return {
    locations: await fetchStatuspageStorageLocations(currentRegionCode, currentRegionName),
    currentRegionCode,
    currentRegionName,
  };
}

export async function resetPersistentStorage(
  input: ResetPersistentStorageInput,
): Promise<PersistentStorageResetResult> {
  const storageRegion = normalizeStorageRegion(input.storageRegion);
  const storageRegions = getFallbackRegionCandidates(storageRegion);
  let lastFailure: PaywallFailure | null = null;

  for (const idToken of tokenCandidates(input.idToken, input.idTokenAlternates)) {
    for (const candidateRegion of storageRegions) {
      const response = await fetch(buildResetStorageUrl(candidateRegion), {
        method: "POST",
        headers: buildPaywallHeaders(idToken),
        body: null,
      });

      const payload = await readPaywallJson(response);
      if (response.ok) {
        return {
          ok: true,
          storageRegion: candidateRegion,
          message: parseMessage(payload),
        };
      }

      lastFailure = {
        status: response.status,
        message: parseMessage(payload) ?? `Persistent storage reset failed with status ${response.status}`,
        payload,
      };

      if (isAuthFailure(lastFailure)) {
        break;
      }
      if (!isRegionFailure(lastFailure)) {
        throw new Error(paywallFailureMessage(lastFailure, "Persistent storage reset failed"));
      }
    }
  }

  throw new Error(paywallFailureMessage(lastFailure, "Persistent storage reset failed"));
}
