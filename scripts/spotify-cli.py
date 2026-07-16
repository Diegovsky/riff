#!/usr/bin/env python3
"""spotify-cli: Authenticated Spotify Web API client for the terminal.

Shares credentials with the Riff desktop app via the system keyring.
Supports OAuth2 PKCE authentication, token refresh, interactive pagination,
and colorized JSON output.

Usage:
    spotify-cli v1/me
    spotify-cli v1/me/playlists
    spotify-cli -X PUT v1/me/player/play -d '{"uris":["spotify:track:..."]}'
    spotify-cli --setup-completions
    spotify-cli --logout
    spotify-cli --check-auth

Full URLs also work:
    spotify-cli https://api.spotify.com/v1/me

Tab completion:
    Run `spotify-cli --setup-completions` to enable tab completion for
    API endpoints in your shell. Supports bash and zsh.

Dependencies:
    If you want to use pip:
        pip install .
    With uv:
        uv sync
"""

import textwrap

import argparse
import base64
import hashlib
import json
import os
import secrets
import sys
import time
import webbrowser
from http.server import BaseHTTPRequestHandler, HTTPServer
from threading import Thread
from urllib.parse import (
    parse_qs,
    urlparse,
    urlsplit,
    urljoin,
    urlunsplit,
    SplitResult,
    urlencode,
)

import secretstorage
import requests
from rich.console import Console
from rich.json import JSON as RichJSON


HAS_RICH = True
console = Console(stderr=True)

# --- Constants (shared with Riff) ---

CLIENT_ID = "782ae96ea60f4cdf986a766049607005"
REDIRECT_URI = "http://127.0.0.1:8898/login"
AUTH_URL = "https://accounts.spotify.com/authorize"
TOKEN_URL = "https://accounts.spotify.com/api/token"
SCOPES = (
    "user-read-private,"
    "playlist-read-private,"
    "playlist-read-collaborative,"
    "user-library-read,"
    "user-library-modify,"
    "user-follow-read,"
    "user-follow-modify,"
    "user-top-read,"
    "user-read-recently-played,"
    "user-read-playback-state,"
    "playlist-modify-public,"
    "playlist-modify-private,"
    "user-modify-playback-state"
)

# Keyring attributes matching Riff's token_store.rs: ATTRS = [("spot_credentials", "yes")]
KEYRING_ATTRS = {"spot_credentials": "yes"}

# Default page size for paginated requests
DEFAULT_PAGE_LIMIT = 2

# OpenAPI spec for endpoint discovery
OPENAPI_SPEC_URL = "https://raw.githubusercontent.com/sonallux/spotify-web-api/main/official-spotify-open-api.yml"
CACHE_DIR = os.path.join(
    os.environ.get("XDG_CACHE_HOME", os.path.expanduser("~/.cache")), "spotify-cli"
)
ENDPOINTS_CACHE = os.path.join(CACHE_DIR, "endpoints.json")
# Cache for 24 hours
ENDPOINTS_CACHE_TTL = 24 * 3600


# --- Endpoint Discovery (OpenAPI-based tab completion) ---


def _ensure_cache_dir():
    os.makedirs(CACHE_DIR, exist_ok=True)


def _endpoints_cache_valid():
    """Check if the endpoints cache exists and is fresh."""
    if not os.path.exists(ENDPOINTS_CACHE):
        return False
    age = time.time() - os.path.getmtime(ENDPOINTS_CACHE)
    return age < ENDPOINTS_CACHE_TTL


def fetch_endpoints(quiet=False):
    """Fetch endpoint paths from the Spotify OpenAPI spec and cache them."""
    import urllib.request

    _ensure_cache_dir()

    if _endpoints_cache_valid():
        with open(ENDPOINTS_CACHE) as f:
            return json.load(f)

    if not quiet:
        print("Fetching Spotify API endpoints from OpenAPI spec...", file=sys.stderr)
    try:
        # We only need the paths section, so we do a lightweight YAML parse
        # Since PyYAML may not be installed, we'll do a simple regex extraction
        req = urllib.request.Request(OPENAPI_SPEC_URL)
        with urllib.request.urlopen(req, timeout=15) as resp:
            content = resp.read().decode("utf-8")

        # Extract paths — they appear as top-level keys under "paths:" in the YAML
        # Format: "  /path/to/endpoint:" at 2-space indent under paths
        import re

        endpoints = []
        in_paths = False
        for line in content.split("\n"):
            if line.strip() == "paths:":
                in_paths = True
                continue
            if in_paths:
                # A path entry is indented exactly 2 spaces and starts with /
                match = re.match(r"^  (/[^:]+):", line)
                if match:
                    path = match.group(1)
                    endpoints.append(path)
                # If we hit a top-level key (no indent), we're past paths
                elif line and not line.startswith(" ") and ":" in line:
                    break

        # Build a structured list with method info
        # For now, just store the paths prefixed with v1
        endpoint_list = []
        for ep in endpoints:
            # The OpenAPI paths don't include /v1 prefix (the server base URL has it)
            full_path = f"v1{ep}"
            endpoint_list.append(full_path)

        # Save cache
        with open(ENDPOINTS_CACHE, "w") as f:
            json.dump(endpoint_list, f, indent=2)

        # Also cache the full spec for endpoint help lookups
        spec_cache = os.path.join(CACHE_DIR, "openapi-spec.yml")
        with open(spec_cache, "w") as f:
            f.write(content)

        if not quiet:
            print(f"Cached {len(endpoint_list)} endpoints.", file=sys.stderr)
        return endpoint_list

    except Exception as e:
        if not quiet:
            print(f"Warning: Could not fetch OpenAPI spec: {e}", file=sys.stderr)
        # Return a basic fallback list
        return _fallback_endpoints()


