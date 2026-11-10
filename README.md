# AniVerse (Rust)

Situs streaming anime subtitle Indonesia dengan backend **Rust (Axum)**, halaman server-side (MiniJinja), dan tampilan gelap beraksen emas bergaya Wibuku. Data katalog, detail, dan episode diambil dari **Nimegami** (`https://nimegami.id`). Jadwal rilis memakai kelompok hari dari MyAnimeList.

Versi aplikasi saat ini: **0.1.3**.

## Fitur

- **Beranda**: banner maksimal 10 anime diurutkan menurut popularitas MyAnimeList, disaring agar hanya judul yang tersedia di katalog Nimegami yang tampil; rilisan terbaru, anime terpopuler mingguan, anime ongoing, jadwal hari ini, anime selesai, dan section "lanjut tonton" dari riwayat perangkat.
- **Sumber anime**: beranda, katalog, genre, pencarian, detail, dan episode memakai Nimegami. `OTAKUDESU_BASE_URL` tidak digunakan sebagai fallback agar konfigurasi lama tidak mengalihkan sumber tanpa sengaja.
- **Kartu yang bermasalah**: entri Nimegami dilewati jika detailnya menghasilkan HTTP 403, 404, 410, atau halaman tanpa metadata anime yang valid. Pemeriksaan ketersediaan disimpan selama 15 menit; gangguan jaringan sementara tidak langsung menyembunyikan judul.
- **Katalog**: pencarian judul, filter status/tipe/genre, urutan (update, populer, rating, judul), serta pagination.
- **Detail anime**: poster, rating, informasi lengkap, genre, sinopsis yang bisa dibuka, daftar episode dengan pencarian dan pengurutan.
- **Halaman tonton**: pemutar video langsung dengan perpindahan sumber otomatis, pemilih resolusi, navigasi episode, daftar unduhan yang bisa diringkas, komentar, balasan, dan suka.
- **Jadwal rilis**: jadwal dari halaman publik MyAnimeList ditampilkan dalam tujuh tanggal per minggu dengan navigasi pekan sebelumnya/berikutnya. Feed menggunakan Jikan-Edge karena API publik Jikan lama sudah dihentikan; genre Hentai dan Erotica dibuang sebelum kartu ditampilkan.
- **Pencarian langsung**: dropdown debounce di header (`/api/search`) plus halaman `/cari`.
- **Bookmark & riwayat**: disimpan di `localStorage`, tanpa akun.
- **Login Google & profil cloud**: autentikasi memakai Supabase Auth; nama dan foto profil bisa diubah, disimpan di Supabase, dan terlihat bersama komentar.
- **Unggah anime**: pengelola dapat mengunggah poster dan video MP4, WebM, atau MOV melalui `/unggahan`. Video besar memakai unggahan bertahap.
- **Histori & profil**: episode terakhir yang dibuka, aktivitas komentar, bookmark, dan koleksi bisa dilihat dari halaman akun.
- **Komentar**: komentar, balasan, dan suka disimpan di Supabase Database agar bisa dibaca pengguna lain.
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
| `MAL_SCHEDULE_API` | `https://jikan.lucashdo.com/v1/schedules` | Feed jadwal yang membaca halaman publik MyAnimeList |
| `MAL_TOP_API` | `https://api.jikan.moe/v4/top/anime` | Daftar popularitas MAL untuk banner beranda; hanya judul yang ada di katalog Nimegami yang ditampilkan |
| `SUPABASE_URL` | — | URL proyek Supabase |
| `SUPABASE_ANON_KEY` | — | Publishable/anon key Supabase; jangan gunakan service-role key |
| `RUST_LOG` | `otakunime=info,tower_http=warn` | Filter log |

## Supabase, Google, dan unggahan

1. Buat proyek Supabase, lalu isi `SUPABASE_URL` dan `SUPABASE_ANON_KEY` di `.env`. Untuk paket desktop/Android, isi sebelum build agar nilai publik tersebut disertakan di aplikasi.
2. Jalankan atau jalankan ulang SQL pada `supabase/schema.sql` di SQL Editor Supabase. Skrip ini membuat tabel unggahan, profil publik, komentar, balasan, suka, bucket `aniverse-media` dan `aniverse-avatars`, serta Row Level Security.
3. Aktifkan provider Google di **Authentication → Providers** pada Supabase dan masukkan OAuth Client ID serta Client Secret dari Google Cloud. Tambahkan alamat situs aplikasi ke daftar redirect URL Supabase.
4. Masuk satu kali memakai Google, lalu beri akun Anda role admin dari SQL Editor (ganti alamat email):

   ```sql
   update auth.users
   set raw_app_meta_data = coalesce(raw_app_meta_data, '{}'::jsonb) || '{"role":"admin"}'::jsonb
   where email = 'you@example.com';
   ```

