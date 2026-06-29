export interface AppsPageResponse<T> {
  data?: {
    apps?: {
      numberReturned?: number;
      numberSupported?: number;
      pageInfo?: {
        hasNextPage?: boolean;
        endCursor?: string;
        totalCount?: number;
      };
      items?: T[];
    };
  };
  errors?: Array<{ message: string }>;
}

export interface PaginatedAppsResult<T> {
  items: T[];
  numberReturned: number;
  numberSupported: number;
  totalCount: number;
  hasNextPage: boolean;
  endCursor?: string;
}

export async function fetchAllAppsPages<T>(
  fetchPage: (cursor: string) => Promise<AppsPageResponse<T>>,
  options: { maxPages: number },
): Promise<PaginatedAppsResult<T>> {
  const items: T[] = [];
  let cursor = "";
  let numberReturned = 0;
  let numberSupported = 0;
  let totalCount = 0;
  let hasNextPage = false;
  let endCursor = "";

  for (let page = 0; page < options.maxPages; page += 1) {
    const payload = await fetchPage(cursor);
    if (payload.errors?.length) {
      throw new Error(payload.errors.map((error) => error.message).join(", "));
    }

    const apps = payload.data?.apps;
    const pageItems = apps?.items ?? [];
    items.push(...pageItems);
    numberReturned += apps?.numberReturned ?? pageItems.length;
    numberSupported = apps?.numberSupported ?? numberSupported;
    hasNextPage = apps?.pageInfo?.hasNextPage ?? false;
    endCursor = apps?.pageInfo?.endCursor ?? "";
    totalCount = apps?.pageInfo?.totalCount ?? totalCount;

    if (!hasNextPage || !endCursor) {
      return {
        items,
        numberReturned,
        numberSupported,
        totalCount,
        hasNextPage: false,
        endCursor: endCursor || undefined,
      };
    }

    cursor = endCursor;
  }

  throw new Error(`GFN apps pagination exceeded ${options.maxPages} pages`);
}