def get_endpoint_help(path):
    """Parse the cached OpenAPI spec and display help for a given endpoint path."""
    import re

    spec_cache = os.path.join(CACHE_DIR, "openapi-spec.yml")
    if not os.path.exists(spec_cache):
        fetch_endpoints(quiet=True)
    if not os.path.exists(spec_cache):
        print(
            "Could not load API spec. Try again after running a request.",
            file=sys.stderr,
        )
        return

    with open(spec_cache) as f:
        lines = f.read().split("\n")

    # Normalize the path to OpenAPI format (strip v1 prefix)
    api_path = path.lstrip("/")
    if api_path.startswith("v1"):
        api_path = api_path[2:]
    if not api_path.startswith("/"):
        api_path = "/" + api_path

    # Find the path section in the YAML
    path_line_idx = _find_path_in_spec(api_path, lines, path)
    if path_line_idx is None:
        print(f"No documentation found for: {path}", file=sys.stderr)
        print(f"Use --list-endpoints to see available endpoints.", file=sys.stderr)
        return

    # Extract the endpoint block until next path entry
    block_lines = []
    for i in range(path_line_idx + 1, len(lines)):
        line = lines[i]
        if re.match(r"^  /[^:]+:", line) or (
            line and not line.startswith(" ") and ":" in line
        ):
            break
        block_lines.append(line)

    # Parse methods from the block
    methods = _parse_methods_from_block(block_lines)

    # Display
    print(f"\n  Endpoint: v1{api_path}", file=sys.stderr)
    print(f"  URL:      https://api.spotify.com/v1{api_path}", file=sys.stderr)
    print(file=sys.stderr)

    for m in methods:
        dep_str = " [DEPRECATED]" if m["deprecated"] else ""
        print(f"  {m['method']}{dep_str}", file=sys.stderr)
        if m["summary"]:
            print(f"    {m['summary']}", file=sys.stderr)
        if m["description"]:
            for dl in m["description"].split("\n")[:3]:
                print(f"    {dl}", file=sys.stderr)
        if m["parameters"]:
            print(f"\n    Parameters:", file=sys.stderr)
            for p in m["parameters"]:
                req_str = " (required)" if p.get("required") else ""
                print(f"      {p['name']} [{p['in']}]{req_str}", file=sys.stderr)
        print(file=sys.stderr)


def _find_path_in_spec(api_path, lines, original_path):
    """Find a path's line index in the OpenAPI spec lines."""
    import re

    for i, line in enumerate(lines):
        if re.match(rf"^  {re.escape(api_path)}:", line):
            return i

    # Try matching with path params (user might have passed actual IDs)
    endpoint_list = fetch_endpoints(quiet=True)
    matched = _match_endpoint_pattern(original_path, endpoint_list)
    if matched:
        api_path = matched[2:]  # strip "v1"
        if not api_path.startswith("/"):
            api_path = "/" + api_path
        for i, line in enumerate(lines):
            if re.match(rf"^  {re.escape(api_path)}:", line):
                return i
    return None


def _parse_methods_from_block(block_lines):
    """Parse HTTP methods, summaries, descriptions, and parameters from a YAML block."""
    import re

    methods = []
    current = None

    for line in block_lines:
        method_match = re.match(r"^    (get|post|put|delete|patch):", line)
        if method_match:
            if current:
                methods.append(current)
            current = {
                "method": method_match.group(1).upper(),
                "summary": None,
                "description": "",
                "parameters": [],
                "deprecated": False,
                "_in_desc": False,
                "_in_params": False,
            }
            continue

        if not current:
            continue

        if re.match(r"^\s+deprecated:\s*true", line):
            current["deprecated"] = True
        elif m := re.match(r"^\s+summary:\s*\|?\s*(.+)", line):
            current["summary"] = m.group(1).strip()
        elif re.match(r"^\s+description:\s*\|", line):
            current["_in_desc"] = True
            current["_in_params"] = False
        elif current["_in_desc"]:
            if re.match(r"^\s{8,}", line):
                current["description"] += line.strip() + "\n"
            else:
                current["_in_desc"] = False
        elif re.match(r"^\s+parameters:", line):
            current["_in_params"] = True
            current["_in_desc"] = False
        elif current["_in_params"]:
            if m := re.match(r"^\s+- name:\s*(.+)", line):
                current["parameters"].append(
                    {"name": m.group(1).strip(), "in": "query", "required": False}
                )
            elif m := re.match(
                r"^\s+- \$ref:\s*['\"]#/components/parameters/(\w+)['\"]", line
            ):
                current["parameters"].append(
                    {
                        "name": _ref_to_param_name(m.group(1)),
                        "in": "query",
                        "required": False,
                    }
                )
            elif (m := re.match(r"^\s+required:\s*(true|false)", line)) and current[
                "parameters"
            ]:
                current["parameters"][-1]["required"] = m.group(1) == "true"
            elif (m := re.match(r"^\s+in:\s*(\w+)", line)) and current["parameters"]:
                current["parameters"][-1]["in"] = m.group(1)

    if current:
        methods.append(current)

    # Clean up internal state keys
    for m in methods:
        m.pop("_in_desc", None)
        m.pop("_in_params", None)
        m["description"] = m["description"].strip()

    return methods


