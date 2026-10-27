from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from audit_repository_devops import action_reference, inventory, query


class QueryTests(unittest.TestCase):
    @patch("audit_repository_devops.subprocess.run")
    def test_queries_use_default_get_not_mutating_methods(self, run):
        run.return_value = SimpleNamespace(returncode=0, stdout='{"names": []}')
        self.assertEqual(query("repos/owner/repo/actions/secrets", "{names:[.secrets[].name]}"), {"names": []})
        command = run.call_args.args[0]
        self.assertEqual(command[:2], ["gh", "api"])
        self.assertNotIn("--method", command)
        self.assertIn("--jq", command)
        self.assertNotIn(".value", command[-1])

    @patch("audit_repository_devops.subprocess.run")
    def test_access_errors_remain_unavailable_not_an_empty_safe_inventory(self, run):
        run.return_value = SimpleNamespace(returncode=1, stderr="HTTP 403")
        self.assertEqual(query("repos/owner/repo/actions/secrets")["status"], "unavailable")


class ActionReferenceTests(unittest.TestCase):
    @patch("audit_repository_devops.query")
    def test_annotated_tags_are_not_misrepresented_as_pinned_commits(self, api):
        api.return_value = {"object": {"sha": "tag-object", "type": "tag"}}
        result = action_reference("owner/action@v1")
        self.assertFalse(result["pinned"])
        self.assertEqual(result["object_type"], "tag")

    @patch("audit_repository_devops.query")
    def test_mutable_branch_is_checked_when_no_tag_exists(self, api):
        api.side_effect = [{"status": "unavailable"}, {"object": {"sha": "commit", "type": "commit"}}]
        self.assertTrue(action_reference("owner/action@stable")["resolved"])
        self.assertTrue(api.call_args.args[0].endswith("heads/stable"))

    @patch("audit_repository_devops.query")
    def test_unresolved_is_not_a_positive_existence_claim(self, api):
        api.return_value = {"status": "unavailable"}
        self.assertFalse(action_reference("owner/action@missing")["resolved"])


class InventoryTests(unittest.TestCase):
    @patch("audit_repository_devops.subprocess.check_output", return_value="source-commit\n")
    @patch("audit_repository_devops.action_reference")
    @patch("audit_repository_devops.query")
    def test_counts_repeated_uses_and_keeps_credential_inventory_names_only(self, api, reference, git):
        api.return_value = {}
        reference.return_value = {"use": "owner/action@v1", "pinned": False, "resolved": True}
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            workflows = root / ".github/workflows"
            workflows.mkdir(parents=True)
            (workflows / "ci.yml").write_text("steps:\n  - uses: owner/action@v1\n  - uses: owner/action@v1\n")
            result = inventory("owner/repo", root)
        self.assertEqual(result["external_action_occurrences"], 2)
        self.assertEqual(result["unique_external_refs"], 1)
        for call in api.call_args_list:
            if call.args[0].endswith(("/secrets", "/variables")):
                self.assertIn("names:", call.args[1])
                self.assertNotIn(".value", call.args[1])


if __name__ == "__main__":
    unittest.main()
