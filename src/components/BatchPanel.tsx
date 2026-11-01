import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { CheckCircle2, ChevronDown, Loader2, RotateCcw, TriangleAlert, X } from "lucide-react";
import { api } from "@/lib/api";
import type { Batch } from "@/lib/types";
import { cn, formatNumber } from "@/lib/utils";
import { useUi } from "@/store/ui";
import { Button } from "./ui/button";

function BatchCard({ batch }: { batch: Batch }) {
  const [open, setOpen] = useState(false);
  const finished = batch.done + batch.failed + batch.skipped;
  const complete = finished >= batch.total;
  const percent = Math.round((finished / Math.max(batch.total, 1)) * 100);
  const verb = batch.installAfter ? "kuruldu" : "indirildi";
  return (
    <div className="anim-rise rounded-lg border border-line-strong bg-raised p-3 shadow-[var(--shadow)]">
      <div className="flex items-center gap-2">
        {!complete ? <Loader2 className="size-3.5 animate-spin text-soft" />
          : batch.failed ? <TriangleAlert className="size-3.5 text-warn" />
          : <CheckCircle2 className="size-3.5 text-good" />}
        <span className="min-w-0 flex-1 truncate text-[13px] font-medium">{batch.label}</span>
        <span className="text-[11px] tabular-nums text-faint">{formatNumber(finished)}/{formatNumber(batch.total)}</span>
        {complete && (
          <button type="button" aria-label="Kapat" className="text-faint hover:text-ink" onClick={() => api.dismissBatch(batch.id)}>
            <X className="size-3.5" />
          </button>
        )}
      </div>
      <div className="mt-2 h-[3px] overflow-hidden rounded-full bg-line-strong">
        <div className={cn("h-full transition-[width]", batch.failed ? "bg-warn" : "bg-soft")} style={{ width: `${percent}%` }} />
      </div>
      <div className="mt-2 text-xs text-soft">
        {formatNumber(batch.done)} {verb}
        {batch.skipped > 0 && ` · ${formatNumber(batch.skipped)} atlandı`}
        {batch.failed > 0 && <span className="text-warn"> · {formatNumber(batch.failed)} başarısız</span>}
      </div>
      {batch.failed > 0 && (
        <div className="mt-2">
          <div className="flex gap-1">
            <Button size="sm" variant="ghost" onClick={() => setOpen(!open)}>
              <ChevronDown className={cn("transition-transform", open && "rotate-180")} />Ayrıntılar
            </Button>
            {complete && <Button size="sm" variant="ghost" onClick={() => api.retryBatch(batch.id)}><RotateCcw />Yeniden dene</Button>}
          </div>
          {open && (
            <ul className="mt-1 max-h-40 space-y-1 overflow-auto text-xs select-text">
              {batch.failures.map((f) => (
                <li key={f.fontId}><span className="font-medium">{f.family}</span><span className="text-soft"> — {f.message}</span></li>
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  );
}

export function BatchPanel() {
  const { data } = useQuery({ queryKey: ["batches"], queryFn: api.listBatches });
  const selecting = useUi((s) => s.selection.size > 0);
  if (!data?.length) return null;
  // Seçim çubuğu görünürken panel onun üstüne çıkar; dar pencerede ikisi çakışmaz.
  return <div className={cn("fixed right-4 z-30 flex w-[320px] flex-col gap-2 transition-[bottom]", selecting ? "bottom-[68px]" : "bottom-4")}>{data.slice(0, 3).map((b) => <BatchCard key={b.id} batch={b} />)}</div>;
}
