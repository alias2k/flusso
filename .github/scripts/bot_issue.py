"""Find the one issue a CI script maintains: the newest open one the workflow's bot opened.

    import bot_issue
    listed = run(["gh", *bot_issue.list_args(TITLE, LABEL)])
    newest, older = bot_issue.own_issue(listed, TITLE)

Used by `postgres-matrix.py` and `bench-regression.py`. Each script opens one issue under a fixed title
and label, keeps it current, and closes it once things recover. A person's issue with the same title, or
the same label, is never edited or closed; of several bot issues only the newest is touched, the rest
are named and left alone. Only open issues are listed, so a closed one is never reopened: the next
failure opens a new one.

>>> own_issue('[{"number": 3, "title": "T", "author": {"login": "someone"}},'
...           ' {"number": 5, "title": "T", "author": {"login": "app/github-actions"}},'
...           ' {"number": 7, "title": "T", "author": {"login": "app/github-actions"}}]', "T")
(7, [5])
"""
import json

# The workflow token's author, as `gh` names it in a filter and in `author.login`.
BOT_AUTHOR = "app/github-actions"
BOT_LOGINS = {BOT_AUTHOR, "github-actions[bot]", "github-actions"}


def list_args(title: str, label: str) -> list[str]:
    """The `gh` arguments that list the bot's open issues with `title` and `label`."""
    # `--author app/github-actions` matches only through `--search`; alone, gh's filter drops bot authors.
    return ["issue", "list", "--state", "open", "--label", label, "--author", BOT_AUTHOR,
            "--search", f'in:title "{title}"', "--json", "number,title,author"]


def own_issue(listed: str, title: str) -> tuple[int | None, list[int]]:
    """The newest open issue in `listed` (gh's JSON) with `title` and the bot as author, and any older ones."""
    numbers = sorted(
        (issue["number"] for issue in json.loads(listed or "[]")
         if issue.get("title") == title and (issue.get("author") or {}).get("login") in BOT_LOGINS),
        reverse=True,
    )
    return (numbers[0] if numbers else None), numbers[1:]


def left_alone(newest: int, older: list[int]) -> str:
    """The line a script prints when it leaves older bot issues alone."""
    return f"maintaining #{newest}; also open with its title and author, left alone: " + ", ".join(f"#{n}" for n in older)
