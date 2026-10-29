import os
import sys
import unittest
from unittest import mock

from tests import support  # noqa: F401
from opennow_decky import host


class HostEnvironmentTests(unittest.TestCase):
    def test_pyinstaller_library_path_is_replaced_by_the_original(self):
        with mock.patch.dict(os.environ, {"LD_LIBRARY_PATH": "/tmp/_MEIabc", "LD_LIBRARY_PATH_ORIG": "/usr/lib/extra"}):
            environment = host.host_environment()
        self.assertEqual(environment["LD_LIBRARY_PATH"], "/usr/lib/extra")
        self.assertNotIn("LD_LIBRARY_PATH_ORIG", environment)

    def test_bundle_library_path_is_dropped_when_there_was_no_original(self):
        with mock.patch.dict(os.environ, {"LD_LIBRARY_PATH": "/tmp/_MEIabc"}):
            os.environ.pop("LD_LIBRARY_PATH_ORIG", None)
            self.assertNotIn("LD_LIBRARY_PATH", host.host_environment())

    def test_child_processes_see_the_host_library_path(self):
        with mock.patch.dict(os.environ, {"LD_LIBRARY_PATH": "/tmp/_MEIabc"}):
            os.environ.pop("LD_LIBRARY_PATH_ORIG", None)
            result = host.run([sys.executable, "-c", "import os; print(os.environ.get('LD_LIBRARY_PATH', 'unset'))"], timeout=10)
        self.assertEqual(result.stdout.strip(), "unset")


if __name__ == "__main__":
    unittest.main()
