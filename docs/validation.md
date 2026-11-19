# AntAn compared with NEC-2 and MININEC

*Türkçesi: [dogrulama.md](dogrulama.md)*

Many users asked how AntAn's results compare with the well-known antenna programs. Most of those
programs are built on one of two calculation engines:

| Engine | Used by |
|---|---|
| **NEC-2** | 4nec2, EZNEC (NEC-2 option), xnec2c, MMANA-NEC tools |
| **MININEC** | MMANA-GAL, MMANA-GAL Pro, ELNEC, early EZNEC versions |

So instead of copying numbers by hand from a few programs, we ran **the same models** — identical
wires, diameters, segment counts, frequencies, feed points and loads — through AntAn, the reference
NEC-2 engine and the reference MININEC engine. We then compared the results.

* **NEC-2:** `nec2c` 1.3.1, a C translation of the original NEC-2 Fortran code (the engine behind
  xnec2c; 4nec2 uses the same algorithm). Installed from the official Ubuntu 24.04 package.
* **MININEC:** `pymininec` 1.2.0 by Ralf Schlatterbeck. It is a faithful Python port of
  MININEC 3, checked line by line against the original BASIC program. MMANA-GAL's calculation
  core is MININEC.
* **AntAn:** the current development version, default settings (40 segments per wavelength).

## Summary

| Comparison | Result |
|---|---|
| Gain vs NEC-2 | **median difference 0.06 dB**, largest 0.37 dB (20 antennas) |
| Gain vs MININEC | identical within **0.01 dB** |
| Radiation efficiency vs NEC-2 | identical (98.86 % / 98.87 % copper dipole; 15.7 % / 15.5 % magnetic loop) |
| Stacking gain vs NEC-2 | identical within **0.02 dB** (2 and 4 Yagis) |
| Magnetic loop vs NEC-2 | resonance within **0.01 %**, loop resistance 36.6 / 36.5 mΩ |
| Feed impedance vs MININEC | practically identical (resonance within 0.05 % except the magnetic loop) |
| Resonant frequency vs NEC-2 | AntAn finds resonance **0.2 % higher** for a thin-wire dipole and **0.6–1.4 % higher** for antennas made of tubing (Yagi, quad, Moxon, delta loop) |
| Front-to-back ratio vs NEC-2 | similar trend, but can differ by several dB in designs with a deep rear null |
| Real ground | AntAn = MININEC/MMANA method. NEC-2's Sommerfeld ground gives a different feed reactance near the ground (see notes). |

**In short:** AntAn gives practically the same answers as MININEC, the engine of MMANA-GAL. Against
NEC-2 (EZNEC, 4nec2) it agrees on gain, efficiency, stacking and patterns. The difference is in
the exact resonant frequency: for antennas made of tubing it is typically about 1 %. This is the
same difference that is commonly seen between MMANA-GAL and EZNEC/4nec2.

### Which one is right?

Table 5 shows what happens when the segment density is doubled and doubled again:

* NEC-2's answer hardly changes. It has already converged at the default density.
* AntAn's and MININEC's answers move slowly towards NEC-2's.

So for the exact resonant frequency, NEC-2's number is the better-converged one. AntAn's
dimensions should be expected to come out about 1 % "long" at VHF/UHF for tube elements: a Yagi
resonant at 144.3 MHz in AntAn resonates around 142.7 MHz in NEC-2. Gain, pattern shape and
stacking distances are not affected in any practically relevant way. As with every modelling
program, plan for final trimming on the real antenna. Improving this convergence is the main
open item for AntAn's solver.

## Results

### 1. Gain and front-to-back ratio