def _match_endpoint_pattern(user_path, endpoint_list):
    """Match a user-provided path (with real IDs) against templated endpoints."""
    user_path = user_path.lstrip("/")
    if not user_path.startswith("v1"):
        user_path = "v1/" + user_path
    user_parts = user_path.split("/")

    for ep in endpoint_list:
        ep_parts = ep.split("/")
        if len(ep_parts) != len(user_parts):
            continue
        if all(
            ep_p.startswith("{") or up == ep_p for up, ep_p in zip(user_parts, ep_parts)
        ):
            return ep
    return None


def _ref_to_param_name(ref_name):
    """Convert a parameter $ref name like 'QueryMarket' to 'market'."""
    import re

    # Strip prefix (Path, Query)
    name = re.sub(r"^(Query|Path)", "", ref_name)
    # CamelCase to lowercase with hyphens
    name = re.sub(r"([A-Z])", r"-\1", name).strip("-").lower()
    return name


def _fallback_endpoints():
    """Minimal hardcoded endpoint list as fallback."""
    return [
        "v1/me",
        "v1/me/playlists",
        "v1/me/albums",
        "v1/me/tracks",
        "v1/me/following",
        "v1/me/top/artists",
        "v1/me/top/tracks",
        "v1/me/player",
        "v1/me/player/play",
        "v1/me/player/pause",
        "v1/me/player/next",
        "v1/me/player/previous",
        "v1/me/player/shuffle",
        "v1/me/player/repeat",
        "v1/me/player/volume",
        "v1/me/player/queue",
        "v1/me/player/recently-played",
        "v1/me/player/currently-playing",
        "v1/me/player/devices",
        "v1/albums/{id}",
        "v1/albums/{id}/tracks",
        "v1/artists/{id}",
        "v1/artists/{id}/albums",
        "v1/artists/{id}/top-tracks",
        "v1/artists/{id}/related-artists",
        "v1/playlists/{playlist_id}",
        "v1/playlists/{playlist_id}/tracks",
        "v1/tracks/{id}",
        "v1/search",
        "v1/browse/new-releases",
        "v1/browse/categories",
        "v1/recommendations",
        "v1/users/{user_id}",
        "v1/users/{user_id}/playlists",
    ]


def get_completions_for(prefix):
    """Return endpoint completions matching a prefix."""
    endpoints = fetch_endpoints(quiet=True)
    prefix = prefix.lstrip("/")
    return [ep for ep in endpoints if ep.startswith(prefix)]


# --- Shell Completion ---


def generate_bash_completion():
    """Generate a bash completion script for spotify-cli."""
    script_path = os.path.abspath(__file__)
    script_name = os.path.basename(__file__)
    # Build a deduplicated list of names to register completion for
    names = list(
        dict.fromkeys(
            [
                "spotify-cli",
                script_name,
                script_path,
                f"./{script_name}",
                f"./scripts/{script_name}",
                f"scripts/{script_name}",
            ]
        )
    )
    complete_lines = "\n".join(
        f"complete -o nospace -F _spotify_cli_completions {name}" for name in names
    )
    return f'''# Bash completion for spotify-cli
# Source this file or add to ~/.bashrc:
#   eval "$("{script_path}" --completions bash)"

_spotify_cli_completions() {{
    local cur prev opts
    COMPREPLY=()
    cur="${{COMP_WORDS[COMP_CWORD]}}"
    prev="${{COMP_WORDS[COMP_CWORD-1]}}"

    # Options
    opts="--help --method --data --raw --verbose --logout --check-auth --list-endpoints --setup-completions -X -d -v -h"

    # If completing after -X, suggest methods
    if [[ "$prev" == "-X" || "$prev" == "--method" ]]; then
        COMPREPLY=( $(compgen -W "GET POST PUT DELETE PATCH" -- "$cur") )
        return 0
    fi

    # If current word starts with -, complete options
    if [[ "$cur" == -* ]]; then
        COMPREPLY=( $(compgen -W "$opts" -- "$cur") )
        return 0
    fi

    # Otherwise, complete API endpoints
    local endpoints
    endpoints=$("{script_path}" --list-endpoints "$cur" 2>/dev/null)
    COMPREPLY=( $(compgen -W "$endpoints" -- "$cur") )
    return 0
}}

{complete_lines}
'''


def generate_zsh_completion():
    """Generate a zsh completion script for spotify-cli."""
    script_path = os.path.abspath(__file__)
    return f'''#compdef spotify-cli
# Zsh completion for spotify-cli
# Add to fpath or source directly: source <(spotify-cli --completions zsh)

_spotify_cli() {{
    local -a endpoints
    local cur="${{words[CURRENT]}}"

    # Options
    _arguments -s \\
        '(-h --help){{-h,--help}}[Show help]' \\
        '(-X --method){{-X,--method}}[HTTP method]:method:(GET POST PUT DELETE PATCH)' \\
        '(-d --data){{-d,--data}}[Request body]:data:' \\
        '--raw[Raw JSON output]' \\
        '(-v --verbose){{-v,--verbose}}[Verbose output]' \\
        '--logout[Clear credentials]' \\
        '--check-auth[Check auth status]' \\
        '--list-endpoints[List matching endpoints]' \\
        '--completions[Generate completion script]:shell:(bash zsh)' \\
        '*:endpoint:_spotify_cli_endpoints'
}}

_spotify_cli_endpoints() {{
    local -a endpoints
    endpoints=(${{(f)"$("{script_path}" --list-endpoints "$cur" 2>/dev/null)"}})
    compadd -S '' -- $endpoints
}}

_spotify_cli "$@"
'''


