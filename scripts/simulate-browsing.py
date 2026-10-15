#!/usr/bin/env python3
"""Drive a running Riff through searches and page visits, to exercise it under load.

For each genre it runs a search scoped to playlists and then one scoped to
artists. In each it scrolls the results, then opens the first 20 of them,
scrolling down every page before going back. Only cards in the search results
are opened.
Pair it with measure-resources.py to see how memory and CPU behave while pages
are opened and closed:

    scripts/measure-resources.py -d 300 -i 5 -o report.md &
    scripts/simulate-browsing.py --rounds 3
"""

import argparse
import sys
import time
import warnings

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi, Gio, GLib  # noqa: E402

GENRES = [
    "jazz",
    "rock",
    "hip hop",
    "classical",
    "electronic",
    "funk",
    "soul",
    "blues",
    "reggae",
    "country",
    "metal",
    "pop",
    "r&b",
    "folk",
    "latin",
    "punk",
    "disco",
    "ambient",
    "house",
    "indie",
]
# Headings of the search result sections, and the button that scopes the search to one
ARTISTS, PLAYLISTS = "Artists", "Playlists"
SHOW_ALL = "Show All"
CARD_ACTION = "card.activate"
RESULTS_TIMEOUT = 15
ATTEMPTS = 3
SCROLL_STEPS = 3
SCROLL_PAUSE = 0.4
# Time for the app to react to a navigation action
SETTLE = 0.5

# Atspi.Action.get_action_name is deprecated, but its replacement is not exposed to Python
warnings.simplefilter("ignore", DeprecationWarning)


def find_app(name):
    desktop = Atspi.get_desktop(0)
    for i in range(desktop.get_child_count()):
        app = desktop.get_child_at_index(i)
        if app and (app.get_name() or "").lower() == name:
            return app
    return None


def walk(node, root=True):
    """Yield every accessible below `node` that is currently on screen.

    Card grids are yielded but not entered: they hold hundreds of nodes, and
    every node costs several D-Bus round trips.
    """
    yield node
    try:
        if not root and (node.get_role_name() == "table" or not node.get_state_set().contains(Atspi.StateType.SHOWING)):
            return
        for i in range(node.get_child_count()):
            child = node.get_child_at_index(i)
            if child:
                yield from walk(child, False)
    except GLib.Error:
        # The widget went away while we were looking at it
        return


def do_action(node, name):
    actions = node.get_action_iface()
    if not actions:
        return False
    for i in range(actions.get_n_actions()):
        if actions.get_action_name(i) == name:
            return actions.do_action(i)
    return False


