#!/usr/bin/env python3
"""Measure RAM and CPU usage of Riff and a reference app, and print a markdown report.

Both apps must already be running. For a fair comparison put them in the same
state first (e.g. both idle, or both playing a track).

    scripts/perf/measure-resources.py                      # 60s, report on stdout
    scripts/perf/measure-resources.py -d 300 -o report.md  # 5 minutes, write to file

"""

import argparse
import os
import statistics
import sys
import time
from datetime import datetime

PAGE_KB = os.sysconf("SC_PAGE_SIZE") // 1024
MB = 1024 * 1024
PERCENTILES = (50, 90, 95, 99)
TIMELINE_ROWS = 20


def read_procs():
    """Return {pid: (name, ppid)} for every readable process."""
    procs = {}
    for entry in os.listdir("/proc"):
        if not entry.isdigit():
            continue
        try:
            with open(f"/proc/{entry}/stat") as f:
                stat = f.read()
        except OSError:
            continue
        # comm may contain spaces and parentheses, so split around the last ')'
        comm = stat[stat.index("(") + 1 : stat.rindex(")")]
        ppid = int(stat[stat.rindex(")") + 2 :].split()[1])
        try:
            name = os.path.basename(os.readlink(f"/proc/{entry}/exe").removesuffix(" (deleted)"))
        except OSError:
            name = comm
        procs[int(entry)] = (name, ppid)
    return procs


def process_tree(procs, app):
    """Pids of every process named `app`, plus all of their descendants."""
    children = {}
    for pid, (_, ppid) in procs.items():
        children.setdefault(ppid, []).append(pid)
    todo = [pid for pid, (name, _) in procs.items() if name.lower() == app.lower() and pid != os.getpid()]
    tree = set()
    while todo:
        pid = todo.pop()
        if pid not in tree:
            tree.add(pid)
            todo.extend(children.get(pid, []))
    return tree


def mem_kb(pid):
    try:
        with open(f"/proc/{pid}/smaps_rollup") as f:
            for line in f:
                if line.startswith("Pss:"):
                    return int(line.split()[1])
    except OSError:
        pass
    # PSS not readable (e.g. process owned by another user): fall back to RSS
    try:
        with open(f"/proc/{pid}/statm") as f:
            return int(f.read().split()[1]) * PAGE_KB
    except OSError:
        return 0


def io_bytes(pid):
    """(bytes read from disk, bytes written to disk) since the process started."""
    try:
        with open(f"/proc/{pid}/io") as f:
            fields = dict(line.split(": ") for line in f)
        return int(fields["read_bytes"]), int(fields["write_bytes"])
    except (OSError, KeyError, ValueError):
        return 0, 0


def thread_counters(pid):
    """{(pid, tid): (cpu_ns, times_scheduled)} for every thread of the process."""
    counters = {}
    try:
        tids = os.listdir(f"/proc/{pid}/task")
    except OSError:
        return counters
    for tid in tids:
        try:
            with open(f"/proc/{pid}/task/{tid}/schedstat") as f:
                cpu_ns, _, scheduled = f.read().split()
        except (OSError, ValueError):
            continue
        counters[(pid, int(tid))] = (int(cpu_ns), int(scheduled))
    return counters


def snapshot(procs, app):
    """Cumulative counters for every process of `app`, or None if it is not running."""
    tree = process_tree(procs, app)
    if not tree:
        return None
    threads = {}
    for pid in tree:
        threads.update(thread_counters(pid))
    return {
        "procs": len(tree),
        "threads": threads,
        "io": {pid: io_bytes(pid) for pid in tree},
        "mem": sum(mem_kb(pid) for pid in tree) / 1024,
    }


def delta(cur, prev, index):
    """Growth of a counter, summed over the keys present in both snapshots."""
    return sum(cur[k][index] - prev[k][index] for k in cur if k in prev)