def setup_shell_completions():
    """Detect user's shell and interactively install tab completion."""
    import subprocess

    script_path = os.path.abspath(__file__)

    # Detect shell
    shell = os.environ.get("SHELL", "")
    shell_name = os.path.basename(shell)

    if shell_name not in ("bash", "zsh"):
        # Try to detect from parent process
        try:
            ppid = os.getppid()
            result = subprocess.run(
                ["ps", "-p", str(ppid), "-o", "comm="], capture_output=True, text=True
            )
            detected = result.stdout.strip()
            if detected in ("bash", "zsh"):
                shell_name = detected
        except Exception:
            pass

    if shell_name not in ("bash", "zsh"):
        print(
            f"Could not detect your shell (got: {shell_name or 'unknown'}).",
            file=sys.stderr,
        )
        print("Supported shells: bash, zsh", file=sys.stderr)
        print(f"\nYou can manually generate a completion script with:", file=sys.stderr)
        print(f"  {script_path} --completions bash", file=sys.stderr)
        print(f"  {script_path} --completions zsh", file=sys.stderr)
        sys.exit(1)

    print(f"Detected shell: {shell_name}", file=sys.stderr)

    # Determine target file and content
    if shell_name == "bash":
        completion_script = generate_bash_completion()
        # Check for common bash completion dirs
        xdg_data = os.environ.get("XDG_DATA_HOME", os.path.expanduser("~/.local/share"))
        completion_dir = os.path.join(xdg_data, "bash-completion", "completions")
        completion_file = os.path.join(completion_dir, "spotify-cli")
        rc_file = os.path.expanduser("~/.bashrc")
        source_line = f'eval "$("{script_path}" --completions bash)"'
        # Prefer the completions directory if it exists or can be created
        use_dir = os.path.isdir(os.path.dirname(completion_dir)) or os.path.isdir(
            completion_dir
        )

        if use_dir:
            if not _install_completion_file(
                shell_name, completion_script, completion_file, completion_dir
            ):
                return
        else:
            if not _install_completion_rc(shell_name, source_line, rc_file):
                return

    elif shell_name == "zsh":
        completion_script = generate_zsh_completion()
        # Common zsh completion locations
        xdg_data = os.environ.get("XDG_DATA_HOME", os.path.expanduser("~/.local/share"))
        completion_dir = os.path.join(xdg_data, "zsh", "site-functions")
        completion_file = os.path.join(completion_dir, "_spotify-cli")
        rc_file = os.path.expanduser("~/.zshrc")
        source_line = f'eval "$("{script_path}" --completions zsh)"'

        # Check if any custom fpath exists
        if os.path.isdir(completion_dir):
            if not _install_completion_file(
                shell_name, completion_script, completion_file, completion_dir
            ):
                return
        else:
            # Offer both options
            print(f"\nI can set up completions in one of two ways:", file=sys.stderr)
            print(
                f"  1. Create {completion_dir}/ and add it to fpath (recommended)",
                file=sys.stderr,
            )
            print(f"  2. Add a source line to {rc_file}", file=sys.stderr)
            try:
                choice = input("\nWhich option? [1/2] ").strip()
            except (EOFError, KeyboardInterrupt):
                print("\nAborted.", file=sys.stderr)
                return

            if choice == "1":
                # Also need to add fpath to .zshrc
                fpath_line = f'fpath=("{completion_dir}" $fpath)'
                if not _install_completion_file(
                    shell_name, completion_script, completion_file, completion_dir
                ):
                    return
                _add_line_to_rc(rc_file, fpath_line, "fpath addition")
            else:
                if not _install_completion_rc(shell_name, source_line, rc_file):
                    return

    # Pre-fetch endpoints so first tab press is fast
    print("\nPre-caching API endpoints...", file=sys.stderr)
    endpoints = fetch_endpoints()
    print(
        f"Done! {len(endpoints)} endpoints available for completion.", file=sys.stderr
    )
    print(f"\nRestart your shell or run: exec {shell_name}", file=sys.stderr)


def _install_completion_file(shell_name, content, filepath, dirpath):
    """Install a completion script as a file, with user approval. Returns True if installed."""
    print(
        textwrap.dedent(f"""\nI will:
      • Create directory: {dirpath}/ (if needed)
      • Write completion script to: {filepath}"""),
        file=sys.stderr,
    )

    try:
        answer = input("\nProceed? [Y/n] ")
    except (EOFError, KeyboardInterrupt):
        print("\nAborted.", file=sys.stderr)
        return False

    if answer.strip().lower() in ("n", "no"):
        print("Aborted.", file=sys.stderr)
        return False

    os.makedirs(dirpath, exist_ok=True)
    with open(filepath, "w") as f:
        f.write(content)

    print(f"✓ Written: {filepath}", file=sys.stderr)
    return True