5. Keluar lalu masuk kembali agar token memuat role admin. Buka **Akun → Unggah anime** untuk menambah poster dan episode.

Bucket media bersifat publik agar video dapat diputar oleh pengunjung. Kebijakan database dan Storage hanya mengizinkan akun dengan `app_metadata.role = 'admin'` untuk mengunggah atau mengubah berkas. Jangan menaruh Supabase service-role key di `.env` aplikasi atau di klien.

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
| `/unggahan` | Daftar anime cloud dan form unggah khusus admin |
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

- Binary server Windows: `cargo build --release` menghasilkan `target/release/otakunime.exe`.
- Versi rilis saat ini: **0.1.3**.
- Artefak Windows x64: MSI `dist/windows/AniVerse_0.1.3_x64_en-US.msi`, installer NSIS `dist/windows/AniVerse_0.1.3_x64-setup.exe`, dan aplikasi portable `dist/windows/AniVerse_0.1.3_x64-portable.exe`.
- APK Android arm64: `dist/android/AniVerse_0.1.3_arm64-release.apk` (`versionCode` 1003).
- Build Windows standar: `cargo tauri build --no-bundle --ci --target x86_64-pc-windows-msvc`, `cargo tauri build --ci --bundles nsis --target x86_64-pc-windows-msvc`, dan `cargo tauri build --ci --bundles msi --target x86_64-pc-windows-msvc` dari `src-tauri/`.
- APK: `cargo tauri android build --apk --target aarch64 --split-per-abi` dari `src-tauri/`. Di Windows yang melarang symbolic link, library native dapat disalin sebagai file biasa ke folder `jniLibs`, lalu paketkan dengan Gradle.
- Android AAB: `cargo tauri android build --aab`.
- Paket distribusi rilis disimpan di `dist/android/` dan `dist/windows/`.
- Rincian build, verifikasi APK, dan catatan validasi MSI 0.1.3 ada di `release-logs/0.1.3-build.md`; catatan rilis sebelumnya ada di `release-logs/0.1.2-build.md`.
- Login dan profil menggunakan Supabase Auth/Database; komentar, balasan, dan suka tersinkron untuk semua pengguna melalui Supabase. Bookmark, histori tontonan, serta tautan unduhan tetap tersimpan per perangkat.
- Android menggunakan package ID `com.otakunime.desktop`. Release harus memakai keystore dan password Anda sendiri. Konfigurasi lokal `src-tauri/gen/android/app/signing.properties` dibaca untuk release dan sudah dikecualikan dari Git; jangan commit file itu atau `release.keystore`. Alternatifnya, isi variabel lingkungan `OTAKUNIME_ANDROID_KEYSTORE`, `OTAKUNIME_ANDROID_STORE_PASSWORD`, `OTAKUNIME_ANDROID_KEY_ALIAS`, dan `OTAKUNIME_ANDROID_KEY_PASSWORD`.
- Jangan membagikan atau menghapus keystore yang dipakai untuk menandatangani rilis Play Store; semua pembaruan aplikasi perlu identitas signing yang sama.

## Catatan rilis 0.1.2

- APK Android arm64 (`versionCode` 1002) dan installer desktop Windows NSIS berhasil dibangun. Versi APK dan tanda tangannya telah diverifikasi.
- File distribusi lama versi 0.1.0 dan 0.1.1 telah dibersihkan dari `dist/` dan output bundler.
- Banner beranda mengikuti urutan popularitas MyAnimeList dan hanya menampilkan anime yang tersedia di katalog Nimegami; judul dewasa dilewati.
- Profil Google dapat mengubah nama dan foto. Foto profil disimpan di bucket publik Supabase dengan izin unggah yang dibatasi ke pemilik akun.
- Efek sorot biru bawaan browser pada tombol dihilangkan, dan label status pemutar disembunyikan selama video sedang berjalan.
- Jadwal tetap menampilkan tujuh tanggal dalam minggu yang dipilih dan berpindah ke tanggal pekan berikutnya atau sebelumnya.

## Disclaimer

Proyek ini hanya untuk pembelajaran. Katalog dan video dari sumber pihak ketiga tetap dimuat dari penyedia masing-masing; file yang diunggah lewat `/unggahan` disimpan di Supabase Storage. Pastikan hanya mengunggah konten yang Anda miliki hak atau izinnya, hormati hak cipta, dan dukung rilis resmi.
