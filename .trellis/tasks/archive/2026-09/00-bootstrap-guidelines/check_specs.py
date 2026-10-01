#!/usr/bin/env python3
"""Read-only check for this bootstrap's eight roots; stdlib only.

Understands the simple inline links/repository paths used in these specs, not
arbitrary Markdown, external URLs, Rust semantics or source symbol resolution.
"""

import re
from pathlib import Path
from tempfile import TemporaryDirectory

PACKAGES = (
    "multizen-core", "profile-manager", "settings-store", "behavioral",
    "browser-launcher", "cdp-driver", "mcp-server", "tauri-app",
)
SECTIONS = ("Pre-Development Checklist", "Quality Check")
LINK = re.compile(r"\[[^\]\n]+\]\(([^)\s]+)\)")
SOURCE = re.compile(r"(?<![\w/])(?:crates/|\.github/|\.trellis/)[\w./-]+")
TEMPLATE = re.compile(
    r"to be filled|\bTODO\b|\bTBD\b|\bFIXME\b|questions to answer|"
    r"document your project|replace with your actual|<!--",
    re.IGNORECASE,
)


def local_links(text, page):
    return [
        (page.parent / link.split("#", 1)[0]).resolve()
        for link in LINK.findall(text)
        if not re.match(r"[a-zA-Z][a-zA-Z0-9+.-]*:", link)
        and not link.startswith("#")
    ]


def page_errors(text, page, repo):
    errors = []
    if not text.strip():
        errors.append("empty file")
    if TEMPLATE.search(text):
        errors.append("template marker")
    if len(re.findall(r"^```", text, re.MULTILINE)) % 2:
        errors.append("unclosed code fence")
    prose = re.sub(r"^```[^\n]*\n.*?^```[^\n]*$", "[code]\n", text,
                   flags=re.MULTILINE | re.DOTALL)
    headings = list(re.finditer(r"^(#{1,6}) [^\n]+", prose, re.MULTILINE))
    for i, heading in enumerate(headings):
        following = headings[i + 1] if i + 1 < len(headings) else None
        body = prose[heading.end():following.start() if following else len(prose)]
        if not body.strip() and (not following or len(following[1]) <= len(heading[1])):
            errors.append("empty heading")
    for target in local_links(prose, page):
        if not target.exists():
            errors.append(f"broken link: {target}")
    for source in set(SOURCE.findall(prose)):
        if not (repo / source.rstrip(".")).exists():
            errors.append(f"missing source path: {source}")
    for number, line in enumerate(text.splitlines(), 1):
        if line.rstrip() != line:
            errors.append(f"trailing whitespace at line {number}")
    if text and not text.endswith("\n"):
        errors.append("missing final newline")
    return errors


def check(repo):
    errors, count = [], 0
    for package in PACKAGES:
        folder = repo / ".trellis/spec" / package / "backend"
        index = folder / "index.md"
        pages = sorted(folder.rglob("*.md"))
        texts = {page: page.read_text(encoding="utf-8") for page in pages}
        count += len(pages)
        if index not in texts:
            errors.append(f"{package}: missing index")
            continue
        index_text = texts[index]
        for section in SECTIONS:
            body = re.search(rf"^## {re.escape(section)}\n(.*?)(?=^## |\Z)",
                             index_text, re.MULTILINE | re.DOTALL)
            if not body or not re.search(r"^- \[[ x]\] ", body[1], re.MULTILINE):
                errors.append(f"{package}: missing checklist: {section}")
        linked = {p for p in local_links(index_text, index)
                  if p.parent == folder.resolve() and p.name != "index.md"}
        expected = {p.resolve() for p in pages if p != index}
        if not expected or linked != expected:
            errors.append(f"{package}: index/page-set mismatch")
        combined = "\n".join(texts.values())
        if not re.search(r"^```rust\n.+?^```", combined, re.MULTILINE | re.DOTALL):
            errors.append(f"{package}: missing Rust example")
        if f"crates/{package}/src/" not in combined:
            errors.append(f"{package}: missing own source reference")
        for page, text in texts.items():
            errors.extend(f"{page.relative_to(repo)}: {error}"
                          for error in page_errors(text, page, repo))
    return errors, count


def self_check():
    # Synthetic files only; never mutate project specs to test a failure.
    with TemporaryDirectory(prefix="check-specs-") as tmp:
        repo = Path(tmp)
        source = repo / "crates/demo/src/lib.rs"
        source.parent.mkdir(parents=True)
        source.touch()
        good = "# Demo\n\nSee `crates/demo/src/lib.rs::example`.\n"
        for package in PACKAGES:
            folder = repo / ".trellis/spec" / package / "backend"
            folder.mkdir(parents=True)
            own_source = repo / "crates" / package / "src/lib.rs"
            own_source.parent.mkdir(parents=True, exist_ok=True)
            own_source.touch()
            (folder / "index.md").write_text(
                "# Index\n\n[Guide](./guide.md)\n\n"
                "## Pre-Development Checklist\n\n- [ ] Read.\n\n"
                "## Quality Check\n\n- [ ] Check.\n", encoding="utf-8")
            (folder / "guide.md").write_text(
                good + f"\n`crates/{package}/src/lib.rs`\n\n```rust\nlet x = 1;\n```\n",
                encoding="utf-8")
        assert check(repo) == ([], 16)
        page = repo / "test.md"
        assert not page_errors("# Title\n\n## Child\n\nContent.\n", page, repo)
        assert not page_errors("# Title\n\n```rust\nlet x = 1;\n```\n", page, repo)
        for bad, expected in (
            ("", "empty file"),
            ("# Demo\n\n(To be filled by the team)\n", "template marker"),
            ("# Demo\n\n## Empty\n", "empty heading"),
            ("[Missing](./missing.md)\n", "broken link"),
            ("`crates/demo/src/missing.rs`\n", "missing source path"),
            ("```rust\n", "unclosed code fence"),
        ):
            assert any(expected in e for e in page_errors(bad, page, repo)), expected
        folder = repo / ".trellis/spec" / PACKAGES[0] / "backend"
        (folder / "orphan.md").write_text(good, encoding="utf-8")
        assert any("index/page-set mismatch" in e for e in check(repo)[0])
        (folder / "index.md").write_text(good, encoding="utf-8")
        assert any("missing checklist" in e for e in check(repo)[0])
        (folder / "index.md").unlink()
        assert any("missing index" in e for e in check(repo)[0])


if __name__ == "__main__":
    self_check()
    root = Path(__file__).resolve().parents[3]
    failures, total = check(root)
    if failures:
        print("\n".join(failures))
        raise SystemExit(1)
    print(f"PASS: self-check; {len(PACKAGES)} packages; {total} Markdown pages; "
          "indexes, checklists, examples, template scan, local links/source paths, whitespace.")
