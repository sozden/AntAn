"""results.json'dan (compare.py çıktısı) markdown tabloları üretir.

    python report.py <klasör> [yakınsama klasörü x1 x2 x4 ...]

Çıktı: <klasör>/tables_en.md ve tables_tr.md.
"""

import json
import os
import statistics
import sys

DIRECTIONAL = {"quad2", "moxon", "yagi3", "yagi5", "yagi7_70", "lfa7", "stack2", "stack4"}


def z(x, k=0):
    if not isinstance(x, dict) or "feeds" not in x:
        return "—"
    r, i = x["feeds"][k]
    return "%.1f %s j%.1f" % (r, "+" if i >= 0 else "−", abs(i))


def num(v, fmt):
    return fmt % v if v is not None else "—"


def tables(results, lang, conv):
    tr = lang == "tr"
    T = (lambda en, t: t) if tr else (lambda en, t: en)
    dec = (lambda s: s.replace(".", ",")) if tr else (lambda s: s)
    title = (lambda r: r["title_tr"]) if tr else (lambda r: r["title_en"])
    out = []

    # 1. Kazanç ve ön/arka
    out.append("### " + T("1. Gain and front-to-back ratio", "1. Kazanç ve ön/arka oranı"))
    out.append("")
    out.append(T("| Antenna | Gain AntAn | NEC-2 | MININEC | Δ NEC | F/B AntAn | NEC-2 | MININEC |",
                 "| Anten | Kazanç AntAn | NEC-2 | MININEC | Δ NEC | Ö/A AntAn | NEC-2 | MININEC |"))
    out.append("|---|---:|---:|---:|---:|---:|---:|---:|")
    dg_nec, dg_min = [], []
    for r in results:
        if r["id"].startswith("magloop"):
            continue
        a, n, m = r["antan"], r["nec"], r["mininec"]
        ga, gn, gm = a["gain_dbi"], n.get("gain_dbi"), m.get("gain_dbi")
        if gn is not None:
            dg_nec.append(abs(ga - gn))
        if gm is not None:
            dg_min.append(abs(ga - gm))
        d = r["id"] in DIRECTIONAL
        fb = lambda x: dec(num(x.get("fb_db"), "%.1f")) if d and isinstance(x, dict) else ""
        out.append("| %s | %s | %s | %s | %s | %s | %s | %s |" % (
            title(r), dec("%.2f" % ga), dec(num(gn, "%.2f")), dec(num(gm, "%.2f")),
            dec(num(ga - gn if gn is not None else None, "%+.2f")), fb(a), fb(n), fb(m)))
    out.append("")
    out.append(T("Gain in dBi; Δ NEC = AntAn − NEC-2. F/B only for directional antennas.",
                 "Kazanç dBi; Δ NEC = AntAn − NEC-2. Ö/A yalnız yönlü antenler için."))
    out.append("")
    stats = (max(dg_nec), statistics.median(dg_nec), max(dg_min), statistics.median(dg_min))

    # 2. Empedans ve rezonans
    out.append("### " + T("2. Feed impedance and resonant frequency", "2. Besleme empedansı ve rezonans frekansı"))
    out.append("")
    out.append(T("| Antenna | Z AntAn (Ω) | Z NEC-2 | Z MININEC | Resonance AntAn (MHz) | NEC-2 | MININEC | AntAn vs NEC-2 |",
                 "| Anten | Z AntAn (Ω) | Z NEC-2 | Z MININEC | Rezonans AntAn (MHz) | NEC-2 | MININEC | AntAn − NEC-2 |"))
    out.append("|---|---|---|---|---:|---:|---:|---:|")
    shifts = []
    for r in results:
        if r["id"].startswith("magloop"):
            continue
        a, n, m = r["antan"], r["nec"], r["mininec"]
        fa, fn, fm = a.get("resonance"), n.get("resonance"), m.get("resonance") if isinstance(m, dict) else None
        sh = (fa / fn - 1) * 100 if fa and fn else None
        if sh is not None:
            shifts.append((r["id"], sh))
        out.append("| %s | %s | %s | %s | %s | %s | %s | %s |" % (
            title(r), dec(z(a)), dec(z(n)), dec(z(m)),
            dec(num(fa and fa / 1e6, "%.3f")), dec(num(fn and fn / 1e6, "%.3f")), dec(num(fm and fm / 1e6, "%.3f")),
            dec(num(sh, "%+.2f %%")) if not tr else (("%%%+.2f" % sh).replace(".", ",").replace("%+", "+").replace("%-", "−") if sh is not None else "—")))
    out.append("")
    out.append(T("Resonance = the frequency where the reactance crosses zero nearest the design frequency (within ±5 %). "
                 "Stacks: impedance of one feed; Z is given at the design frequency.",
                 "Rezonans = reaktansın tasarım frekansına en yakın sıfırdan geçtiği frekans (±%5 içinde). "
                 "İstiflerde tek beslemenin empedansı; Z tasarım frekansındadır."))
    out.append("")

    # 3. Manyetik loop
    out.append("### " + T("3. Magnetic loop", "3. Manyetik loop"))
    out.append("")
    out.append(T("| Quantity | AntAn | NEC-2 | MININEC |", "| Büyüklük | AntAn | NEC-2 | MININEC |"))
    out.append("|---|---:|---:|---:|")
    by = {r["id"]: r for r in results}
    d, c = by.get("magloop_direct"), by.get("magloop")
    if d:
        g = lambda k, f: d[k].get(f) if isinstance(d[k], dict) else None
        out.append("| %s | %s | %s | %s |" % (T("Resonance, direct feed (MHz)", "Rezonans, doğrudan besleme (MHz)"),
                   *[dec(num(g(k, "resonance") and g(k, "resonance") / 1e6, "%.4f")) for k in ("antan", "nec", "mininec")]))
        out.append("| %s | %s | %s | %s |" % (T("Loop resistance at resonance (mΩ)", "Rezonansta halka direnci (mΩ)"),
                   *[dec(num(g(k, "r_res") and g(k, "r_res") * 1e3, "%.1f")) for k in ("antan", "nec", "mininec")]))
        out.append("| %s | %s | %s | — |" % (T("Efficiency (conductor loss)", "Verim (iletken kaybı)"),
                   dec(num(d["antan"].get("efficiency") and d["antan"]["efficiency"] * 100, "%.1f %%")),
                   dec(num(d["nec"].get("efficiency") and d["nec"]["efficiency"] * 100, "%.1f %%"))))
    cu = by.get("dipole_cu")
    if cu:
        out.append("| %s | %s | %s | — |" % (T("Efficiency, Ø2 mm copper dipole (for comparison)", "Verim, Ø2 mm bakır dipol (karşılaştırma için)"),
                   dec(num(cu["antan"].get("efficiency") and cu["antan"]["efficiency"] * 100, "%.2f %%")),
                   dec(num(cu["nec"].get("efficiency") and cu["nec"]["efficiency"] * 100, "%.2f %%"))))
    if c:
        def best(k):
            v = c[k].get("swr_min") if isinstance(c[k], dict) else None
            return (dec("%.4f" % (v[0] / 1e6)), dec("%.2f" % v[1])) if v else ("—", "—")
        bs = [best(k) for k in ("antan", "nec", "mininec")]
        out.append("| %s | %s | %s | %s |" % (T("Coupling loop: frequency of lowest SWR (MHz)", "Kuplaj halkalı: en düşük SWR frekansı (MHz)"), *[b[0] for b in bs]))
        out.append("| %s | %s | %s | %s |" % (T("Coupling loop: lowest SWR (same dimensions)", "Kuplaj halkalı: en düşük SWR (aynı boyutlar)"), *[b[1] for b in bs]))
    out.append("")

    # 4. İstif kazanç artışı
    if "stack2" in by and "yagi5" in by:
        out.append("### " + T("4. Stacking gain (over one 5-element Yagi)", "4. İstif kazanç artışı (tek 5 elemanlı Yagi'ye göre)"))
        out.append("")
        out.append(T("| Stack | AntAn | NEC-2 | MININEC |", "| İstif | AntAn | NEC-2 | MININEC |"))
        out.append("|---|---:|---:|---:|")
        for sid in ("stack2", "stack4"):
            s = by[sid]
            vals = []
            for k in ("antan", "nec", "mininec"):
                a, b = s[k].get("gain_dbi"), by["yagi5"][k].get("gain_dbi")
                vals.append(dec(num(a - b if a is not None and b is not None else None, "+%.2f dB")))
            out.append("| %s | %s | %s | %s |" % (title(s), *vals))
        out.append("")

    # 5. Yakınsama
    if conv:
        out.append("### " + T("5. Convergence: feed impedance versus segment density", "5. Yakınsama: parça sıklığına göre besleme empedansı"))
        out.append("")
        ids = ["dipole", "invv", "quad2", "yagi5", "yagi7_70"]
        out.append(T("| Antenna | Segments per λ | AntAn | NEC-2 | MININEC |", "| Anten | λ başına parça | AntAn | NEC-2 | MININEC |"))
        out.append("|---|---:|---|---|---|")
        for i in ids:
            for mult, res in conv:
                rr = {x["id"]: x for x in res}.get(i)
                if not rr:
                    continue
                out.append("| %s | %d | %s | %s | %s |" % (title(rr), 40 * mult, dec(z(rr["antan"])), dec(z(rr["nec"])), dec(z(rr["mininec"]))))
        out.append("")
    return "\n".join(out), stats, shifts


def main():
    folder = sys.argv[1]
    results = json.load(open(os.path.join(folder, "results.json"), encoding="utf-8"))
    conv = []
    for i, cf in enumerate(sys.argv[2:]):
        conv.append((2 ** i, json.load(open(os.path.join(cf, "results.json"), encoding="utf-8"))))
    for lang in ("en", "tr"):
        text, stats, shifts = tables(results, lang, conv)
        open(os.path.join(folder, "tables_%s.md" % lang), "w", encoding="utf-8").write(text)
    print("|ΔG| NEC max %.2f medyan %.2f; MININEC max %.2f medyan %.2f" % stats)
    for s in shifts:
        print("kayma %-10s %+.2f %%" % s)


if __name__ == "__main__":
    main()
