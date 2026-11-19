"""AntAn'ı NEC-2 (nec2c) ve MININEC (pymininec) ile karşılaştırır.

Girdi: `cargo run --release -p antan-core --example validate -- <klasör>` ile
yazılan cases.json ve <id>.nec dosyaları. Çıktı: <klasör>/results.json ve
markdown tabloları (tables_en.md, tables_tr.md).

Kullanım:
    python compare.py <klasör>
Ortam değişkenleri:
    NEC2C   nec2c komutu (varsayılan: "wsl -d Ubuntu-24.04 nec2c"); dosya yolları
            WSL'de /mnt/<sürücü>/... biçimine çevrilir.
Gerekenler: numpy, pymininec (pip install pymininec).
"""

import json
import math
import os
import re
import subprocess
import sys
import time

import numpy as np
from mininec.mininec import (Angle, Excitation, Geo_Container, Medium, Mininec,
                             Series_RLC_Load, Skin_Effect_Load, Wire)


def wsl_path(p):
    p = os.path.abspath(p).replace("\\", "/")
    if re.match(r"^[A-Za-z]:/", p):
        return "/mnt/" + p[0].lower() + p[2:]
    return p


# ------------------------------------------------------------------ NEC-2

def nec_deck(text, grounded, real_ground):
    """AntAn'ın NEC dosyasını karşılaştırma için düzenler: gerçek zeminde
    Sommerfeld-Norton (GN 2), 2° ızgarada desen."""
    out = []
    for line in text.splitlines():
        if line.startswith("RP"):
            nt = 46 if grounded else 91
            out.append("RP 0 %d 180 1000 0 0 2 2" % nt)
            continue
        if real_ground and line.startswith("GN 0"):
            line = "GN 2" + line[4:]
        out.append(line)
    return "\n".join(out) + "\n"


def run_nec(deck_path, out_path):
    # nec2c uzun dosya adlarını kabul etmez: klasörde kısa adlarla çalıştırılır.
    folder = os.path.dirname(os.path.abspath(deck_path))
    i, o = os.path.basename(deck_path), os.path.basename(out_path)
    cmd = os.environ.get("NEC2C", "wsl -d Ubuntu-24.04 nec2c").split()
    if cmd[0] == "wsl":
        args = cmd[:3] + ["--cd", wsl_path(folder)] + cmd[3:] + ["-i", i, "-o", o]
    else:
        args = cmd + ["-i", i, "-o", o]
    t0 = time.perf_counter()
    r = subprocess.run(args, capture_output=True, cwd=folder)
    if r.returncode != 0:
        raise RuntimeError((r.stdout + r.stderr).decode(errors="replace").strip()[:200])
    return (time.perf_counter() - t0) * 1e3


def parse_nec(path):
    lines = open(path, encoding="latin-1").read().splitlines()
    feeds, pattern, eff = [], {}, None
    i = 0
    while i < len(lines):
        l = lines[i]
        if "ANTENNA INPUT PARAMETERS" in l:
            j = i + 3
            while j < len(lines) and lines[j].strip():
                v = lines[j].split()
                # TAG SEG Vr Vi Ir Ii Zr Zi Yr Yi P
                feeds.append((float(v[6]), float(v[7])))
                j += 1
            i = j
            continue
        if "RADIATION PATTERNS" in l:
            j = i + 1
            while j < len(lines) and not re.match(r"^\s*-?\d", lines[j]):
                j += 1
            while j < len(lines) and re.match(r"^\s*-?\d", lines[j]):
                v = lines[j].split()
                pattern[(round(float(v[0])), round(float(v[1])))] = float(v[4])
                j += 1
            i = j
            continue
        m = re.search(r"EFFICIENCY\s*=\s*([\d.]+)\s*PERCENT", l, re.IGNORECASE)
        if m:
            eff = float(m.group(1)) / 100.0
        i += 1
    return feeds, pattern, eff


def summarize(pattern, grounded):
    """En büyük kazanç ve ön/arka, AntAn ile aynı tanımla."""
    (bt, bp), g = max(pattern.items(), key=lambda kv: kv[1])
    back = (bt, (bp + 180) % 360) if grounded else (180 - bt, (bp + 180) % 360)
    gb = pattern.get(back, -999.0)
    return g, bt, bp, g - gb


def nec_sweep_deck(text, freqs):
    out = []
    for line in text.splitlines():
        if line.startswith("FR") or line.startswith("RP") or line.startswith("EN"):
            continue
        out.append(line)
    step = (freqs[-1] - freqs[0]) / (len(freqs) - 1)
    out.append("FR 0 %d 0 0 %.9f %.9f" % (len(freqs), freqs[0] / 1e6, step / 1e6))
    out.append("XQ")
    out.append("EN")
    return "\n".join(out) + "\n"


def parse_nec_sweep(path):
    """Her frekans için ilk beslemenin empedansı (sırayla)."""
    lines = open(path, encoding="latin-1").read().splitlines()
    zs = []
    for i, l in enumerate(lines):
        if "ANTENNA INPUT PARAMETERS" in l:
            v = lines[i + 3].split()
            zs.append((float(v[6]), float(v[7])))
    return zs


