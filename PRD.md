# PRODUCT REQUIREMENTS DOCUMENT (PRD) - Magnolia

## 1. Executive Summary & Latar Belakang

### 1.1 Deskripsi Produk
**Magnolia** adalah utilitas pencarian desktop lokal (*local desktop search utility*) berkecepatan tinggi yang dibangun secara murni menggunakan bahasa pemrograman **Rust** dan framework UI berbasis akselerasi GPU **egui/eframe**. Magnolia dirancang dengan filosofi *zero-browser / zero-webview runtime*, menghasilkan jejak memori yang sangat minim (*minimal memory footprint*), waktu startup instan (*instant startup*), dan performa pencarian seketika (*zero-latency keystroke search*).

### 1.2 Masalah yang Diselesaikan
Pencarian file bawaan sistem operasi modern (seperti Windows Search Indexer) sering kali menghadapi kritik pengguna karena:
1. **Beban Resource yang Berat:** Mengonsumsi penggunaan CPU, I/O disk, dan RAM yang tinggi karena melakukan pengindeksan isi konten (*content indexing*), parsing metadata dokumen kompleks, dan background indexing yang terus-menerus.
2. **Indexing yang Sangat Lambat:** Memerlukan waktu berjam-jam untuk memindai jutaan file dalam hard drive besar.
3. **Hasil yang Tidak Instan:** Respon pencarian pada menu Windows sering tertunda, memicu pencarian web Bing yang tidak diinginkan, atau gagal menemukan file sistem teknis secara cepat.

Magnolia menyelesaikan masalah ini dengan mengadopsi arsitektur in-memory indexing cerdas yang terinspirasi dari standar industri (ala Voidtools Everything):
* **Fokus Eksklusif Metadata Nama File & Folder:** Tidak memindai isi konten file, melainkan hanya mengindeks nama file, struktur folder, ukuran, dan waktu modifikasi.
* **Akselerasi MFT (Master File Table) NTFS:** Mampu memindai ratusan ribu hingga jutaan file dalam hitungan 1–3 detik melalui pembacaan langsung tabel MFT via Win32 IOCTL.
* **Zero-Allocation Search Engine:** Algoritma pencarian berbasis *byte slice matching* tanpa alokasi memori heap baru pada setiap ketukan keyboard.
* **Integrasi Native Windows Shell:** Menggunakan shell context menu (`IContextMenu`), icon native Windows via GDI, dan navigasi Explorer native.

---

## 2. Target Pengguna & Hak Akses (User Personas & Roles)

### 2.1 Target Pengguna (User Personas)
1. **Software Developers & Engineers:** Membutuhkan akses cepat ke file kode sumber, repositori, artefak build, dependencies, dan file konfigurasi di seluruh drive tanpa lag.
2. **Power Users & System Administrators:** Memerlukan utilitas pencarian yang dapat menyaring file menggunakan negasi (`!term`), wildcard (`*.rs`), dan regular expression (*regex*), serta mampu mengelola file secara langsung melalui integrasi shell Windows.
3. **General Users:** Pengguna yang menginginkan pengganti Windows Explorer Search yang cepat, ringan, bersih, dan langsung menemukan file unduhan atau dokumen seketika.

### 2.2 Roles & Hak Akses Sistem
Magnolia beroperasi sebagai aplikasi desktop mandiri (*standalone client*) dengan 2 tingkat hak akses eksekusi di lingkungan Windows:

| Role / Mode | Hak Akses Sistem | Kemampuan & Perilaku Sistem |
|---|---|---|
| **Standard User (Non-Elevated)** | Izin pengguna standar Windows (tanpa UAC prompt) | • Menggunakan parallel directory DFS traversal untuk indexing.<br>• Memantau folder unduhan (`%USERPROFILE%\Downloads`) secara realtime.<br>• Melakukan pencarian, inline renaming, copy path, dan peluncuran file default. |
| **Administrator (Elevated)** | Hak akses elevated Administrator (UAC Administrator) | • Membuka handle raw volume `\\.\C:` dsb.<br>• Menjalankan **Direct NTFS MFT Enumeration** (kecepatan scan ~1.5 juta file dalam 1–2 detik).<br>• Membaca **NTFS USN Change Journal** secara realtime untuk pemantauan pembuatan, perubahan, penghapusan, dan rename file di seluruh volume disk dengan beban CPU 0.0%. |

