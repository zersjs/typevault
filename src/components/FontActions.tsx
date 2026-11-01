import { useMutation } from "@tanstack/react-query";
import { AlertTriangle, ArrowUpCircle, Check, Download, Loader2, Pause, Play, X } from "lucide-react";
import { api, errorText } from "@/lib/api";
import type { FontSummary } from "@/lib/types";
import { useUi } from "@/store/ui";
import { Button } from "./ui/button";
import { Tooltip } from "./ui/controls";

export function useAction<T>(fn: () => Promise<T>, onDone?: (value: T) => void) {
  const toast = useUi((s) => s.toast);
  return useMutation({
    mutationFn: fn,
    onSuccess: onDone,
    onError: (e) => toast({ kind: "error", title: "İşlem başarısız", detail: errorText(e) }),
  });
}

/** Fontun gerçek durumuna göre tek birincil eylem. */
export function FontActions({ font, large }: { font: FontSummary; large?: boolean }) {
  const progress = useUi((s) => s.progress[font.id]);
  const installing = useUi((s) => s.installing.has(font.id));
  const size = large ? "md" : "sm";
  const install = useAction(() => api.installFont(font.id));
  const download = useAction(() => api.downloadFont(font.id));
  const update = useAction(() => api.downloadFont(font.id, font.localStatus === "installed"));
  const pause = useAction(() => api.pauseDownload(font.downloadId!));
  const resume = useAction(() => api.resumeDownload(font.downloadId!));
  const cancel = useAction(() => api.cancelDownload(font.downloadId!));
  const stop = (e: React.MouseEvent) => e.stopPropagation();

  const status = progress?.status ?? font.downloadStatus;

  if (installing) {
    return <Button size={size} variant="secondary" disabled onClick={stop}><Loader2 className="animate-spin" />Kuruluyor</Button>;
  }
  if (status === "downloading" || status === "queued") {
    const total = progress?.filesTotal ?? 0;
    const percent = total ? Math.round((((progress?.filesDone ?? 0) + (progress?.fileFraction ?? 0)) / total) * 100) : 0;
    return (
      <div className="flex items-center gap-1" onClick={stop}>
        <div className="relative flex h-7 w-[84px] items-center justify-center overflow-hidden rounded-md border border-line bg-raised text-[11px] tabular-nums text-soft">
          <div className="absolute inset-y-0 left-0 bg-hover transition-[width]" style={{ width: `${percent}%` }} />
          <span className="relative">{status === "queued" ? "Sırada" : `%${percent}`}</span>
        </div>
        {font.downloadId !== null && (
          <>
            <Tooltip text="Duraklat">
              <Button aria-label="Duraklat" size="icon" variant="ghost" onClick={() => pause.mutate()}><Pause /></Button>
            </Tooltip>
            <Tooltip text="İptal et">
              <Button aria-label="İptal et" size="icon" variant="ghost" onClick={() => cancel.mutate()}><X /></Button>
            </Tooltip>
          </>
        )}
      </div>
    );
  }
  if (status === "paused") {
    return (
      <div className="flex items-center gap-1" onClick={stop}>
        <Button size={size} onClick={() => resume.mutate()}><Play />Devam et</Button>
        <Tooltip text="İptal et">
          <Button aria-label="İptal et" size="icon" variant="ghost" onClick={() => cancel.mutate()}><X /></Button>
        </Tooltip>
      </div>
    );
  }
  if (font.localStatus === "install_failed") {
    return (
      <Tooltip text={font.localError ?? "Kurulum başarısız"}>
        <Button size={size} variant="danger" onClick={(e) => { stop(e); install.mutate(); }}><AlertTriangle />Yeniden dene</Button>
      </Tooltip>
    );
  }
  if (font.updateAvailable) {
    return (
      <div className="flex items-center gap-1.5" onClick={stop}>
        <Button size={size} onClick={() => update.mutate()}><ArrowUpCircle />Güncelle</Button>
      </div>
    );
  }
  if (font.localStatus === "installed") {
    return <span className="inline-flex h-7 items-center gap-1 px-1 text-xs font-medium text-good"><Check className="size-3.5" />Kurulu</span>;
  }
  return (
    <div className="flex items-center gap-1" onClick={stop}>
      {font.localStatus === "available" && (
        <Tooltip text="Kurmadan indir">
          <Button aria-label="İndir" size={large ? "md" : "icon"} variant="ghost" onClick={() => download.mutate()}>
            <Download />{large && "Yalnızca indir"}
          </Button>
        </Tooltip>
      )}
      <Button size={size} variant="primary" onClick={() => install.mutate()} disabled={install.isPending}>Kur</Button>
    </div>
  );
}