| Antenna | Gain AntAn | NEC-2 | MININEC | Δ NEC | F/B AntAn | NEC-2 | MININEC |
|---|---:|---:|---:|---:|---:|---:|---:|
| Half-wave dipole, 14.2 MHz, Ø2 mm, free space | 2.13 | 2.14 | 2.13 | -0.01 |  |  |  |
| Same dipole, copper wire (loss) | 2.08 | 2.09 | 2.08 | -0.01 |  |  |  |
| Dipole 10 m over perfect ground | 8.04 | 8.04 | 8.03 | +0.00 |  |  |  |
| Dipole 10 m over average ground (εr 13, σ 0.005) | 6.96 | 7.11 | 6.96 | -0.15 |  |  |  |
| Inverted V 7.1 MHz, 110°, Ø2 mm | 1.89 | 1.85 | 1.89 | +0.04 |  |  |  |
| Folded dipole 144.3 MHz, Ø4 mm | 2.37 | 2.37 | 2.37 | +0.00 |  |  |  |
| λ/4 vertical on perfect ground, 14.2 MHz, Ø10 mm | 5.15 | 5.15 | 5.14 | -0.00 |  |  |  |
| Ground plane, 4 radials 45° down, 145 MHz | 1.87 | 2.24 | 1.86 | -0.37 |  |  |  |
| 2-element quad 144.3 MHz, Ø4 mm | 7.21 | 6.95 | 7.20 | +0.26 | 26.2 | 20.1 | 26.2 |
| Delta loop 14.2 MHz, Ø4 mm | 3.07 | 3.09 | 3.06 | -0.02 |  |  |  |
| Moxon rectangle 144.3 MHz, Ø4 mm | 6.05 | 5.73 | 6.05 | +0.32 | 28.7 | 20.9 | 28.6 |
| 3-element Yagi 144.3 MHz, Ø10 mm | 7.71 | 7.90 | 7.70 | -0.19 | 25.9 | 26.5 | 25.7 |
| 5-element Yagi 144.3 MHz, Ø10 mm | 10.83 | 11.03 | 10.82 | -0.20 | 20.8 | 15.5 | 21.0 |
| 7-element Yagi 432.2 MHz, Ø6 mm | 12.54 | 12.50 | 12.53 | +0.04 | 12.4 | 13.6 | 12.4 |
| 7-element LFA Yagi 144.3 MHz, Ø10 mm | 12.59 | 12.66 | 12.58 | -0.07 | 15.0 | 14.1 | 15.0 |
| Coil-loaded dipole, half size, 7.1 MHz (2 coils, Q 200) | 0.53 | 0.53 | 0.53 | +0.00 |  |  |  |
| Two 5-el. Yagis stacked vertically, 1.25 λ | 13.98 | 14.16 | 13.97 | -0.18 | 21.2 | 15.3 | 21.4 |
| Four 5-el. Yagis, 2 × 2, 1.25 λ | 16.99 | 17.17 | 16.98 | -0.18 | 21.6 | 15.5 | 21.8 |

Gain in dBi; Δ NEC = AntAn − NEC-2. F/B only for directional antennas.

### 2. Feed impedance and resonant frequency

| Antenna | Z AntAn (Ω) | Z NEC-2 | Z MININEC | Resonance AntAn (MHz) | NEC-2 | MININEC | AntAn − NEC-2 |
|---|---|---|---|---:|---:|---:|---:|
| Half-wave dipole, 14.2 MHz, Ø2 mm, free space | 71.1 − j4.2 | 71.9 − j1.0 | 71.2 − j4.3 | 14.241 | 14.209 | 14.241 | +0.22 % |
| Same dipole, copper wire (loss) | 71.9 − j3.5 | 72.8 − j0.2 | 72.0 − j3.5 | 14.233 | 14.202 | 14.234 | +0.22 % |
| Dipole 10 m over perfect ground | 71.6 − j22.6 | 72.6 − j19.5 | 71.7 − j22.6 | 14.424 | 14.393 | 14.424 | +0.21 % |
| Dipole 10 m over average ground (εr 13, σ 0.005) | 71.6 − j22.6 | 70.7 − j11.7 | 71.7 − j22.6 | 14.424 | 14.314 | 14.424 | +0.77 % |
| Inverted V 7.1 MHz, 110°, Ø2 mm | 54.8 + j4.1 | 56.2 + j26.1 | 54.8 + j4.0 | 7.082 | 6.989 | 7.082 | +1.34 % |
| Folded dipole 144.3 MHz, Ø4 mm | 338.1 + j116.8 | 333.3 + j121.1 | 338.1 + j116.0 | 137.163 | — | 137.219 | — |
| λ/4 vertical on perfect ground, 14.2 MHz, Ø10 mm | 36.5 + j3.6 | 37.2 + j6.1 | 36.5 + j3.5 | 14.114 | 14.054 | 14.115 | +0.43 % |
| Ground plane, 4 radials 45° down, 145 MHz | 55.1 + j2.7 | 52.9 + j5.7 | 55.1 + j2.6 | 144.206 | 143.262 | 144.226 | +0.66 % |
| 2-element quad 144.3 MHz, Ø4 mm | 122.1 − j4.5 | 138.0 + j3.6 | 122.2 − j4.5 | 144.928 | 143.739 | 144.936 | +0.83 % |
| Delta loop 14.2 MHz, Ø4 mm | 123.9 + j60.7 | 126.7 + j100.7 | 124.1 + j60.8 | 13.864 | 13.675 | 13.865 | +1.39 % |
| Moxon rectangle 144.3 MHz, Ø4 mm | 49.2 − j2.5 | 60.4 + j4.1 | 49.1 − j2.8 | 144.795 | 143.362 | 144.850 | +1.00 % |
| 3-element Yagi 144.3 MHz, Ø10 mm | 28.3 + j0.0 | 23.1 + j9.7 | 28.5 − j0.1 | 144.293 | 142.732 | 144.317 | +1.09 % |
| 5-element Yagi 144.3 MHz, Ø10 mm | 32.0 − j0.0 | 38.0 + j13.1 | 32.0 − j0.3 | 144.300 | 142.735 | 144.337 | +1.10 % |
| 7-element Yagi 432.2 MHz, Ø6 mm | 51.9 − j0.0 | 25.3 + j16.1 | 54.3 + j0.8 | 432.195 | 427.286 | — | +1.15 % |
| 7-element LFA Yagi 144.3 MHz, Ø10 mm | 47.5 − j6.7 | 81.2 − j14.5 | 47.4 − j7.3 | 144.991 | 148.108 | 145.104 | -2.10 % |
| Coil-loaded dipole, half size, 7.1 MHz (2 coils, Q 200) | 33.1 − j0.2 | 33.3 + j26.2 | 33.1 − j2.7 | 7.100 | 7.058 | 7.104 | +0.60 % |
| Two 5-el. Yagis stacked vertically, 1.25 λ | 32.5 − j0.9 | 38.3 + j12.7 | 32.5 − j1.2 | 144.410 | — | — | — |
| Four 5-el. Yagis, 2 × 2, 1.25 λ | 32.6 − j1.4 | 38.1 + j11.8 | 32.7 − j1.7 | 144.477 | — | — | — |