def _install_completion_rc(shell_name, source_line, rc_file):
    """Add a source/eval line to the shell's RC file, with user approval. Returns True if installed."""
    # Check if already present
    if os.path.exists(rc_file):
        with open(rc_file) as f:
            if source_line in f.read():
                print(f"✓ Completion already configured in {rc_file}", file=sys.stderr)
                return True

    print(f"\nI will append this line to {rc_file}:", file=sys.stderr)
    print(f"  {source_line}", file=sys.stderr)

    try:
        answer = input("\nProceed? [Y/n] ")
    except (EOFError, KeyboardInterrupt):
        print("\nAborted.", file=sys.stderr)
        return False

    if answer.strip().lower() in ("n", "no"):
        print("Aborted.", file=sys.stderr)
        return False

    _add_line_to_rc(rc_file, source_line, "completion")
    return True


def _add_line_to_rc(rc_file, line, description):
    """Append a line to a shell RC file."""
    # Ensure file ends with newline before appending
    needs_newline = False
    if os.path.exists(rc_file):
        with open(rc_file, "rb") as f:
            f.seek(0, 2)  # end
            if f.tell() > 0:
                f.seek(-1, 2)
                needs_newline = f.read(1) != b"\n"

    with open(rc_file, "a") as f:
        if needs_newline:
            f.write("\n")
        f.write(f"\n# spotify-cli {description}\n")
        f.write(f"{line}\n")

    print(f"✓ Added to {rc_file}", file=sys.stderr)


# --- Argument Parsing ---


def parse_args():
    parser = argparse.ArgumentParser(
        prog="spotify-cli",
        description="Authenticated Spotify Web API client for the terminal.",
        epilog=(
            "Examples:\n"
            "  %(prog)s v1/me\n"
            "  %(prog)s v1/me/playlists\n"
            "  %(prog)s -X PUT v1/me/player/play "
            '-d \'{"uris":["spotify:track:..."]}\'\n'
            "  %(prog)s -v v1/artists/0OdUWJ0sBjDrqHygGUXeCF\n"
            "  %(prog)s --setup-completions   # enable tab completion\n"
            "  %(prog)s --logout\n"
            "  %(prog)s --check-auth\n"
            "\n"
            "Dependencies:\n"
            "  pip install secretstorage requests rich\n"
            "\n"
            "  Required:  requests, secretstorage\n"
            "  Optional:  rich (colorized JSON output)\n"
        ),
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "url",
        nargs="?",
        help="Spotify API path (e.g. v1/me/playlists) or full URL",
    )
    parser.add_argument(
        "-X", "--method", default="GET", help="HTTP method (default: GET)"
    )
    parser.add_argument(
        "-p",
        "--param",
        help="Add a search param to the URL",
        action="append",
        default=[],
        dest="params",
    )
    parser.add_argument("-d", "--data", help="Request body (JSON string)")
    parser.add_argument(
        "--raw", action="store_true", help="Output raw JSON without syntax highlighting"
    )
    parser.add_argument(
        "-v", "--verbose", action="store_true", help="Show request/response headers"
    )
    parser.add_argument(
        "--logout", action="store_true", help="Clear stored credentials from keyring"
    )
    parser.add_argument(
        "--check-auth",
        action="store_true",
        help="Check if valid credentials exist in keyring",
    )
    parser.add_argument(
        "--completions",
        metavar="SHELL",
        choices=["bash", "zsh"],
        help=argparse.SUPPRESS,  # Hidden, used internally by completion scripts
    )
    parser.add_argument(
        "--setup-completions",
        action="store_true",
        help="Install shell tab completion (auto-detects your shell)",
    )
    parser.add_argument(
        "--list-endpoints",
        metavar="PREFIX",
        nargs="?",
        const="",
        help="List API endpoints matching PREFIX (used by shell completion)",
    )
    return parser.parse_args()


def build_url(args: argparse.Namespace) -> str:
    # Auto-prepend Spotify API base URL if not a full URL
    SPOTIFY_API_BASE = "https://api.spotify.com"
    url: SplitResult = urlsplit(args.url)

    params = args.params
    if url.query:
        params += url.query.split("&")
    params: list[list[str]] = [par.split("=") for par in params]
    params: dict[str, str] = {par[0]: par[1] for par in params}

    if not url.scheme:
        url = url._replace(scheme=SPOTIFY_API_BASE, path=f"/{url.path}")

    # Add default page limit if not already specified
    if args.method.upper() == "GET":
        params["limit"] = str(DEFAULT_PAGE_LIMIT)

    query = urlencode(params)
    return urlunsplit(url._replace(query=query)).removesuffix("/")


# --- Keyring (Secret Service / D-Bus) — shared with Riff ---


def _get_keyring_collection():
    """Get and unlock the default Secret Service collection."""
    conn = secretstorage.dbus_init()
    collection = secretstorage.get_default_collection(conn)
    if collection.is_locked():
        collection.unlock()
    return collection


def get_credentials():
    """Retrieve Spotify credentials from the system keyring.

    Returns a dict with access_token, refresh_token, token_expiry_time or None.
    """
    try:
        collection = _get_keyring_collection()
        items = list(collection.search_items(KEYRING_ATTRS))
        if not items:
            return None

        item = items[0]
        if item.is_locked():
            item.unlock()

        return json.loads(item.get_secret().decode("utf-8"))
    except Exception as e:
        if "-v" in sys.argv or "--verbose" in sys.argv:
            print(f"[keyring] Error retrieving credentials: {e}", file=sys.stderr)
        return None


