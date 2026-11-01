import { useEffect, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { FolderOpen, HardDrive, Library as LibraryIcon, Loader2, RefreshCw } from "lucide-react";
import { useAction } from "@/components/FontActions";
import { EmptyState, FontGrid } from "@/components/FontGrid";
import { SelectAllButton } from "@/components/Toolbar";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import type { FontQuery } from "@/lib/types";
import { cn, formatBytes, formatNumber } from "@/lib/utils";
import { useUi } from "@/store/ui";
import { Chip, PageTitle } from "./Browse";

type Tab = "installed" | "downloaded" | "updates" | "failed" | "available" | "system";

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 pr-7">
      <div className="text-[20px] font-semibold tabular-nums tracking-tight">{value}</div>
      <div className="mt-0.5 truncate text-[11px] text-faint">{label}</div>
    </div>
  );
}

function SystemFonts() {
  const search = useUi((s) => s.search);
  const client = useQueryClient();
  const list = useQuery({ queryKey: ["system-fonts", search], queryFn: () => api.listSystemFonts(search) });
  const scan = useAction(api.scanSystemFonts, () => client.invalidateQueries({ queryKey: ["system-fonts"] }));
  const items = list.data ?? [];
  useEffect(() => {
    if (list.data?.length === 0 && !search) scan.mutate();
  }, [list.data]);
  return (
    <div className="min-h-0 flex-1 overflow-y-auto px-4 pb-16">
      <div className="mb-2 flex items-center gap-3">
        <span className="text-xs text-soft">{formatNumber(items.length)} aile macOS'ta kullanılabilir</span>
        <Button size="sm" className="ml-auto" onClick={() => scan.mutate()} disabled={scan.isPending}>
          {scan.isPending ? <Loader2 className="animate-spin" /> : <RefreshCw />}Sistemi tara
        </Button>
      </div>
      {items.length === 0 ? (
        <EmptyState icon={HardDrive} title={scan.isPending ? "Sistem taranıyor" : "Font bulunamadı"} detail="Tarama, macOS'ta kurulu tüm fontları listeler. Hiçbir dosya değiştirilmez." />
      ) : (
        <ul className="rounded-[10px] border border-line">
          {items.map((f) => (
            <li key={f.family} className="flex items-center gap-3 border-b border-line px-3 py-2 [contain-intrinsic-size:auto_44px] [content-visibility:auto] last:border-0">
              <span className="min-w-0 flex-1 truncate text-[17px]" style={{ fontFamily: `"${f.family}"` }}>{f.family}</span>
              <span className="w-14 shrink-0 text-right text-[11px] tabular-nums text-faint">{f.styles} stil</span>
              <span className={cn("w-20 shrink-0 text-right text-[11px]", f.managed ? "text-good" : "text-faint")}>{f.managed ? "TYPEVAULT" : "Sistem"}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

export function Library() {
  const [tab, setTab] = useState<Tab>("installed");
  const stats = useQuery({ queryKey: ["stats"], queryFn: api.libraryStats });
  const navigate = useUi((s) => s.navigate);
  const s = stats.data;
  const query = useMemo<FontQuery>(() => {
    switch (tab) {
      case "installed": return { localStatus: "installed" };
      case "downloaded": return { localStatus: "downloaded" };
      case "failed": return { localStatus: "install_failed" };
      case "updates": return { updatesOnly: true };
      default: return { localStatus: "available" };
    }
  }, [tab]);
  const tabs: { id: Tab; label: string; count?: number }[] = [
    { id: "installed", label: "Kurulu", count: s?.installed },
    { id: "downloaded", label: "İndirildi", count: s?.downloaded },
    { id: "updates", label: "Güncelleme var", count: s?.updates },
    { id: "failed", label: "Kurulum başarısız", count: s?.failed },
    { id: "available", label: "Kullanılabilir", count: s?.available },
    { id: "system", label: "Sistem Fontları" },
  ];
  const header = (
    <div>
      <PageTitle title="Kurulu">
        <Button size="sm" className="ml-auto" onClick={() => api.revealInstalled()}><FolderOpen />Finder'da göster</Button>
      </PageTitle>
      <div className="flex px-4 pt-4">
        <Stat label="Kurulu aile" value={formatNumber(s?.installed ?? 0)} />
        <Stat label="Kurulu dosya" value={formatNumber(s?.installedFiles ?? 0)} />
        <Stat label="Yerel dosya" value={formatNumber(s?.localFiles ?? 0)} />
        <Stat label="Yerel boyut" value={formatBytes(s?.localBytes ?? 0)} />
      </div>
      <div className="flex flex-wrap items-center gap-1 px-4 pb-3 pt-3">
        {tabs.map((t) => (
          <Chip key={t.id} active={tab === t.id} onClick={() => setTab(t.id)}>
            {t.label}{t.count ? ` ${formatNumber(t.count)}` : ""}
          </Chip>
        ))}
        {tab !== "system" && <div className="ml-auto"><SelectAllButton query={query} /></div>}
      </div>
    </div>
  );
  if (tab === "system") {
    return <div className="flex min-h-0 flex-1 flex-col">{header}<SystemFonts /></div>;
  }
  return (
    <FontGrid
      query={query}
      header={header}
      empty={
        <EmptyState
          icon={LibraryIcon}
          title={tab === "installed" ? "Henüz font kurulmadı" : "Burada font yok"}
          detail={tab === "installed" ? "Bir font seçip Kur düğmesine basın; Photoshop ve diğer uygulamalarda hemen kullanılabilir." : undefined}
          action={tab === "installed" ? <Button onClick={() => navigate({ name: "discover" })}>Fontları keşfet</Button> : undefined}
        />
      }
    />
  );
}
