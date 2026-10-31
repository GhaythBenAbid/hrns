import subprocess, time, json, re, sys, os, statistics, threading
sys.path.insert(0, os.path.dirname(__file__))
from cgwin import has_window

APPS = {
  "hrns": {"path": sys.argv[1], "owner": "hrns", "match": "/hrns.app/"},
  "t3":   {"path": "/Applications/T3 Code (Nightly).app", "owner": "T3 Code (Nightly)", "match": "/T3 Code (Nightly).app/"},
}
TRIALS, SETTLE, WINDOW = 1, 15, 60
sh = lambda c: subprocess.run(c, shell=True, capture_output=True, text=True).stdout

def procs():
    out = {}
    for line in sh("ps -axo pid=,ppid=,rss=,time=,command=").splitlines():
        p = line.split(None, 4)
        if len(p) < 5: continue
        m = re.match(r"(?:(\d+)-)?(?:(\d+):)?(\d+):([\d.]+)", p[3])
        d, h, mi, s = m.groups()
        cpu = int(d or 0)*86400 + int(h or 0)*3600 + int(mi)*60 + float(s)
        out[int(p[0])] = dict(ppid=int(p[1]), rss=int(p[2]), cpu=cpu, cmd=p[4])
    return out

def webkit_pids(ps): return {p for p, v in ps.items() if "com.apple.WebKit" in v["cmd"]}

def tree(app, ps, extra):
    roots = {p for p, v in ps.items() if app["match"] in v["cmd"] and v["ppid"] == 1}
    s, changed = set(roots), True
    while changed:
        changed = False
        for p, v in ps.items():
            if p not in s and v["ppid"] in s: s.add(p); changed = True
    return s | (extra & set(ps))

def alive(app): return [p for p, v in procs().items() if app["match"] in v["cmd"]]
def kill(app):
    sh(f"osascript -e 'quit app \"{app['path']}\"'")
    for i in range(30):
        if not alive(app): break
        time.sleep(0.5)
    else:
        sh(f"pkill -9 -f '{app['match']}'"); time.sleep(1)
    assert not alive(app) and not has_window(app["owner"]), "app still running"
    time.sleep(3)

def launch(app):
    before = webkit_pids(procs())
    t0 = time.perf_counter()
    subprocess.Popen(["open", "-n", app["path"]])
    while time.perf_counter() - t0 < 40:
        if has_window(app["owner"]): break
        time.sleep(0.005)
    else: return None, set()
    return time.perf_counter() - t0, before

def footprint(pids):
    out = sh("footprint " + " ".join(f"-p {p}" for p in pids) + " 2>/dev/null")
    m = re.findall(r"phys_footprint:\s*([\d.]+)\s*(KB|MB|GB)", out)
    unit = {"KB": 1/1024, "MB": 1, "GB": 1024}
    vals = [float(v)*unit[u] for v, u in m]
    return round(sum(vals), 1)

