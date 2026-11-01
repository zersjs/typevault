import { useQuery } from "@tanstack/react-query";
import { CheckCircle2, CircleAlert, Clock, Download, Loader2, Pause, Play, RotateCcw, Trash2, X } from "lucide-react";
import { useAction } from "@/components/FontActions";
import { EmptyState } from "@/components/FontGrid";
import { Button } from "@/components/ui/button";
import { Tooltip } from "@/components/ui/controls";
import { api } from "@/lib/api";
import type { DownloadRow, Progress } from "@/lib/types";
import { cn, formatBytes, formatNumber } from "@/lib/utils";
import { useUi } from "@/store/ui";
import { PageTitle } from "./Browse";

const STATUS: Record<string, string> = {
  queued: "Sırada", downloading: "İndiriliyor", paused: "Duraklatıldı", completed: "Tamamlandı", failed: "Başarısız", cancelled: "İptal edildi",
};

function fraction(row: DownloadRow, live?: Progress) {
  const done = live?.filesDone ?? row.filesDone;
  const total = live?.filesTotal ?? row.filesTotal;
  if (row.status === "completed") return 1;
  return total ? (done + (live?.fileFraction ?? 0)) / total : 0;
}

function Row({ row }: { row: DownloadRow }) {
  const live = useUi((s) => s.progress[row.fontId]);
  const status = live?.status ?? row.status;
  const pause = useAction(() => api.pauseDownload(row.id));
  const resume = useAction(() => api.resumeDownload(row.id));
  const cancel = useAction(() => api.cancelDownload(row.id));
  const install = useAction(() => api.installFont(row.fontId));
  const openDetail = useUi((s) => s.openDetail);
  const percent = Math.round(fraction(row, live) * 100);
  const bytes = live?.receivedBytes ?? row.receivedBytes;
  const filesDone = live?.filesDone ?? row.filesDone;
  const filesTotal = live?.filesTotal ?? row.filesTotal;
  const Icon = status === "completed" ? CheckCircle2 : status === "failed" ? CircleAlert : status === "downloading" ? Loader2 : status === "paused" ? Pause : Clock;
  return (
    <li className="flex items-center gap-3 border-b border-line px-3 py-2.5 last:border-0">
      <Icon className={cn("size-3.5 shrink-0", status === "completed" ? "text-good" : status === "failed" ? "text-bad" : "text-faint", status === "downloading" && "animate-spin")} />
      <div className="min-w-0 flex-1">
        <button type="button" className="block max-w-full truncate text-[13px] font-medium" onClick={() => openDetail(row.fontId)}>{row.family}</button>
        <div className="mt-1.5 h-[3px] overflow-hidden rounded-full bg-line-strong">
          <div className={cn("h-full transition-[width]", status === "failed" ? "bg-bad" : "bg-soft")} style={{ width: `${percent}%` }} />
        </div>
        <div className="mt-1 truncate text-[11px] tabular-nums text-faint">
          {row.error ?? [
            STATUS[status],
            filesTotal > 0 && `${filesDone}/${filesTotal} dosya`,
            bytes > 0 && formatBytes(bytes),
            status === "downloading" && live && live.speedBps > 0 && `${formatBytes(live.speedBps)}/sn`,
          ].filter(Boolean).join(" · ")}
        </div>
      </div>
      <div className="flex shrink-0 items-center gap-1">
        {(status === "downloading" || status === "queued") && (
          <Tooltip text="Duraklat"><Button aria-label="Duraklat" size="icon" variant="ghost" onClick={() => pause.mutate()}><Pause /></Button></Tooltip>
        )}
        {status === "paused" && (
          <Tooltip text="Devam et"><Button aria-label="Devam et" size="icon" variant="ghost" onClick={() => resume.mutate()}><Play /></Button></Tooltip>
        )}
        {status === "failed" && (
          <Button size="sm" onClick={() => resume.mutate()}><RotateCcw />Yeniden dene</Button>
        )}
        {(status === "downloading" || status === "queued" || status === "paused") && (
          <Tooltip text="İptal et"><Button aria-label="İptal et" size="icon" variant="ghost" onClick={() => cancel.mutate()}><X /></Button></Tooltip>
        )}
        {status === "completed" && row.localStatus === "downloaded" && (
          <Button size="sm" variant="primary" onClick={() => install.mutate()}>Kur</Button>
        )}
        {status === "completed" && row.localStatus === "installed" && (
          <span className="px-1 text-xs text-good">Kurulu</span>
        )}
      </div>
    </li>
  );
}

export function Downloads() {
  const { data } = useQuery({ queryKey: ["downloads"], queryFn: api.listDownloads });
  const progress = useUi((s) => s.progress);
  const toast = useUi((s) => s.toast);
  const rows = data ?? [];
  const active = rows.filter((r) => r.status === "downloading" || r.status === "queued");
  const paused = rows.filter((r) => r.status === "paused");
  const installable = rows.filter((r) => r.status === "completed" && r.localStatus === "downloaded");
  const finished = rows.filter((r) => ["completed", "failed", "cancelled"].includes(r.status));
  const open = rows.filter((r) => r.status !== "completed" && r.status !== "cancelled");
  const overall = open.length ? open.reduce((sum, r) => sum + fraction(r, progress[r.fontId]), 0) / open.length : 0;
  const speed = Object.values(progress).reduce((sum, p) => sum + (p.speedBps || 0), 0);

  const pauseAll = useAction(api.pauseAll);
  const resumeAll = useAction(api.resumeAll);
  const clear = useAction(api.clearFinished);
  const installAll = useAction(api.installAllDownloaded, (n) => toast({ kind: "info", title: `${formatNumber(n)} font kuruluyor` }));

  return (
    <div className="min-h-0 flex-1 overflow-y-auto pb-20">
      <PageTitle title="İndirilenler">
        <div className="ml-auto flex items-center gap-1.5">
          {active.length > 0 && <Button size="sm" onClick={() => pauseAll.mutate()}><Pause />Tümünü duraklat</Button>}
          {paused.length > 0 && <Button size="sm" onClick={() => resumeAll.mutate()}><Play />Tümünü sürdür</Button>}
          {finished.length > 0 && <Button size="sm" variant="ghost" onClick={() => clear.mutate()}><Trash2 />Bitenleri temizle</Button>}
          {installable.length > 0 && <Button size="sm" variant="primary" onClick={() => installAll.mutate()}>Tümünü Kur</Button>}
        </div>
      </PageTitle>
      {open.length > 0 && (
        <div className="px-4 pt-4">
          <div className="flex justify-between text-xs tabular-nums text-soft">
            <span>{formatNumber(active.length)} etkin · {formatNumber(paused.length)} duraklatıldı{speed > 0 && ` · ${formatBytes(speed)}/sn`}</span>
            <span>%{Math.round(overall * 100)}</span>
          </div>
          <div className="mt-1.5 h-1 overflow-hidden rounded-full bg-line-strong">
            <div className="h-full bg-accent transition-[width]" style={{ width: `${overall * 100}%` }} />
          </div>
        </div>
      )}
      <div className="px-4 pt-4">
        {rows.length === 0 ? (
          <EmptyState icon={Download} title="İndirme kuyruğu boş" detail="Bir fontu kurduğunuzda veya indirdiğinizde ilerlemesi burada görünür." />
        ) : (
          <ul className="rounded-[10px] border border-line">{rows.map((r) => <Row key={r.id} row={r} />)}</ul>
        )}
      </div>
    </div>
  );
}