def measure(apps, duration, interval):
    """Sample every app until `duration` has passed or Ctrl+C is pressed.

    Returns ({app: {"samples": [...], "read": bytes, "write": bytes}}, elapsed seconds).
    """
    data = {app: {"samples": [], "read": 0, "write": 0} for app in apps}
    procs = read_procs()
    prev = {app: snapshot(procs, app) for app in apps}
    start = prev_time = time.monotonic()
    try:
        while prev_time - start < duration:
            left = duration - (prev_time - start)
            print(f"\rmeasuring... {left:.0f}s left ", end="", file=sys.stderr)
            time.sleep(min(interval, left))
            now = time.monotonic()
            dt = now - prev_time
            procs = read_procs()
            for app in apps:
                cur = snapshot(procs, app)
                old, prev[app] = prev[app], cur
                if not cur or not old:
                    continue
                d = data[app]
                d["read"] += delta(cur["io"], old["io"], 0)
                d["write"] += delta(cur["io"], old["io"], 1)
                d["samples"].append(
                    {
                        # middle of the interval this sample covers
                        "t": (prev_time + now) / 2 - start,
                        "cpu": delta(cur["threads"], old["threads"], 0) / 1e9 / dt * 100,
                        "wakeups": delta(cur["threads"], old["threads"], 1) / dt,
                        "mem": cur["mem"],
                        "threads": len(cur["threads"]),
                        "procs": cur["procs"],
                    }
                )
            prev_time = now
    except KeyboardInterrupt:
        pass
    print("\r" + " " * 30 + "\r", end="", file=sys.stderr)
    return data, prev_time - start


def percentile(values, p):
    ordered = sorted(values)
    k = (len(ordered) - 1) * p / 100
    lo = int(k)
    hi = min(lo + 1, len(ordered) - 1)
    return ordered[lo] + (ordered[hi] - ordered[lo]) * (k - lo)


def summarize(d):
    samples = d["samples"]
    if not samples:
        return None
    return {
        "samples": samples,
        "cpu": [s["cpu"] for s in samples],
        "mem": [s["mem"] for s in samples],
        "wakeups": [s["wakeups"] for s in samples],
        "procs": max(s["procs"] for s in samples),
        "threads": max(s["threads"] for s in samples),
        "mem_growth": samples[-1]["mem"] - samples[0]["mem"],
        "read": d["read"] / MB,
        "write": d["write"] / MB,
    }


OVERVIEW = [
    ("Processes", lambda s: f"{s['procs']}"),
    ("Threads", lambda s: f"{s['threads']}"),
    ("CPU avg", lambda s: f"{statistics.fmean(s['cpu']):.1f} %"),
    ("Wakeups avg", lambda s: f"{statistics.fmean(s['wakeups']):.0f} /s"),
    ("Memory avg (PSS)", lambda s: f"{statistics.fmean(s['mem']):.1f} MB"),
    ("Memory growth (first → last sample)", lambda s: f"{s['mem_growth']:+.1f} MB"),
    ("Disk read", lambda s: f"{s['read']:.1f} MB"),
    ("Disk written", lambda s: f"{s['write']:.1f} MB"),
]

DISTRIBUTIONS = [("CPU %", "cpu", "{:.1f}"), ("Memory MB", "mem", "{:.1f}"), ("Wakeups /s", "wakeups", "{:.0f}")]


def table(header, rows, left_columns=1):
    lines = ["| " + " | ".join(header) + " |"]
    lines.append("|" + "---|" * left_columns + "---:|" * (len(header) - left_columns))
    lines += ["| " + " | ".join(row) + " |" for row in rows]
    return lines


def timeline(running, stats, elapsed):
    """One row per equal time slice, averaging the samples that fall in it."""
    count = min(TIMELINE_ROWS, max(len(stats[a]["samples"]) for a in running))
    header = ["Time"]
    for a in running:
        header += [f"{a} CPU avg", f"{a} CPU max", f"{a} mem", f"{a} wakeups"]
    rows = []
    for i in range(count):
        seconds = int(i * elapsed / count)
        row = [f"{seconds // 60}:{seconds % 60:02d}"]
        for a in running:
            b = [s for s in stats[a]["samples"] if min(count - 1, int(s["t"] / elapsed * count)) == i]
            if not b:
                row += ["–"] * 4
                continue
            row += [
                f"{statistics.fmean(s['cpu'] for s in b):.1f} %",
                f"{max(s['cpu'] for s in b):.1f} %",
                f"{statistics.fmean(s['mem'] for s in b):.0f} MB",
                f"{statistics.fmean(s['wakeups'] for s in b):.0f} /s",
            ]
        rows.append(row)
    return table(header, rows)