---

## 3. Fitur Utama & Kebutuhan Fungsional (Functional Requirements)

### 3.1 Modul Pengindeksan & Penyimpanan Data (Indexing & Database Engine)
* **Modul:** `src/db.rs`, `src/ntfs.rs`, `src/scanner.rs`
* **Deskripsi Singkat:**
  Mengelola penyimpanan in-memory seluruh file dan direktori sistem serta persistensinya ke media penyimpanan lokal. Menggunakan model data hemat memori (*compact parent-id model*) di mana string direktori dipisahkan ke dalam tabel kamus folder unik (`folder_paths`), sehingga jutaan entri file hanya menyimpan referensi `parent_id: u32` alih-alih duplikasi string path lengkap.
* **Kebutuhan Teknis:**
  - Persistensi biner atomic via `bincode` ke `%APPDATA%\Magnolia\magnolia.db` dengan mekanisme safe swap (`.db.tmp` $\to$ remove target $\to$ rename).
  - Pembacaan tabel MFT NTFS menggunakan `FSCTL_ENUM_USN_DATA` dan `MFT_ENUM_DATA_V0`.
  - Fallback multi-drive concurrent crawler menggunakan thread pool paralel (`scan_drives_parallel`).

### 3.2 Modul Pencarian & Filter Instan (Query Engine)
* **Modul:** `src/query.rs`
* **Deskripsi Singkat:**
  Mesin pencarian in-memory dengan evaluasi multi-kondisi yang berjalan paralel pada 4 worker threads. Mendukung pencocokan nama instan tanpa alokasi memori heap baru per frame.
* **Fitur Pencarian:**
  - **Substring Matching:** Pencarian case-insensitive instan berbasis byte comparison (`contains_ignore_case`).
  - **Negation Filter:** Mendukung operator tanda seru `!term` untuk mengecualikan hasil tertentu (contoh: `laporan !backup`).
  - **Wildcard Search:** Mendukung karakter bintang `*` (nol atau banyak karakter) dan tanda tanya `?` (tepat satu karakter).
  - **Regular Expression (Regex):** Toggle regex dinamis menggunakan engine `regex`.
  - **Match Case Toggle:** Pencocokan sensitif huruf kapital/kecil.
  - **Match Path Toggle:** Memperluas cakupan pencarian hingga ke string path lengkap direktori.

### 3.3 Modul Antarmuka Pengguna & Interaksi (UI & Table Presentation)
* **Modul:** `src/main.rs`, `src/icon.rs`
* **Deskripsi Singkat:**
  Antarmuka grafis desktop native dark-mode berbasis GPU refresh rate penuh tanpa browser. Menyajikan hasil pencarian dalam bentuk tabel terstruktur dengan kolom yang dapat diurutkan dan diubah ukurannya (*resizable columns*).
* **Fitur UI:**
  - **Interactive Column Resizing:** Pembatas kolom (*divider*) selebar 6px yang dapat di-drag untuk mengatur lebar kolom Name, Path, Size, dan Date Modified secara leluasa dengan kursor `ResizeColumn`.
  - **Column Alignment:** Seluruh teks pada header dan sel tabel rata kiri (*align left*) secara pixel-perfect.
  - **Native Windows Shell Icons:** Menampilkan ikon asli Windows untuk folder dan jenis file (gambar, dokumen, audio, executable) via Win32 `SHGetFileInfoW` dan GDI extraction.
  - **Single-Click Selection:** Klik kiri 1 kali memberikan visual highlight seleksi baris tanpa membuka file.
  - **Double-Click Direct Launch:** Klik kiri 2 kali langsung membuka file/folder menggunakan handler aplikasi default via Win32 `ShellExecuteW`.
  - **Native Open in Explorer:** Opsi menu untuk membuka lokasi folder di Windows Explorer dengan otomatis men-scroll dan menyeleksi (*highlight*) file tersebut menggunakan Win32 `SHOpenFolderAndSelectItems`.

