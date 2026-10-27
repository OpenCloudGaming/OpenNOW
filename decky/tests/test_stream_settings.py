import json
import os
import stat
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tests import support  # noqa: F401
from opennow_decky import files, stream_settings


class StreamSettingsTests(unittest.TestCase):
    def setUp(self):
        self.data_dir = Path(tempfile.mkdtemp())
        self.path = self.data_dir / "settings.json"

    def test_missing_file_reads_the_core_defaults(self):
        self.assertEqual(stream_settings.read(self.data_dir), stream_settings.DEFAULTS)

    @unittest.skipUnless(os.name == "posix", "Unix file permissions")
    def test_writes_and_backups_are_private_with_existing_insecure_files(self):
        original = {"fps": 60, "sessionProxyUrl": "http://fixture:synthetic@localhost:8080"}
        previous_umask = os.umask(0o022)
        try:
            stream_settings.write(self.data_dir, "fps", 90)
            self.assertEqual(stat.S_IMODE(self.path.stat().st_mode), 0o600)
            for suffix in ("", ".tmp", ".bak"):
                path = self.path.with_name(self.path.name + suffix)
                path.write_text(json.dumps(original))
                path.chmod(0o644)
            for fps in (90, 120, 60):
                previous = self.path.read_bytes()
                stream_settings.write(self.data_dir, "fps", fps)
                backup = self.path.with_name(self.path.name + ".bak")
                self.assertEqual(stat.S_IMODE(self.path.stat().st_mode), 0o600)
                self.assertEqual(stat.S_IMODE(backup.stat().st_mode), 0o600)
                self.assertEqual(backup.read_bytes(), previous)
                self.assertIn("sessionProxyUrl", json.loads(self.path.read_text()))
        finally:
            os.umask(previous_umask)

    @unittest.skipUnless(os.name == "posix", "Unix file links")
    def test_settings_writes_reject_link_destinations_without_touching_the_target(self):
        for suffix in ("", ".tmp", ".bak"):
            for hard_link in (False, True):
                with self.subTest(suffix=suffix, hard_link=hard_link), tempfile.TemporaryDirectory() as directory:
                    data_dir = Path(directory)
                    target = data_dir / "unrelated"
                    target.write_text('{"fps": 60}')
                    target.chmod(0o644)
                    destination = data_dir / ("settings.json" + suffix)
                    if hard_link:
                        os.link(target, destination)
                    else:
                        destination.symlink_to(target)
                    if suffix:
                        (data_dir / "settings.json").write_text('{"fps": 60}')
                    with self.assertRaises((OSError, stream_settings.SettingsError)):
                        stream_settings.write(data_dir, "fps", 90)
                    self.assertEqual(target.read_text(), '{"fps": 60}')
                    self.assertEqual(stat.S_IMODE(target.stat().st_mode), 0o644)

    @unittest.skipUnless(os.name == "posix", "Unix file permissions")
    def test_new_files_are_private_before_any_chmod(self):
        chmod = os.fchmod
        observed = []

        def check_mode(descriptor, mode):
            observed.append(stat.S_IMODE(os.fstat(descriptor).st_mode))
            self.assertEqual(observed[-1], 0o600)
            chmod(descriptor, mode)

        previous_umask = os.umask(0o022)
        try:
            with mock.patch.object(files.os, "fchmod", side_effect=check_mode):
                stream_settings.write(self.data_dir, "fps", 90)
                stream_settings.write(self.data_dir, "fps", 120)
            self.assertTrue(observed)
        finally:
            os.umask(previous_umask)

    @unittest.skipUnless(os.name == "posix", "Unix file links")
    def test_settings_writes_do_not_create_dangling_link_targets(self):
        for suffix in ("", ".tmp", ".bak"):
            with self.subTest(suffix=suffix), tempfile.TemporaryDirectory() as directory:
                data_dir = Path(directory)
                target = data_dir / "must-not-be-created"
                (data_dir / ("settings.json" + suffix)).symlink_to(target)
                if suffix:
                    (data_dir / "settings.json").write_text('{"fps": 60}')
                with self.assertRaises(OSError):
                    stream_settings.write(data_dir, "fps", 90)
                self.assertFalse(target.exists())

    def test_write_keeps_every_other_key_and_backs_up_the_previous_file(self):
        original = {"fps": 60, "themePack": "nocturne", "favoriteGameIds": ["1"], "unknownFutureKey": {"a": 1}}
        self.path.write_text(json.dumps(original))
        values = stream_settings.write(self.data_dir, "resolution", "1280x800")
        self.assertEqual(values["resolution"], "1280x800")
        saved = json.loads(self.path.read_text())
        self.assertEqual(saved, {**original, "resolution": "1280x800"})
        self.assertEqual(json.loads((self.data_dir / "settings.json.bak").read_text()), original)
        self.assertFalse((self.data_dir / "settings.json.tmp").exists())

    def test_bitrate_is_clamped_and_rounded(self):
        self.assertEqual(stream_settings.write(self.data_dir, "maxBitrateMbps", 512)["maxBitrateMbps"], 200)
        self.assertEqual(stream_settings.write(self.data_dir, "maxBitrateMbps", 47.6)["maxBitrateMbps"], 48)

    def test_invalid_values_are_rejected_before_touching_the_file(self):
        for key, value in [("resolution", "1x1"), ("fps", 75), ("codec", "vp9"), ("enableHdr", 1),
                           ("maxBitrateMbps", True), ("themePack", "x")]:
            with self.subTest(key=key):
                with self.assertRaises(stream_settings.SettingsError):
                    stream_settings.write(self.data_dir, key, value)
        self.assertFalse(self.path.exists())

    def test_h264_is_refused_with_ten_bit_colour(self):
        self.path.write_text(json.dumps({"colorQuality": "10bit_420"}))
        with self.assertRaises(stream_settings.SettingsError):
            stream_settings.write(self.data_dir, "codec", "h264")
        self.assertEqual(stream_settings.write(self.data_dir, "codec", "h265")["codec"], "h265")

    def test_corrupt_file_is_reported_and_never_overwritten(self):
        self.path.write_text("{not json")
        with self.assertRaises(stream_settings.SettingsError):
            stream_settings.read(self.data_dir)
        with self.assertRaises(stream_settings.SettingsError):
            stream_settings.write(self.data_dir, "fps", 90)
        self.assertEqual(self.path.read_text(), "{not json")

    def test_wrongly_typed_persisted_values_read_as_defaults(self):
        self.path.write_text(json.dumps({"fps": "fast", "maxBitrateMbps": 0.5, "enableHdr": True}))
        values = stream_settings.read(self.data_dir)
        self.assertEqual((values["fps"], values["maxBitrateMbps"], values["enableHdr"]), (60, 0.5, True))

    def test_identity_choice_records_the_marker_the_core_checks(self):
        for value in (False, True):
            with self.subTest(value=value):
                self.path.write_text("{}")
                stream_settings.write(self.data_dir, "identifyAsSteamDeck", value)
                saved = json.loads(self.path.read_text())
                self.assertIs(saved["identifyAsSteamDeck"], value)
                self.assertIs(saved[stream_settings.IDENTITY_DECIDED_MARKER], True)

    def test_only_the_identity_choice_records_the_marker(self):
        stream_settings.write(self.data_dir, "fps", 90)
        saved = json.loads(self.path.read_text())
        self.assertNotIn(stream_settings.IDENTITY_DECIDED_MARKER, saved)
        self.assertEqual(saved["fps"], 90)


if __name__ == "__main__":
    unittest.main()
