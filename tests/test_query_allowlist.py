import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("query_allowlist", Path(__file__).resolve().parents[1] / "docker/update-query-allowlist.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class QueryAllowlistTests(unittest.TestCase):
    def test_migrates_only_exact_legacy_helper_output(self):
        with tempfile.TemporaryDirectory(prefix="teamhub-query-test-") as directory:
            root = Path(directory)
            (root / "query_ip_allowlist.manager-backup").write_text("127.0.0.1\n")
            target = root / "query_ip_allowlist.txt"
            target.write_text("127.0.0.1\n172.19.0.3/32\n")
            self.assertTrue(module.update(root, "172.19.0.3"))
            self.assertIn("# BEGIN TEAMHUB MANAGER QUERY", target.read_text())
            self.assertTrue(module.update(root, "172.19.0.4"))
            self.assertNotIn("172.19.0.3/32", target.read_text())

    def test_exact_ip_idempotency_backup_and_address_rotation(self):
        with tempfile.TemporaryDirectory(prefix="teamhub-query-test-") as directory:
            root = Path(directory)
            target = root / "query_ip_allowlist.txt"
            original = "127.0.0.1\n::1\n# Existing operator entry\n10.1.1.7/32\n"
            target.write_text(original, encoding="utf-8")
            self.assertTrue(module.update(root, "172.19.0.3"))
            self.assertIn("172.19.0.3/32", target.read_text())
            self.assertFalse(module.update(root, "172.19.0.3"))
            self.assertTrue(module.update(root, "172.19.0.4"))
            self.assertNotIn("172.19.0.3/32", target.read_text())
            self.assertIn("10.1.1.7/32", target.read_text())
            self.assertEqual((root / "query_ip_allowlist.manager-backup").read_text(), original)

    def test_refuses_global_ip_explicit_deny_and_symlink(self):
        with tempfile.TemporaryDirectory(prefix="teamhub-query-test-") as directory:
            root = Path(directory)
            target = root / "query_ip_allowlist.txt"
            target.write_text("127.0.0.1\n")
            with self.assertRaises(ValueError):
                module.update(root, "8.8.8.8")
            (root / "query_ip_denylist.txt").write_text("172.19.0.3/32\n")
            with self.assertRaises(ValueError):
                module.update(root, "172.19.0.3")
            self.assertEqual(target.read_text(), "127.0.0.1\n")
            target.unlink()
            try:
                target.symlink_to(root / "query_ip_denylist.txt")
            except OSError:
                return  # Windows hosts without symlink privilege: Linux CI covers this branch.
            with self.assertRaises(ValueError):
                module.update(root, "172.19.0.4")


if __name__ == "__main__":
    unittest.main()
