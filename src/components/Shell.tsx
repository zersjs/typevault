import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { AlertCircle, CheckCircle2, Compass, Download, FolderOpen, Heart, Info, LayoutGrid, Library, Search, Settings, X } from "lucide-react";
import { api } from "@/lib/api";
import { cn, formatNumber } from "@/lib/utils";
import { useUi, type Route } from "@/store/ui";

const NAV: { route: Route; label: string; icon: typeof Compass; key: string }[] = [
  { route: { name: "discover" }, label: "Keşfet", icon: Compass, key: "discover" },
  { route: { name: "all" }, label: "Tüm Fontlar", icon: LayoutGrid, key: "all" },
  { route: { name: "collections" }, label: "Koleksiyonlar", icon: FolderOpen, key: "collections" },
  { route: { name: "library" }, label: "Kurulu", icon: Library, key: "library" },
  { route: { name: "downloads" }, label: "İndirilenler", icon: Download, key: "downloads" },
  { route: { name: "favorites" }, label: "Favoriler", icon: Heart, key: "favorites" },
  { route: { name: "settings" }, label: "Ayarlar", icon: Settings, key: "settings" },
];

export function Sidebar() {
  const route = useUi((s) => s.route);
  const navigate = useUi((s) => s.navigate);
  const stats = useQuery({ queryKey: ["stats"], queryFn: api.libraryStats });
  const counts: Record<string, number | undefined> = {
    library: stats.data?.installed || undefined,
    downloads: stats.data?.activeDownloads || undefined,
    favorites: stats.data?.favorites || undefined,
  };
  const active = route.name === "collection" ? "collections" : route.name;
  return (
    <aside className="flex w-[208px] shrink-0 flex-col border-r border-line bg-panel">
      <div data-tauri-drag-region className="h-[52px] shrink-0" />
      <div data-tauri-drag-region className="px-4 pb-4 text-[12px] font-semibold tracking-[0.2em] text-ink">TYPEVAULT</div>
      <nav className="flex flex-col gap-px px-2">
        {NAV.map(({ route: target, label, icon: Icon, key }) => (
          <button
            key={key}
            type="button"
            onClick={() => navigate(target)}
            className={cn(
              "flex h-[30px] items-center gap-2.5 rounded-md px-2.5 text-[13px] transition-colors",
              active === key ? "bg-hover text-ink" : "text-soft hover:bg-hover/60 hover:text-ink",
            )}
          >
            <Icon className="size-[15px]" strokeWidth={1.75} />
            <span>{label}</span>
            {counts[key] !== undefined && <span className="ml-auto text-[11px] tabular-nums text-faint">{formatNumber(counts[key]!)}</span>}
          </button>
        ))}
      </nav>
      <div className="mt-auto"><SyncStatus /></div>
    </aside>
  );
}

const PHASES: Record<string, string> = {
  catalog: "Katalog alınıyor",
  tags: "Etiketler alınıyor",
  store: "Katalog kaydediliyor",
  enrich: "Lisans ve glifler doğrulanıyor",
};

function SyncStatus() {
  const sync = useUi((s) => s.sync);
  if (!sync || !PHASES[sync.phase]) return null;
  const percent = sync.total ? Math.round((sync.done / sync.total) * 100) : 0;
  return (
    <div className="border-t border-line px-3.5 py-3 text-[11px] text-soft">
      <div className="flex justify-between gap-2">
        <span className="truncate">{PHASES[sync.phase]}</span>
        {sync.total > 0 && <span className="tabular-nums text-faint">%{percent}</span>}
      </div>
      <div className="mt-1.5 h-[3px] overflow-hidden rounded-full bg-line-strong">
        <div className="h-full bg-soft transition-[width]" style={{ width: `${sync.total ? percent : 8}%` }} />
      </div>
    </div>
  );
}

export function TopBar() {
  const search = useUi((s) => s.search);
  const setSearch = useUi((s) => s.setSearch);
  const previewText = useUi((s) => s.previewText);
  const setPreviewText = useUi((s) => s.setPreviewText);
  const route = useUi((s) => s.route);
  const navigate = useUi((s) => s.navigate);
  const [local, setLocal] = useState(search);
  const input = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const timer = setTimeout(() => {
      if (local === useUi.getState().search) return;
      setSearch(local);
      if (local && !["discover", "all", "collection", "favorites", "library"].includes(route.name)) navigate({ name: "all" });
    }, 180);
    return () => clearTimeout(timer);
  }, [local]);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey && e.key.toLowerCase() === "k") {
        e.preventDefault();
        input.current?.focus();
        input.current?.select();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <header data-tauri-drag-region className="flex h-[52px] shrink-0 items-center gap-3 border-b border-line px-4">
      <div className="relative w-[280px]">
        <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-faint" />
        <input
          ref={input}
          value={local}
          onChange={(e) => setLocal(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              setLocal("");
              e.currentTarget.blur();
            }
          }}
          placeholder="Font, tasarımcı veya kategori ara"
          className="h-8 w-full rounded-md border border-line bg-raised pl-8 pr-10 text-[13px] text-ink placeholder:text-faint focus:border-line-strong"
        />
        <kbd className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 font-sans text-[11px] text-faint">⌘K</kbd>
      </div>
      <input
        value={previewText}
        onChange={(e) => setPreviewText(e.target.value)}
        placeholder="Önizleme metni yazın"
        aria-label="Önizleme metni"
        className="h-8 min-w-0 flex-1 rounded-md border border-transparent bg-transparent px-2.5 text-[13px] text-ink placeholder:text-faint hover:border-line focus:border-line-strong"
      />
    </header>
  );
}

const TOAST_ICON = { success: CheckCircle2, error: AlertCircle, info: Info };

export function Toaster() {
  const toasts = useUi((s) => s.toasts);
  const dismiss = useUi((s) => s.dismissToast);
  return (
    <div className="pointer-events-none fixed bottom-[68px] left-1/2 z-50 flex w-[360px] -translate-x-1/2 flex-col gap-2">
      {toasts.map((t) => {
        const Icon = TOAST_ICON[t.kind];
        return (
          <div key={t.id} className="anim-rise pointer-events-auto flex items-start gap-2.5 rounded-lg border border-line-strong bg-raised px-3 py-2.5 shadow-[var(--shadow)]">
            <Icon className={cn("mt-px size-4 shrink-0", t.kind === "success" ? "text-good" : t.kind === "error" ? "text-bad" : "text-soft")} />
            <div className="min-w-0 flex-1">
              <div className="text-[13px] font-medium">{t.title}</div>
              {t.detail && <div className="mt-0.5 break-words text-xs text-soft">{t.detail}</div>}
            </div>
            <button type="button" aria-label="Kapat" onClick={() => dismiss(t.id)} className="text-faint hover:text-ink"><X className="size-3.5" /></button>
          </div>
        );
      })}
    </div>
  );
}