Resonance = the frequency where the reactance crosses zero nearest the design frequency (within ±5 %). Stacks: impedance of one feed; Z is given at the design frequency.

### 3. Magnetic loop

| Quantity | AntAn | NEC-2 | MININEC |
|---|---:|---:|---:|
| Resonance, direct feed (MHz) | 7.1000 | 7.0995 | 7.0630 |
| Loop resistance at resonance (mΩ) | 36.6 | 36.5 | 36.5 |
| Efficiency (conductor loss) | 15.7 % | 15.5 % | — |
| Efficiency, Ø2 mm copper dipole (for comparison) | 98.86 % | 98.87 % | — |
| Coupling loop: frequency of lowest SWR (MHz) | 7.1000 | 7.1018 | 7.0630 |
| Coupling loop: lowest SWR (same dimensions) | 1.00 | 1.48 | 1.00 |

### 4. Stacking gain (over one 5-element Yagi)

| Stack | AntAn | NEC-2 | MININEC |
|---|---:|---:|---:|
| Two 5-el. Yagis stacked vertically, 1.25 λ | +3.15 dB | +3.13 dB | +3.15 dB |
| Four 5-el. Yagis, 2 × 2, 1.25 λ | +6.16 dB | +6.14 dB | +6.16 dB |

### 5. Convergence: feed impedance versus segment density

