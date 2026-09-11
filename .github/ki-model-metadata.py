#!/usr/bin/env python3
"""Prepare and validate Ki-Model's product version and upstream mapping."""

import argparse
import json
from pathlib import Path
import re
import subprocess


def read_json(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def write_json(path, value):
    Path(path).write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def semver(value):
    if not isinstance(value, str) or not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", value):
        raise ValueError(f"Expected stable X.Y.Z SemVer: {value!r}")
    return tuple(map(int, value.split(".")))


def upstream(path):
    data = read_json(path)
    if data.get("schemaVersion") != 1 or data.get("repository") != "iOfficeAI/aionrs":
        raise ValueError(f"Invalid upstream identity in {path}")
    if not re.fullmatch(r"v\d+\.\d+\.\d+", data.get("tag", "")):
        raise ValueError(f"Invalid upstream tag in {path}")
    sha = data.get("peeledCommit", "")
    if not re.fullmatch(r"[0-9a-f]{40}", sha):
        raise ValueError(f"Invalid upstream SHA in {path}")
    subprocess.run(["git", "merge-base", "--is-ancestor", sha, "HEAD"], check=True)
    return {key: data[key] for key in ("repository", "tag", "peeledCommit")}


def entries():
    data = read_json("ki-model-versions.json")
    if data.get("schemaVersion") != 1 or not isinstance(data.get("versions"), list):
        raise ValueError("Expected schemaVersion 1 and a versions array")
    previous = (-1, -1, -1)
    for entry in data["versions"]:
        version = semver(entry["version"])
        if version <= previous or entry["tag"] != "ki-model-v" + entry["version"]:
            raise ValueError("Version history must be unique and strictly increasing")
        previous = version
    return data


def validate(release=False, base_sha=None):
    version = Path("ki-model-version.txt").read_text(encoding="utf-8").strip()
    semver(version)
    if read_json(".release-please-manifest.ki-model.json") != {".": version}:
        raise ValueError("Product version and Release Please manifest disagree")
    history = entries()["versions"]
    pending = Path("ki-model-upstream-pending.json")
    if pending.exists():
        upstream(pending)
    if version == "0.0.0":
        if release or history or not pending.exists() or Path("ki-model-upstream.json").exists():
            raise ValueError("0.0.0 is only an unpublished bootstrap state")
    else:
        source = upstream("ki-model-upstream.json")
        expected = {"version": version, "tag": "ki-model-v" + version, "upstream": source}
        if not history or history[-1] != expected:
            raise ValueError("Current version does not match the final upstream mapping")
        if release and pending.exists():
            raise ValueError("Release PR must promote and remove the pending baseline")
        changelog = Path("CHANGELOG.ki-model.md").read_text(encoding="utf-8")
        if not re.search(r"^## \[?" + re.escape(version) + r"(?:\]|\s)", changelog, re.MULTILINE):
            raise ValueError("Product CHANGELOG is missing the current version")
    if base_sha:
        if not re.fullmatch(r"[0-9a-f]{40}", base_sha):
            raise ValueError("Base SHA must be a full commit SHA")
        path = base_sha + ":ki-model-versions.json"
        exists = subprocess.run(["git", "cat-file", "-e", path], capture_output=True)
        if exists.returncode == 0:
            old = json.loads(subprocess.check_output(["git", "show", path]))["versions"]
            if history[:len(old)] != old:
                raise ValueError("Existing version mappings cannot be changed or removed")
    return version


def prepare(version):
    if Path("ki-model-version.txt").read_text(encoding="utf-8").strip() != version:
        raise ValueError("Release Please version differs from the selected version")
    semver(version)
    pending = Path("ki-model-upstream-pending.json")
    source_path = pending if pending.exists() else Path("ki-model-upstream.json")
    source = upstream(source_path)
    data = entries()
    entry = {"version": version, "tag": "ki-model-v" + version, "upstream": source}
    if data["versions"] and data["versions"][-1]["version"] == version:
        if data["versions"][-1] != entry:
            raise ValueError("Existing version mapping differs from the selected baseline")
    else:
        if data["versions"] and semver(version) <= semver(data["versions"][-1]["version"]):
            raise ValueError("New versions must increase")
        data["versions"].append(entry)
    if pending.exists():
        pending.rename("ki-model-upstream.json")
    write_json("ki-model-versions.json", data)
    validate(release=True)


def provenance(metadata_path, output):
    version = validate(release=True)
    metadata = read_json(metadata_path)
    members = set(metadata["workspace_members"])
    crates = [{"name": p["name"], "version": p["version"]}
              for p in metadata["packages"] if p["id"] in members
              and any("lib" in t["kind"] for t in p["targets"])]
    sha = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    write_json(output, {"schemaVersion": 1, "repository": "xlihub/Ki-Model",
                        "version": version, "tag": "ki-model-v" + version,
                        "commit": sha, "upstream": upstream("ki-model-upstream.json"),
                        "sdkCrates": sorted(crates, key=lambda c: c["name"])})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    check = commands.add_parser("validate")
    check.add_argument("--release", action="store_true")
    check.add_argument("--base-sha")
    stage = commands.add_parser("prepare")
    stage.add_argument("version")
    source = commands.add_parser("provenance")
    source.add_argument("metadata")
    source.add_argument("output")
    args = parser.parse_args()
    if args.command == "validate":
        print("Ki-Model metadata valid:", validate(args.release, args.base_sha))
    elif args.command == "prepare":
        prepare(args.version)
    else:
        provenance(args.metadata, args.output)


if __name__ == "__main__":
    main()