### 3.4 Modul Context Menu Shell Asli Windows & Inline Rename
* **Modul:** `src/context_menu.rs`, `src/main.rs`
* **Deskripsi Singkat:**
  Integrasi penuh dengan sistem menu konteks klik kanan Windows Explorer asli serta kemampuan manipulasi nama file langsung di tempat (*in-place renaming*).
* **Fitur:**
  - **Native Windows Explorer IContextMenu:** Klik kanan pada baris memicu menu konteks Windows Explorer asli (Open, Edit, 7-Zip, WinRAR, Delete, Properties, dll.) pada dedicated STA thread dengan hidden message window dan message pump forwarding untuk `IContextMenu2` & `IContextMenu3`.
  - **Inline Cell Rename:** Memicu pengeditan nama file langsung di dalam sel kolom "Name" melalui tombol keyboard <kbd>F2</kbd> atau klik opsi "Rename" pada menu konteks Windows. Bagian nama dasar (*stem*) otomatis terblok dengan ekstensi file tetap terlindungi. Tekan <kbd>Enter</kbd> untuk menyimpan atau <kbd>Escape</kbd> / klik luar untuk membatalkan.

### 3.5 Modul Pemantauan Filesystem Real-Time (Live Watcher)
* **Modul:** `src/ntfs.rs`
* **Deskripsi Singkat:**
  Menjaga indeks data selalu mutakhir secara otomatis saat ada file yang diunduh, dibuat, diubah, atau dihapus oleh aplikasi lain.
* **Fitur:**
  - **NTFS USN Change Journal Watcher:** Loop background thread membaca catatan kernel NTFS via `FSCTL_READ_USN_JOURNAL` setiap 500ms (aktif pada mode elevated).
  - **Downloads Folder Watcher:** Thread pemantau direktori `%USERPROFILE%\Downloads` untuk menangkap file yang baru selesai diunduh dari browser secara instan.

---

## 4. Alur Kerja Pengguna (User Flow)