def save_credentials(creds):
    """Save credentials to the system keyring with Riff-compatible attributes."""
    try:
        collection = _get_keyring_collection()
        for item in collection.search_items(KEYRING_ATTRS):
            item.delete()
        collection.create_item(
            "Spotify Credentials",
            KEYRING_ATTRS,
            json.dumps(creds).encode("utf-8"),
            replace=True,
        )
        return True
    except Exception:
        return False


def clear_credentials():
    """Remove credentials from the system keyring."""
    try:
        collection = _get_keyring_collection()
        for item in collection.search_items(KEYRING_ATTRS):
            item.delete()
        return True
    except Exception:
        return False


# --- OAuth2 PKCE Authentication Flow ---


def generate_code_verifier():
    """Generate a random code verifier (43-128 chars, URL-safe)."""
    return secrets.token_urlsafe(96)[:128]


def generate_code_challenge(verifier):
    """Create a S256 code challenge from the verifier."""
    digest = hashlib.sha256(verifier.encode("ascii")).digest()
    return base64.urlsafe_b64encode(digest).rstrip(b"=").decode("ascii")


class OAuthCallbackHandler(BaseHTTPRequestHandler):
    """HTTP handler that captures the OAuth redirect callback."""

    auth_code = None
    state = None
    error = None

    def do_GET(self):
        parsed = urlparse(self.path)
        params = parse_qs(parsed.query)

        if "error" in params:
            OAuthCallbackHandler.error = params["error"][0]
        elif "code" in params:
            OAuthCallbackHandler.auth_code = params["code"][0]
            OAuthCallbackHandler.state = params.get("state", [None])[0]

        # Send a nice response to the browser
        html = (
            "<html><body style='font-family:sans-serif;text-align:center;"
            "padding:40px;background:#191414;color:#1DB954'>"
            "<h1>&#10004; Authentication successful!</h1>"
            "<p style='color:#fff'>You can close this tab and return to your terminal.</p>"
            "</body></html>"
        )
        self.send_response(200)
        self.send_header("Content-Type", "text/html")
        self.send_header("Content-Length", str(len(html)))
        self.end_headers()
        self.wfile.write(html.encode())

    def log_message(self, format, *args):
        """Suppress default HTTP server logging."""
        pass


def do_oauth_pkce_flow(verbose=False):
    """Perform the full OAuth2 PKCE flow. Returns credentials dict or None."""
    code_verifier = generate_code_verifier()
    code_challenge = generate_code_challenge(code_verifier)
    state = secrets.token_urlsafe(32)

    # Build authorization URL
    auth_params = {
        "client_id": CLIENT_ID,
        "response_type": "code",
        "redirect_uri": REDIRECT_URI,
        "scope": SCOPES,
        "state": state,
        "code_challenge_method": "S256",
        "code_challenge": code_challenge,
    }
    auth_request_url = (
        f"{AUTH_URL}?{'&'.join(f'{k}={v}' for k, v in auth_params.items())}"
    )

    # Start local server to catch the redirect
    server = HTTPServer(("127.0.0.1", 8898), OAuthCallbackHandler)
    server_thread = Thread(target=server.handle_request, daemon=True)
    server_thread.start()

    print("Opening browser for Spotify login...", file=sys.stderr)
    print(
        f"If the browser doesn't open, visit:\n  {auth_request_url}\n", file=sys.stderr
    )
    webbrowser.open(auth_request_url)

    # Wait for the callback
    server_thread.join(timeout=120)
    server.server_close()

    if OAuthCallbackHandler.error:
        print(f"Authentication failed: {OAuthCallbackHandler.error}", file=sys.stderr)
        return None
    if not OAuthCallbackHandler.auth_code:
        print("Authentication timed out (no callback received).", file=sys.stderr)
        return None
    if OAuthCallbackHandler.state != state:
        print("Authentication failed: state mismatch.", file=sys.stderr)
        return None

    auth_code = OAuthCallbackHandler.auth_code

    if verbose:
        print(f"[oauth] Received auth code, exchanging for token...", file=sys.stderr)

    # Exchange code for token
    token_data = {
        "grant_type": "authorization_code",
        "code": auth_code,
        "redirect_uri": REDIRECT_URI,
        "client_id": CLIENT_ID,
        "code_verifier": code_verifier,
    }

    resp = requests.post(TOKEN_URL, data=token_data)
    if resp.status_code != 200:
        print(
            f"Token exchange failed ({resp.status_code}): {resp.text}", file=sys.stderr
        )
        return None

    token_json = resp.json()
    expires_in = token_json.get("expires_in", 3600)

    creds = {
        "access_token": token_json["access_token"],
        "refresh_token": token_json["refresh_token"],
        # Store as epoch timestamp (seconds) for cross-language compat with Riff's SystemTime
        "token_expiry_time": _system_time_from_now(expires_in),
    }

    if save_credentials(creds):
        print("Credentials saved to keyring.", file=sys.stderr)
    else:
        print("Warning: Could not save credentials to keyring.", file=sys.stderr)

    return creds


# --- Token Refresh ---


