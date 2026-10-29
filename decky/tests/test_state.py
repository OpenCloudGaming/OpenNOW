import json
import tempfile
import unittest
from pathlib import Path

from tests import support  # noqa: F401
from opennow_decky.state import PluginState


class PluginStateTests(unittest.TestCase):
    def setUp(self):
        self.path = Path(tempfile.mkdtemp()) / "state.json"
        self.now = 10.0

    def make(self):
        return PluginState(self.path, clock=lambda: self.now)

    def test_games_round_trip_newest_first_and_replace_duplicates(self):
        state = self.make()
        state.add_game(1, "100", "Rage", "STEAM", "https://img.nvidiagrid.net/r.jpg")
        self.now = 20.0
        state.add_game(2, "200", "Fortnite", "EPIC", "")
        self.now = 30.0
        state.add_game(3, "100", "Rage", "STEAM", "")
        titles = [(game["appId"], game["title"]) for game in self.make().to_dict()["games"]]
        self.assertEqual(titles, [(3, "Rage"), (2, "Fortnite")])
        self.make().remove_game(2)
        self.assertEqual([game["appId"] for game in self.make().to_dict()["games"]], [3])

    def test_invalid_stored_values_are_dropped(self):
        self.path.write_text(json.dumps({
            "customPath": 5,
            "launcherAppId": "x",
            "games": [{"appId": 1, "variantId": "abc", "title": "x", "store": "", "art": "", "addedAt": 1}, "junk"],
            "preferences": {"consoleLayout": "yes", "collection": False},
        }))
        state = self.make().to_dict()
        self.assertEqual((state["customPath"], state["launcherAppId"], state["games"]), ("", None, []))
        self.assertEqual(state["preferences"], {"consoleLayout": True, "collection": False, "gamePageButton": True,
                                                 "nightlyChannel": False})

    def test_unknown_preferences_and_bad_games_are_refused(self):
        state = self.make()
        with self.assertRaises(ValueError):
            state.set_preference("nope", True)
        with self.assertRaises(ValueError):
            state.add_game(1, "12a", "Rage", "STEAM", "")


if __name__ == "__main__":
    unittest.main()
