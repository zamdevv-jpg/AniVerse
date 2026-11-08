# AniVerse (Rust)

Situs streaming anime subtitle Indonesia dengan backend **Rust (Axum)**, halaman server-side (MiniJinja), dan tampilan gelap beraksen emas bergaya Wibuku. Data katalog utama diambil dari **Nimegami** (`https://nimegami.id`), **Otakudesu** (`https://otakudesu.or.at`) melengkapi judul yang belum ada, dan **Doronime** (`https://doronime.id`) tersedia sebagai sumber katalog tambahan dengan pemeriksaan genre di halaman detail. Jadwal rilis memakai kelompok hari dari MyAnimeList.

Versi aplikasi saat ini: **0.1.1**.

## Fitur

- **Beranda**: hero slider episode terbaru, rilisan terbaru, anime terpopuler mingguan, anime ongoing, jadwal hari ini, anime selesai, dan section "lanjut tonton" dari riwayat perangkat.
- **Sumber anime**: Nimegami diprioritaskan untuk beranda, katalog, genre, dan pencarian. Jika kartu Otakudesu mengarah ke judul yang sama di Nimegami, halaman detail dan episodenya memakai Nimegami. Doronime bisa dipilih dari filter sumber; judul baru lolos setelah genre detail diperiksa.
- **Kartu yang bermasalah**: entri Nimegami dilewati jika detailnya menghasilkan HTTP 403, 404, 410, atau halaman tanpa metadata anime yang valid. Pemeriksaan ketersediaan disimpan selama 15 menit; gangguan jaringan sementara tidak langsung menyembunyikan judul.
- **Katalog**: pencarian judul, filter status/tipe/genre, urutan (update, populer, rating, judul), serta pagination. Hasil dari beberapa sumber dideduplikasi berdasarkan judul dan musim.
- **Detail anime**: poster, rating, informasi lengkap, genre, sinopsis yang bisa dibuka, daftar episode dengan pencarian dan pengurutan.
- **Halaman tonton**: pemutar iframe, pemilih server video (mirror), navigasi episode sebelumnya/berikutnya, daftar episode seri, tautan unduhan, dan rekomendasi. Tautan **Buka player** menyediakan opsi membuka server terpilih di tab baru jika host menolak pemutaran di iframe atau terus memuat.
- **Jadwal rilis**: dikelompokkan per hari mengikuti MyAnimeList. Genre Hentai dan Erotica dibuang sebelum kartu ditampilkan.
- **Pencarian langsung**: dropdown debounce di header (`/api/search`) plus halaman `/cari`.
- **Bookmark & riwayat**: disimpan di `localStorage`, tanpa akun.
- **Akun lokal**: buat akun dengan nama, email, dan kata sandi untuk profil di perangkat ini. Kata sandi disimpan sebagai hash; akun belum tersinkron antarperangkat.
- **Histori & profil**: episode terakhir yang dibuka, aktivitas komentar, bookmark, dan koleksi bisa dilihat dari halaman akun.
- **Komentar**: komentar, balasan, dan suka per episode disimpan lokal di perangkat.
- **Unduhan**: buka tautan unduhan dari penyedia anime atau simpan tautannya di halaman Unduhan. File diunduh langsung dari penyedia.
- **Mode offline**: simpan halaman detail anime agar informasi dan aset yang sempat dimuat bisa dibuka kembali tanpa internet. Video dari sumber luar tidak disimpan secara offline.
- **Lainnya**: halaman error 404, halaman anime acak (`/acak`), `robots.txt`, endpoint `/healthz`.
- **Tampilan responsif**: antarmuka gelap abu-abu dengan aksen emas yang mengambil pola navigasi, pencarian, genre, kartu, dan area tontonan dari referensi Wibuku. Di ponsel tersedia navigasi bawah; di PC, header navigasi dan kisi konten menggunakan ruang layar lebar.

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
| `DORONIME_BASE_URL` | `https://doronime.id` | Sumber katalog tambahan dengan filter genre dewasa |
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
| `/akun` | Masuk, daftar, dan profil lokal |
| `/histori` | Histori tontonan perangkat |
| `/unduhan` | Tautan unduhan tersimpan |
| `/offline` | Halaman anime yang disimpan offline |
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
│  ├─ account.html / history.html / downloads.html / offline.html
│  └─ partials/    # icon sprite, header, footer, mobile nav, kartu
└─ static/
   ├─ css/style.css
   ├─ js/app.js
   ├─ js/service-worker.js
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
- Artefak rilis 0.1.1: `dist/android/AniVerse_0.1.1_arm64-release.apk`, `dist/windows/AniVerse_0.1.1_x64-setup.exe`, dan `dist/windows/AniVerse_0.1.1_x64_en-US.msi`.
- Login, histori, komentar, dan daftar unduhan bersifat lokal pada instalasi/perangkat. Untuk sinkronisasi akun dan komentar publik, aplikasi memerlukan layanan backend bersama.
- Android menggunakan package ID `com.otakunime.desktop`. Release harus memakai keystore dan password Anda sendiri. Konfigurasi lokal `src-tauri/gen/android/app/signing.properties` dibaca untuk release dan sudah dikecualikan dari Git; jangan commit file itu atau `release.keystore`. Alternatifnya, isi variabel lingkungan `OTAKUNIME_ANDROID_KEYSTORE`, `OTAKUNIME_ANDROID_STORE_PASSWORD`, `OTAKUNIME_ANDROID_KEY_ALIAS`, dan `OTAKUNIME_ANDROID_KEY_PASSWORD`.
- Jangan membagikan atau menghapus keystore yang dipakai untuk menandatangani rilis Play Store; semua pembaruan aplikasi perlu identitas signing yang sama.

## Disclaimer

Proyek ini hanya untuk pembelajaran. Seluruh data dan video milik pihak ketiga, termasuk `nimegami.id` dan `otakudesu.or.at`; situs ini tidak menyimpan atau meng-host file video apa pun. Hormati hak cipta dan dukung rilis resmi.
