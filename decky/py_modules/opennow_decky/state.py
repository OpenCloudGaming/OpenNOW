import time
from pathlib import Path
from typing import Any

from .files import read_json, write_json_atomic

PREFERENCES = {
    "consoleLayout": True,
    "collection": True,
    "gamePageButton": True,
    "nightlyChannel": False,
}


class PluginState:
    def __init__(self, path: Path, clock=time.time):
        self.path = path
        self.clock = clock
        stored = read_json(path)
        stored = stored if isinstance(stored, dict) else {}
        preferences = stored.get("preferences") if isinstance(stored.get("preferences"), dict) else {}
        self.custom_path: str = stored.get("customPath") if isinstance(stored.get("customPath"), str) else ""
        self.launcher_app_id: int | None = stored.get("launcherAppId") if isinstance(stored.get("launcherAppId"), int) else None
        self.games: list[dict] = [game for game in stored.get("games") or [] if valid_game(game)]
        versions = stored.get("flatpakVersions") if isinstance(stored.get("flatpakVersions"), dict) else {}
        self.flatpak_versions: dict[str, str] = {commit: version for commit, version in versions.items()
                                                 if isinstance(commit, str) and isinstance(version, str)}
        self.preferences = {key: preferences.get(key, default) if isinstance(preferences.get(key), bool) else default
                            for key, default in PREFERENCES.items()}

    def to_dict(self) -> dict[str, Any]:
        return {
            "customPath": self.custom_path,
            "launcherAppId": self.launcher_app_id,
            "games": sorted(self.games, key=lambda game: game["addedAt"], reverse=True),
            "preferences": dict(self.preferences),
            "flatpakVersions": dict(self.flatpak_versions),
        }

    def save(self) -> dict[str, Any]:
        value = self.to_dict()
        write_json_atomic(self.path, value)
        return value

    def set_preference(self, key: str, value: bool) -> dict[str, Any]:
        if key not in PREFERENCES or not isinstance(value, bool):
            raise ValueError(f"Unknown preference {key}")
        self.preferences[key] = value
        return self.save()

    def set_custom_path(self, path: str) -> dict[str, Any]:
        self.custom_path = path.strip()
        return self.save()

    def remember_flatpak_version(self, commit: str, version: str) -> dict[str, Any]:
        self.flatpak_versions = {commit: version}
        return self.save()

    def set_launcher(self, app_id: int | None) -> dict[str, Any]:
        self.launcher_app_id = app_id
        return self.save()

    def add_game(self, app_id: int, variant_id: str, title: str, store: str, art: str) -> dict[str, Any]:
        game = {"appId": app_id, "variantId": variant_id, "title": title, "store": store, "art": art, "addedAt": self.clock()}
        if not valid_game(game):
            raise ValueError("Invalid game shortcut")
        self.games = [entry for entry in self.games if entry["appId"] != app_id and entry["variantId"] != variant_id]
        self.games.append(game)
        return self.save()

    def remove_game(self, app_id: int) -> dict[str, Any]:
        self.games = [entry for entry in self.games if entry["appId"] != app_id]
        return self.save()


def valid_game(game: Any) -> bool:
    return (
        isinstance(game, dict)
        and isinstance(game.get("appId"), int)
        and isinstance(game.get("variantId"), str) and game["variantId"].isdigit()
        and isinstance(game.get("title"), str) and bool(game["title"])
        and isinstance(game.get("store"), str)
        and isinstance(game.get("art"), str)
        and isinstance(game.get("addedAt"), (int, float))
    )
