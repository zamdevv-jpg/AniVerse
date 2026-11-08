# AniVerse (Rust)

Situs streaming anime subtitle Indonesia dengan backend **Rust (Axum)**, halaman server-side (MiniJinja), dan tampilan gelap beraksen emas bergaya Wibuku. Data katalog diambil dari **Nimegami** (`https://nimegami.id`) sebagai sumber utama, dengan **Otakudesu** (`https://otakudesu.or.at`) sebagai pelengkap dan sumber cadangan.

## Fitur

- **Beranda**: hero slider episode terbaru, rilisan terbaru, anime terpopuler mingguan, anime ongoing, jadwal hari ini, anime selesai, dan section "lanjut tonton" dari riwayat perangkat.
- **Sumber anime**: Nimegami diprioritaskan untuk beranda, katalog, genre, dan pencarian. Otakudesu melengkapi judul yang belum ada di Nimegami dan menjadi cadangan saat sumber utama gagal.
- **Kartu yang bermasalah**: entri Nimegami dilewati jika detailnya menghasilkan HTTP 403, 404, 410, atau halaman tanpa metadata anime yang valid. Pemeriksaan ketersediaan disimpan selama 15 menit; gangguan jaringan sementara tidak langsung menyembunyikan judul.
- **Katalog**: pencarian judul, filter status/tipe/genre, urutan (update, populer, rating, judul), serta pagination. Hasil dari beberapa sumber dideduplikasi berdasarkan judul dan musim.
- **Detail anime**: poster, rating, informasi lengkap, genre, sinopsis yang bisa dibuka, daftar episode dengan pencarian dan pengurutan.
- **Halaman tonton**: pemutar iframe, pemilih server video (mirror), navigasi episode sebelumnya/berikutnya, daftar episode seri, tautan unduhan, dan rekomendasi.
- **Jadwal rilis**: dikelompokkan per hari dengan penanda hari ini (WIB) dan nav hari yang lengket.
- **Pencarian langsung**: dropdown debounce di header (`/api/search`) plus halaman `/cari`.
- **Bookmark & riwayat**: disimpan di `localStorage`, tanpa akun.
- **Lainnya**: halaman error 404, halaman anime acak (`/acak`), `robots.txt`, endpoint `/healthz`.
- **Tema**: hampir hitam `#07070a`, aksen emas `#ffd23f`, kartu membulat, glass header, bottom nav mobile, animasi halus, dan mode `prefers-reduced-motion`.

## Teknologi

| Bagian | Pustaka |
| --- | --- |
| Web framework | `axum` 0.8 |
| Query string | `axum-extra` (`query`) |
| Runtime | `tokio` |
| HTTP client | `reqwest` (rustls) |
| Parsing HTML | `scraper` |
| Template | `minijinja` |
| Static & compression | `tower-http` |
| Logging | `tracing` |
| Tanggal | `chrono` |

## Menjalankan

Prasyarat: Rust stable (MSVC toolchain di Windows) dan Visual Studio Build Tools (C++).

```bash
cd otakunime-rust
cp .env.example .env      # opsional, nilai default sudah cukup
cargo run
```

Buka `http://127.0.0.1:3000`.

Build rilis:

```bash
cargo build --release
./target/release/otakunime
```

> Jalankan dari root proyek agar folder `templates/` dan `static/` ditemukan.

## Konfigurasi

Semua variabel dibaca dari environment atau `.env`:

| Variabel | Default | Keterangan |
| --- | --- | --- |
| `SITE_NAME` | `AniVerse` | Nama situs |
| `SITE_TAGLINE` | `Nonton anime sub indo, update tiap hari.` | Tagline footer |
| `SITE_DESCRIPTION` | — | Meta description default |
| `HOST` | `127.0.0.1` | Alamat bind |
| `PORT` | `3000` | Port |
| `STATIC_DIR` | `static` | Folder aset |
| `NIMEGAMI_BASE_URL` | `https://nimegami.id` | Alamat sumber anime utama |
| `OTAKUDESU_BASE_URL` | `https://otakudesu.or.at` | Alamat sumber pelengkap/cadangan |
| `RUST_LOG` | `otakunime=info,tower_http=warn` | Filter log |