def sample(app, extra, label):
    """Sample tree for WINDOW seconds: CPU via ps time deltas, energy via top."""
    topf = f"top3_{label}.txt"
    top = subprocess.Popen(f"top -l {WINDOW//2 + 1} -s 2 -o power -n 600 -stats pid,power,idlew,th > {topf}", shell=True)
    ps0 = procs(); pids0 = tree(app, ps0, extra); t0 = time.time()
    rss, counts, cpus = [], [], []
    prev, pt = ps0, t0
    while time.time() - t0 < WINDOW:
        ps = procs(); pids = tree(app, ps, extra); now = time.time()
        if now > pt:
            cpus.append(round(sum(ps[p]["cpu"] - (prev[p]["cpu"] if p in prev else 0) for p in pids) / (now - pt) * 100, 2))
        prev, pt = ps, now
        rss.append(sum(ps[p]["rss"] for p in pids) / 1024); counts.append(len(pids))
        time.sleep(2)
    ps1 = procs(); pids1 = tree(app, ps1, extra); dt = time.time() - t0
    cpu = sum(ps1[p]["cpu"] - (ps0[p]["cpu"] if p in ps0 else 0) for p in pids1)
    top.wait()
    # parse top: per sample sum power/idlew over tree pids (skip first sample, it's cumulative)
    samples, cur = [], None
    for line in open(topf):
        if line.startswith("Processes:"):
            cur = {}; samples.append(cur); continue
        f = line.split()
        if cur is not None and len(f) >= 4 and f[0].isdigit():
            try: cur[int(f[0])] = (float(f[1]), float(f[2]), int(re.match(r"\d+", f[3]).group()))
            except: pass
    samples = samples[1:]
    power = [sum(s[p][0] for p in pids1 if p in s) for s in samples]
    wake = [sum(s[p][1] for p in pids1 if p in s) for s in samples]
    threads = [sum(s[p][2] for p in pids1 if p in s) for s in samples]
    breakdown = {}
    for p in pids1:
        c = ps1[p]["cmd"]; name = os.path.basename(c.split(" -")[0]).strip()[:60]
        b = breakdown.setdefault(name, {"n": 0, "rss_mb": 0, "cpu_s": 0})
        b["n"] += 1; b["rss_mb"] += ps1[p]["rss"]/1024; b["cpu_s"] += ps1[p]["cpu"] - (ps0[p]["cpu"] if p in ps0 else 0)
    return dict(
        cpu_pct=round(cpu / dt * 100, 2),
        rss_avg_mb=round(statistics.mean(rss), 1), rss_peak_mb=round(max(rss), 1),
        footprint_mb=footprint(pids1), procs=max(counts),
        threads=round(statistics.mean(threads)) if threads else None,
        energy_avg=round(statistics.mean(power), 2) if power else None,
        energy_peak=round(max(power), 2) if power else None,
        wakeups_avg=round(statistics.mean(wake), 1) if wake else None,
        power_series=power, cpu_series=cpus,
        breakdown={k: {kk: round(vv, 2) for kk, vv in v.items()} for k, v in breakdown.items()},
    )

def bundle(app):
    kb = int(sh(f"du -sk '{app['path']}'").split()[0])
    return round(kb / 1024, 1)

res = {"meta": {
    "date": time.strftime("%Y-%m-%d %H:%M"),
    "machine": sh("sysctl -n machdep.cpu.brand_string").strip() + ", " + str(int(sh("sysctl -n hw.memsize"))//2**30) + " GB",
    "os": "macOS " + sh("sw_vers -productVersion").strip(),
    "power": sh("pmset -g batt").splitlines()[0].split("'")[1],
    "t3_version": sh("defaults read '/Applications/T3 Code (Nightly).app/Contents/Info' CFBundleShortVersionString").strip(),
    "hrns_commit": sh("git -C /Users/macbook/Desktop/personal/hrns rev-parse --short HEAD").strip(),
    "trials": TRIALS, "settle_s": SETTLE, "window_s": WINDOW}}

for k, app in APPS.items(): kill(app)
for k, app in APPS.items():
    print("==", k, flush=True)
    r = {"bundle_mb": bundle(app), "launch_s": []}
    for i in range(TRIALS):
        t, _ = launch(app); print(" launch", t, flush=True)
        r["launch_s"].append(round(t, 3) if t else None)
        time.sleep(4); kill(app)
    t, before = launch(app)
    time.sleep(SETTLE)
    extra = webkit_pids(procs()) - before if k == "hrns" else set()
    print(" sampling foreground idle", flush=True)
    r["idle_fg"] = sample(app, extra, k + "_fg")
    sh("osascript -e 'tell application \"Finder\" to activate'"); time.sleep(5)
    print(" sampling background idle", flush=True)
    r["idle_bg"] = sample(app, extra, k + "_bg")
    kill(app)
    res[k] = r
    print(json.dumps(r, indent=1), flush=True)

json.dump(res, open("results_series.json", "w"), indent=1)
print("DONE")
