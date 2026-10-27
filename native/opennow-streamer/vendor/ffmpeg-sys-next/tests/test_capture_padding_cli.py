from pathlib import Path
import subprocess
import sys
import unittest


SCRIPT = Path(__file__).with_name("v4l2_request_capture_padding.py")


class CapturePaddingCliTests(unittest.TestCase):
    def test_requires_a_prepared_source_file(self):
        result = subprocess.run([sys.executable, str(SCRIPT)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 2)
        self.assertIn("prepared_source", result.stderr)
        self.assertNotIn("Traceback", result.stderr)

    def test_help_describes_the_prepared_source(self):
        result = subprocess.run([sys.executable, str(SCRIPT), "--help"], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0)
        self.assertIn("hwcontext_v4l2request.c", result.stdout)
        self.assertIn("patches", result.stdout)


if __name__ == "__main__":
    unittest.main()