def resonance(sw, f0):
    """X'in sıfırdan geçtiği, f0'a en yakın frekans (doğrusal ara değerleme)."""
    best = None
    for (fa, _, xa), (fb, _, xb) in zip(sw, sw[1:]):
        if xa == 0 or (xa < 0) != (xb < 0):
            fr = fa + (fb - fa) * (0 - xa) / (xb - xa) if xb != xa else fa
            if best is None or abs(fr - f0) < abs(best - f0):
                best = fr
    return best


def swr_min(sw, z0=50.0):
    def s(r, x):
        g = abs(complex(r - z0, x) / complex(r + z0, x))
        return (1 + g) / (1 - g) if g < 1 else 1e9
    f, r, x = min(sw, key=lambda p: s(p[1], p[2]))
    return f, s(r, x)


# ------------------------------------------------------------------ MININEC

def run_mininec(case):
    a = case["model"]
    f = case["frequency"] / 1e6
    geo = Geo_Container()
    wires = []
    for k, w in enumerate(a["wires"]):
        (x1, y1, z1), (x2, y2, z2) = w["start"], w["end"]
        wo = Wire(w["segments"], x1, y1, z1, x2, y2, z2, 0.5 * w["diameter"], tag=k + 1)
        geo.append(wo)
        wires.append(wo)
    geo.compute_tags()
    g = a.get("ground", "FreeSpace")
    media = None
    if g == "Perfect":
        media = [Medium(0, 0)]
    elif isinstance(g, dict) and "Real" in g:
        media = [Medium(g["Real"]["permittivity"], g["Real"]["conductivity"])]
    m = Mininec(f, geo, media=media)

    def nearest_pulse(pos, wire=None):
        pts = np.array([p.point for p in m.pulses])
        d = np.linalg.norm(pts - np.array(pos), axis=1)
        dmin = d.min()
        cands = [i for i in range(len(d)) if d[i] <= dmin + 1e-6]
        if wire is not None:
            for i in cands:
                if getattr(m.pulses[i].geobj, "tag", None) == wire + 1:
                    return i
        return cands[0]

    for k, w in enumerate(a["wires"]):
        cond = w["material"]["conductivity"]
        if cond is not None and math.isfinite(cond):
            ld = Skin_Effect_Load(wires[k], cond)
            m.register_load(ld, None, k + 1)
    for ld, pos in zip(a.get("loads", []), case["load_pos"]):
        kind = ld["kind"]
        if "SeriesRlc" in kind:
            r, l, c = kind["SeriesRlc"]["r"], kind["SeriesRlc"]["l"], kind["SeriesRlc"]["c"]
            load = Series_RLC_Load(R=r or None, L=l or None, C=c or None)
        else:
            raise ValueError("unsupported load " + str(kind))
        m.register_load(load, nearest_pulse(pos, ld["wire"]))
    # Kaynaklar. Birden çok telin birleştiği noktada (ör. GP tabanı) MININEC her
    # tel çifti için ayrı düğüm kurar: besleme hepsine aynı gerilimle konur,
    # giriş akımı düğüm akımlarının toplamıdır.
    groups = []
    pts = np.array([p.point for p in m.pulses])
    for s, pos in zip(a["sources"], case["source_pos"]):
        v = s["voltage"]
        volt = complex(v[0], v[1]) if isinstance(v, list) else complex(v["re"], v["im"])
        d = np.linalg.norm(pts - np.array(pos), axis=1)
        at = [i for i in range(len(d)) if d[i] <= d.min() + 1e-6]
        mine = [i for i in at if any(getattr(sg, "geobj", None) is not None and sg.geobj.tag == s["wire"] + 1 for sg in getattr(m.pulses[i], "segs", []))]
        idx = mine if len(mine) > 1 else [nearest_pulse(pos, s["wire"])]
        g = []
        for i in idx:
            ex = Excitation(volt)
            m.register_source(ex, i)
            g.append(ex)
        groups.append((volt, g))
    t0 = time.perf_counter()
    m.compute()
    grounded = media is not None
    nt = 46 if grounded else 91
    m.compute_far_field(Angle(0, 2, nt), Angle(0, 2, 180))
    ms = (time.perf_counter() - t0) * 1e3
    # pymininec'in kendi çıktısıyla aynı sıra: gain.T[2] ile zen/azi düz dizileri eşleşir.
    gain = np.asarray(m.far_field.gain).T[2]
    zen = np.asarray(m.far_field.zen)
    azi = np.asarray(m.far_field.azi)
    pattern = {}
    for t, p, gg in zip(zen.flat, azi.flat, gain.flat):
        pattern[(round(float(t)), round(float(p)) % 360)] = float(gg)
    def zin(k):
        volt, g = groups[k]
        z = volt / sum(complex(e.current) for e in g)
        return z.real, z.imag

    feeds = [zin(k) for k in range(len(groups))]
    # Tarama (yalnız tek beslemeli modellerde; MININEC yavaş olduğu için seyrek).
    sweep = []
    if len(groups) == 1 and case.get("sweep_n"):
        f0 = case["frequency"]
        span = case["sweep_span"]
        narrow = case["id"].startswith("magloop")
        n = 121 if narrow else 41
        for i in range(n):
            ff = f0 * (1 - span + 2 * span * i / (n - 1))
            m.f = ff / 1e6
            m.compute()
            sweep.append((ff,) + zin(0))
        if narrow:
            # Yüksek Q: SWR minimumu çevresinde ince tarama.
            fm, _ = swr_min(sweep)
            st = 2 * span * f0 / (n - 1)
            for i in range(41):
                ff = fm - st + 2 * st * i / 40
                m.f = ff / 1e6
                m.compute()
                sweep.append((ff,) + zin(0))
            sweep.sort()
    return feeds, pattern, ms, sweep


