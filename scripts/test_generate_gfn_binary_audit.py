import importlib.util
from pathlib import Path
import unittest


spec = importlib.util.spec_from_file_location("gfn_audit", Path(__file__).with_name("generate-gfn-binary-audit.py"))
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


class AuditChecks(unittest.TestCase):
    def test_passive_report_elements(self):
        document = audit.ReportHTML()
        document.feed('<section id="one"><h1>Title</h1><a href="#one">One</a></section>')
        document.feed('<table><tr><th style="text-align: right;">Size</th></tr></table>')
        self.assertEqual(document.ids, {"one"})
        self.assertEqual(document.hrefs, ["#one"])

    def test_active_elements_and_attributes_are_rejected(self):
        for source in (
            '<script>alert(1)</script>', '<iframe></iframe>', '<form></form>',
            '<img src="https://example.com/a">', '<img srcset="https://example.com/a">',
            '<p onclick="alert(1)">text</p>', '<p style="background:red">text</p>',
            '<base href="https://example.com/">', '<object data="file"></object>',
        ):
            with self.subTest(source=source), self.assertRaises(ValueError):
                audit.ReportHTML().feed(source)

    def test_duplicate_ids_are_rejected(self):
        with self.assertRaises(ValueError):
            audit.ReportHTML().feed('<p id="one"></p><p id="one"></p>')

    def test_existing_local_and_public_links(self):
        self.assertEqual(audit.check_links(["README.md", "#README", "https://example.com/"]), 1)

    def test_unsafe_or_missing_links_are_rejected(self):
        for href in ("javascript:alert(1)", "https://user:pass@example.com/", "../../../../outside", "missing-file.md"):
            with self.subTest(href=href), self.assertRaises(ValueError):
                audit.check_links([href])


if __name__ == "__main__":
    unittest.main()
