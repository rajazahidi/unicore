# UniCore OS `v2.0.0` — Manual Pengguna Rasmi (Bahasa Melayu)

Selamat datang ke **UniCore OS `v2.0.0` (Edisi Desktop Berdaulat)** — sistem pengendalian desktop moden yang ringan, pantas, bebas kembung (*zero-bloat*), dan mesra pengguna untuk kerja harian, privasi tahap tinggi dengan VPN Pasca-Kuantum terbina, audio Equalizer DSP studio, serta keserasian aplikasi Linux, PC Klasik/Moden (`.exe`/`.msi`/DOS), Aplikasi Web (PWA) dan Mudah Alih (`.apk`).

---

## 1. Mengemudi Persekitaran Desktop Anda

### 1.1 Menu Mula, Bar Tugas & Menu Klik Kanan
- **Menu Mula (Kekunci `Super` / ikon grid di bar bawah):** Carian segera semua aplikasi, 8 aplikasi tersemat (*Pinned Apps*), penapis kategori (*Semua, Internet, Pejabat, Media, Permainan, Sistem*), pintasan pantas folder peribadi (*Dokumen, Muat Turun, Gambar*), Pemilih Emoji, dan menu Kuasa.
- **Bar Tugas (*Taskbar*):** Memaparkan pintasan pantas, tetingkap aktif, Dulang Sistem (*System Tray* & Keluar USB Selamat), kawalan Kelantangan, butang Pembantu AI (`AI`), Tetapan Pantas, Jam (24-jam atau 12-jam), serta butang Kuasa.
- **Menu Klik Kanan Desktop:** Klik kanan pada ruang kosong desktop untuk **Cipta Folder Baharu**, **Cipta Dokumen Teks Baharu**, buka **Pusat Kawalan**, **Pengurus Peranti**, **Pengurus Storan**, **Tetapan Paparan**, **Studio 10 Tema**, **AegisTunnel VPN**, **Equalizer ResonanceFX**, atau **Terminal**.
- **Penjelajah Komputer (`Super + E`):** Teroka folder tempatan, cakera dalaman, pemacu USB (dilekap automatik), dan pemacu `~/Cloud`. Klik kanan pada fail atau folder untuk **Buka Terminal Di Sini**, **Salin Laluan**, **Cetak Fail**, **Ekstrak / Mampat ZIP**, atau **Jalankan Dalam Sandbox**.

### 1.2 Pintasan Papan Kekunci Utama
| Pintasan | Tindakan |
| :--- | :--- |
| `Super` | Buka / tutup Menu Mula UniCore |
| `Super + E` | Buka Penjelajah Komputer |
| `Super + A` | Buka Pembantu AI |
| `Super + .` | Buka Pemilih Emoji |
| `Super + V` | Buka Sejarah Papan Keratan |
| `Super + L` | Kunci Skrin |
| `PrintScreen` | Buka Tangkap Skrin & Perakam Video MP4 |
| `Ctrl + Shift + Esc` | Buka Pengurus Tugas (*Task Manager*) |
| `Alt + Shift` | Tukar aksara papan kekunci bukan-Latin / US |
| `Alt + Tab` | Tukar antara tetingkap terbuka |

---

## 2. Pusat Kawalan UniCore (`--control-panel`)

Buka **Menu Mula → Pusat Kawalan** untuk mencapai semua tetapan utama dalam satu grid kemas 2×5:

1. **Wi-Fi & Rangkaian:** Sambung Wi-Fi/Ethernet, urus rangkaian tersimpan, lekap pemacu rangkaian SMB/SFTP/WebDAV, serta profil WireGuard/OpenVPN.
2. **Audio & Bluetooth:** Laras kelantangan/mikrofon PipeWire, tukar peranti output atau profil Bluetooth A2DP, atau buka **Equalizer ResonanceFX**.
3. **Paparan & Gaya:** Tetapkan berbilang monitor (*Kiri, Kanan, Cermin*), resolusi, skala paparan, putaran skrin (`0°/90°/180°/270°`), Cahaya Malam, kecerahan komputer riba & monitor luaran DDC/CI, susun atur bar tugas, serta **10 Tema Desktop**.
4. **Bahasa, Wilayah & Papan Kekunci (`--region`):**
   - **12 Bahasa Antarabangsa:** Bahasa Melayu, Inggeris, Bahasa Indonesia, Cina Ringkas, Jepun, Korea, Arab, Sepanyol, Perancis, Jerman, Portugis, dan Rusia.
   - **21 Susun Atur Papan Kekunci Global:** Termasuk QWERTY, AZERTY, QWERTZ, Dvorak, Colemak, Arab, Rusia, Jepun, Korea, Cina, Thai, dan Vietnam (papan kekunci bukan-Latin dipadankan secara automatik dengan US — tekan `Alt + Shift` bila-bila masa untuk bertukar aksara, lengkap dengan fon Arab & CJK terbina!).
   - **32 Zon Masa Dunia & Format Jam 24-Jam / 12-Jam.**
