# Nalaro AI Desktop

Aplikasi AI offline Windows x64 berbasis **Tauri 2 + Rust + llama.cpp**. Semua komponen runtime aplikasi dibundel di installer, **kecuali file model GGUF** yang diimpor pengguna setelah pemasangan.

## Unduh installer Windows

**Build berhasil:** [GitHub Actions – Windows x64 installer](https://github.com/khoirulzz/localai2/actions/runs/40208730768)

1. Buka link GitHub Actions dan masuk ke akun GitHub jika diminta.
2. Di bagian **Artifacts**, pilih **Nalaro-AI-Desktop-Windows-x64-Installer** (berupa arsip ZIP).
3. Ekstrak ZIP tersebut, lalu jalankan berkas installer **.exe** di dalamnya.
4. Setelah pemasangan selesai, buka **Nalaro AI** dari Start Menu seperti aplikasi Windows lainnya.
5. Klik **Kelola model → Impor model GGUF**, pilih file yang sudah ada di komputer, lalu pilih model untuk mengobrol.

Tidak perlu membuka CMD atau menjalankan llama-server secara manual. Binary llama.cpp b11871 CPU dan dependensi Windows WebView2 disertakan pada installer. Model GGUF disalin ke folder data aplikasi sehingga bertahan setelah installer diperbarui.

> **Catatan:** Installer belum ditandatangani dengan sertifikat code signing; Windows SmartScreen mungkin menunjukkan peringatan penerbit tidak dikenal. Pastikan file berasal dari workflow repository ini.

## Fitur v0.1.0

- Impor GGUF dari disk lokal, manajemen model dan ganti model.
- Streaming respons dengan tombol hentikan.
- Riwayat chat disimpan ke SQLite lokal.
- Mode Cepat / Pikir / Pikir lebih (bergantung perilaku dan format model).
- Pengaturan temperature, context, CPU threads, batas token output, dan instruksi kustom.
- Statistik token, waktu, dan estimasi TPS.
- Server hanya di-bind ke `127.0.0.1`, menggunakan API key unik per proses, dijalankan tanpa jendela terminal dan dihentikan saat aplikasi keluar.

### Batasan v0.1.0

- Backend inference saat ini **CPU saja**; CUDA/Vulkan belum tersedia di installer pertama.
- Input gambar, PDF, pencarian Tavily, dan ringkasan otomatis lintas riwayat yang tersedia pada proyek Android **belum dipindahkan sepenuhnya** ke desktop.
- Tingkatan thinking di sini adalah pilihan antarmuka/prompt; tidak menjamin pengaturan tingkat reasoning native yang sama pada semua format model.
- Kompilasi dan unit test berhasil di runner Windows GitHub, tetapi pengujian interaktif GUI di PC pengguna masih perlu dilakukan.

## Developer

- Toolchain: Node.js, Rust stable (MSVC), Visual Studio Build Tools, Windows SDK.
- Jalankan `npm install` untuk Tauri CLI.
- Build otomatis: workflow [`.github/workflows/build-windows.yml`](.github/workflows/build-windows.yml).
- Binary `llama-server.exe` dibundel oleh workflow dari tag `b11871` (digest SHA-256 divalidasi jika tersedia pada metadata asset).
- API lokal diakses hanya oleh backend Rust melalui bearer token, bukan langsung dari WebView.

Project ini tidak menyertakan bobot model dan tidak membutuhkan akun, Python, Docker, Ollama atau CMD saat digunakan.