### 4.1 Alur Peluncuran & Pengindeksan Awal (First Launch Flow)
1. Pengguna membuka executable `magnolia.exe`.
2. Sistem memeriksa ketersediaan file cache `%APPDATA%\Magnolia\magnolia.db`.
3. **Jika cache ada:** Sistem memuat database biner ke RAM dalam hitungan milidetik dan langsung menampilkan status *"Instant ready: N items loaded from cache"*.
4. **Jika cache belum ada (First Launch):** 
   - Sistem mendeteksi seluruh logical drives (`C:\`, `D:\`, dst.).
   - Sistem mencoba mode cepat pembacaan MFT NTFS; jika tidak memiliki izin Administrator, sistem beralih otomatis ke mode crawler paralel.
   - Hasil disimpan secara atomic ke `magnolia.db` dan antarmuka pencarian siap digunakan seketika.

### 4.2 Alur Pencarian File (Search Flow)
1. Pengguna mengetikkan kata kunci di kolom pencarian (misal: `*.pdf !draft`).
2. Event `response.changed()` menangkap perubahan teks dan memicu `update_search()`.
3. Mesin pencari membagi seluruh entri in-memory ke 4 worker thread menggunakan zero-allocation byte comparator.
4. Hasil terfilter (maksimal 2.000 entri teratas) diurutkan berdasarkan kolom aktif dan dirender secara instan ke virtual scroll area.

### 4.3 Alur Interaksi File (Launch, Context Menu, & Rename Flow)
1. **Membuka File:** Pengguna melakukan klik ganda (*double-click*) pada baris hasil pencarian $\to$ Sistem menjalankan Win32 `ShellExecuteW` $\to$ File terbuka di aplikasi default.
2. **Context Menu Windows:** Pengguna mengklik kanan baris $\to$ Sistem memunculkan menu Windows Explorer asli $\to$ Pengguna memilih aksi pihak ketiga (misal: ekstrak dengan 7-Zip atau melihat Properties).
3. **Mengubah Nama File (Inline Rename):**
   - Pengguna memilih opsi "Rename" di context menu atau menekan tombol <kbd>F2</kbd> pada baris yang aktif.
   - Kolom "Name" bertransisi menjadi kotak input teks editable dengan nama file dasar (*stem*) terblok otomatis.
   - Pengguna mengetik nama baru dan menekan <kbd>Enter</kbd>.
   - Sistem memanggil `std::fs::rename`, memperbarui entri di database memori, dan menyegarkan tampilan tabel.

---

## 5. Spesifikasi Teknis & Non-Fungsional (Non-Functional Requirements)

### 5.1 Teknologi yang Digunakan
* **Bahasa Pemrograman:** Rust (Edition 2021, Compiler rustc 1.75+)
* **GUI Framework:** `eframe` / `egui 0.29.1` (Hardware-accelerated rendering via OpenGL/Glow, zero-browser architecture)
* **Windows API Bindings:** Crate `windows 0.58` dengan fitur:
  - `Win32_Foundation`, `Win32_Graphics_Gdi`
  - `Win32_Storage_FileSystem`, `Win32_System_IO`, `Win32_System_Ioctl`
  - `Win32_UI_Shell`, `Win32_UI_Shell_Common`, `Win32_UI_WindowsAndMessaging`
  - `Win32_System_Com`, `Win32_System_Ole`, `Win32_Security`
* **Serialisasi & Penyimpanan:** `bincode 1.3`, `serde 1.0` (Derive)
* **Utilitas & Format:** `chrono 0.4` (Waktu lokal), `regex 1.10` (Pencocokan ekspresi reguler), `rfd 0.15` (Native folder picker dialog), `image 0.25` (PNG asset decoding), `winres 0.1` (PE metadata & icon embedding)
* **Profil Rilis Kompiler (`Cargo.toml`):**
  - `opt-level = 3` (Optimasi biner tingkat tinggi)
  - `lto = true` (Link-Time Optimization)
  - `codegen-units = 1` (Maksimalisasi inlining dan vectorization CPU)

### 5.2 Keamanan & Akses Sistem
* **Autentikasi & Otorisasi:** Sebagai utilitas desktop sistem lokal, Magnolia tidak menggunakan token web (seperti JWT/OAuth). Hak akses dibatasi oleh hak pengguna Windows aktif (Standard vs Administrator DACL).
* **Atomic File Operations:** Penulisan database ke disk dilakukan melalui pembuatan file sementara (`.db.tmp`) sebelum proses rename atomic guna mencegah kerusakan data (*data corruption*) jika aplikasi ditutup paksa di tengah penyimpanan.
* **Safe COM Life-Cycle:** Inisialisasi dan pelepasan OLE/COM (`OleInitialize` / `OleUninitialize`) dijalankan pada thread STA terisolasi dengan penanganan `Release` otomatis pada antarmuka COM (`IShellItem`, `IContextMenu`).

---

## 6. Batasan Sistem Saat Ini (System Scope & Out of Scope)

### 6.1 Batasan Sistem Saat Ini (Current Scope)
1. **Target Platform:** Khusus sistem operasi **Microsoft Windows** (Windows 10 / Windows 11 arsitektur x86_64).
2. **Lingkup Pengindeksan:** Khusus metadata file dan folder (Nama, Direktori Path, Ukuran File, Tanggal Modifikasi).
3. **Maksimal Tampilan Hasil:** Dibatasi maksimal 2.000 hasil teratas per kueri pencarian demi menjaga stabilitas frame rate antarmuka egui.
4. **Dukungan MFT:** Pembacaan MFT langsung hanya berlaku untuk partisi berformat file sistem **NTFS** dengan hak Administrator; partisi non-NTFS (FAT32, exFAT) menggunakan fallback directory crawler.

### 6.2 Di Luar Cakupan (Out of Scope)
1. **Pengindeksan Konten File (Full-Text Content Search):** Magnolia tidak membaca atau mengindeks isi teks di dalam file PDF, Word, Excel, source code, atau database internal.
2. **Dukungan Cross-Platform Non-Windows:** Tidak mendukung Linux atau macOS karena arsitektur sangat bergantung pada subsistem Windows Shell (`IContextMenu`, ShellExecute, MFT IOCTL).
3. **Sinkronisasi Cloud & Jaringan:** Magnolia dirancang untuk drive fisik lokal dan tidak mengindeks network storage (SMB/NFS) atau penyimpanan cloud secara real-time.
