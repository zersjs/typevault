import { useEffect } from "react";
import { QueryClient, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { TooltipProvider } from "@radix-ui/react-tooltip";
import { Loader2, WifiOff } from "lucide-react";
import { BatchPanel } from "@/components/BatchPanel";
import { BulkBar } from "@/components/BulkBar";
import { FontDetail } from "@/components/FontDetail";
import { Sidebar, Toaster, TopBar } from "@/components/Shell";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import type { InstallEvent, Progress, SyncProgress } from "@/lib/types";
import { formatNumber } from "@/lib/utils";
import { AllFonts, Discover, Favorites } from "@/screens/Browse";
import { CollectionDetail, Collections } from "@/screens/Collections";
import { Downloads } from "@/screens/Downloads";
import { Library } from "@/screens/Library";
import { Settings } from "@/screens/Settings";
import { useUi, type Route } from "@/store/ui";

export const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: 30_000, refetchOnWindowFocus: false, retry: 1 } },
});

const LIVE_KEYS = ["fonts", "stats", "collections", "downloads", "batches", "catalog", "font"];

function useBackendEvents() {
  const client = useQueryClient();
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | null = null;
    const refresh = () => {
      if (timer) return;
      timer = setTimeout(() => {
        timer = null;
        for (const key of LIVE_KEYS) client.invalidateQueries({ queryKey: [key] });
      }, 500);
    };
    const ui = useUi.getState;
    const pending = [
      listen<Progress[]>("download://progress", (e) => {
        ui().mergeProgress(e.payload);
        if (e.payload.some((p) => p.status === "failed")) {
          const failed = e.payload.find((p) => p.status === "failed")!;
          ui().toast({ kind: "error", title: "İndirme başarısız", detail: failed.error ?? undefined });
        }
        if (e.payload.some((p) => p.status !== "downloading")) refresh();
      }),
      listen<InstallEvent>("install://progress", (e) => {
        const { fontId, family, state, message } = e.payload;
        ui().setInstalling(fontId, state === "installing");
        // Toplu işlerde tek kart gösterilir; bildirim yalnız ilk birkaç font içindir.
        if (state === "installed" && Object.keys(ui().progress).length < 3 && ui().installing.size === 0) {
          ui().toast(message
            ? { kind: "info", title: family, detail: message }
            : { kind: "success", title: `${family} kuruldu`, detail: "Tasarım uygulamalarınızda kullanılabilir." });
        }
        if (state === "failed") ui().toast({ kind: "error", title: `${family} kurulamadı`, detail: message ?? undefined });
      }),
      listen("library://changed", refresh),
      listen("library://font-changed", refresh),
      listen<SyncProgress>("catalog://progress", (e) => {
        const p = e.payload;
        if (p.phase === "done") {
          ui().setSync(null);
        } else if (p.phase === "error") {
          ui().setSync(null);
          ui().toast({ kind: "error", title: "Katalog eşitlenemedi", detail: p.message ?? undefined });
        } else {
          ui().setSync(p);
        }
        refresh();
      }),
    ];
    api.downloadSnapshot().then((items) => ui().mergeProgress(items));
    return () => {
      if (timer) clearTimeout(timer);
      pending.forEach((p) => p.then((off) => off()));
    };
  }, [client]);
}

function useTheme() {
  const { data } = useQuery({ queryKey: ["settings"], queryFn: api.getSettings });
  const theme = data?.theme ?? "dark";
  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      document.documentElement.dataset.theme = theme === "system" ? (media.matches ? "dark" : "light") : theme;
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme]);
}

const SHORTCUTS: Route[] = [
  { name: "discover" }, { name: "all" }, { name: "collections" }, { name: "library" }, { name: "downloads" }, { name: "favorites" },
];

function useShortcuts() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const ui = useUi.getState();
      if (e.metaKey && e.key === ",") {
        e.preventDefault();
        ui.navigate({ name: "settings" });
      } else if (e.metaKey && /^[1-6]$/.test(e.key)) {
        e.preventDefault();
        ui.navigate(SHORTCUTS[Number(e.key) - 1]);
      } else if (e.key === "Escape" && ui.selection.size > 0 && !ui.detailId) {
        ui.clearSelection();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}

function FirstRun({ error }: { error?: string }) {
  const sync = useUi((s) => s.sync);
  return (
    <div className="flex flex-1 items-center justify-center">
      <div className="anim-rise w-[340px] text-center">
        {error ? <WifiOff className="mx-auto size-6 text-faint" strokeWidth={1.5} /> : <Loader2 className="mx-auto size-5 animate-spin text-faint" />}
        <h1 className="mt-4 text-[17px] font-semibold">{error ? "Katalog alınamadı" : "Font kataloğu hazırlanıyor"}</h1>
        <p className="mt-1.5 text-[13px] leading-relaxed text-soft">
          {error ?? (sync?.total ? `${formatNumber(sync.total)} aile kaydediliyor` : "Google Fonts ile ilk eşitleme yapılıyor.")}
        </p>
        {error && <Button className="mt-4" onClick={() => api.syncCatalog()}>Yeniden dene</Button>}
      </div>
    </div>
  );
}

function Screen({ route }: { route: Route }) {
  switch (route.name) {
    case "discover": return <Discover />;
    case "all": return <AllFonts />;
    case "collections": return <Collections />;
    case "collection": return <CollectionDetail key={route.id} id={route.id} />;
    case "library": return <Library />;
    case "downloads": return <Downloads />;
    case "favorites": return <Favorites />;
    case "settings": return <Settings />;
  }
}

export function App() {
  useBackendEvents();
  useTheme();
  useShortcuts();
  const route = useUi((s) => s.route);
  const toasts = useUi((s) => s.toasts);
  const catalog = useQuery({ queryKey: ["catalog"], queryFn: api.catalogStatus });
  const empty = catalog.data?.total === 0;
  const syncError = empty && !catalog.data?.syncing ? toasts.find((t) => t.title === "Katalog eşitlenemedi")?.detail ?? "Sunucuya ulaşılamadı. İnternet bağlantınızı kontrol edin." : undefined;
  return (
    <TooltipProvider>
      <div className="flex h-full">
        <Sidebar />
        <main className="flex min-w-0 flex-1 flex-col">
          <TopBar />
          {empty ? <FirstRun error={syncError} /> : catalog.data ? <Screen route={route} /> : null}
        </main>
      </div>
      <FontDetail />
      <BulkBar />
      <BatchPanel />
      <Toaster />
    </TooltipProvider>
  );
}
