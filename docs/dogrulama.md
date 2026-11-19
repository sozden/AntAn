# AntAn'ın NEC-2 ve MININEC ile karşılaştırılması

*English: [validation.md](validation.md)*

Birçok kullanıcı AntAn'ın sonuçlarının bilinen anten programlarıyla ne kadar uyuştuğunu sordu.
Bu programların çoğu iki hesap çekirdeğinden birini kullanır:

| Hesap çekirdeği | Kullanan programlar |
|---|---|
| **NEC-2** | 4nec2, EZNEC (NEC-2 seçeneği), xnec2c, MMANA-NEC araçları |
| **MININEC** | MMANA-GAL, MMANA-GAL Pro, ELNEC, eski EZNEC sürümleri |

Bu yüzden birkaç programdan elle sayı aktarmak yerine **aynı modelleri** üç programda da
çalıştırdık: AntAn, başvuru NEC-2 çekirdeği ve başvuru MININEC çekirdeği. Modellerde teller,
çaplar, parça sayıları, frekanslar, besleme noktaları ve yükler birebir aynıydı. Sonra sonuçları
karşılaştırdık.

* **NEC-2:** `nec2c` 1.3.1. Özgün NEC-2 Fortran kodunun C çevirisidir (xnec2c'nin çekirdeği; 4nec2
  de aynı algoritmayı kullanır). Ubuntu 24.04'ün resmî paketinden kuruldu.
* **MININEC:** Ralf Schlatterbeck'in `pymininec` 1.2.0'ı. MININEC 3'ün Python'a sadık bir
  çevirisidir ve özgün BASIC programıyla satır satır doğrulanmıştır. MMANA-GAL'ın hesap çekirdeği
  MININEC'tir.
* **AntAn:** şu anki geliştirme sürümü, varsayılan ayarlar (dalga boyu başına 40 parça).

## Özet

| Karşılaştırma | Sonuç |
|---|---|
| Kazanç, NEC-2'ye göre | **medyan fark 0,06 dB**, en büyük 0,37 dB (20 anten) |
| Kazanç, MININEC'e göre | **0,01 dB** içinde aynı |
| Işıma verimi, NEC-2'ye göre | aynı (bakır dipolde %98,86 / %98,87; manyetik loop'ta %15,7 / %15,5) |
| İstif kazanç artışı, NEC-2'ye göre | **0,02 dB** içinde aynı (2 ve 4 Yagi) |
| Manyetik loop, NEC-2'ye göre | rezonans **%0,01** içinde, halka direnci 36,6 / 36,5 mΩ |
| Besleme empedansı, MININEC'e göre | pratikte aynı (manyetik loop dışında rezonans %0,05 içinde) |
| Rezonans frekansı, NEC-2'ye göre | AntAn rezonansı ince telli dipolde **%0,2**, borudan yapılmış antenlerde (Yagi, quad, Moxon, delta loop) **%0,6–1,4 yüksek** buluyor |
| Ön/arka oranı, NEC-2'ye göre | eğilim benzer, ama arkada derin sıfırı olan tasarımlarda birkaç dB farklı çıkabiliyor |
| Gerçek zemin | AntAn = MININEC/MMANA yöntemi. NEC-2'nin Sommerfeld zemini, yere yakın antenlerde besleme reaktansını farklı veriyor (notlara bakın). |

**Kısacası:** AntAn, MMANA-GAL'ın çekirdeği olan MININEC ile pratikte aynı sonucu veriyor. NEC-2
(EZNEC, 4nec2) ile kazançta, verimde, istifte ve diyagramlarda uyuşuyor. Fark rezonans
frekansının tam değerinde: borudan yapılmış antenlerde tipik olarak %1 civarında. Bu, MMANA-GAL ile
EZNEC/4nec2 arasında da sık görülen farkın aynısı.

### Hangisi doğru?

Tablo 5, parça sıklığı iki kat ve dört kat artırılınca ne olduğunu gösteriyor:

* NEC-2'nin sonucu neredeyse hiç değişmiyor. Varsayılan sıklıkta zaten oturmuş durumda.
* AntAn'ın ve MININEC'in sonuçları yavaşça NEC-2'ninkine yaklaşıyor.

