"""Tests for bot_issue.py: finding the one issue a CI script maintains.

    python3 -m unittest discover -s .github/scripts -p 'test_bot_issue.py'

The scripts that use it (`postgres-matrix.py`, `bench-regression.py`) are tested end to end over a fake
`gh` in their own test files; these pin the lookup itself.
"""
import doctest
import json
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import bot_issue  # noqa: E402

TITLE = "Something broke on main"


def listed(*issues: tuple) -> str:
    return json.dumps([{"number": n, "title": title, "author": author} for n, title, author in issues])


def bot(n: int, title: str = TITLE, login: str = bot_issue.BOT_AUTHOR) -> tuple:
    return n, title, {"login": login, "is_bot": True}


class OwnIssue(unittest.TestCase):
    def test_none_listed_is_none(self):
        self.assertEqual(bot_issue.own_issue("", TITLE), (None, []))
        self.assertEqual(bot_issue.own_issue("[]", TITLE), (None, []))

    def test_the_newest_bot_issue_wins_and_the_rest_are_named(self):
        self.assertEqual(bot_issue.own_issue(listed(bot(41), bot(57), bot(12)), TITLE), (57, [41, 12]))

    def test_a_persons_issue_is_never_its_own(self):
        self.assertEqual(bot_issue.own_issue(listed((99, TITLE, {"login": "someone"})), TITLE), (None, []))
        self.assertEqual(bot_issue.own_issue(listed((99, TITLE, None), (98, TITLE, {})), TITLE), (None, []))

    def test_every_spelling_of_the_bot_counts(self):
        for login in sorted(bot_issue.BOT_LOGINS):
            with self.subTest(login=login):
                self.assertEqual(bot_issue.own_issue(listed(bot(5, login=login)), TITLE), (5, []))

    def test_the_title_must_match_exactly(self):
        found = listed(bot(5, TITLE + " (not this one)"), bot(6, TITLE.lower()))
        self.assertEqual(bot_issue.own_issue(found, TITLE), (None, []))


class ListArgs(unittest.TestCase):
    def test_it_asks_for_the_bots_open_labelled_issues_with_their_authors(self):
        args = bot_issue.list_args(TITLE, "bug")
        self.assertEqual(args[:4], ["issue", "list", "--state", "open"])
        self.assertEqual(args[args.index("--label") + 1], "bug")
        self.assertEqual(args[args.index("--author") + 1], "app/github-actions")
        self.assertEqual(args[args.index("--search") + 1], f'in:title "{TITLE}"')
        self.assertEqual(set(args[args.index("--json") + 1].split(",")), {"number", "title", "author"})

    def test_the_left_alone_line_names_both(self):
        self.assertEqual(bot_issue.left_alone(57, [41, 12]),
                         "maintaining #57; also open with its title and author, left alone: #41, #12")


def load_tests(loader, tests, ignore):
    tests.addTests(doctest.DocTestSuite(bot_issue))
    return tests


if __name__ == "__main__":
    unittest.main()
