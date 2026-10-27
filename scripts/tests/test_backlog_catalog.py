import sys
from pathlib import Path
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from prepare_backlog_import import normalize
from triage_archived_backlog import apply_review_overrides, triage


def issue(number=1, title="Reported bug", labels=()):
    return {
        "number": number, "title": title, "html_url": f"https://github.com/example/repo/issues/{number}",
        "state": "open", "user": {"login": "original-author"}, "assignees": [{"login": "upstream-owner"}],
        "body": "Original text", "labels": [{"name": label} for label in labels],
        "created_at": "2026-10-01T00:00:00Z", "updated_at": "2026-10-02T00:00:00Z",
    }


def record(**kwargs):
    return {"issue": issue(**kwargs), "comments": [], "reviews": [], "review_comments": []}


class ImportTests(unittest.TestCase):
    def test_preserves_provenance_without_inheriting_assignment(self):
        item = normalize(record(labels=["P1", "help wanted", "status:ready-for-work"]), "capture")
        self.assertEqual(item["author"], "original-author")
        self.assertEqual(item["external_id"], "https://github.com/example/repo/issues/1")
        self.assertNotIn("owner", item)
        self.assertNotIn("priority", item)
        self.assertIn("upstream:p1", item["labels"])
        self.assertIn("upstream:help-wanted", item["labels"])
        self.assertNotIn("status:ready-for-work", item["labels"])
        self.assertIn('"status:ready-for-work"', item["body"])

    def test_comments_keep_identity_and_author_for_safe_reimport(self):
        source = record()
        source["comments"] = [{"user": {"login": "reviewer"}, "body": "Evidence",
                               "html_url": "https://github.com/example/repo/issues/1#issuecomment-123",
                               "created_at": "2026-10-03T00:00:00Z"}]
        one = normalize(source, "capture")
        two = normalize(source, "capture")
        self.assertEqual(one, two)
        self.assertEqual(one["comments"][0]["author"], "reviewer")
        self.assertTrue(one["comments"][0]["external_id"].endswith("issuecomment-123"))

    def test_deleted_author_remains_explicit(self):
        source = record()
        source["issue"]["user"] = None
        self.assertEqual(normalize(source, "capture")["author"], "deleted-github-user")


class TriageTests(unittest.TestCase):
    def test_local_review_can_update_the_queue_without_claiming_acceptance(self):
        items = [{"number": 950, "stage": "small-fix-candidate", "engineering_review_complete": False}]
        apply_review_overrides(items, {"950": {"stage": "implemented-awaiting-review"}})
        self.assertEqual(items[0]["stage"], "implemented-awaiting-review")
        self.assertFalse(items[0]["engineering_review_complete"])

    def test_review_overrides_cannot_replace_source_identity_or_acceptance(self):
        for key in ["source_url", "kata_ref", "engineering_review_complete"]:
            with self.assertRaises(RuntimeError):
                apply_review_overrides([{"number": 1}], {"1": {key: True}})

    def test_review_findings_override_small_diff_assumptions(self):
        self.assertEqual(triage(record(number=949))[0], "review-findings")

    def test_no_auto_closure_for_upstream_wontfix(self):
        self.assertEqual(triage(record(labels=["wontfix"]))[0], "historical-decision-review")

    def test_safety_screen_is_not_an_implementation_approval(self):
        stage, reason = triage(record(title="Writer overwrites newer saves"))
        self.assertEqual(stage, "safety-review")
        self.assertIn("not a verified diagnosis", reason)

    def test_known_small_candidates_still_require_reproduction(self):
        stage, reason = triage(record(number=950))
        self.assertEqual(stage, "small-fix-candidate")
        self.assertIn("reproduce", reason)


if __name__ == "__main__":
    unittest.main()
