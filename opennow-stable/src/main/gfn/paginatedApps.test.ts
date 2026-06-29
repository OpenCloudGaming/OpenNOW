/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import { fetchAllAppsPages, type AppsPageResponse } from "./paginatedApps";

test("fetchAllAppsPages follows cursors until every page is collected", async () => {
  const cursors: string[] = [];
  const pages: Record<string, AppsPageResponse<number>> = {
    "": {
      data: {
        apps: {
          numberReturned: 40,
          numberSupported: 45,
          pageInfo: { hasNextPage: true, endCursor: "page-2", totalCount: 45 },
          items: Array.from({ length: 40 }, (_, index) => index),
        },
      },
    },
    "page-2": {
      data: {
        apps: {
          numberReturned: 5,
          numberSupported: 45,
          pageInfo: { hasNextPage: false, endCursor: "", totalCount: 45 },
          items: [40, 41, 42, 43, 44],
        },
      },
    },
  };

  const result = await fetchAllAppsPages((cursor) => {
    cursors.push(cursor);
    return Promise.resolve(pages[cursor]);
  }, { maxPages: 5 });

  assert.deepEqual(cursors, ["", "page-2"]);
  assert.equal(result.items.length, 45);
  assert.deepEqual(result.items.slice(-5), [40, 41, 42, 43, 44]);
  assert.equal(result.numberReturned, 45);
  assert.equal(result.numberSupported, 45);
  assert.equal(result.totalCount, 45);
  assert.equal(result.hasNextPage, false);
});

test("fetchAllAppsPages fails instead of silently truncating at the page cap", async () => {
  await assert.rejects(
    fetchAllAppsPages(
      async () => ({
        data: {
          apps: {
            numberReturned: 40,
            pageInfo: { hasNextPage: true, endCursor: "next", totalCount: 80 },
            items: Array.from({ length: 40 }, (_, index) => index),
          },
        },
      }),
      { maxPages: 1 },
    ),
    /exceeded 1 pages/,
  );
});