Yani tam rezonans frekansı için daha iyi oturmuş sayı NEC-2'ninki. Boru elemanlı VHF/UHF
antenlerde AntAn'ın boyutlarının yaklaşık %1 "uzun" çıkacağını beklemek gerekir: AntAn'da
144,3 MHz'te rezonansa gelen bir Yagi, NEC-2'de 142,7 MHz civarında rezonansa geliyor. Kazanç,
diyagram ve istif aralıkları pratikte etkilenmiyor. Her modelleme programında olduğu gibi gerçek
antende son ayar (budama) payı bırakın. Bu yakınsamayı iyileştirmek, AntAn çözücüsünün açık duran
en önemli işi.

## Sonuçlar

### 1. Kazanç ve ön/arka oranı

| Anten | Kazanç AntAn | NEC-2 | MININEC | Δ NEC | Ö/A AntAn | NEC-2 | MININEC |
|---|---:|---:|---:|---:|---:|---:|---:|
| Yarım dalga dipol, 14,2 MHz, Ø2 mm, serbest uzay | 2,13 | 2,14 | 2,13 | -0,01 |  |  |  |
| Aynı dipol, bakır tel (kayıplı) | 2,08 | 2,09 | 2,08 | -0,01 |  |  |  |
| Dipol, kusursuz zeminden 10 m yüksekte | 8,04 | 8,04 | 8,03 | +0,00 |  |  |  |
| Dipol, ortalama topraktan 10 m yüksekte (εr 13, σ 0,005) | 6,96 | 7,11 | 6,96 | -0,15 |  |  |  |
| Ters V 7,1 MHz, 110°, Ø2 mm | 1,89 | 1,85 | 1,89 | +0,04 |  |  |  |
| Katlanmış dipol 144,3 MHz, Ø4 mm | 2,37 | 2,37 | 2,37 | +0,00 |  |  |  |
| λ/4 dikey, kusursuz zemin, 14,2 MHz, Ø10 mm | 5,15 | 5,15 | 5,14 | -0,00 |  |  |  |
| Ground plane, 4 radyal 45° eğik, 145 MHz | 1,87 | 2,24 | 1,86 | -0,37 |  |  |  |
| 2 elemanlı quad 144,3 MHz, Ø4 mm | 7,21 | 6,95 | 7,20 | +0,26 | 26,2 | 20,1 | 26,2 |
| Delta loop 14,2 MHz, Ø4 mm | 3,07 | 3,09 | 3,06 | -0,02 |  |  |  |
| Moxon dikdörtgeni 144,3 MHz, Ø4 mm | 6,05 | 5,73 | 6,05 | +0,32 | 28,7 | 20,9 | 28,6 |
| 3 elemanlı Yagi 144,3 MHz, Ø10 mm | 7,71 | 7,90 | 7,70 | -0,19 | 25,9 | 26,5 | 25,7 |
| 5 elemanlı Yagi 144,3 MHz, Ø10 mm | 10,83 | 11,03 | 10,82 | -0,20 | 20,8 | 15,5 | 21,0 |
| 7 elemanlı Yagi 432,2 MHz, Ø6 mm | 12,54 | 12,50 | 12,53 | +0,04 | 12,4 | 13,6 | 12,4 |
| 7 elemanlı LFA Yagi 144,3 MHz, Ø10 mm | 12,59 | 12,66 | 12,58 | -0,07 | 15,0 | 14,1 | 15,0 |
| Bobinle kısaltılmış dipol, yarı boy, 7,1 MHz (2 bobin, Q 200) | 0,53 | 0,53 | 0,53 | +0,00 |  |  |  |
| İki 5 el. Yagi dikey istif, 1,25 λ | 13,98 | 14,16 | 13,97 | -0,18 | 21,2 | 15,3 | 21,4 |
| Dört 5 el. Yagi, 2 × 2, 1,25 λ | 16,99 | 17,17 | 16,98 | -0,18 | 21,6 | 15,5 | 21,8 |

Kazanç dBi; Δ NEC = AntAn − NEC-2. Ö/A yalnız yönlü antenler için.

### 2. Besleme empedansı ve rezonans frekansı

