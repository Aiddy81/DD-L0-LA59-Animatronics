"""Writes the release notes for one release, as Markdown on stdout.

PRs land in Documentation and reach main through one Documentation -> main PR,
so GitHub's generated notes would list little more than that PR. These notes
come from the merge commits between the previous tag and HEAD instead: every
"Merge pull request #N from owner/branch", grouped by branch prefix, with its
title, author and the issues it closed (one read-only GraphQL call, using
GITHUB_TOKEN or GH_TOKEN; without a token the title comes from the merge
commit). Commits that came in outside any PR are listed after them, and a
SHA-256 table from SHA256SUMS ends the notes.

    python3 .github/scripts/release_notes.py --repo OWNER/NAME --tag TAG \
        [--prev TAG] [--head REF] [--sums release-out/SHA256SUMS]
"""

import argparse
import json
import os
import re
import subprocess
import sys
import urllib.request

GROUPS = [
    ("feature/", "Features"),
    ("bugfix/", "Fixes"),
    ("docs/", "Docs"),
    ("ci/", "CI"),
    ("", "Other"),
]
# PRs from these branches carry other PRs' work to main; listing them would
# repeat everything.
INTEGRATION_BRANCHES = {"Documentation", "dev", "main"}
MERGE_RE = re.compile(r"^Merge pull request #(\d+) from [^/\s]+/(\S+)$")


def git(*args):
    return subprocess.run(
        ["git", *args], check=True, capture_output=True, text=True
    ).stdout


def previous_tag(head):
    """The highest version tag already in head's history, if any."""
    for tag in git("tag", "--merged", head, "--sort=-v:refname").split():
        if re.fullmatch(r"v\d+(\.\d+)*", tag):
            return tag
    return None


def commits(rev_range):
    """(sha, parents, subject, body) for each commit in the range."""
    out = git("log", "--format=%H%x00%P%x00%s%x00%b%x1e", *rev_range)
    for record in out.split("\x1e"):
        record = record.strip("\n")
        if record:
            sha, parents, subject, body = record.split("\x00", 3)
            yield sha, parents.split(), subject, body.strip()


def fetch_prs(repo, numbers, token):
    """{number: {title, author, issues}} from GitHub, or {} without a token."""
    if not token or not numbers:
        return {}
    owner, name = repo.split("/", 1)
    fields = (
        "title author { login } "
        "closingIssuesReferences(first: 20) { nodes { number } }"
    )
    found = {}
    numbers = sorted(numbers)
    for start in range(0, len(numbers), 50):
        aliases = " ".join(
            f"pr{n}: pullRequest(number: {n}) {{ {fields} }}"
            for n in numbers[start : start + 50]
        )
        query = (
            "query($owner: String!, $name: String!) "
            f"{{ repository(owner: $owner, name: $name) {{ {aliases} }} }}"
        )
        request = urllib.request.Request(
            "https://api.github.com/graphql",
            data=json.dumps(
                {"query": query, "variables": {"owner": owner, "name": name}}
            ).encode(),
            headers={
                "Authorization": f"Bearer {token}",
                "Content-Type": "application/json",
                "User-Agent": "ddl0-release-notes",
            },
        )
        with urllib.request.urlopen(request, timeout=30) as response:
            reply = json.load(response)
        if reply.get("errors"):
            sys.exit(f"GitHub API error: {reply['errors']}")
        for alias, pr in reply["data"]["repository"].items():
            if pr:
                found[int(alias[2:])] = {
                    "title": pr["title"],
                    "author": (pr.get("author") or {}).get("login"),
                    "issues": sorted(
                        i["number"] for i in pr["closingIssuesReferences"]["nodes"]
                    ),
                }
    return found


def group_of(branch):
    for prefix, heading in GROUPS:
        if branch.startswith(prefix):
            return heading
    return "Other"


def sums_table(path):
    rows = ["| File | SHA-256 |", "|---|---|"]
    with open(path, encoding="utf-8") as f:
        for line in f:
            digest, _, name = line.strip().partition("  ")
            if digest:
                rows.append(f"| `{name.lstrip('*')}` | `{digest}` |")
    return "\n".join(rows)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--repo", required=True, help="OWNER/NAME")
    parser.add_argument("--tag", required=True, help="the tag being released")
    parser.add_argument("--prev", help="previous tag (default: latest in history)")
    parser.add_argument("--head", default="HEAD")
    parser.add_argument("--sums", help="SHA256SUMS file for the checksum table")
    args = parser.parse_args()

    url = f"https://github.com/{args.repo}"
    prev = args.prev or previous_tag(args.head)
    rev_range = [f"{prev}..{args.head}"] if prev else [args.head]

    prs = {}  # number -> (branch, title from the merge commit)
    in_prs = set()
    loose = []
    all_commits = list(commits(rev_range))
    for sha, parents, subject, body in all_commits:
        match = MERGE_RE.match(subject)
        if match and len(parents) == 2:
            number, branch = int(match[1]), match[2]
            if branch not in INTEGRATION_BRANCHES:
                prs[number] = (branch, body.splitlines()[0] if body else branch)
                in_prs.update(
                    git("rev-list", f"{parents[0]}..{parents[1]}").split()
                )
    for sha, parents, subject, _ in all_commits:
        if len(parents) < 2 and sha not in in_prs:
            loose.append((sha, subject))

    token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
    details = fetch_prs(args.repo, prs.keys(), token)

    lines = []
    if prev:
        lines.append(f"Changes since {prev} ([compare]({url}/compare/{prev}...{args.tag})).")
    else:
        lines.append("First release.")
    grouped = {}
    for number in sorted(prs):
        branch, fallback_title = prs[number]
        info = details.get(number, {})
        line = f"- {info.get('title', fallback_title)} ([#{number}]({url}/pull/{number}))"
        if info.get("author"):
            line += f" by @{info['author']}"
        if info.get("issues"):
            line += ", closes " + ", ".join(
                f"[#{i}]({url}/issues/{i})" for i in info["issues"]
            )
        grouped.setdefault(group_of(branch), []).append(line)
    for _, heading in GROUPS:
        if heading in grouped:
            lines += ["", f"## {heading}", "", *grouped[heading]]
    if loose:
        lines += ["", "## Other commits", ""]
        lines += [f"- {s} ([`{sha[:7]}`]({url}/commit/{sha}))" for sha, s in loose]
    if not prs and not loose:
        lines += ["", "No changes since the previous release."]
    if args.sums:
        lines += ["", "## SHA-256", "", sums_table(args.sums)]
    print("\n".join(lines))


if __name__ == "__main__":
    main()
