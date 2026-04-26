---
description: 
alwaysApply: true
---

# AGENTS.md

## Graphify-first policy
For any codebase question, architecture question, or "where/how/why" query:
1. Call Graphify MCP first (`graph_stats`, `query_graph`, `get_node`, `get_neighbors`, `shortest_path`) to gather graph context.
2. Base the answer on Graphify results plus file verification when needed.
3. If Graphify is unavailable, state that explicitly and continue with normal code search.

## Core Priorities

1. Performance first.
2. Reliability first.
3. Keep behavior predictable under load and during failures (session restarts, reconnects, partial streams).

If a tradeoff is required, choose correctness and robustness over short-term convenience.

## Maintainability

Long term maintainability is a core priority. If you add new functionality, first check if there is shared logic that can be extracted to a separate module. Duplicate logic across multiple files is a code smell and should be avoided. Don't be afraid to change existing code. Don't take shortcuts by just adding local logic to solve a problem.
