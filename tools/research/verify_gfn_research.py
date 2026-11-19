#!/usr/bin/env python3
"""Check Markdown links, REA citation retention, and read-only replay plans."""

import argparse
import json
import pathlib
import re

from rea_batch import READ_ONLY


def collect(value, records):
    if isinstance(value, dict):
        if "evidence_id" in value and "operation" in value:
            records[value["evidence_id"]] = value
            return
        for key, child in value.items():
            if key == "text" and isinstance(child, str):
                try:
                    collect(json.loads(child), records)
                except json.JSONDecodeError:
                    pass
            else:
                collect(child, records)
    elif isinstance(value, list):
        for child in value:
            collect(child, records)


def verify(docs, evidence):
    records = {}
    for root in evidence:
        if not root.is_dir():
            raise ValueError(f"Evidence directory is missing: {root}")
        for path in root.rglob("*.json"):
            collect(json.loads(path.read_text()), records)
    citations = set()
    documents = list(docs.rglob("*.md"))
    if not documents:
        raise ValueError("No Markdown documents found")
    for path in documents:
        content = path.read_text()
        citations.update(re.findall(r"ev_[0-9a-f]{64}", content))
        for link in re.findall(r"\]\(([^)]+)\)", content):
            if "://" in link or link.startswith("#"):
                continue
            target = link.split("#", 1)[0]
            if not (path.parent / target).is_file():
                raise ValueError(f"Broken link in {path}: {link}")
        uses = set(re.findall(r"\[\^([^\]]+)\](?!:)", content))
        definitions = set(re.findall(r"^\[\^([^\]]+)\]:", content, re.MULTILINE))
        if uses - definitions:
            raise ValueError(f"Undefined footnotes in {path}: {uses - definitions}")
        if "/home/user/" in content or "This draft" in content:
            raise ValueError(f"Uncurated scratch reference in {path}")
    if citations - records.keys():
        raise ValueError(f"Citations without retained evidence: {citations - records.keys()}")
    query_count = 0
    for path in (docs / "queries").glob("*.json"):
        queries = json.loads(path.read_text())
        if not isinstance(queries, list) or not queries:
            raise ValueError(f"Invalid query plan: {path}")
        for query in queries:
            if query["name"] not in READ_ONLY or not isinstance(query.get("arguments", {}), dict):
                raise ValueError(f"Invalid or non-read-only query: {path}")
            if "/home/user/" in json.dumps(query):
                raise ValueError(f"Machine-specific query input: {path}")
            query_count += 1
    if not citations or not query_count:
        raise ValueError("Evidence citations and replay queries must be present")
    for citation in citations:
        record = records[citation]
        if record.get("authority") != "shipped-artifact":
            raise ValueError(f"Unexpected evidence authority: {citation}")
    print(f"Verified {len(documents)} Markdown files, {len(citations)} retained REA citations, and {query_count} read-only replay queries")
    print("Citation presence does not verify the semantic interpretation or runtime behavior")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--docs", required=True, type=pathlib.Path)
    parser.add_argument("--evidence", required=True, nargs="+", type=pathlib.Path)
    args = parser.parse_args()
    verify(args.docs, args.evidence)
