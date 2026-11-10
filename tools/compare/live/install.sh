#!/usr/bin/env bash
# Puts the vimtracker bridge's control surface script where Ableton Live looks for one, or takes it
# away again (#364):
#
#   tools/compare/live/install.sh            link it: just live-bridge-install
#   tools/compare/live/install.sh --copy     copy it, for a Live that won't follow the link
#   tools/compare/live/install.sh uninstall  remove it: just live-bridge-uninstall
#   tools/compare/live/install.sh status     say what's there
#
# It makes one thing, `Remote Scripts/VimtrackerBridge` in Live's User Library, a symbolic link to
# `tools/compare/live/VimtrackerBridge` (or a copy of it, marked with a `.vimtracker-copy` file),
# and makes the `Remote Scripts` folder if there isn't one. Nothing else in the library or in Live
# is touched. Uninstalling removes only that link or that marked copy.
#
# The User Library is `~/Music/Ableton/User Library` unless ABLETON_USER_LIBRARY names another,
# such as one moved in Live's Settings, Library.

set -euo pipefail

LIBRARY="${ABLETON_USER_LIBRARY:-$HOME/Music/Ableton/User Library}"
SCRIPTS="$LIBRARY/Remote Scripts"
TARGET="$SCRIPTS/VimtrackerBridge"
SOURCE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/VimtrackerBridge"
MARK=".vimtracker-copy"

ours() {
    [ -L "$TARGET" ] || [ -f "$TARGET/$MARK" ]
}

remove() {
    if [ -L "$TARGET" ]; then
        rm "$TARGET"
    elif [ -f "$TARGET/$MARK" ]; then
        rm -rf "$TARGET"
    fi
}

case "${1:-install}" in
    install | --copy)
        if [ ! -d "$LIBRARY" ]; then
            echo "There's no User Library at $LIBRARY. Open Live once to make it, or set ABLETON_USER_LIBRARY." >&2
            exit 1
        fi
        if [ -e "$TARGET" ] && ! ours; then
            echo "$TARGET is there and isn't the bridge's, so it's left alone." >&2
            exit 1
        fi
        mkdir -p "$SCRIPTS"
        remove
        if [ "${1:-install}" = "--copy" ]; then
            mkdir "$TARGET"
            cp "$SOURCE"/*.py "$TARGET"/
            touch "$TARGET/$MARK"
            echo "Copied the bridge to $TARGET"
        else
            ln -s "$SOURCE" "$TARGET"
            echo "Linked $TARGET to $SOURCE"
        fi
        echo "Restart Live, then choose VimtrackerBridge as a control surface in Settings, Link, Tempo & MIDI."
        ;;
    uninstall)
        if ours; then
            remove
            echo "Removed $TARGET"
        else
            echo "The bridge isn't installed at $TARGET"
        fi
        ;;
    status)
        if [ -L "$TARGET" ]; then
            echo "$TARGET links to $(readlink "$TARGET")"
        elif ours; then
            echo "$TARGET is a copy of the bridge"
        elif [ -e "$TARGET" ]; then
            echo "$TARGET is there but isn't the bridge's"
        else
            echo "The bridge isn't installed at $TARGET"
        fi
        ;;
    *)
        echo "usage: $0 [install | --copy | uninstall | status]" >&2
        exit 2
        ;;
esac
