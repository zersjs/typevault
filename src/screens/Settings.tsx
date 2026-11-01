import { useEffect, useState, type ReactNode } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useAction } from "@/components/FontActions";
import { Button } from "@/components/ui/button";
import { Select, Switch } from "@/components/ui/controls";
import { api, errorText } from "@/lib/api";
import { formatBytes, formatNumber } from "@/lib/utils";
import { useUi } from "@/store/ui";
import { PageTitle } from "./Browse";

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="mt-6">
      <h2 className="mb-2 text-xs font-medium text-soft">{title}</h2>
      <div className="rounded-[10px] border border-line bg-panel">{children}</div>
    </section>
  );
}

function Row({ label, detail, children }: { label: string; detail?: string; children: ReactNode }) {
  return (
    <div className="flex min-h-12 items-center gap-6 border-b border-line px-3.5 py-2.5 last:border-0">
      <div className="min-w-0 flex-1">
        <div className="text-[13px]">{label}</div>
        {detail && <div className="mt-0.5 break-words text-[11.5px] leading-snug text-faint">{detail}</div>}
      </div>
      <div className="flex shrink-0 items-center gap-2">{children}</div>
    </div>
  );
}

function Input({ value, onCommit, placeholder, type = "text", width = 220 }: {
  value: string; onCommit: (v: string) => void; placeholder?: string; type?: string; width?: number;
}) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  return (
    <input
      type={type}
      value={draft}
      placeholder={placeholder}
      style={{ width }}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => draft !== value && onCommit(draft)}
      onKeyDown={(e) => e.key === "Enter" && e.currentTarget.blur()}
      className="h-7 rounded-md border border-line bg-raised px-2 text-xs text-ink placeholder:text-faint focus:border-line-strong"
    />
  );
}

