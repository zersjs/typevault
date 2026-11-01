# TYPEVAULT Windows Geçiş Rehberi

Bu belge, projeyi Windows makinede Claude Code ile sürdürmek için yazıldı. Önce bunu baştan sona oku, sonra aşamaları sırayla uygula.

## Mevcut durum

- macOS (Apple Silicon) sürümü çalışıyor ve gerçek fontlarla test edildi.
- **Windows portuna başlanmadı.** Kod şu anda Windows'ta derlenmez: `src-tauri/src/installer/coretext.rs` macOS'a özgü `block2` ve `core_foundation` paketlerini kullanıyor.
- Bu belgedeki Windows bilgileri henüz Windows'ta denenmedi. Çelişki görürsen gerçek davranışa uy ve belgeyi güncelle.

## Hedef

macOS'taki deneyimin aynısı: **fontu bul → Kur'a bas → yönetici izni istemeden Photoshop, Illustrator, Figma ve Affinity'de kullan.** Arayüz Türkçe kalır. macOS sürümü bozulmaz.

## Kurulum

Windows 10 1809 veya üstü gerekir. Kullanıcı başına font kurulumu bu sürümle geldi.

1. [Node.js 22](https://nodejs.org)
2. [Rust](https://rustup.rs), `stable-x86_64-pc-windows-msvc` araç zinciri
3. Visual Studio Build Tools: "Desktop development with C++" ve Windows SDK
4. WebView2 Runtime (Windows 11'de hazır gelir)

```powershell
git clone https://github.com/zersjs/typevault.git
cd typevault
npm install
npm run typecheck
```

Kök `.cargo/config.toml` yalnız Mac için `DEVELOPER_DIR` tanımlar ve Windows'ta etkisizdir. `Cargo.lock` içinde `scheduled-thread-pool` 0.2.7'de tutuldu, çünkü 0.2.8 Rust 1.95 ister. Rust sürümün 1.95 veya üstüyse bu kısıt gerekli değildir.

## Değişecek noktalar

### 1. Önizleme protokolü (en kolay gözden kaçan sorun)

Tauri 2, özel protokolleri Windows'ta WebView2 üzerinden şu adresten sunar:

| Platform | Adres |
| --- | --- |
| macOS | `tvfont://localhost/...` |
| Windows | `http://tvfont.localhost/...` |

`tvfont://localhost` şu üç yerde sabit yazılı. Hepsi platforma göre seçilmeli, yoksa kartlardaki font önizlemeleri sessizce genel yazı tipine düşer:

- `src/lib/fontLoader.ts`: `const base`
- `src-tauri/src/fonts/preview.rs`: `rewrite_css` içindeki `asset` adresi ve ilgili test
- `src-tauri/tauri.conf.json`: `csp` ve `devCsp` içindeki `style-src`, `font-src`, `connect-src`. Bu alanlara `http://tvfont.localhost` eklenmeli.

### 2. Kurulum katmanı

`src-tauri/src/installer/mod.rs` platformdan bağımsızdır ve `coretext` modülündeki şu API'ye bağlıdır:

| İşlev | macOS | Windows karşılığı |
| --- | --- | --- |
| `register(paths)` | `CTFontManagerRegisterFontURLs` | Kayıt defteri girdisi + `AddFontResourceW` + `WM_FONTCHANGE` |
| `unregister(paths)` | `CTFontManagerUnregisterFontURLs` | `RemoveFontResourceW` + girdiyi sil + `WM_FONTCHANGE` |
| `is_registered(path)` | `CTFontManagerGetScopeForURL` | Kayıt defteri girdisi bu yolu gösteriyor mu |
| `describe(path)` | `CTFontManagerCreateFontDescriptorsFromURL` | DirectWrite dosya çözümlemesi veya `ttf-parser` |
| `available_postscript_names()` | `CTFontManagerCopyAvailablePostScriptNames` | DirectWrite sistem koleksiyonu |
| `locate(name)` | `CTFontDescriptor` + URL özniteliği | PostScript adından dosya yoluna DirectWrite eşlemesi |
| `scan_system_fonts()` | `CTFontCollection` | DirectWrite sistem koleksiyonu |

Önerilen yapı: `coretext.rs` dosyasını `#[cfg(target_os = "macos")]` ile sınırla, aynı imzalarla `src-tauri/src/installer/windows.rs` ekle, `mod.rs` içinde ortak bir takma adla seç:

```rust
#[cfg(target_os = "macos")]
#[path = "coretext.rs"]
pub mod platform;
#[cfg(windows)]
#[path = "windows.rs"]
pub mod platform;
```

Ardından `mod.rs` içindeki `coretext::` çağrılarını `platform::` yap. `user_font_root` da şuraya taşınmalı:

| | macOS | Windows |
| --- | --- | --- |
| Kurulum kökü | `~/Library/Fonts/TYPEVAULT` | `%LOCALAPPDATA%\Microsoft\Windows\Fonts\TYPEVAULT` |
| Kalıcılık | CoreText kalıcı kapsam | `HKCU\Software\Microsoft\Windows NT\CurrentVersion\Fonts` |

`Cargo.toml` için:

```toml
[target.'cfg(windows)'.dependencies]
windows = { version = "0.61", features = [
  "Win32_Foundation",
  "Win32_Graphics_Gdi",
  "Win32_Graphics_DirectWrite",
  "Win32_UI_WindowsAndMessaging",
] }
winreg = "0.55"
```

Sürümleri kurmadan önce güncel olanla karşılaştır.

#### Windows kurulum sırası

1. Dosya doğrulaması aynen kalır: özet, tablo sınırı, `parse::inspect`.
2. Dosyayı kurulum köküne kopyala.
3. `HKCU\...\Fonts` altına değeri yaz. Ad: `<PostScript adı> (TrueType)` veya OTF için `(OpenType)`. Veri: tam dosya yolu.
4. `AddFontResourceW(yol)` çağır. Dönüş 0 ise başarısızdır; kayıt defteri girdisini ve kopyayı geri al.
5. `SendMessageTimeoutW(HWND_BROADCAST, WM_FONTCHANGE, ...)` gönder. `SMTO_ABORTIFHUNG` kullan, yanıt vermeyen pencere uygulamayı dondurmasın.
6. DirectWrite'tan `GetSystemFontCollection(checkForUpdates = TRUE)` ile PostScript adını doğrula. **Sistem doğrulamadan font kurulu sayılmaz.**

Kaldırma sırası: `RemoveFontResourceW` → kayıt defteri değerini sil → `WM_FONTCHANGE` → dosyayı sil.

#### Dikkat edilecek Windows farkları

- **Kilitli dosyalar:** GDI'nin yüklediği font silinemez ve üzerine yazılamaz. Tasarım uygulaması açıksa silme başarısız olabilir. Birkaç kez yeniden dene, olmazsa hatayı Türkçe göster. Kaydı silip dosyayı öksüz bırakma. `install_one` içindeki güncelleme akışında `rename` da aynı nedenle başarısız olabilir.
- **Oturum etkisi:** `AddFontResourceW` yalnız geçerli oturumu etkiler. Kalıcılığı kayıt defteri girdisi sağlar. Oturum açılışında Windows'un bu girdiyi yüklediği test edilmeli.
- **Toplu kurulum hızı:** `WM_FONTCHANGE` her fontta yayınlanırsa toplu işlem yavaşlar. Ailede veya toplu işin sonunda bir kez yayınla. DirectWrite koleksiyonunu da dosya başına yeniden kurma, `install_one` içindeki `locate` çağrısı için toplu iş başına bir `HashMap` önbelleği tut.
- **Yol karşılaştırma:** `security::is_inside` büyük/küçük harfe duyarlı çalışır, Windows yolları değildir. `\\?\` önekli yollar da eşleşmeyebilir. Karşılaştırmadan önce normalleştir ve testini yaz.
- **Dosya adları:** `sanitize_filename`, `CON`, `NUL`, `AUX`, `COM1` gibi ayrılmış adları ve sondaki nokta veya boşluğu reddetmeli.
- **`HOME`:** Windows'ta tanımlı olmayabilir. `installer::user_font_root` için `LOCALAPPDATA` kullan.

### 3. Pencere

`src-tauri/tauri.conf.json` içinde `titleBarStyle`, `hiddenTitle` ve `trafficLightPosition` macOS içindir. İlk aşamada standart Windows başlık çubuğunu kullan. `src/components/Shell.tsx` içindeki 52 piksellik sürükleme boşluğu Windows'ta gereksizdir; yalnız Mac'te göster. Özel başlık çubuğu daha sonra ele alınabilir.

### 4. Paketleme

`tauri.conf.json` içinde `bundle.targets` değeri `["app", "dmg"]`. `npx tauri build --bundles nsis` ile çalıştır. `bundle.icon` listesine `icons/icon.ico` ekle; dosya depoda var. `macOS.signingIdentity` Windows'ta etkisizdir. Kullanıcı başına kurulum için NSIS `installMode: "currentUser"` yönetici izni gerektirmez.

### 5. Arayüz

Platform bilgisini tek yerde tut, örneğin `src/lib/platform.ts`:

```ts
export const isMac = navigator.userAgent.includes("Mac");
export const modKey = (e: KeyboardEvent) => (isMac ? e.metaKey : e.ctrlKey);
export const osName = isMac ? "macOS" : "Windows";
export const fileManager = isMac ? "Finder" : "Dosya Gezgini";
```

| Dosya | Değişiklik |
| --- | --- |
| `src/App.tsx`, `src/components/Shell.tsx`, `src/components/FontGrid.tsx` | `e.metaKey` yerine `modKey(e)` |
| `src/components/Shell.tsx` | `⌘K` etiketi Windows'ta `Ctrl K` |
| `src/screens/Library.tsx`, `src/screens/Collections.tsx` | "macOS" ve "Finder'da göster" metinleri |
| `src/screens/Settings.tsx` | `~/Library/Fonts/TYPEVAULT` yolu |
| `src/index.css`, `FontCard.tsx`, `FontDetail.tsx` | Yazı tipi yığınına `"Segoe UI Variable", "Segoe UI"` |
| `vite.config.ts` | `build.target` için `safari15` yanına `chrome110` |
| `src-tauri/src/installer/mod.rs` | Hata iletilerindeki "macOS" sözcüğü |

### 6. Testler

Şu testler Mac'e özgü dosya yolları kullanır, `#[cfg(target_os = "macos")]` ile sınırla veya Windows karşılığını ekle (`C:\Windows\Fonts\arial.ttf`):

- `src-tauri/src/fonts/parse.rs`: `reads_system_font`, `rejects_truncated_files`
- `src-tauri/src/installer/coretext.rs`: tüm testler
- `src-tauri/src/security/mod.rs`: `safe_join_blocks_traversal`, `is_inside` kontrolleri `/a/b` yolları kullanıyor

## Uygulama sırası

1. Yukarıdaki kurulum adımları: `npm install`, `npm run typecheck`.
2. `installer` katmanını ayır, önce gerçek işlevlerle en küçük `windows.rs` dosyasını derlet. `cargo test --lib` tüm testleri geçsin.
3. `tvfont` adreslerini düzelt, `npm run tauri dev` ile kartlarda gerçek font örneklerini gör.
4. Tek font kur ve kaldır. Doğrulamayı uygulama dışından yap.
5. Toplu kurulum, kesinti ve hata senaryoları.
6. Arayüz metinleri, kısayollar, paketleme.
7. Mac'te yeniden derleyip gerilemeyi kontrol et.

## Kabul testleri

Bu senaryolar macOS'ta gerçek hataları yakaladı. Windows'ta da gerçek font ve gerçek uygulamayla uygulanmalı, veritabanına bakarak değil **uygulama dışından** doğrulanmalı.

| Senaryo | Beklenen sonuç |
| --- | --- |
| Tek font kur | Yönetici izni yok, Word yazı tipi listesinde görünür |
| Uygulamayı kapat, oturumu kapatıp aç | Font hâlâ kullanılabilir |
| Tasarım uygulaması açıkken kur | Yeniden başlatma gerekip gerekmediği belgelenir |
| Kaldır | Kayıt defteri girdisi ve dosya gider, font listeden çıkar |
| Kullanımdaki fontu kaldır | Anlaşılır hata, kayıt ve dosya tutarlı kalır |
| 14 MB'lık indirmede süreci `taskkill /F` ile öldür | Kaldığı yerden sürer, dosya taze indirmeyle bayt bayt aynıdır |
| Kurulum ortasında süreci öldür | Açılışta "Kurulum yarıda kaldı" görünür, Yeniden dene tamamlar |
| Toplu kurulumda bir dosyayı bozuk bırak | Diğerleri kurulur, bozuk olan raporlanır, Yeniden dene yeniden indirir |
| Başka kaynaktan kurulu font | `external` olarak işaretlenir, dosyasına dokunulmaz, TYPEVAULT kaldıramaz |
| 3 aileyi toplu kur, kaldır, dosyalarını sil | Sessiz hata yok, seçilen her font işlenir |
| Türkçe | `ç Ç ğ Ğ ı İ i I ö Ö ş Ş ü Ü` gerçek cmap ile doğrulanır |

### Doğrulanması gereken açık soru

Bazı Adobe sürümlerinin kullanıcı başına kurulan fontları görüp görmediği **bu projede denenmedi**. Photoshop, Illustrator, Figma ve Affinity'de gerçek test yap. Görmeyen uygulama olursa sonucu bildir. Tüm kullanıcılar için kurulum `C:\Windows\Fonts` ve yönetici izni gerektirir; bu seçeneği kullanıcı onayı olmadan ekleme.

## Değişmeyecek kurallar

- Sahte ilerleme, sahte başarı ve çalışmayan düğme olmaz.
- Sistem doğrulamadan font kurulu sayılmaz.
- TYPEVAULT'un sahip olmadığı fontlar değiştirilmez ve silinmez.
- Lisansı doğrulanamayan font kurulmaz.
- Kurulum için kabuk betiği, `sudo` veya yönetici izni kısayolu kullanılmaz.
- Arayüz metinleri Türkçe kalır.
- Kurulu font kök klasörü dışında dosya silinmez. `is_inside` denetimi korunur.

## Veri ve mimari notları

- Google Fonts Developer API anahtar ister. Bu yüzden katalog `fonts.google.com/metadata/fonts` ve `download/list` uç noktalarını kullanır. Bunlar **resmî değildir**. Anahtarlı API yolu kodda var ama gerçek anahtarla denenmedi.
- İndirmelerde `Accept-Encoding: identity` şarttır. Kaldırılırsa Range ile devam eden dosyalar kesik kalır.
- Veritabanı yazma işlemlerinde okumayla başlayan her işlem `TransactionBehavior::Immediate` kullanır.
- Dosya silme, veritabanı işlemi tamamlandıktan sonra yapılır.
- Kod sınırları için `src-tauri/src/` altındaki `commands`, `downloader`, `installer`, `fonts`, `metadata`, `database`, `archive`, `security` modüllerine bak.