## Rute

| Rute | Keterangan |
| --- | --- |
| `/` | Beranda |
| `/anime` | Katalog + filter (`q`, `status`, `type`, `order`, `genre`, `page`) |
| `/anime/:slug` | Detail anime |
| `/watch/:slug` | Halaman tonton episode |
| `/jadwal` | Jadwal rilis |
| `/cari` | Halaman pencarian |
| `/bookmark` | Bookmark perangkat |
| `/acak` | Redirect ke anime acak |
| `/api/search?q=` | API pencarian (JSON) |
| `/healthz` | Health check |
| `/robots.txt` | Robots |

## Struktur

```
otakunime-rust/
├─ src/
│  ├─ main.rs      # setup Axum, filter MiniJinja, shutdown
│  ├─ models.rs    # model data + pembentuk URL filter
│  ├─ source.rs    # agregasi sumber, parsing Otakudesu, dan deduplikasi
│  ├─ nimegami.rs  # parsing katalog/detail/episode Nimegami dan pemeriksaan ketersediaan
│  └─ web.rs       # handler & konteks template
├─ templates/
│  ├─ base.html    # layout
│  ├─ index.html / browse.html / anime.html / watch.html
│  ├─ schedule.html / search.html / bookmark.html / error.html
│  └─ partials/    # icon sprite, header, footer, mobile nav, kartu
└─ static/
   ├─ css/style.css
   ├─ js/app.js
   └─ favicon.svg
```

## Catatan teknis

- **Cache**: HTML sumber disimpan di memori dengan TTL beberapa menit hingga 30 menit dan dipakai sebagai fallback bila sumber error (stale-on-error). Pemeriksaan ketersediaan detail Nimegami memiliki cache 15 menit.
- **Anti-challenge**: request ke kedua sumber mengirim User-Agent browser dan `Accept-Language`; Otakudesu juga memakai cookie `sg=1; ts_theme=darkmode` dan mencoba ulang respons halaman verifikasi.
- **Server video**: mirror diambil dari tombol `.gov-the-embed` (payload base64 berisi iframe), dengan fallback `<select class="mirror">` pada tema lama.
- **Gambar**: URL gambar asli dipertahankan supaya CDN tidak memperbesar thumbnail ke ukuran kartu. Jika gambar gagal dimuat, UI menampilkan inisial judul.

## Build desktop dan Android (Tauri 2)

- Windows installer: `cargo tauri build` (NSIS `.exe` dan MSI `.msi` jika toolchain bundler tersedia).
- Android APK: `cargo tauri android build --apk`.
- Android AAB: `cargo tauri android build --aab`.
- Paket distribusi hasil build terakhir disimpan di `dist/android/` (APK arm64) dan `dist/windows/` (installer NSIS dan MSI).
- Android menggunakan package ID `com.otakunime.desktop`. Release harus memakai keystore dan password Anda sendiri. Konfigurasi lokal `src-tauri/gen/android/app/signing.properties` dibaca untuk release dan sudah dikecualikan dari Git; jangan commit file itu atau `release.keystore`. Alternatifnya, isi variabel lingkungan `OTAKUNIME_ANDROID_KEYSTORE`, `OTAKUNIME_ANDROID_STORE_PASSWORD`, `OTAKUNIME_ANDROID_KEY_ALIAS`, dan `OTAKUNIME_ANDROID_KEY_PASSWORD`.
- Jangan membagikan atau menghapus keystore yang dipakai untuk menandatangani rilis Play Store; semua pembaruan aplikasi perlu identitas signing yang sama.

## Disclaimer

Proyek ini hanya untuk pembelajaran. Seluruh data dan video milik pihak ketiga, termasuk `nimegami.id` dan `otakudesu.or.at`; situs ini tidak menyimpan atau meng-host file video apa pun. Hormati hak cipta dan dukung rilis resmi.
