#!/usr/bin/env python3
"""Resize a running Riff's window back and forth, to profile how it handles resizing."""

import argparse
import math
import sys
import time
import warnings

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi, Gio, GLib  # noqa: E402

CARD_ACTION = "card.activate"
SETTLE = 1
PAGE_LOAD = 3

warnings.simplefilter("ignore", DeprecationWarning)


def size(text):
    width, _, height = text.partition("x")
    try:
        return int(width), int(height)
    except ValueError:
        raise argparse.ArgumentTypeError(f"expected WIDTHxHEIGHT, got '{text}'") from None


def find_app():
    desktop = Atspi.get_desktop(0)
    for i in range(desktop.get_child_count()):
        app = desktop.get_child_at_index(i)
        if app and (app.get_name() or "").lower() == "riff":
            return app
    return None


def window_size():
    app = find_app()
    for j in range(app.get_child_count() if app else 0):
        window = app.get_child_at_index(j)
        if window and window.get_role_name() == "frame":
            extents = window.get_extents(Atspi.CoordType.WINDOW)
            return extents.width, extents.height
    return None


def walk(node, root=True):
    yield node
    try:
        if not root and (node.get_role_name() == "table" or not node.get_state_set().contains(Atspi.StateType.SHOWING)):
            return
        for i in range(node.get_child_count()):
            child = node.get_child_at_index(i)
            if child:
                yield from walk(child, False)
    except GLib.Error:
        return


def do_action(node, name):
    actions = node.get_action_iface()
    if not actions:
        return False
    for i in range(actions.get_n_actions()):
        if actions.get_action_name(i) == name:
            # The reply doesn't reliably say whether the action ran, so don't trust it
            actions.do_action(i)
            return True
    return False


def sweep(low, high, phase):
    return round(low + (high - low) * (1 - math.cos(2 * math.pi * phase)) / 2)


class Riff:
    def __init__(self, app_id):
        self.bus = Gio.bus_get_sync(Gio.BusType.SESSION)
        self.app_id = app_id
        self.path = path = "/" + app_id.replace(".", "/")
        self.actions = Gio.DBusActionGroup.get(self.bus, app_id, path)
        try:
            (names,) = self.bus.call_sync(app_id, path, "org.gtk.Actions", "List", None,
                                          GLib.VariantType("(as)"), Gio.DBusCallFlags.NONE, 2000, None).unpack()
        except GLib.Error:
            sys.exit("Riff is not running.")
        if "resize_window" not in names:
            sys.exit("Riff has no 'resize_window' action; this Riff build is too old.")

    def show(self):
        self.bus.call_sync(self.app_id, self.path, "org.freedesktop.Application", "Activate",
                           GLib.Variant("(a{sv})", ({},)), None, Gio.DBusCallFlags.NONE, 2000, None)
        time.sleep(SETTLE)

    def open_artist(self):
        self.actions.activate_action("nav_artists", None)
        self.bus.flush_sync()
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            time.sleep(SETTLE)
            app = find_app()
            grid = next((n for n in walk(app) if n.get_role_name() == "table"), None) if app else None
            cells = [grid.get_child_at_index(i) for i in range(min(grid.get_child_count(), 5))] if grid else []
            card = next((c for c in cells if c and c.get_name()), None)
            name = card.get_name() if card else None
            if card and any(do_action(node, CARD_ACTION) for node in walk(card)):
                time.sleep(PAGE_LOAD)
                return name
        return None

    def page_scroll_bar(self):
        app = find_app()
        bars = [n for n in walk(app) if n.get_role_name() == "scroll bar" and n.get_value_iface()] if app else []
        if not bars:
            return None
        return max(bars, key=lambda n: n.get_extents(Atspi.CoordType.WINDOW).x).get_value_iface()

    def resize(self, width, height):
        self.actions.activate_action("resize_window", GLib.Variant("(ii)", (width, height)))
        self.bus.flush_sync()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("-d", "--duration", type=float, default=20, help="seconds to keep resizing (default: 20)")
    parser.add_argument("-r", "--rate", type=float, default=60, help="resizes per second (default: 60)")
    parser.add_argument("-p", "--period", type=float, default=1.5,
                        help="seconds for one sweep from small to large and back (default: 1.5)")
    parser.add_argument("--min", type=size, default=(400, 500), metavar="WxH", help="smallest size (default: 400x500)")
    parser.add_argument("--max", type=size, default=(1600, 1000), metavar="WxH", help="largest size (default: 1600x1000)")
    parser.add_argument("-s", "--scroll-period", type=float, default=4,
                        help="seconds for one scroll down the page and back up, or 0 to not scroll (default: 4)")
    parser.add_argument("-a", "--axis", choices=("both", "width", "height"), default="both",
                        help="which dimension to change (default: both)")
    parser.add_argument("--page", choices=("artist", "current"), default="artist",
                        help="page to resize on: the first saved artist, or whatever is showing (default: artist)")
    parser.add_argument("--app-id", default="dev.diegovsky.Riff", help="application id (default: dev.diegovsky.Riff)")
    args = parser.parse_args()

    riff = Riff(args.app_id)
    riff.show()
    if args.page == "artist":
        artist = riff.open_artist()
        if not artist:
            sys.exit("Could not open a saved artist (is accessibility disabled, or are there none?).")
        print(f"Resizing on the page for {artist}")
    original = window_size()
    if original is None:
        print("Could not read the window size (is accessibility disabled?); it won't be restored.")
    fixed = original or args.max
    scroll_bar = riff.page_scroll_bar() if args.scroll_period > 0 else None
    if args.scroll_period > 0 and not scroll_bar:
        print("Could not find the page's scroll bar; it won't be scrolled.")

    interval = 1 / args.rate
    resizes = 0
    start = time.monotonic()
    try:
        while (elapsed := time.monotonic() - start) < args.duration:
            phase = (elapsed / args.period) % 1
            width = sweep(args.min[0], args.max[0], phase) if args.axis != "height" else fixed[0]
            height = sweep(args.min[1], args.max[1], phase) if args.axis != "width" else fixed[1]
            riff.resize(width, height)
            if scroll_bar:
                try:
                    top = sweep(0, round(scroll_bar.get_maximum_value()), (elapsed / args.scroll_period) % 1)
                    scroll_bar.set_current_value(top)
                except GLib.Error:
                    scroll_bar = None
            resizes += 1
            time.sleep(max(0, start + resizes * interval - time.monotonic()))
    except KeyboardInterrupt:
        pass
    elapsed = time.monotonic() - start

    if original:
        riff.resize(*original)
    print(f"Sent {resizes} resizes in {elapsed:.1f}s ({resizes / elapsed:.0f}/s).")


if __name__ == "__main__":
    main()