| Anten | Z AntAn (Ω) | Z NEC-2 | Z MININEC | Rezonans AntAn (MHz) | NEC-2 | MININEC | Fark AntAn − NEC-2 (%) |
|---|---|---|---|---:|---:|---:|---:|
| Yarım dalga dipol, 14,2 MHz, Ø2 mm, serbest uzay | 71,1 − j4,2 | 71,9 − j1,0 | 71,2 − j4,3 | 14,241 | 14,209 | 14,241 | +0,22 |
| Aynı dipol, bakır tel (kayıplı) | 71,9 − j3,5 | 72,8 − j0,2 | 72,0 − j3,5 | 14,233 | 14,202 | 14,234 | +0,22 |
| Dipol, kusursuz zeminden 10 m yüksekte | 71,6 − j22,6 | 72,6 − j19,5 | 71,7 − j22,6 | 14,424 | 14,393 | 14,424 | +0,21 |
| Dipol, ortalama topraktan 10 m yüksekte (εr 13, σ 0,005) | 71,6 − j22,6 | 70,7 − j11,7 | 71,7 − j22,6 | 14,424 | 14,314 | 14,424 | +0,77 |
| Ters V 7,1 MHz, 110°, Ø2 mm | 54,8 + j4,1 | 56,2 + j26,1 | 54,8 + j4,0 | 7,082 | 6,989 | 7,082 | +1,34 |
| Katlanmış dipol 144,3 MHz, Ø4 mm | 338,1 + j116,8 | 333,3 + j121,1 | 338,1 + j116,0 | 137,163 | — | 137,219 | — |
| λ/4 dikey, kusursuz zemin, 14,2 MHz, Ø10 mm | 36,5 + j3,6 | 37,2 + j6,1 | 36,5 + j3,5 | 14,114 | 14,054 | 14,115 | +0,43 |
| Ground plane, 4 radyal 45° eğik, 145 MHz | 55,1 + j2,7 | 52,9 + j5,7 | 55,1 + j2,6 | 144,206 | 143,262 | 144,226 | +0,66 |
| 2 elemanlı quad 144,3 MHz, Ø4 mm | 122,1 − j4,5 | 138,0 + j3,6 | 122,2 − j4,5 | 144,928 | 143,739 | 144,936 | +0,83 |
| Delta loop 14,2 MHz, Ø4 mm | 123,9 + j60,7 | 126,7 + j100,7 | 124,1 + j60,8 | 13,864 | 13,675 | 13,865 | +1,39 |
| Moxon dikdörtgeni 144,3 MHz, Ø4 mm | 49,2 − j2,5 | 60,4 + j4,1 | 49,1 − j2,8 | 144,795 | 143,362 | 144,850 | +1,00 |
| 3 elemanlı Yagi 144,3 MHz, Ø10 mm | 28,3 + j0,0 | 23,1 + j9,7 | 28,5 − j0,1 | 144,293 | 142,732 | 144,317 | +1,09 |
| 5 elemanlı Yagi 144,3 MHz, Ø10 mm | 32,0 − j0,0 | 38,0 + j13,1 | 32,0 − j0,3 | 144,300 | 142,735 | 144,337 | +1,10 |
| 7 elemanlı Yagi 432,2 MHz, Ø6 mm | 51,9 − j0,0 | 25,3 + j16,1 | 54,3 + j0,8 | 432,195 | 427,286 | — | +1,15 |
| 7 elemanlı LFA Yagi 144,3 MHz, Ø10 mm | 47,5 − j6,7 | 81,2 − j14,5 | 47,4 − j7,3 | 144,991 | 148,108 | 145,104 | −2,10 |
| Bobinle kısaltılmış dipol, yarı boy, 7,1 MHz (2 bobin, Q 200) | 33,1 − j0,2 | 33,3 + j26,2 | 33,1 − j2,7 | 7,100 | 7,058 | 7,104 | +0,60 |
| İki 5 el. Yagi dikey istif, 1,25 λ | 32,5 − j0,9 | 38,3 + j12,7 | 32,5 − j1,2 | 144,410 | — | — | — |
| Dört 5 el. Yagi, 2 × 2, 1,25 λ | 32,6 − j1,4 | 38,1 + j11,8 | 32,7 − j1,7 | 144,477 | — | — | — |

Rezonans = reaktansın tasarım frekansına en yakın sıfırdan geçtiği frekans (±%5 içinde). İstiflerde tek beslemenin empedansı; Z tasarım frekansındadır.

### 3. Manyetik loop

| Büyüklük | AntAn | NEC-2 | MININEC |
|---|---:|---:|---:|
| Rezonans, doğrudan besleme (MHz) | 7,1000 | 7,0995 | 7,0630 |
| Rezonansta halka direnci (mΩ) | 36,6 | 36,5 | 36,5 |
| Verim (iletken kaybı) | 15,7 % | 15,5 % | — |
| Verim, Ø2 mm bakır dipol (karşılaştırma için) | 98,86 % | 98,87 % | — |
| Kuplaj halkalı: en düşük SWR frekansı (MHz) | 7,1000 | 7,1018 | 7,0630 |
| Kuplaj halkalı: en düşük SWR (aynı boyutlar) | 1,00 | 1,48 | 1,00 |

### 4. İstif kazanç artışı (tek 5 elemanlı Yagi'ye göre)

| İstif | AntAn | NEC-2 | MININEC |
|---|---:|---:|---:|
| İki 5 el. Yagi dikey istif, 1,25 λ | +3,15 dB | +3,13 dB | +3,15 dB |
| Dört 5 el. Yagi, 2 × 2, 1,25 λ | +6,16 dB | +6,14 dB | +6,16 dB |

### 5. Yakınsama: parça sıklığına göre besleme empedansı

| Anten | λ başına parça | AntAn | NEC-2 | MININEC |
|---|---:|---|---|---|
| Yarım dalga dipol, 14,2 MHz, Ø2 mm, serbest uzay | 40 | 71,1 − j4,2 | 71,9 − j1,0 | 71,2 − j4,3 |
| Yarım dalga dipol, 14,2 MHz, Ø2 mm, serbest uzay | 80 | 71,5 − j3,1 | 72,0 − j0,7 | 71,6 − j3,2 |
| Yarım dalga dipol, 14,2 MHz, Ø2 mm, serbest uzay | 160 | 71,7 − j2,1 | 72,0 − j0,5 | 71,8 − j2,3 |
| Ters V 7,1 MHz, 110°, Ø2 mm | 40 | 54,8 + j4,1 | 56,2 + j26,1 | 54,8 + j4,0 |
| Ters V 7,1 MHz, 110°, Ø2 mm | 80 | 55,1 + j15,5 | 56,1 + j26,3 | 55,2 + j15,4 |
| Ters V 7,1 MHz, 110°, Ø2 mm | 160 | 55,3 + j21,1 | 56,3 + j26,5 | 55,4 + j21,0 |
| 2 elemanlı quad 144,3 MHz, Ø4 mm | 40 | 122,1 − j4,5 | 138,0 + j3,6 | 122,2 − j4,5 |
| 2 elemanlı quad 144,3 MHz, Ø4 mm | 80 | 132,6 − j0,4 | 137,9 + j2,5 | 132,8 − j0,6 |
| 2 elemanlı quad 144,3 MHz, Ø4 mm | 160 | 136,5 + j0,4 | 138,1 + j1,8 | 136,7 + j0,2 |
| 5 elemanlı Yagi 144,3 MHz, Ø10 mm | 40 | 32,0 − j0,0 | 38,0 + j13,1 | 32,0 − j0,3 |
| 5 elemanlı Yagi 144,3 MHz, Ø10 mm | 80 | 33,7 + j5,0 | 39,8 + j14,5 | 33,4 + j4,0 |
| 5 elemanlı Yagi 144,3 MHz, Ø10 mm | 160 | 35,9 + j9,2 | 41,2 + j15,3 | 34,5 + j6,0 |
| 7 elemanlı Yagi 432,2 MHz, Ø6 mm | 40 | 51,9 − j0,0 | 25,3 + j16,1 | 54,3 + j0,8 |
| 7 elemanlı Yagi 432,2 MHz, Ø6 mm | 80 | 38,5 + j2,5 | 23,7 + j19,4 | 45,6 + j1,2 |
| 7 elemanlı Yagi 432,2 MHz, Ø6 mm | 160 | 29,6 + j10,0 | 22,5 + j23,5 | 41,8 + j2,8 |

## Tek tek durumlarla ilgili notlar

* **Ters V, ground plane:** AntAn ve MININEC beslemeyi tam tellerin birleştiği noktaya koyar.
  NEC-2 beslemeyi yalnız bir parçanın ortasına koyabilir; bu yüzden NEC'te besleme tepeden ya da
  tabandan bir parça uzakta durur. Bu iki durumdaki reaktans farkının bir kısmı buradan gelir (NEC'te
  bu antenler zaten böyle modellenir).
* **Ortalama toprak üzerinde dipol:** AntAn, MININEC ve MMANA gibi, akımları kusursuz zemin
  görüntüsüyle hesaplar. Gerçek toprak yalnız ışıma diyagramını etkiler; besleme empedansı kusursuz
  zemindekiyle aynı çıkar. Burada kullanılan Sommerfeld-Norton zeminli NEC-2 (GN 2) toprağın
  akımlara etkisini de hesaba katar: reaktans farklıdır (−j22,6 ve −j11,7 Ω), kazanç ise yalnız
  0,15 dB farklıdır. Bu sınırlama AntAn'ın README'sinde yazılıdır.
* **Ön/arka oranı:** Ö/A, ana lobun küçük bir arka loba oranıdır. Arkadaki sıfırın biraz kayması
  bu oranı birkaç dB değiştirir. Bu yüzden Ö/A'sı 20 dB'nin üstündeki tasarımlarda çekirdekler
  arasında 5–8 dB fark normaldir. Ana lobun yönü ve büyüklüğü yakından uyuşur.
* **Manyetik loop:** burada AntAn ve NEC-2 neredeyse birebir uyuşuyor; MININEC rezonansı %0,5 aşağıda
  buluyor. Kuplaj halkalı loop o kadar dar bantlı ki (SWR < 2 yaklaşık 2 kHz) *aynı* kuplaj halkası
  boyutlarıyla bulunan en düşük SWR çekirdekler arasında farklı çıkıyor (1,00 / 1,48). "Kuplaj
  halkasını eşle" düğmesi her durumda doğru boyu bulur.
* **LFA:** AntAn'ın rezonansı NEC-2'ninkinden düşük bulduğu tek durum (−%2,1). Halka beslemeli
  sürülen elemanda birbirine çok yakın paralel iletkenler var; bu iki çekirdek türü için de zor
  bir geometri.
* **70 cm Yagi:** MININEC'te reaktans ±%5 boyunca neredeyse düz kaldığından taramasından rezonans
  okunamadı.

## Karşılaştırma nasıl yapıldı

1. `cargo run --release -p antan-core --example validate -- <klasör>` AntAn'ın şablonlarıyla 20
   deney modelini kurar. Eleman boyları bum düzeltmesiz elektriksel boylardır; aksi yazmadıkça
   malzemeler kayıpsız iletkendir. Program her modeli çözer, tarar (±%5, manyetik loop'ta ±%1,2)
   ve modeli (JSON) ve NEC-2 kart dosyasını yazar.
2. `python tools/validation/compare.py <klasör>` aynı modelleri `nec2c` ve `pymininec` ile
   çalıştırır:
   * NEC dosyaları AntAn'ın kendi *NEC-2 olarak dışa aktar* çıktısıdır. Yalnız desen kartı (2°
     ızgara) ve gerçek zeminde zemin kartı (GN 2, Sommerfeld-Norton) değiştirildi.
   * MININEC'te her besleme ve yük AntAn'ınkine en yakın düğüme konur. Birden çok telin birleştiği
     noktada besleme birleşimdeki bütün düğümlere uygulanır.
3. `python tools/validation/report.py <klasör> [x1 x2 x4 klasörleri]` aşağıdaki tabloları yazar.
   `SEGMULT=2` ya da `4` ile validate örneği bütün parça sayılarını katlar (yakınsama tablosu).

Kazanç 2° ızgaradaki en büyük değerdir. Ö/A, EZNEC'le aynı tanımla hesaplandı: serbest uzayda tam
ters yön, zemin üzerinde aynı yükseliş açısında ters azimut.
