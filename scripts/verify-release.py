"""Verify that release assets belong to the requested tag and all builds passed."""

import hashlib
import json
import os
from pathlib import Path
import re
import sys
from urllib.parse import quote
from urllib.request import Request, urlopen


TARGETS = (
    "x86_64-pc-windows-msvc",
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
)


def metadata() -> None:
    repository = os.environ["GITHUB_REPOSITORY"]
    run_id = os.environ["SOURCE_RUN_ID"]
    tag = os.environ["RELEASE_TAG"]
    if not run_id.isdecimal():
        raise ValueError("Source run ID must be a number")

    def get(route: str):
        request = Request(
            f"https://api.github.com/repos/{repository}/{route}",
            headers={
                "Authorization": f"Bearer {os.environ['GH_TOKEN']}",
                "Accept": "application/vnd.github+json",
                "User-Agent": "vig-release-verifier",
            },
        )
        with urlopen(request) as response:
            return json.load(response)

    reference = get(f"git/ref/tags/{quote(tag, safe='')}")["object"]
    while reference["type"] == "tag":
        reference = get(f"git/tags/{reference['sha']}")["object"]
    if reference["type"] != "commit":
        raise ValueError("Release tag does not identify a commit")
    run = get(f"actions/runs/{run_id}")
    if run["path"] != ".github/workflows/build-tags.yml":
        raise ValueError("Source artifacts must come from the tagged build workflow")
    if run["head_sha"] != reference["sha"]:
        raise ValueError("Build commit differs from release tag")
    jobs = get(f"actions/runs/{run_id}/jobs?per_page=100")["jobs"]
    for target in TARGETS:
        matches = [job for job in jobs if job["name"] == f"Build {target}"]
        if len(matches) != 1 or matches[0]["conclusion"] != "success":
            raise ValueError(f"Build has not succeeded: {target}")
    artifacts = get(f"actions/runs/{run_id}/artifacts?per_page=100")["artifacts"]
    for target in TARGETS:
        matches = [artifact for artifact in artifacts if artifact["name"] == f"vig-{target}"]
        if len(matches) != 1 or matches[0]["expired"]:
            raise ValueError(f"Required build artifact is absent or expired: {target}")
    print(f"Verified five successful builds of {reference['sha']} for {tag}")


def files(directory: Path, tag: str) -> None:
    safe_tag = re.sub(r"[^A-Za-z0-9._-]", "-", tag)
    expected = set()
    for target in TARGETS:
        extension = "zip" if target.endswith("windows-msvc") else "tar.gz"
        name = f"vig-{safe_tag}-{target}.{extension}"
        checksum_name = name + ".sha256"
        expected.update((name, checksum_name))
        digest, recorded_name = (directory / checksum_name).read_text(encoding="utf-8").split()
        if not re.fullmatch(r"[0-9a-f]{64}", digest) or recorded_name != name:
            raise ValueError(f"Invalid checksum file: {checksum_name}")
        with (directory / name).open("rb") as stream:
            actual = hashlib.file_digest(stream, "sha256").hexdigest()
        if actual != digest:
            raise ValueError(f"SHA-256 mismatch: {name}")
    if {path.name for path in directory.iterdir()} != expected:
        raise ValueError("Release must contain exactly five packages and five checksum files")
    print("Verified five release packages and five SHA-256 checksums")


if __name__ == "__main__":
    if sys.argv[1] == "metadata":
        metadata()
    elif sys.argv[1] == "files":
        files(Path("dist"), os.environ["RELEASE_TAG"])
    else:
        raise ValueError("Expected metadata or files")
