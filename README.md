# AntAn

Eznec/MMANA tarzı bir anten analiz ve hesaplama programı — Rust ile, sıfırdan.

## Neler var (v0.1 iskelet)

- **antan-core**: arayüzden bağımsız hesap motoru
  - İnce tel moment yöntemi (MoM) çözücüsü, serbest uzay, karışık potansiyel formülasyonu
  - Boru/çubuk elemanlar: dış çap + et kalınlığı (içi dolu da olabilir), malzeme seçimi
    (alüminyum 6063/6061, bakır, pirinç, paslanmaz, kayıpsız) → deri etkisi dahil iç empedans
  - Uzak alan, kazanç/yönlülük, 3B örüntü, yatay/düşey kesitler, huzme genişliği, ön/arka oranı
  - Eşleme: SWR/dönüş kaybı, L devresi, hairpin (beta), gamma eşleme
  - Mekanik: kütle, kendi ağırlığıyla sarkma, rüzgâr yükü
  - Şablonlar: dipol, N elemanlı Yagi-Uda (N ≥ 2)
  - `tests/validation.rs`: ince yarım dalga dipol (~73+j42 Ω, 2.15 dBi), 3/5 elemanlı Yagi
    için enerji korunumu (kazanç = yönlülük), yönlülük kontrolü, parça sayısına göre yakınsama
- **antan-app**: egui tabanlı masaüstü arayüz
  - Eleman ağacı + özellik paneli (boy, bum konumu, çap, et kalınlığı, malzeme, parça sayısı)
  - Döndürülebilir 3B tel-kafes ışıma diyagramı
  - Yatay/düşey kutupsal kesitler + akım dağılımı grafiği
  - SWR/frekans taraması ve tablosu
  - Eşleme sekmesi: L devresi, hairpin, gamma sonuçları bir arada
  - `ANTAN_AUTOSHOT=<yol>` / `ANTAN_AUTOSHOT_TAB=cuts|sweep|matching` ortam değişkenleri,
    etkileşimsiz doğrulama için otomatik ekran görüntüsü alıp kapanır.

## Çalıştırma

```
cargo run -p antan
```

## Bilinen sınırlar / yol haritası

- Tel birleşimleri (junction / T-bağlantı) yok — her eleman bağımsız, kesişmemeli.
- Toprak düzlemi / gerçek zemin etkisi yok (yalnız serbest uzay).
- Kademeli (teleskopik, farklı çaplı bölümlerden oluşan) elemanlar henüz yok.
- Yalnız dipol ve Yagi-Uda şablonları var; quad, loop, vertikal, log-periyodik vb. eklenmeli.
- Gamma eşleme modeli yaklaşıktır (Healey yöntemi); NEC/ölçümle doğrulanmalı.
- Sağ özellik panelinde metin sola hafif taşıyor (kozmetik, iç kenar boşluğu ayarlanmalı).
- Otomatik eleman optimizasyonu (boy/aralık ayarı) yok.
- Proje/anten kaydetme-yükleme (JSON serileştirme çekirdekte hazır) arayüze bağlanmadı.

İskelet buraya kadar: mimari oturdu, fizik doğrulandı (bkz. `tests/validation.rs`), arayüz
çalışıyor ve ekran görüntüsü alınabiliyor. Devamı (daha fazla anten türü, kademeli elemanlar,
optimizasyon, Smith chart, dosya kaydetme/yükleme, arayüz cilası) için daha güçlü bir modele
devredilmesi planlanıyor.
