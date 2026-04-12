#!/usr/bin/env python3
from __future__ import annotations

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Sequence

SCRIPT_PATH = Path(__file__).resolve()
REPO_ROOT = SCRIPT_PATH.parent.parent
MAX_GIT_ARG_COUNT = 200
MAX_GIT_ARG_CHARS = 7000
EOL_RECORD_RE = re.compile(r"^i/(?P<index>\S+)\s+w/(?P<worktree>\S+)\s+attr/(?P<attr>.*)$")


@dataclass(frozen=True)
class EolInfo:
    path: str
    index: str
    worktree: str
    attr: str

    @property
    def is_lf_to_crlf(self) -> bool:
        return self.index == "lf" and self.worktree == "crlf"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Inspect the current unstaged git diff and rewrite only files whose "
            "index line endings are LF while the working tree line endings are CRLF."
        )
    )
    parser.add_argument(
        "--apply",
        action="store_true",
        help="Rewrite matching files from CRLF to LF. Without this flag the script only reports candidates.",
    )
    return parser.parse_args()


def run_git(args: Sequence[str]) -> bytes:
    try:
        completed = subprocess.run(
            ["git", *args],
            cwd=REPO_ROOT,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
    except subprocess.CalledProcessError as exc:
        stderr = exc.stderr.decode("utf-8", "replace").strip()
        command = " ".join(["git", *args])
        raise RuntimeError(f"git command failed: {command}\n{stderr}") from exc
    return completed.stdout


def split_nul_output(output: bytes) -> list[str]:
    if not output:
        return []
    return [entry for entry in output.decode("utf-8", "surrogateescape").split("\0") if entry]


def read_unstaged_diff_paths() -> list[str]:
    return split_nul_output(run_git(["diff", "--name-only", "--diff-filter=M", "-z"]))


def chunk_paths(paths: Sequence[str]) -> Iterable[list[str]]:
    chunk: list[str] = []
    chunk_chars = 0

    for path in paths:
        path_chars = len(path) + 1
        if chunk and (len(chunk) >= MAX_GIT_ARG_COUNT or chunk_chars + path_chars > MAX_GIT_ARG_CHARS):
            yield chunk
            chunk = []
            chunk_chars = 0

        chunk.append(path)
        chunk_chars += path_chars

    if chunk:
        yield chunk


def parse_eol_record(record: str) -> EolInfo:
    metadata, separator, path = record.partition("\t")
    if not separator:
        raise RuntimeError(f"unexpected git ls-files --eol record: {record!r}")

    match = EOL_RECORD_RE.match(metadata)
    if match is None:
        raise RuntimeError(f"unexpected git ls-files --eol metadata: {metadata!r}")

    return EolInfo(
        path=path,
        index=match.group("index"),
        worktree=match.group("worktree"),
        attr=match.group("attr").rstrip(),
    )


def read_eol_info(paths: Sequence[str]) -> dict[str, EolInfo]:
    info_by_path: dict[str, EolInfo] = {}
    for chunk in chunk_paths(paths):
        output = run_git(["ls-files", "--eol", "-z", "--", *chunk])
        for record in split_nul_output(output):
            info = parse_eol_record(record)
            info_by_path[info.path] = info
    return info_by_path


def normalize_to_lf(path: Path) -> bool:
    original = path.read_bytes()
    normalized = original.replace(b"\r\n", b"\n")
    if normalized == original:
        return False
    path.write_bytes(normalized)
    return True


def print_paths(title: str, paths: Sequence[str]) -> None:
    print(f"{title} ({len(paths)}):")
    for path in paths:
        print(f"  {path}")


def main() -> int:
    args = parse_args()
    diff_paths = read_unstaged_diff_paths()
    if not diff_paths:
        print("No unstaged modified files found.")
        return 0

    eol_info_by_path = read_eol_info(diff_paths)

    candidates: list[str] = []
    skipped: list[str] = []

    for path in diff_paths:
        info = eol_info_by_path.get(path)
        file_path = REPO_ROOT / path

        if info is None:
            skipped.append(f"{path} (no git eol info)")
            continue
        if not file_path.is_file():
            skipped.append(f"{path} (not a regular file)")
            continue
        if info.is_lf_to_crlf:
            candidates.append(path)
            continue

        skipped.append(f"{path} (i/{info.index} -> w/{info.worktree})")

    print_paths("Unstaged modified files", diff_paths)
    print()
    print_paths("LF-in-index / CRLF-in-worktree candidates", candidates)
    if skipped:
        print()
        print_paths("Left untouched", skipped)

    if not args.apply:
        print()
        print("Dry run only. Re-run with --apply to rewrite the candidate files to LF.")
        return 0

    rewritten_count = 0
    unchanged_count = 0
    for path in candidates:
        if normalize_to_lf(REPO_ROOT / path):
            rewritten_count += 1
        else:
            unchanged_count += 1

    print()
    print(f"Rewrote {rewritten_count} file(s) to LF.")
    if unchanged_count:
        print(f"{unchanged_count} candidate file(s) already contained no CRLF bytes.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except RuntimeError as exc:
        print(f"error: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc
