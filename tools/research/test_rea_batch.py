import argparse
import hashlib
import json
import pathlib
import tempfile
import unittest
from unittest.mock import patch

import rea_batch


class ReaBatchTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        root = pathlib.Path(self.directory.name)
        target = root / "target"
        target.write_bytes(b"local fixture")
        plan = root / "plan.json"
        plan.write_text(json.dumps([dict(name="search_procedures", arguments=dict(pattern="main"))]))
        self.args = argparse.Namespace(target=target, sha256=hashlib.sha256(target.read_bytes()).hexdigest(),
                                       plan=plan, output=root / "output", rea_command="rea", timeout=60)

    def test_digest_mismatch_never_starts_mcp(self):
        self.args.sha256 = "0" * 64
        with patch.object(rea_batch, "Connection") as connection:
            with self.assertRaises(ValueError):
                rea_batch.run(self.args)
            connection.assert_not_called()
        self.assertFalse(self.args.output.exists())

    def test_mutating_query_never_starts_mcp(self):
        self.args.plan.write_text(json.dumps([dict(name="open_binary", arguments={})]))
        with patch.object(rea_batch, "Connection") as connection:
            with self.assertRaises(ValueError):
                rea_batch.run(self.args)
            connection.assert_not_called()
        self.assertFalse(self.args.output.exists())

    def test_existing_output_is_not_overwritten(self):
        self.args.output.mkdir()
        marker = self.args.output / "marker"
        marker.write_text("retain")
        with patch.object(rea_batch, "Connection") as connection:
            with self.assertRaises(FileExistsError):
                rea_batch.run(self.args)
            connection.assert_not_called()
        self.assertEqual(marker.read_text(), "retain")

    def test_success_saves_response_and_closes_target(self):
        with patch.object(rea_batch, "Connection") as connection:
            connection.return_value.request.side_effect = [dict(protocolVersion="2025-06-18"), {}, dict(content=[]), {}]
            rea_batch.run(self.args)
            calls = connection.return_value.request.call_args_list
            self.assertEqual(calls[-1].args[1]["name"], "close_binary")
            connection.return_value.close.assert_called_once()
        self.assertEqual(json.loads((self.args.output / "000-search_procedures.json").read_text()), dict(content=[]))

    def test_query_error_preserves_response_and_closes_connection(self):
        with patch.object(rea_batch, "Connection") as connection:
            connection.return_value.request.side_effect = [{}, {}, dict(isError=True, content=[])]
            with self.assertRaises(RuntimeError):
                rea_batch.run(self.args)
            connection.return_value.close.assert_called_once()
        self.assertTrue(json.loads((self.args.output / "000-search_procedures.json").read_text())["isError"])


if __name__ == "__main__":
    unittest.main()