def ratio(a, b):
    return f"{a / b:.2f}×" if b > 0.05 else "n/a"


def report(apps, stats, elapsed, interval):
    running = [a for a in apps if stats[a]]
    out = [
        "# Resource usage report",
        "",
        f"- Date: {datetime.now():%Y-%m-%d %H:%M}",
        f"- Duration: {elapsed:.0f}s, sampled every {interval:g}s",
        f"- CPU is % of one core ({os.cpu_count()} cores available)",
        "",
        "## Overview",
        "",
    ]
    out += table(["Metric"] + apps, [[label] + [fmt(stats[a]) if stats[a] else "–" for a in apps] for label, fmt in OVERVIEW])

    out += ["", "## Distribution", ""]
    rows = []
    for label, key, fmt in DISTRIBUTIONS:
        for a in running:
            v = stats[a][key]
            cells = [min(v)] + [percentile(v, p) for p in PERCENTILES] + [max(v), statistics.fmean(v)]
            rows.append([label, a] + [fmt.format(c) for c in cells])
    out += table(["Metric", "App", "min"] + [f"p{p}" for p in PERCENTILES] + ["max", "avg"], rows, left_columns=2)

    out += ["", "## Timeline", ""] + timeline(running, stats, elapsed)

    out += ["", "## Summary", ""]
    for app in apps:
        s = stats[app]
        if not s:
            out.append(f"- **{app}** was not running, so it was not measured.")
            continue
        out.append(
            f"- **{app}** averaged {statistics.fmean(s['mem']):.0f} MB of memory and {statistics.fmean(s['cpu']):.1f}% CPU "
            f"(p95: {percentile(s['mem'], 95):.0f} MB, {percentile(s['cpu'], 95):.1f}% CPU)."
        )
        if s["mem_growth"] > max(20, 0.1 * statistics.fmean(s["mem"])):
            out.append(
                f"- ⚠ **{app}** memory grew by {s['mem_growth']:.0f} MB during the run; "
                "re-run with a longer `--duration` to check for a leak."
            )
    if len(running) == 2:
        target, reference = running
        avg = {a: {k: statistics.fmean(stats[a][k]) for k in ("mem", "cpu")} for a in running}
        t, r = avg[target], avg[reference]
        out.append(
            f"- {target} uses {ratio(t['mem'], r['mem'])} the memory and "
            f"{ratio(t['cpu'], r['cpu'])} the CPU of {reference}."
        )
        heavier = [what for what, k in (("memory", "mem"), ("CPU", "cpu")) if t[k] > r[k] * 1.1 and t[k] - r[k] > 1]
        if heavier:
            out.append(f"- **Verdict:** {target} is heavier than {reference} on {' and '.join(heavier)}.")
        else:
            out.append(f"- **Verdict:** {target} is no heavier than {reference}.")
    return "\n".join(out) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("-d", "--duration", type=float, default=60, help="seconds to measure (default: 60)")
    parser.add_argument("-i", "--interval", type=float, default=1, help="seconds between samples (default: 1)")
    parser.add_argument("-o", "--output", help="write the markdown report to this file instead of stdout")
    parser.add_argument("--app", default="riff", help="process name to measure (default: riff)")
    parser.add_argument(
        "--compare", default="spotify", help="process name to compare against, or 'none' (default: spotify)"
    )
    args = parser.parse_args()

    apps = [args.app] if args.compare.lower() in ("", "none") else [args.app, args.compare]
    procs = read_procs()
    if not any(process_tree(procs, app) for app in apps):
        sys.exit(f"{' and '.join(apps)} {'is' if len(apps) == 1 else 'are'} not running.")
    data, elapsed = measure(apps, args.duration, args.interval)
    stats = {app: summarize(d) for app, d in data.items()}
    if not any(stats.values()):
        sys.exit("No samples were collected.")

    text = report(apps, stats, elapsed, args.interval)
    if args.output:
        with open(args.output, "w") as f:
            f.write(text)
        print(f"Report written to {args.output}", file=sys.stderr)
    else:
        print(text, end="")


if __name__ == "__main__":
    main()
