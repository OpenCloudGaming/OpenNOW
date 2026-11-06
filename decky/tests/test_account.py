import json
import tempfile
import unittest
from pathlib import Path

from tests import support  # noqa: F401
from opennow_decky.account import signed_in


class SignedInTests(unittest.TestCase):
    def setUp(self):
        self.data_dir = Path(tempfile.mkdtemp())

    def write(self, value):
        (self.data_dir / "accounts.json").write_text(json.dumps(value))

    def test_fresh_install_is_not_signed_in(self):
        self.assertFalse(signed_in(self.data_dir))

    def test_an_account_that_needs_sign_in_does_not_count(self):
        self.write({"activeUserId": "a", "accounts": [{"userId": "a", "needsSignIn": True}]})
        self.assertFalse(signed_in(self.data_dir))
        self.write({"activeUserId": "a", "accounts": [{"userId": "a", "needsSignIn": True}, {"userId": "b"}]})
        self.assertTrue(signed_in(self.data_dir))

    def test_signing_out_of_every_account_is_not_signed_in(self):
        self.write({"activeUserId": None, "accounts": []})
        self.assertFalse(signed_in(self.data_dir))

    def test_legacy_auth_state_counts_until_the_core_migrates_it(self):
        (self.data_dir / "auth-state.json").write_text("{}")
        self.assertTrue(signed_in(self.data_dir))


if __name__ == "__main__":
    unittest.main()
