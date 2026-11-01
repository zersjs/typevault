# TYPEVAULT

Türkçe arayüzlü, açık kaynak masaüstü font yöneticisi. Tauri 2, Rust, React, TypeScript, SQLite.

Windows üzerinde çalışıyorsan önce [docs/WINDOWS.md](docs/WINDOWS.md) dosyasını oku ve aşamaları sırayla uygula.

## Komutlar

- Tür denetimi: `npm run typecheck`
- Rust testleri: `src-tauri` içinde `cargo test --lib`
- Geliştirme: `npm run tauri dev`
- Paket: `npm run tauri build`

## Kurallar

- Arayüz metinleri Türkçe olur.
- Sahte ilerleme, sahte başarı veya çalışmayan düğme eklenmez.
- İşletim sistemi doğrulamadan font kurulu sayılmaz.
- TYPEVAULT'un sahip olmadığı fontlara dokunulmaz.
- Değişiklikler birim testle yetinmez, gerçek uygulamada gerçek fontla denenir.