class Riff:
    def __init__(self, app_id, pause):
        self.app = find_app("riff")
        if not self.app:
            sys.exit("Riff is not running (or accessibility is disabled).")
        self.bus = Gio.bus_get_sync(Gio.BusType.SESSION)
        self.actions = Gio.DBusActionGroup.get(self.bus, app_id, "/" + app_id.replace(".", "/"))
        self.pause = pause
        self.pages_opened = 0

    def app_action(self, name):
        self.actions.activate_action(name, None)
        self.bus.flush_sync()
        time.sleep(SETTLE)

    def sections(self):
        """{heading: [cards]} for each card grid on screen; the heading is None for untitled grids."""
        sections, heading = {}, None
        for node in walk(self.app):
            role = node.get_role_name()
            if role == "label" and node.get_name() in (ARTISTS, PLAYLISTS):
                heading = node.get_name()
            elif role == "table":
                cells = [node.get_child_at_index(i) for i in range(node.get_child_count())]
                # Placeholder cards have no name yet
                sections.setdefault(heading, []).extend(c for c in cells if c and c.get_name())
        return sections

    def search_entry(self):
        """The search page's text entry, or None when another page is showing."""
        return next((n for n in walk(self.app) if n.get_role_name() == "entry" and n.get_editable_text_iface()), None)

    def wait_for_cards(self, heading):
        """Wait until the cards under `heading` have loaded and stopped changing."""
        deadline = time.monotonic() + RESULTS_TIMEOUT
        previous = None
        while time.monotonic() < deadline:
            time.sleep(1)
            names = [c.get_name() for c in self.sections().get(heading, [])]
            if names and names == previous:
                return True
            previous = names
        return False

    def search(self, query, scope):
        """Search for `query`, then narrow the results to the `scope` section."""
        # Start from the home screen so the navigation stack doesn't keep growing
        self.app_action("nav_albums")
        self.app_action("search")
        entry = self.search_entry()
        if not entry:
            sys.exit("Could not find the search entry.")
        entry.get_editable_text_iface().set_text_contents(query)
        if not self.wait_for_cards(scope):
            return False

        # Each section heading is followed by its "Show All" button
        after_heading = False
        for node in walk(self.app):
            role = node.get_role_name()
            if role == "label" and node.get_name() == scope:
                after_heading = True
            elif after_heading and role == "button" and node.get_name() == SHOW_ALL:
                do_action(node, "click")
                return self.wait_for_cards(None)
        return False

    def scroll_down(self):
        """Scroll the current page to the bottom, a few times so more content can load."""
        bars = [n for n in walk(self.app) if n.get_role_name() == "scroll bar" and n.get_value_iface()]
        if not bars:
            return
        # The page's scrollbar is the rightmost one; the other belongs to the sidebar
        bar = max(bars, key=lambda n: n.get_extents(Atspi.CoordType.WINDOW).x).get_value_iface()
        for _ in range(SCROLL_STEPS):
            try:
                bar.set_current_value(bar.get_maximum_value())
            except GLib.Error:
                return
            time.sleep(SCROLL_PAUSE)

    def open_card(self, index):
        """Open the search result at `index` and scroll the page it leads to.

        Returns the result's name, or None if there is no such result.
        """
        if not self.search_entry():
            print("  no longer on the search results, moving on")
            return None
        for _ in range(ATTEMPTS):
            cards = self.sections().get(None, [])
            if index >= len(cards):
                return None
            name = cards[index].get_name()
            # The action lives on the card widget inside the grid cell
            if any(do_action(node, CARD_ACTION) for node in walk(cards[index])):
                self.pages_opened += 1
                time.sleep(self.pause)
                self.scroll_down()
                return name
            # The grid may have been rebuilt under us; look again
            time.sleep(1)
        sys.exit(f"Card '{name}' has no '{CARD_ACTION}' action; this Riff build is too old.")

    def back(self):
        self.app_action("nav_pop")


def browse(riff, genre, count):
    for scope in (PLAYLISTS, ARTISTS):
        print(f"Searching {scope.lower()} for '{genre}'")
        if not riff.search(genre, scope):
            print("  no results, skipping")
            continue
        riff.scroll_down()
        for i in range(count):
            name = riff.open_card(i)
            if name is None:
                break
            print(f"  {name}")
            riff.back()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("-n", "--count", type=int, default=20, help="results to open per search (default: 20)")
    parser.add_argument("-p", "--pause", type=float, default=1, help="seconds to wait on each page before scrolling (default: 1)")
    parser.add_argument("-r", "--rounds", type=int, default=1, help="times to repeat the whole run (default: 1)")
    parser.add_argument("--app-id", default="dev.diegovsky.Riff", help="application id (default: dev.diegovsky.Riff)")
    args = parser.parse_args()

    riff = Riff(args.app_id, args.pause)
    start = time.monotonic()
    try:
        for round_number in range(args.rounds):
            if args.rounds > 1:
                print(f"Round {round_number + 1} of {args.rounds}")
            for genre in GENRES:
                browse(riff, genre, args.count)
        riff.app_action("nav_albums")
    except KeyboardInterrupt:
        pass
    print(f"Opened {riff.pages_opened} pages in {time.monotonic() - start:.0f}s.")


if __name__ == "__main__":
    main()
