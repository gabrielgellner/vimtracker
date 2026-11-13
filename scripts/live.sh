#!/bin/sh
# Drive vimtracker in a kitty window of its own, minimised, through kitty's remote control, to
# check a change in the real program: what it draws and how keys reach it.
#
#   scripts/live.sh start [args]   start it in a fresh folder, with these arguments
#   scripts/live.sh keys a shift+semicolon escape ...   press keys, by kitty's names
#   scripts/live.sh text ' 5'  type text as it is, with \r for Enter
#   scripts/live.sh screen          print what's on screen
#   scripts/live.sh shot out.png    save a picture of its window, kitty's images and all
#   scripts/live.sh stop            close it
#
# Each key is pressed and let go, and Option is Alt, as the program's Option keys need. The
# socket is VT_LIVE_SOCKET, /tmp/vimtracker-live.sock by default; a Unix socket's path must stay
# short. The folder is VT_LIVE_DIR, or a fresh one in the system's temporary folder. It uses the
# release build, so run `cargo build --release` first.
#
# VT_LIVE_SHOW=1 opens the window where it can be seen, at the 163x42 the program is laid out
# for, rather than minimised, as `shot` needs: the system only pictures a window on screen.
# `shot` takes that window alone, never the rest of the screen. Set XDG_CONFIG_HOME and
# XDG_CACHE_HOME to folders of the check's own first, so it leaves the real ones alone.
set -eu

kitty=${KITTY:-/Applications/kitty.app/Contents/MacOS/kitty}
kitten=$(dirname "$kitty")/kitten
socket=${VT_LIVE_SOCKET:-/tmp/vimtracker-live.sock}
root=$(cd "$(dirname "$0")/.." && pwd)
remote() { "$kitten" @ --to "unix:$socket" "$@"; }

case ${1:-} in
start)
    shift
    [ -S "$socket" ] && remote close-window --match title:vimtracker-live 2>/dev/null || true
    rm -f "$socket"
    folder=${VT_LIVE_DIR:-$(mktemp -d)}
    if [ "${VT_LIVE_SHOW:-}" = 1 ]; then
        width=163c height=42c start_as=normal
    else
        width=120c height=30c start_as=minimized
    fi
    "$kitty" --config NONE -o allow_remote_control=socket-only --listen-on "unix:$socket" \
        -o macos_option_as_alt=yes \
        -o macos_quit_when_last_window_closed=yes -o initial_window_width="$width" \
        -o initial_window_height="$height" -o remember_window_size=no --start-as="$start_as" \
        --title vimtracker-live --directory "$folder" \
        "$root/target/release/vimtracker" "$@" >/dev/null 2>&1 &
    tries=0
    until [ -S "$socket" ] && remote ls >/dev/null 2>&1; do
        tries=$((tries + 1))
        [ $tries -gt 50 ] && echo "kitty didn't start" >&2 && exit 1
        sleep 0.1
    done
    sleep 1
    echo "started in $folder"
    ;;
keys)
    shift
    for key in "$@"; do
        remote send-key "$key"
        sleep 0.15
    done
    ;;
text)
    shift
    remote send-text "$*"
    sleep 0.3
    ;;
screen)
    remote get-text
    ;;
shot)
    [ -n "${2:-}" ] || { echo "say where to save it: scripts/live.sh shot out.png" >&2; exit 1; }
    id=$(remote ls | /usr/bin/python3 -c 'import json, sys; print(json.load(sys.stdin)[0]["platform_window_id"])')
    screencapture -x -o -l "$id" "$2"
    echo "saved $2"
    ;;
stop)
    remote close-window 2>/dev/null || true
    rm -f "$socket"
    ;;
*)
    sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
    exit 1
    ;;
esac