export function Settings() {
  const client = useQueryClient();
  const toast = useUi((s) => s.toast);
  const { data: s } = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });
  const storage = useQuery({ queryKey: ["storage"], queryFn: api.storageInfo });
  const catalog = useQuery({ queryKey: ["catalog"], queryFn: api.catalogStatus });
  const syncing = useUi((u) => u.sync !== null);

  const save = async (key: string, value: string) => {
    try {
      await api.setSetting(key, value);
      client.invalidateQueries({ queryKey: ["settings"] });
    } catch (e) {
      toast({ kind: "error", title: "Ayar kaydedilemedi", detail: errorText(e) });
    }
  };

  const sync = useAction(api.syncCatalog);
  const pickDir = useAction(api.chooseDownloadDir, (path) => {
    if (path) client.invalidateQueries({ queryKey: ["settings"] });
  });
  const resetDir = useAction(api.resetDownloadDir, () => client.invalidateQueries({ queryKey: ["settings"] }));
  const clearCache = useAction(api.clearCache, () => {
    client.invalidateQueries({ queryKey: ["storage"] });
    toast({ kind: "success", title: "Önizleme önbelleği temizlendi" });
  });
  const exportData = useAction(api.exportLibrary, (path) => {
    if (path) toast({ kind: "success", title: "Kitaplık dışa aktarıldı", detail: path });
  });
  const testKey = useAction(() => api.testApiKey(s?.google_api_key ?? ""), (n) => {
    toast({ kind: "success", title: "API anahtarı geçerli", detail: `${formatNumber(n)} aile döndürdü` });
  });

  if (!s) return null;
  const customDir = s.download_dir !== s.default_download_dir;
  const last = catalog.data?.lastSync ? new Date(catalog.data.lastSync).toLocaleString("tr-TR") : "Henüz tamamlanmadı";

  return (
    <div className="min-h-0 flex-1 overflow-y-auto pb-16">
      <div className="mx-auto max-w-[680px]">
        <PageTitle title="Ayarlar" />
        <div className="px-4">
          <Section title="Görünüm">
            <Row label="Tema">
              <Select
                value={s.theme}
                onChange={(v) => save("theme", v ?? "dark")}
                options={[{ value: "dark", label: "Koyu" }, { value: "light", label: "Açık" }, { value: "system", label: "Sisteme uy" }]}
              />
            </Row>
            <Row label="Varsayılan önizleme metni">
              <Input value={s.preview_text} onCommit={(v) => save("preview_text", v)} placeholder="TYPEVAULT Aa" />
            </Row>
          </Section>

          <Section title="Türkçe">
            <Row label="Türkçe destekli fontları öne çıkar" detail="Tüm Türkçe glifleri doğrulanan fontlar listelerde önce gösterilir.">
              <Switch label="Türkçe öncelik" checked={s.turkish_priority === "1"} onChange={(v) => save("turkish_priority", v ? "1" : "0")} />
            </Row>
          </Section>

          <Section title="İndirme ve kurulum">
            <Row label="Eşzamanlı indirme" detail="Aynı anda indirilen en fazla aile sayısı.">
              <Select
                value={s.download_concurrency}
                onChange={(v) => save("download_concurrency", v ?? "4")}
                options={["1", "2", "3", "4", "6", "8", "12"].map((n) => ({ value: n, label: n }))}
              />
            </Row>
            <Row label="Değişken fontlar" detail="Statik kesimler Photoshop, Affinity ve Figma'da en geniş uyumu sağlar. İkisi birlikte kurulmaz, adlar çakışır.">
              <Select
                value={s.font_format}
                onChange={(v) => save("font_format", v ?? "static")}
                options={[{ value: "static", label: "Statik tercih et" }, { value: "variable", label: "Değişken tercih et" }]}
              />
            </Row>
            <Row label="İndirme klasörü" detail={s.download_dir}>
              {customDir && <Button size="sm" variant="ghost" onClick={() => resetDir.mutate()}>Varsayılan</Button>}
              <Button size="sm" onClick={() => pickDir.mutate()}>Değiştir</Button>
              <Button size="sm" variant="ghost" onClick={() => api.revealDownloads()}>Göster</Button>
            </Row>
          </Section>

          <Section title="Katalog">
            <Row label="Otomatik eşitleme" detail="Katalog haftada bir arka planda yenilenir.">
              <Switch label="Otomatik eşitleme" checked={s.auto_sync === "1"} onChange={(v) => save("auto_sync", v ? "1" : "0")} />
            </Row>
            <Row
              label="Katalog"
              detail={`${formatNumber(catalog.data?.total ?? 0)} aile · ${formatNumber(catalog.data?.turkishFull ?? 0)} tam Türkçe · Son eşitleme: ${last}`}
            >
              <Button size="sm" disabled={syncing} onClick={() => sync.mutate()}>{syncing ? "Eşitleniyor" : "Şimdi eşitle"}</Button>
            </Row>
            <Row label="Google Fonts API anahtarı" detail="İsteğe bağlı. Katalog anahtarsız çalışır; anahtar, resmî Developer API'yi ek kaynak olarak açar.">
              <Input value={s.google_api_key} type="password" width={170} onCommit={(v) => save("google_api_key", v.trim())} placeholder="Anahtar yok" />
              <Button size="sm" disabled={!s.google_api_key} onClick={() => testKey.mutate()}>Sına</Button>
            </Row>
          </Section>

          <Section title="Depolama">
            <Row label="İndirilen fontlar" detail={storage.data ? formatBytes(storage.data.fontsBytes) : undefined}>{null}</Row>
            <Row label="Kurulu fontlar" detail={storage.data ? `${formatBytes(storage.data.installedBytes)} · ~/Library/Fonts/TYPEVAULT` : undefined}>
              <Button size="sm" variant="ghost" onClick={() => api.revealInstalled()}>Göster</Button>
            </Row>
            <Row label="Önizleme önbelleği" detail={storage.data ? formatBytes(storage.data.cacheBytes) : undefined}>
              <Button size="sm" onClick={() => clearCache.mutate()}>Önbelleği temizle</Button>
            </Row>
            <Row label="Kitaplık meta verisi" detail="İndirilen, kurulu ve favori fontları JSON olarak kaydeder.">
              <Button size="sm" onClick={() => exportData.mutate()}>Dışa aktar</Button>
            </Row>
          </Section>
        </div>
      </div>
    </div>
  );
}
