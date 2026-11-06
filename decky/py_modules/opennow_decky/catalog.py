import time
from pathlib import Path

from .files import read_json, write_json_atomic
from .net import http_json, open_url, require_host

PUBLIC_GRAPHQL = "https://public.games.geforce.com/graphql"
SERVER_INFO = "https://prod.cloudmatchbeta.nvidiagrid.net/v2/serverInfo"
ARTWORK_HOSTS = ("img.nvidiagrid.net",)
DEFAULT_VPC = "NP-AMS-08"
PAGE_SIZE = 1000
MAX_PAGES = 10
CACHE_TTL_SECONDS = 24 * 60 * 60
MAX_ARTWORK_BYTES = 16 * 1024 * 1024
TRADEMARKS = str.maketrans("", "", "™®©")

CATALOG_QUERY = """query DeckyCatalog($vpcId: String!, $locale: String!, $first: Int!, $after: String!) {
  apps(vpcId: $vpcId, language: $locale, first: $first, after: $after) {
    pageInfo { hasNextPage endCursor }
    items {
      id title
      images { GAME_BOX_ART TV_BANNER HERO_IMAGE GAME_LOGO KEY_ART }
      variants { id appStore storeId }
    }
  }
}"""


class CatalogError(RuntimeError):
    pass


def server_vpc(fetch=http_json) -> str:
    try:
        server_id = fetch(SERVER_INFO)["requestStatus"]["serverId"]
    except (OSError, ValueError, KeyError, TypeError):
        return DEFAULT_VPC
    return server_id if isinstance(server_id, str) and server_id else DEFAULT_VPC


def compact_game(item: dict) -> dict | None:
    title = item.get("title")
    title = title.translate(TRADEMARKS).strip() if isinstance(title, str) else None
    variants = [
        {"id": str(variant["id"]), "store": str(variant.get("appStore") or "UNKNOWN"), "storeId": str(variant.get("storeId") or "")}
        for variant in item.get("variants") or []
        if str(variant.get("id", "")).isdigit()
    ]
    if not isinstance(title, str) or not title or not variants:
        return None
    images = item.get("images") or {}
    return {
        "id": str(item.get("id") or variants[0]["id"]),
        "title": title,
        "variants": variants,
        "art": {
            "grid": images.get("GAME_BOX_ART") or "",
            "wide": images.get("TV_BANNER") or "",
            "hero": images.get("HERO_IMAGE") or images.get("KEY_ART") or "",
            "logo": images.get("GAME_LOGO") or "",
        },
    }


def fetch_catalog(vpc: str, fetch=http_json) -> list[dict]:
    games: list[dict] = []
    cursor = ""
    for _ in range(MAX_PAGES):
        payload = fetch(PUBLIC_GRAPHQL, {
            "query": CATALOG_QUERY,
            "variables": {"vpcId": vpc, "locale": "en_US", "first": PAGE_SIZE, "after": cursor},
        })
        if payload.get("errors"):
            raise CatalogError(str(payload["errors"][0].get("message", "Catalog query failed")))
        apps = (payload.get("data") or {}).get("apps") or {}
        games.extend(game for game in map(compact_game, apps.get("items") or []) if game)
        page = apps.get("pageInfo") or {}
        cursor = page.get("endCursor") or ""
        if not page.get("hasNextPage") or not cursor:
            break
    return games


def normalize(value: str) -> str:
    kept = "".join(character.lower() if character.isalnum() else " " for character in value)
    return " ".join(kept.split())


class Catalog:
    def __init__(self, cache_path: Path, fetch=http_json, clock=time.time):
        self.cache_path = cache_path
        self.fetch = fetch
        self.clock = clock
        self.games: list[dict] = []
        self.fetched_at = 0.0
        self.by_steam_id: dict[str, tuple[dict, dict]] = {}
        self.by_title: dict[str, dict] = {}
        cached = read_json(cache_path)
        if isinstance(cached, dict) and isinstance(cached.get("games"), list):
            self._index(cached["games"], float(cached.get("fetchedAt") or 0))

    def _index(self, games: list[dict], fetched_at: float) -> None:
        self.games = games
        self.fetched_at = fetched_at
        self.by_steam_id = {
            variant["storeId"]: (game, variant)
            for game in games
            for variant in game["variants"]
            if variant["store"] == "STEAM" and variant["storeId"]
        }
        self.by_title = {}
        for game in games:
            self.by_title.setdefault(normalize(game["title"]), game)

    def ensure(self, force: bool = False) -> None:
        if self.games and not force and self.clock() - self.fetched_at < CACHE_TTL_SECONDS:
            return
        try:
            games = fetch_catalog(server_vpc(self.fetch), self.fetch)
        except (OSError, ValueError, KeyError, TypeError, CatalogError) as error:
            if self.games:
                return
            raise CatalogError(f"Couldn't reach the GeForce NOW catalog: {error}") from error
        if not games:
            if self.games:
                return
            raise CatalogError("The GeForce NOW catalog came back empty")
        now = self.clock()
        write_json_atomic(self.cache_path, {"fetchedAt": now, "games": games})
        self._index(games, now)

    def search(self, query: str, limit: int = 40) -> list[dict]:
        tokens = normalize(query).split()
        if not tokens:
            return []
        ranked = []
        for game in self.games:
            title = normalize(game["title"])
            if all(token in title for token in tokens):
                ranked.append((not title.startswith(tokens[0]), len(title), game))
        ranked.sort(key=lambda entry: (entry[0], entry[1]))
        return [game for _, _, game in ranked[:limit]]

    def steam_matches(self, steam_app_id: str) -> list[dict]:
        match = self.by_steam_id.get(steam_app_id)
        if not match:
            return []
        game, steam = match
        others = [variant for variant in game["variants"] if variant is not steam]
        return [{"game": game, "variant": variant} for variant in [steam, *others]]


    def title_matches(self, title: str) -> list[dict]:
        game = self.by_title.get(normalize(title.translate(TRADEMARKS)))
        if not game:
            return []
        return [{"game": game, "variant": variant} for variant in game["variants"]]


def fetch_artwork(url: str, timeout: float = 30) -> tuple[bytes, str]:
    require_host(url, ARTWORK_HOSTS)
    with open_url(url, timeout=timeout) as response:
        data = response.read(MAX_ARTWORK_BYTES + 1)
    if len(data) > MAX_ARTWORK_BYTES:
        raise CatalogError("Artwork is too large")
    return data, "png" if data.startswith(b"\x89PNG") else "jpg"