# ------------------------------------------------------------------ ana akış

def main():
    folder = sys.argv[1]
    cases = json.load(open(os.path.join(folder, "cases.json"), encoding="utf-8"))
    results = []
    for c in cases:
        g = c["model"].get("ground", "FreeSpace")
        grounded = g != "FreeSpace"
        real = isinstance(g, dict) and "Real" in g
        r = {"id": c["id"], "title_en": c["title_en"], "title_tr": c["title_tr"], "frequency": c["frequency"],
             "unknowns": c["unknowns"], "antan": c["antan"], "antan_ms": c["ms"]}
        # NEC-2
        deck = nec_deck(open(os.path.join(folder, c["id"] + ".nec")).read(), grounded, real)
        dp = os.path.join(folder, c["id"] + "_cmp.nec")
        op = os.path.join(folder, c["id"] + "_cmp.out")
        open(dp, "w").write(deck)
        try:
            ms = run_nec(dp, op)
            feeds, pat, eff = parse_nec(op)
            gg, th, ph, fb = summarize(pat, grounded)
            r["nec"] = {"feeds": feeds, "gain_dbi": gg, "theta": th, "phi": ph, "fb_db": fb, "efficiency": eff, "ms": ms}
            sw_a = c["antan"].get("sweep", [])
            if sw_a and len(feeds) == 1:
                freqs = [p[0] for p in sw_a]
                sp = os.path.join(folder, c["id"] + "_sw.nec")
                so = os.path.join(folder, c["id"] + "_sw.out")
                open(sp, "w").write(nec_sweep_deck(open(os.path.join(folder, c["id"] + ".nec")).read() if not real else deck, freqs))
                run_nec(sp, so)
                zs = parse_nec_sweep(so)
                r["nec"]["sweep"] = [(f,) + z for f, z in zip(freqs, zs)]
        except Exception as e:  # noqa: BLE001
            r["nec"] = {"error": str(e)}
        # MININEC
        try:
            feeds, pat, ms, sweep = run_mininec(c)
            gg, th, ph, fb = summarize(pat, grounded)
            r["mininec"] = {"feeds": feeds, "gain_dbi": gg, "theta": th, "phi": ph, "fb_db": fb, "ms": ms, "sweep": sweep}
        except Exception as e:  # noqa: BLE001
            r["mininec"] = {"error": repr(e)}
        for k in ("antan", "nec", "mininec"):
            sw = r[k].get("sweep") if isinstance(r[k], dict) else None
            if sw:
                fr = resonance(sw, c["frequency"])
                r[k]["resonance"] = fr
                r[k]["swr_min"] = swr_min(sw)
                if fr:
                    # Rezonanstaki direnç (doğrusal ara değerleme).
                    for (fa, ra, _), (fb, rb, _) in zip(sw, sw[1:]):
                        if fa <= fr <= fb:
                            r[k]["r_res"] = ra + (rb - ra) * (fr - fa) / (fb - fa) if fb > fa else ra
                            break
        results.append(r)
        def fz(x):
            return "%.2f%+.2fj" % tuple(x["feeds"][0]) if "feeds" in x else x.get("error", "?")[:60]

        def fr(x):
            v = x.get("resonance") if isinstance(x, dict) else None
            return "%.4f" % (v / 1e6) if v else "-"
        print("%-15s AntAn %-18s G %6.2f fr %s | NEC %-18s G %6s fr %s | MININEC %-18s G %6s fr %s" % (
            c["id"], fz(r["antan"]), r["antan"]["gain_dbi"], fr(r["antan"]),
            fz(r["nec"]), "%.2f" % r["nec"]["gain_dbi"] if "gain_dbi" in r["nec"] else "-", fr(r["nec"]),
            fz(r["mininec"]), "%.2f" % r["mininec"]["gain_dbi"] if "gain_dbi" in r["mininec"] else "-", fr(r["mininec"])), flush=True)
    json.dump(results, open(os.path.join(folder, "results.json"), "w", encoding="utf-8"), indent=1, ensure_ascii=False)


if __name__ == "__main__":
    main()