def _system_time_from_now(seconds_from_now):
    """Create a token_expiry_time compatible with Riff's SystemTime serialization.

    Riff serializes SystemTime as {"secs_since_epoch": N, "nanos_since_epoch": N}.
    """
    now = time.time()
    expiry = now + seconds_from_now
    return {"secs_since_epoch": int(expiry), "nanos_since_epoch": 0}


def _is_token_expired(creds):
    """Check if the token is expired based on token_expiry_time."""
    expiry = creds.get("token_expiry_time")
    if expiry is None:
        return True

    # Handle Riff's SystemTime format: {"secs_since_epoch": N, "nanos_since_epoch": N}
    if isinstance(expiry, dict):
        expiry_secs = expiry.get("secs_since_epoch", 0)
    elif isinstance(expiry, (int, float)):
        expiry_secs = expiry
    else:
        return True

    # Consider expired if less than 60 seconds remaining
    return time.time() > (expiry_secs - 60)


def refresh_token(creds, verbose=False):
    """Refresh the access token using the refresh token. Returns new creds or None."""
    refresh = creds.get("refresh_token")
    if not refresh:
        return None

    if verbose:
        print("[auth] Refreshing access token...", file=sys.stderr)

    token_data = {
        "grant_type": "refresh_token",
        "refresh_token": refresh,
        "client_id": CLIENT_ID,
    }

    resp = requests.post(TOKEN_URL, data=token_data)
    if resp.status_code != 200:
        if verbose:
            print(
                f"[auth] Token refresh failed ({resp.status_code}): {resp.text}",
                file=sys.stderr,
            )
        return None

    token_json = resp.json()
    expires_in = token_json.get("expires_in", 3600)

    new_creds = {
        "access_token": token_json["access_token"],
        "refresh_token": token_json.get("refresh_token", refresh),
        "token_expiry_time": _system_time_from_now(expires_in),
    }

    save_credentials(new_creds)
    if verbose:
        print("[auth] Token refreshed and saved.", file=sys.stderr)

    return new_creds


def get_valid_credentials(verbose=False):
    """Get valid (non-expired) credentials, refreshing or re-authenticating as needed."""
    creds = get_credentials()

    if creds is None:
        print("No stored credentials found. Starting login flow...", file=sys.stderr)
        return do_oauth_pkce_flow(verbose=verbose)

    if _is_token_expired(creds):
        if verbose:
            print("[auth] Token expired, attempting refresh...", file=sys.stderr)
        new_creds = refresh_token(creds, verbose=verbose)
        if new_creds:
            return new_creds
        # Refresh failed — re-authenticate
        print("Token refresh failed. Starting login flow...", file=sys.stderr)
        clear_credentials()
        return do_oauth_pkce_flow(verbose=verbose)

    return creds


# --- HTTP Requests ---


def make_request(url, method, data, access_token, verbose=False):
    """Make an authenticated HTTP request to the Spotify API.

    Returns (response_json, response) tuple.
    """
    headers = {
        "Authorization": f"Bearer {access_token}",
        "Accept": "application/json",
    }

    body = None
    if data:
        headers["Content-Type"] = "application/json"
        body = data

    if verbose:
        print(f"\n[request] {method} {url}", file=sys.stderr)
        for k, v in headers.items():
            if k == "Authorization":
                print(f"[request]   {k}: Bearer <redacted>", file=sys.stderr)
            else:
                print(f"[request]   {k}: {v}", file=sys.stderr)
        if body:
            print(f"[request]   Body: {body}", file=sys.stderr)

    resp = requests.request(method, url, headers=headers, data=body, timeout=30)

    if verbose:
        print(f"\n[response] {resp.status_code} {resp.reason}", file=sys.stderr)
        for k, v in resp.headers.items():
            print(f"[response]   {k}: {v}", file=sys.stderr)

    return resp


# --- Output Formatting ---

# Keys whose arrays should be truncated when they contain many scalar values
TRUNCATE_THRESHOLD = 5


def _truncate_long_arrays(data):
    """Recursively truncate long arrays of scalars for cleaner output."""
    if isinstance(data, dict):
        result = {}
        for key, value in data.items():
            if isinstance(value, list) and len(value) > TRUNCATE_THRESHOLD:
                # Check if it's an array of scalars (strings, numbers, bools)
                if all(isinstance(item, (str, int, float, bool)) for item in value):
                    result[key] = value[:TRUNCATE_THRESHOLD] + [
                        f"... ({len(value)} total)"
                    ]
                else:
                    result[key] = [_truncate_long_arrays(item) for item in value]
            elif isinstance(value, (dict, list)):
                result[key] = _truncate_long_arrays(value)
            else:
                result[key] = value
        return result
    elif isinstance(data, list):
        return [_truncate_long_arrays(item) for item in data]
    return data


def print_json(data, raw=False):
    """Pretty-print JSON data with optional syntax highlighting."""
    # Truncate noisy arrays unless raw mode
    display_data = data if raw else _truncate_long_arrays(data)
    json_str = json.dumps(display_data, indent=2)

    if raw or not HAS_RICH:
        print(json_str)
    else:
        out = Console()
        out.print(RichJSON(json_str))


# --- Pagination ---


