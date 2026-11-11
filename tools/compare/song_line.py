"""Specs that play a line from a song, read from the song as they run rather than kept here.

A spec with a `[song]` table is a stub: its `project` names a project folder outside the
repository, and `machine` or `track` which of its sound tracks to read, by its machine's key or
its name. Reading it runs `vimtracker notes`, which reads the song with the program's own model,
never writing to it, and gives that track's notes once through its chain, the song's tempo and the
track's settings. The whole spec is then written from them into `out/songs/` under the stub's
name, and read in its place, so every side plays the same notes and nothing of the line is written
into the repository (decision 142). Only `nuance-chirp-riff` is one so far, and
`nuance_specs.song_spec` writes it.

The folder can be changed without touching the spec, by `VT_COMPARE_SONG` or `run.py --song`, the
flag first. A folder with no song in it skips the spec, and any spec whose input is its render,
with a message, rather than failing, as on a machine without the song.
"""

from __future__ import annotations

import os
import subprocess
import tomllib
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPOSITORY = HERE.parent.parent
# The environment variable naming a project folder to read in place of the spec's own.
ENVIRONMENT = "VT_COMPARE_SONG"
# The vimtracker that reads the song, which `run.py --binary` changes.
BINARY = REPOSITORY / "target" / "release" / "vimtracker"
# A project folder given to `run.py --song`, which wins over the environment and the spec.
CHOSEN: Path | None = None


class Skipped(Exception):
    """A spec that can't run here, as the song it reads isn't, which is no failure."""


def project(table: dict) -> Path:
    """The project folder a spec's `[song]` reads: `--song`'s, the environment's, or its own."""
    given = CHOSEN or os.environ.get(ENVIRONMENT) or table["project"]
    return Path(given).expanduser()


def resolve(path: Path) -> Path:
    """The spec to read for a spec file: itself, or for a stub with a `[song]`, the whole spec
    written from the song, which raises `Skipped` if the song isn't there."""
    with open(path, "rb") as file:
        table = tomllib.load(file).get("song")
    if table is None:
        return path
    folder = project(table)
    if not (folder / "song.ron").is_file():
        raise Skipped(
            f"{path.name}: skipped, as there's no song at {folder}; "
            f"give another project folder with --song or {ENVIRONMENT}"
        )
    if not BINARY.exists():
        raise SystemExit(f"{BINARY} isn't there: run `cargo build --release` first")
    out = HERE / "out" / "songs"
    out.mkdir(parents=True, exist_ok=True)
    notes = out / f"{path.stem}-line.toml"
    command = [str(BINARY), "notes", str(folder), str(notes)]
    for option in ("track", "machine"):
        if table.get(option):
            command += [f"--{option}", str(table[option])]
    result = subprocess.run(command, capture_output=True, text=True)
    if result.returncode != 0:
        raise SystemExit(result.stderr.strip() or f"{BINARY} notes failed")
    with open(notes, "rb") as file:
        line = tomllib.load(file)
    import nuance_specs

    whole = out / path.name
    whole.write_text(
        f"# {path.name} with the line of {folder}, written by song_line.py as it ran.\n\n"
        + nuance_specs.song_spec(line, folder)
    )
    return whole