5. **Pengurus Peranti UniCore (`--devmgr`):** Senarai peranti perkakasan langsung mengikut kategori: CPU, GPU, Rangkaian, Audio, Kamera, Storan, USB, Bluetooth, Input HID, dan Pengimbas SANE beserta pemacu kernel aktif.
6. **Pengurus Storan UniCore (`--storage`):** Pantau cakera dan partisyen, semak kesihatan S.M.A.R.T., keluarkan pemacu USB dengan selamat, dan bersihkan fail cache sementara.
7. **Pencetak & Pengimbas:** Aktifkan perkhidmatan cetakan CUPS atas permintaan (`0 MB` RAM ketika melahu), imbas pencetak USB/Wi-Fi (IPP Everywhere + Gutenprint), dan kesan pengimbas dokumen SANE secara langsung (*Simple Scan*).
8. **Keselamatan & Sandaran:** Tukar profil Tembok Api (*Rumah/LAN* vs *Wi-Fi Awam*), urus akaun pengguna, cipta sandaran `/data` bersulit AES-256, atau lancarkan **AegisTunnel VPN**.
9. **AegisTunnel VPN (`--vpn`):** Studio VPN Pasca-Kuantum & Anti-DPI terbina.
10. **Equalizer ResonanceFX (`--equalizer`):** Studio Equalizer 10-jalur & DSP Audio Ruang PipeWire terbina.

---

## 3. VPN Terbina AegisTunnel (`--vpn`)

UniCore OS membenamkan enjin **AegisTunnel VPN** (Rust asli) yang menggunakan **`0 MB` RAM** apabila tidak disambungkan:
- **Pemasangan Pantas:** Buka **Pusat Kawalan → AegisTunnel VPN**, klik **Generate Keypair** (kunci disimpan selamat `chmod 0600`), masukkan alamat pelayan (*Server Endpoint*) dan *Peer Public Key* anda (atau uji serta-merta menggunakan **Local Loopback Test Mode**), kemudian klik **Connect AegisTunnel**.
- **Anti-DPI & Mod Halimunan:** Pilih **Auto Happy Eyeballs (UDP + REALITY Fallback)** untuk menggunakan UDP pantas bertopeng AmneziaWG dan beralih automatik ke **REALITY TLS 1.3 TCP/443** sekiranya rangkaian menyekat trafik VPN.
- **Keselamatan Pasca-Kuantum Hibrid:** Aktifkan **Hybrid PQ ML-KEM-768** dan klik **Rotate PQ Key** bila-bila masa untuk memutar kunci pasca-kuantum secara langsung tanpa memutuskan sambungan.
- **Penapis DNS Tempatan (*DNS Sinkhole*):** Menyekat domain iklan dan penjejak terus di dalam terowong VPN.

---

## 4. Equalizer PipeWire Terbina ResonanceFX (`--equalizer`)

Tingkatkan kualiti bunyi pembesar suara dan fon kepala dengan studio C++17 **ResonanceFX Audio DSP** (`0 MB` RAM ketika dimatikan):
1. Buka **Pusat Kawalan → Equalizer ResonanceFX** dan klik **Enable PipeWire DSP**.
2. Pilih mana-mana daripada **7 Praset Akustik 1-Klik**: *Flat Reference*, *Pure Bass Boost*, *Vocal & Podcast Studio*, *Cinema 3D Surround*, *Hi-Fi Warm Tube*, *Gaming Footsteps*, atau *8D Spatial Orbit Live*.
3. Laraskan **Equalizer Grafik 10-Jalur (31 Hz–16 kHz)**, Preamp, **Pure Bass**, **Silky Clarity**, **3D Surround**, **Tube Warmth**, **Binaural / 8D Orbit**, **Convolver IR UPOLA**, **Schroeder Reverb**, **Stereo Chorus**, **Stereo Delay**, dan **Loudness**, lalu klik **Apply Live to PipeWire** untuk perubahan serta-merta.
4. Klik **Disable PipeWire DSP (0 MB)** bila-bila masa untuk mematikan penapis sepenuhnya.

