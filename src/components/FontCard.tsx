import { memo, useEffect, useState } from "react";
import { Heart, TriangleAlert } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import { loadCardFace } from "@/lib/fontLoader";
import type { FontSummary } from "@/lib/types";
import { TURKISH_LABEL, categoryLabel, cn } from "@/lib/utils";
import { useUi } from "@/store/ui";
import { FontActions } from "./FontActions";
import { Checkbox, Tooltip } from "./ui/controls";

export function useCardFace(font: Parameters<typeof loadCardFace>[0]) {
  const [state, setState] = useState<{ family: string | null; failed: boolean }>({ family: null, failed: false });
  useEffect(() => {
    let live = true;
    setState({ family: null, failed: false });
    // Hızlı kaydırmada ekrandan çıkan kartlar ağ isteği başlatmaz.
    const timer = setTimeout(async () => {
      const attempt = async (source: typeof font) => {
        const face = loadCardFace(source);
        if (!(await face.ready)) return null;
        const css = `${face.italic ? "italic " : ""}${face.weight} 32px "${face.family}"`;
        const load = () => document.fonts.load(css, "TYPEVAULT Aa").catch(() => []);
        let loaded = await load();
        // WebKit stylesheet yüklenir yüklenmez yüzü görmeyebilir; bir kez daha sorulur.
        if (loaded.length === 0) {
          await new Promise((resolve) => setTimeout(resolve, 180));
          loaded = await load();
        }
        return loaded.length > 0 || source.previewFileId === null ? face.family : null;
      };
      // Yerel dosya okunamazsa genel bir yazı tipi göstermek yerine uzak önizlemeye dönülür.
      let family = await attempt(font);
      if (!family && font.previewFileId !== null) family = await attempt({ ...font, previewFileId: null });
      if (live) setState({ family, failed: family === null });
    }, 70);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [font.id, font.previewFileId]);
  return state;
}

export const FontCard = memo(function FontCard({ font, text, size, selected, selecting }: {
  font: FontSummary; text: string; size: number; selected: boolean; selecting: boolean;
}) {
  const face = useCardFace(font);
  const openDetail = useUi((s) => s.openDetail);
  const toggleSelect = useUi((s) => s.toggleSelect);
  const client = useQueryClient();

  return (
    <article
      onClick={() => (selecting ? toggleSelect(font.id) : openDetail(font.id))}
      className={cn(
        "group flex h-full min-w-0 flex-col rounded-[10px] border bg-panel px-3.5 py-3 transition-colors",
        selected ? "border-soft" : "border-line hover:border-line-strong",
      )}
    >
      <header className="flex h-5 items-center gap-2">
        <Checkbox
          checked={selected}
          onChange={() => toggleSelect(font.id)}
          label={`${font.family} seç`}
          className={cn(!selecting && !selected && "opacity-0 group-hover:opacity-100")}
        />
        <h3 className="min-w-0 truncate text-[12.5px] font-medium">{font.family}</h3>
        <span className="shrink-0 text-[11px] text-faint">{categoryLabel(font.category)}</span>
        <button
          type="button"
          aria-label={font.favorite ? "Favorilerden çıkar" : "Favorilere ekle"}
          onClick={async (e) => {
            e.stopPropagation();
            await api.toggleFavorite(font.id);
            client.invalidateQueries({ queryKey: ["fonts"] });
            client.invalidateQueries({ queryKey: ["stats"] });
          }}
          className={cn("ml-auto text-faint hover:text-ink", !font.favorite && "opacity-0 group-hover:opacity-100")}
        >
          <Heart className={cn("size-3.5", font.favorite && "fill-current text-ink")} />
        </button>
      </header>

      <div className="flex min-h-0 flex-1 items-center">
        {face.family ? (
          <div
            className="specimen w-full"
            style={{
              fontFamily: `"${face.family}", -apple-system, sans-serif`,
              fontSize: size,
              fontWeight: font.previewWeight,
              fontStyle: font.previewItalic ? "italic" : "normal",
            }}
          >
            {text}
          </div>
        ) : face.failed ? (
          <span className="text-xs text-faint">Önizleme için internet gerekli</span>
        ) : (
          <div className="skeleton h-[40%] w-3/4" />
        )}
      </div>

      <footer className="flex h-7 items-center gap-2 text-[11px] text-faint">
        <span className="tabular-nums">{font.isVariable ? "Değişken" : `${font.variantCount} stil`}</span>
        <TurkishMark font={font} />
        <div className="ml-auto"><FontActions font={font} /></div>
      </footer>
    </article>
  );
});

export function TurkishMark({ font }: { font: Pick<FontSummary, "turkishStatus" | "turkishMissing"> }) {
  const status = font.turkishStatus;
  if (status === "unknown") return null;
  if (status === "full") return <span className="text-good">Türkçe</span>;
  const tip = status === "partial" ? `Eksik glifler: ${font.turkishMissing}` : TURKISH_LABEL.none;
  return (
    <Tooltip text={tip}>
      <span className="inline-flex items-center gap-1 text-warn"><TriangleAlert className="size-3" />{status === "partial" ? "Kısmi" : "Türkçe yok"}</span>
    </Tooltip>
  );
}
