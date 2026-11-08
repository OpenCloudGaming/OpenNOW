import tempfile
import unittest
from pathlib import Path

from tests import support  # noqa: F401
from opennow_decky import catalog
from opennow_decky.net import HostError


def item(title, variant_id, store="STEAM", store_id="", **images):
    return {"id": f"uuid-{variant_id}", "title": title, "images": images,
            "variants": [{"id": variant_id, "appStore": store, "storeId": store_id}]}


class FakeApi:
    def __init__(self, pages, vpc="NP-NWK-04", fail=False):
        self.pages = pages
        self.vpc = vpc
        self.fail = fail
        self.requests = []

    def __call__(self, url, body=None):
        if self.fail:
            raise OSError("offline")
        if url == catalog.SERVER_INFO:
            return {"requestStatus": {"serverId": self.vpc}}
        self.requests.append(body["variables"])
        index = len(self.requests) - 1
        has_next = index + 1 < len(self.pages)
        return {"data": {"apps": {"items": self.pages[index],
                                  "pageInfo": {"hasNextPage": has_next, "endCursor": f"c{index + 1}" if has_next else ""}}}}


class CatalogTests(unittest.TestCase):
    def setUp(self):
        self.cache = Path(tempfile.mkdtemp()) / "catalog.json"
        self.now = 1000.0

    def make(self, api):
        return catalog.Catalog(self.cache, fetch=api, clock=lambda: self.now)

    def test_pages_through_the_catalog_in_the_users_vpc_and_indexes_steam_ids(self):
        api = FakeApi([[item("Cyberpunk 2077", "100", store_id="1091500", GAME_BOX_ART="https://img.nvidiagrid.net/a.jpg")],
                       [item("Fortnite", "200", store="EPIC")]])
        games = self.make(api)
        games.ensure()
        self.assertEqual([request["vpcId"] for request in api.requests], ["NP-NWK-04", "NP-NWK-04"])
        self.assertEqual([request["after"] for request in api.requests], ["", "c1"])
        self.assertEqual(len(games.games), 2)
        [match] = games.steam_matches("1091500")
        self.assertEqual((match["game"]["title"], match["variant"]["id"]), ("Cyberpunk 2077", "100"))
        self.assertEqual(match["game"]["art"]["grid"], "https://img.nvidiagrid.net/a.jpg")
        self.assertEqual(games.steam_matches("1"), [])

    def test_a_steam_page_offers_every_store_version_with_steam_first(self):
        game = {"id": "g", "title": "Cyberpunk 2077", "images": {}, "variants": [
            {"id": "1", "appStore": "GOG", "storeId": ""},
            {"id": "2", "appStore": "STEAM", "storeId": "1091500"},
            {"id": "3", "appStore": "EPIC", "storeId": ""}]}
        games = self.make(FakeApi([[game]]))
        games.ensure()
        self.assertEqual([m["variant"]["store"] for m in games.steam_matches("1091500")], ["STEAM", "GOG", "EPIC"])

    def test_shortcut_titles_match_every_store_version_of_the_game(self):
        game = {"id": "g", "title": "Assassin's Creed Mirage", "images": {},
                "variants": [{"id": "1", "appStore": "UPLAY", "storeId": ""}, {"id": "2", "appStore": "EPIC", "storeId": ""}]}
        games = self.make(FakeApi([[game, item("Fortnite", "3", store="EPIC")]]))
        games.ensure()
        self.assertEqual(len(games.title_matches("Assassin’s Creed® Mirage")), 2)
        matches = games.title_matches("ASSASSIN'S CREED® MIRAGE")
        self.assertEqual([m["variant"]["store"] for m in matches], ["UPLAY", "EPIC"])
        self.assertEqual(games.title_matches("Fortnite: Battle Royale"), [])

    def test_cache_is_reused_within_a_day_and_survives_restarts(self):
        api = FakeApi([[item("Rage", "1")]])
        self.make(api).ensure()
        reopened = self.make(FakeApi([], fail=True))
        reopened.ensure()
        self.assertEqual([game["title"] for game in reopened.games], ["Rage"])

    def test_stale_cache_is_kept_when_the_network_fails(self):
        self.make(FakeApi([[item("Rage", "1")]])).ensure()
        self.now += catalog.CACHE_TTL_SECONDS + 1
        stale = self.make(FakeApi([], fail=True))
        stale.ensure()
        self.assertEqual(len(stale.games), 1)

    def test_no_cache_and_no_network_is_an_error(self):
        with self.assertRaises(catalog.CatalogError):
            self.make(FakeApi([], fail=True)).ensure()

    def test_unknown_vpc_falls_back_to_the_default(self):
        api = FakeApi([[item("Rage", "1")]], vpc=None)
        self.make(api).ensure()
        self.assertEqual(api.requests[0]["vpcId"], catalog.DEFAULT_VPC)

    def test_search_matches_every_word_and_prefers_title_prefixes(self):
        games = self.make(FakeApi([[item("Assassin's Creed Valhalla", "1"), item("Valheim", "2"),
                                    item("Assassin's Creed Mirage", "3"), item("The Witcher 3", "4")]]))
        games.ensure()
        self.assertEqual([game["title"] for game in games.search("val")], ["Valheim", "Assassin's Creed Valhalla"])
        self.assertEqual([game["title"] for game in games.search("assassins mirage")], [])
        self.assertEqual([game["title"] for game in games.search("assassin s mirage")], ["Assassin's Creed Mirage"])
        self.assertEqual(games.search("   "), [])

    def test_trademark_symbols_are_dropped_from_titles(self):
        self.assertEqual(catalog.compact_game(item("Cyberpunk 2077®", "1"))["title"], "Cyberpunk 2077")
        self.assertEqual(catalog.compact_game(item("Assassin’s Creed™ Mirage", "1"))["title"], "Assassin’s Creed Mirage")

    def test_items_without_a_numeric_variant_are_dropped(self):
        self.assertIsNone(catalog.compact_game({"title": "Broken", "variants": [{"id": "abc"}]}))
        self.assertIsNone(catalog.compact_game({"title": "", "variants": [{"id": "1"}]}))

    def test_artwork_only_comes_from_the_gfn_image_host(self):
        for url in ["http://img.nvidiagrid.net/a.jpg", "https://example.com/a.jpg", "file:///etc/passwd"]:
            with self.subTest(url=url):
                with self.assertRaises(HostError):
                    catalog.fetch_artwork(url)


if __name__ == "__main__":
    unittest.main()