| Antenna | Segments per λ | AntAn | NEC-2 | MININEC |
|---|---:|---|---|---|
| Half-wave dipole, 14.2 MHz, Ø2 mm, free space | 40 | 71.1 − j4.2 | 71.9 − j1.0 | 71.2 − j4.3 |
| Half-wave dipole, 14.2 MHz, Ø2 mm, free space | 80 | 71.5 − j3.1 | 72.0 − j0.7 | 71.6 − j3.2 |
| Half-wave dipole, 14.2 MHz, Ø2 mm, free space | 160 | 71.7 − j2.1 | 72.0 − j0.5 | 71.8 − j2.3 |
| Inverted V 7.1 MHz, 110°, Ø2 mm | 40 | 54.8 + j4.1 | 56.2 + j26.1 | 54.8 + j4.0 |
| Inverted V 7.1 MHz, 110°, Ø2 mm | 80 | 55.1 + j15.5 | 56.1 + j26.3 | 55.2 + j15.4 |
| Inverted V 7.1 MHz, 110°, Ø2 mm | 160 | 55.3 + j21.1 | 56.3 + j26.5 | 55.4 + j21.0 |
| 2-element quad 144.3 MHz, Ø4 mm | 40 | 122.1 − j4.5 | 138.0 + j3.6 | 122.2 − j4.5 |
| 2-element quad 144.3 MHz, Ø4 mm | 80 | 132.6 − j0.4 | 137.9 + j2.5 | 132.8 − j0.6 |
| 2-element quad 144.3 MHz, Ø4 mm | 160 | 136.5 + j0.4 | 138.1 + j1.8 | 136.7 + j0.2 |
| 5-element Yagi 144.3 MHz, Ø10 mm | 40 | 32.0 − j0.0 | 38.0 + j13.1 | 32.0 − j0.3 |
| 5-element Yagi 144.3 MHz, Ø10 mm | 80 | 33.7 + j5.0 | 39.8 + j14.5 | 33.4 + j4.0 |
| 5-element Yagi 144.3 MHz, Ø10 mm | 160 | 35.9 + j9.2 | 41.2 + j15.3 | 34.5 + j6.0 |
| 7-element Yagi 432.2 MHz, Ø6 mm | 40 | 51.9 − j0.0 | 25.3 + j16.1 | 54.3 + j0.8 |
| 7-element Yagi 432.2 MHz, Ø6 mm | 80 | 38.5 + j2.5 | 23.7 + j19.4 | 45.6 + j1.2 |
| 7-element Yagi 432.2 MHz, Ø6 mm | 160 | 29.6 + j10.0 | 22.5 + j23.5 | 41.8 + j2.8 |

## Notes on individual cases

* **Inverted V, ground plane:** AntAn and MININEC place the source exactly on the junction of the
  wires. NEC-2 can only place a source at the centre of a segment, so the NEC source sits one
  segment away from the apex or base. This accounts for part of the reactance difference in these
  two cases (it is the usual way such antennas are modelled in NEC).
* **Dipole over average ground:** AntAn, like MININEC and MMANA, calculates the currents with a
  perfect-ground image. Real soil only affects the radiation pattern, so the feed impedance is the
  same as over perfect ground. NEC-2 with Sommerfeld-Norton ground (GN 2, used here) also includes
  the effect of the soil on the currents: the reactance differs (−j22.6 vs −j11.7 Ω), while gain
  differs by only 0.15 dB. This limitation is listed in AntAn's README.
* **Front-to-back ratio:** F/B is the ratio of the main lobe to a small rear lobe. A small change
  of the rear null changes it by several dB, so F/B differences of 5–8 dB between engines for
  designs with F/B above 20 dB are normal. The direction and size of the main lobe agree closely.
* **Magnetic loop:** here AntAn and NEC-2 agree almost exactly. MININEC places the resonance
  0.5 % lower. The loop with the coupling loop is so narrow-band (SWR < 2 over about 2 kHz) that
  the lowest SWR found with *the same* coupling-loop dimensions differs between engines (1.00 / 1.48).
  The "Match coupling loop" button finds the right size for each case.
* **LFA:** the only case where AntAn's resonance is lower than NEC-2's (−2.1 %). The loop-fed
  driven element has closely spaced parallel conductors, which is a difficult geometry for both
  engine types.
* **70 cm Yagi:** MININEC's reactance stays almost flat over ±5 %, so no resonance could be read
  from its sweep.

## How the comparison was made

1. `cargo run --release -p antan-core --example validate -- <folder>` builds the 20 test models
   with AntAn's templates. Element dimensions are electrical lengths with no boom correction, and
   materials are perfect conductors unless stated. The program then solves each model, sweeps it
   (±5 %, magnetic loop ±1.2 %) and writes the model (JSON) and a NEC-2 card file.
2. `python tools/validation/compare.py <folder>` runs the same models in `nec2c` and `pymininec`:
   * The NEC decks are AntAn's own *Export as NEC-2* output. Only the pattern card (2° grid) and,
     for real ground, the ground card (GN 2, Sommerfeld-Norton) were changed.
   * In MININEC each source and load is put on the node nearest to AntAn's. At junctions of
     several wires the source is applied to all junction nodes.
3. `python tools/validation/report.py <folder> [x1 x2 x4 folders]` writes the tables below. With
   `SEGMULT=2` or `4` the validate example multiplies all segment counts (convergence table).

Gain is the maximum on a 2° grid. F/B uses the same definition as EZNEC: the exactly opposite
direction in free space, and the same elevation at the opposite azimuth over ground.
