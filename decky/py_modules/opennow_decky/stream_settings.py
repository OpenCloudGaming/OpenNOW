from pathlib import Path
from typing import Any

from . import process
from .files import read_json, write_json_atomic

RESOLUTIONS = (
    "1280x800", "1440x900", "1680x1050", "1920x1200", "2560x1600",
    "1280x720", "1600x900", "1920x1080", "2560x1440", "3840x2160",
)
FRAME_RATES = (30, 60, 90, 120)
CODECS = ("auto", "av1", "h265", "h264")
BITRATE_RANGE = (10, 200)

DEFAULTS: dict[str, Any] = {
    "resolution": "1920x1080",
    "fps": 60,
    "maxBitrateMbps": 75,
    "codec": "auto",
    "enableHdr": False,
    "identifyAsSteamDeck": False,
    "steamBigPictureMode": False,
}

# The core turns `identifyAsSteamDeck` on once on Steam Deck hardware and records this marker so it
# never overrides the choice again. A write from the plugin is that choice, so it carries the marker;
# without it the core re-applies its default at the next start and discards what the user picked.
IDENTITY_DECIDED_MARKER = "steamDeckIdentityDefaultApplied"


class SettingsError(ValueError):
    pass


def settings_path(data_dir: Path) -> Path:
    return data_dir / "settings.json"


def read_all(data_dir: Path) -> dict[str, Any]:
    path = settings_path(data_dir)
    if not path.exists():
        return {}
    persisted = read_json(path)
    if not isinstance(persisted, dict):
        raise SettingsError("OpenNOW's settings file can't be read. Open OpenNOW once to repair it.")
    return persisted


def read(data_dir: Path) -> dict[str, Any]:
    persisted = read_all(data_dir)
    return {key: persisted[key] if readable(key, persisted.get(key)) else default for key, default in DEFAULTS.items()}


def readable(key: str, value: Any) -> bool:
    if key == "maxBitrateMbps":
        return isinstance(value, (int, float)) and not isinstance(value, bool)
    return type(value) is type(DEFAULTS[key])


def validate(key: str, value: Any, persisted: dict[str, Any]) -> Any:
    if key not in DEFAULTS:
        raise SettingsError(f"{key} is not a stream setting")
    if key == "resolution":
        if value not in RESOLUTIONS:
            raise SettingsError(f"Unsupported resolution {value}")
    elif key == "fps":
        if value not in FRAME_RATES:
            raise SettingsError(f"Unsupported frame rate {value}")
    elif key == "maxBitrateMbps":
        if isinstance(value, bool) or not isinstance(value, (int, float)):
            raise SettingsError("Bitrate must be a number")
        low, high = BITRATE_RANGE
        value = int(round(min(max(value, low), high)))
    elif key == "codec":
        if value not in CODECS:
            raise SettingsError(f"Unsupported codec {value}")
        if value == "h264" and persisted.get("colorQuality", "8bit_420") != "8bit_420":
            raise SettingsError("H.264 only supports 8-bit 4:2:0 colour. Change colour quality in OpenNOW first.")
    elif not isinstance(value, bool):
        raise SettingsError(f"{key} must be on or off")
    return value


def write(data_dir: Path, key: str, value: Any) -> dict[str, Any]:
    with process.core_lock(data_dir):
        persisted = read_all(data_dir)
        persisted[key] = validate(key, value, persisted)
        if key == "identifyAsSteamDeck":
            persisted[IDENTITY_DECIDED_MARKER] = True
        write_json_atomic(settings_path(data_dir), persisted, keep_backup=True)
        return read(data_dir)