def find_next_url(data):
    """Find the 'next' pagination URL in a response.

    Checks top-level 'next' and also common nested structures.
    """
    if isinstance(data, dict):
        # Direct next field (e.g., /v1/me/playlists response)
        if "next" in data and data["next"] and "items" in data:
            return data["next"]
        # Check nested objects that might have pagination
        for key, value in data.items():
            if (
                isinstance(value, dict)
                and "next" in value
                and value["next"]
                and "items" in value
            ):
                return value["next"]
    return None


def get_pagination_info(data):
    """Extract pagination info from the response."""
    if isinstance(data, dict):
        if "total" in data and "offset" in data and "limit" in data:
            return data["offset"], data["limit"], data["total"]
        # Check nested
        for value in data.values():
            if (
                isinstance(value, dict)
                and "total" in value
                and "offset" in value
                and "limit" in value
            ):
                return value["offset"], value["limit"], value["total"]
    return None, None, None


# --- Main ---


def main():
    # Intercept "v1/path --help" or "v1/path -h" before argparse eats it
    if ("--help" in sys.argv or "-h" in sys.argv) and len(sys.argv) > 2:
        # Find the positional arg (not a flag)
        path_arg = None
        for arg in sys.argv[1:]:
            if arg in ("--help", "-h"):
                continue
            if not arg.startswith("-"):
                path_arg = arg
                break
        if path_arg and ("v1" in path_arg or "/" in path_arg):
            get_endpoint_help(path_arg)
            return

    args = parse_args()
    if not args.raw:
        args.raw = sys.stdout.isatty()

    # Handle --completions (internal, used by the installed completion scripts)
    if args.completions:
        if args.completions == "bash":
            print(generate_bash_completion())
        elif args.completions == "zsh":
            print(generate_zsh_completion())
        return

    # Handle --setup-completions (interactive installer)
    if args.setup_completions:
        setup_shell_completions()
        return

    # Handle --list-endpoints (used by shell completion, fast path)
    if args.list_endpoints is not None:
        matches = get_completions_for(args.list_endpoints)
        for ep in matches:
            print(ep)
        return

    # Handle --logout
    if args.logout:
        if not clear_credentials():
            sys.exit("Failed to clear credentials.")
        print("Credentials cleared from keyring.", file=sys.stderr)
        return

    # Handle --check-auth
    if args.check_auth:
        creds = get_credentials()
        if creds is None:
            sys.exit("No credentials stored in keyring.")
        if _is_token_expired(creds):
            new_creds = refresh_token(creds, verbose=args.verbose)
            if new_creds:
                print("Token refreshed successfully. Authenticated.", file=sys.stderr)
            else:
                sys.exit("Token refresh failed. Re-authentication required.")
        else:
            print("Authenticated. Token is valid.", file=sys.stderr)
        return

    if not args.url:
        sys.exit("Error: URL or path is required. Use --help for usage.")

    creds = get_valid_credentials(verbose=args.verbose)
    if not creds:
        sys.exit("Error: Could not obtain valid credentials.")

    # Make the request
    method = args.method.upper()
    current_url = build_url(args)
    page_num = 1

    while True:
        resp = make_request(
            current_url, method, args.data, creds["access_token"], verbose=args.verbose
        )

        # Handle 401 — try refresh and retry once
        if resp.status_code == 401:
            if args.verbose:
                print("[auth] Got 401, attempting token refresh...", file=sys.stderr)
            new_creds = refresh_token(creds, verbose=args.verbose)
            if new_creds:
                creds = new_creds
                resp = make_request(
                    current_url,
                    method,
                    args.data,
                    creds["access_token"],
                    verbose=args.verbose,
                )
            else:
                sys.exit(
                    "Error: Authentication failed. Try --logout and re-authenticate."
                )

        # Handle rate limiting
        if resp.status_code == 429:
            retry_after = int(resp.headers.get("Retry-After", 5))
            print(f"Rate limited. Retrying in {retry_after}s...", file=sys.stderr)
            time.sleep(retry_after)
            continue

        # Handle other errors
        if resp.status_code >= 400:
            try:
                print_json(resp.json(), raw=args.raw)
            except (json.JSONDecodeError, ValueError):
                print(resp.text, file=sys.stderr)
            sys.exit(1)

        # Handle empty responses (204 No Content)
        if resp.status_code == 204 or not resp.content:
            if args.verbose:
                print(f"[response] {resp.status_code} (no content)", file=sys.stderr)
            break

        # Parse and display JSON response
        try:
            data = resp.json()
        except (json.JSONDecodeError, ValueError):
            print(resp.text)
            break

        # Print the JSON
        print_json(data, raw=args.raw)

        if not sys.stdout.isatty():
            break

        # Check for pagination
        next_url = find_next_url(data)
        if not next_url:
            break

        # Show pagination info
        offset, limit, total = get_pagination_info(data)
        if offset is not None and limit is not None and total is not None:
            current_end = min(offset + limit, total)
            info_str = f"  [{current_end}/{total} items]"
        else:
            info_str = ""

        # Prompt for next page
        try:
            answer = input(
                f"\n--- Page {page_num}{info_str} --- Fetch next page? [Y/n] "
            )
        except (EOFError, KeyboardInterrupt):
            print("", file=sys.stderr)
            break

        if answer.strip().lower() in ("n", "no"):
            break

        # For subsequent pages, always use GET with no body
        current_url = next_url
        method = "GET"
        args.data = None
        page_num += 1


if __name__ == "__main__":
    main()