---

## 5. Aplikasi, Aplikasi Web, Subsistem Mudah Alih & Game PC Klasik

- **Kedai Aplikasi Universal (`Menu Mula → Kedai Aplikasi`):** Pasang aplikasi Linux (Flatpak), PC Desktop, Pejabat, Media, dan Permainan dengan kawalan kebenaran *sandbox* terperinci (Rangkaian, GPU, Dokumen, Kamera, Mikrofon).
- **Aplikasi Web 1-Klik (`--webapps`):** Jadikan laman web harian sebagai aplikasi desktop kendiri dengan profil terasing (`0 MB` RAM melahu).
- **Subsistem Aplikasi Mudah Alih (`Pusat UniCore → Sistem → Subsistem Mudah Alih`):** Jalankan aplikasi `.apk` dalam mod berbilang tetingkap (`0 MB` RAM sebelum dibuka), dilengkapi pembantu 1-klik **Pengesahan GSF Peranti** (`google.com/android/uncertified`) serta pemasangan **Pasaran Aplikasi Terbuka (Aurora)**.
- **Auto-Talaan Permainan PC Klasik Dipermudahkan (`Menu Mula → Game Klasik` / `--retro-game`):**
  1. Pilih fail `.exe`, `.msi`, CD `.iso`, arkib `.zip`/`.7z`, atau folder permainan anda.
  2. Pilih 1 daripada **5 kad Auto-Talaan ringkas**: **Pintar Automatik**, **Klasik 2D 90-an**, **Klasik 3D 2000-an**, **Retro DOS / 16-Bit**, atau **Mod Ultra-Selamat** (mengunci 1 teras CPU, mengehadkan 60 FPS, dan menetapkan tetingkap selamat `800×600` secara automatik supaya permainan lama tidak ranap pada PC moden!).
  3. Buka **Hab Permainan (`--games`)** untuk melihat semua permainan anda, mengkonfigurasi alat kawalan (*gamepad* USB/Bluetooth + pemeta kekunci `--gamepad`), serta sandaran simpanan automatik (`--saves`).

---

## 6. 10 Tema Desktop, Sambung Telefon, Awan, Kawalan Keluarga & Kemas Kini

- **Studio 10 Tema Desktop (`--themes`):** Tukar kertas dinding, bar tugas, bingkai tetingkap, ikon, terminal, notifikasi, dan warna aplikasi serentak dalam 1 klik (*Midnight, Daylight, Aurora, Nordic, Dracula, Emerald, Sunset, Sakura, Cyberpunk,* dan *Retro Classic 98*).
- **Sambung Telefon (`--phone`):** Segerakkan notifikasi, SMS, papan keratan, perkongsian fail Wi-Fi (KDE Connect, `0 MB` melahu) serta cermin skrin USB/Wi-Fi (`scrcpy`).
- **Pemacu Awan (`--cloud`):** Lekap storan awan OAuth/WebDAV/Nextcloud terus ke dalam `~/Cloud`.
- **Migrasi Data PC Selamat (`--migrate`):** Import Dokumen, Gambar, Muzik, Video, Penanda Buku Pelayar dan Fon daripada partisyen NTFS dalam mod **Baca-Sahaja (*Read-Only*)** tanpa sebarang risiko.
- **Keselamatan Keluarga & Kawalan Ibu Bapa (`--parental`):** DNS Selamat Keluarga 1-klik (`1.1.1.3`), had masa skrin harian, waktu tidur, dan sekatan aplikasi khusus.
- **Kemas Kini OTA A/B Tanpa Gangguan (`Sistem → Kemas Kini OS`):** Ditandatangani secara kriptografi Ed25519; dipasang pada slot sistem tidak aktif dan undur automatik sekiranya semakan kesihatan but gagal.
