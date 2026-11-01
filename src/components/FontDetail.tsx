import { useEffect, useMemo, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Heart, Trash2, X } from "lucide-react";
import { api } from "@/lib/api";
import { loadDetailFace } from "@/lib/fontLoader";
import type { FontDetail as Detail } from "@/lib/types";
import { TURKISH_LABEL, WEIGHT_NAMES, categoryLabel, cn, formatBytes } from "@/lib/utils";
import { useUi } from "@/store/ui";
import { FontActions, useAction } from "./FontActions";
import { Button } from "./ui/button";
import { Select, Slider } from "./ui/controls";

const TABS = [
  { id: "specimen", label: "Örnek" },
  { id: "alphabet", label: "Alfabe" },
  { id: "numbers", label: "Rakam ve Noktalama" },
  { id: "turkish", label: "Türkçe" },
  { id: "glyphs", label: "Karakter" },
] as const;

const ALPHABET_UPPER = "ABCÇDEFGĞHIİJKLMNOÖPRSŞTUÜVYZ";
const ALPHABET_LOWER = "abcçdefgğhıijklmnoöprsştuüvyz";
const NUMBERS = "0123456789";
const PUNCTUATION = ".,:;!?¿¡…–—'\"«»()[]{}/\\@&#%₺$€£*+=";
const TURKISH_PANGRAM = "Pijamalı hasta yağız şoföre çabucak güvendi.";
const TURKISH_CHARS = ["ç", "Ç", "ğ", "Ğ", "ı", "İ", "i", "I", "ö", "Ö", "ş", "Ş", "ü", "Ü"];

function GlyphGrid({ chars, css, missing }: { chars: string[]; css: React.CSSProperties; missing?: string }) {
  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(56px,1fr))] gap-px overflow-hidden rounded-lg border border-line bg-line">
      {chars.map((c) => (
        <div key={c} className={cn("flex aspect-square items-center justify-center bg-panel text-[26px]", missing?.includes(c) && "text-bad")} style={css}>{c}</div>
      ))}
    </div>
  );
}

function Fact({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex gap-3 border-b border-line py-2 last:border-0">
      <dt className="w-28 shrink-0 text-xs text-faint">{label}</dt>
      <dd className="min-w-0 flex-1 select-text break-words text-xs text-ink">{children}</dd>
    </div>
  );
}

function Body({ detail }: { detail: Detail }) {
  const font = detail.summary;
  const client = useQueryClient();
  const toast = useUi((s) => s.toast);
  const previewText = useUi((s) => s.previewText);
  const [text, setText] = useState(previewText || "Hamburgefonstiv");
  const [size, setSize] = useState(72);
  const [tab, setTab] = useState<(typeof TABS)[number]["id"]>("specimen");
  const [ready, setReady] = useState<string | null>(null);
  const [isVariable, setIsVariable] = useState(false);
  const [axes, setAxes] = useState<Record<string, number>>({});

  const weights = useMemo(() => [...new Set(detail.variants.filter((v) => v.style === "normal").map((v) => v.weight))], [detail.variants]);
  const [weight, setWeight] = useState(font.previewWeight);
  const hasItalic = detail.variants.some((v) => v.style === "italic");
  const [italic, setItalic] = useState(font.previewItalic);
  const fileKey = detail.files.map((f) => f.id).join();

  useEffect(() => {
    let live = true;
    const face = loadDetailFace(font.id, detail.variants, detail.axes, detail.files);
    setIsVariable(face.variable);
    face.ready.then(async () => {
      try {
        await document.fonts.load(`${weight} 72px "${face.family}"`, TURKISH_PANGRAM);
      } catch { /* yedek yığın */ }
      if (live) setReady(face.family);
    });
    return () => { live = false; };
  }, [font.id, fileKey]);

  const variation = Object.entries(axes).filter(([tag]) => tag !== "wght").map(([tag, v]) => `"${tag}" ${v}`).join(", ");
  const css: React.CSSProperties = {
    fontFamily: ready ? `"${ready}", -apple-system, sans-serif` : undefined,
    fontWeight: axes.wght ?? weight,
    fontStyle: italic ? "italic" : "normal",
    fontVariationSettings: isVariable && variation ? variation : undefined,
    fontSynthesis: "none",
  };

  const uninstall = useAction(() => api.uninstallFont(font.id), () => {
    toast({ kind: "success", title: `${font.family} kaldırıldı` });
    client.invalidateQueries({ queryKey: ["font", font.id] });
  });
  const remove = useAction(() => api.deleteDownloaded(font.id), () => {
    toast({ kind: "info", title: "İndirilen dosyalar silindi" });
    client.invalidateQueries();
  });
  const favorite = useAction(() => api.toggleFavorite(font.id), () => client.invalidateQueries());

  const owned = detail.files.some((f) => f.owner === "typevault" && f.installed === "installed");
  const external = detail.files.some((f) => f.owner === "external");

  return (
    <>
      <header data-tauri-drag-region className="flex items-center gap-3 border-b border-line px-5 py-3.5">
        <div className="min-w-0 flex-1">
          <Dialog.Title className="truncate text-[17px] font-semibold">{font.family}</Dialog.Title>
          <p className="mt-0.5 truncate text-xs text-soft">
            {[categoryLabel(font.category), detail.designers.slice(0, 3).join(", ")].filter(Boolean).join(" · ")}
          </p>
        </div>
        <Button aria-label="Favori" size="icon" variant="ghost" onClick={() => favorite.mutate()}>
          <Heart className={cn(font.favorite && "fill-current text-ink")} />
        </Button>
        {owned && <Button size="sm" variant="danger" onClick={() => uninstall.mutate()} disabled={uninstall.isPending}>Kaldır</Button>}
        <FontActions font={font} large />
        <Dialog.Close asChild><Button aria-label="Kapat" size="icon" variant="ghost"><X /></Button></Dialog.Close>
      </header>

      <div className="flex-1 overflow-y-auto">
        <div className="flex flex-wrap items-center gap-3 border-b border-line px-5 py-2.5">
          {weights.length > 1 && !axes.wght && (
            <Select
              value={String(weight)}
              onChange={(v) => setWeight(Number(v))}
              options={weights.map((w) => ({ value: String(w), label: `${w} ${WEIGHT_NAMES[w] ?? ""}` }))}
            />
          )}
          {hasItalic && (
            <Button size="sm" variant={italic ? "primary" : "secondary"} onClick={() => setItalic(!italic)}><span className="italic">İtalik</span></Button>
          )}
          <div className="ml-auto flex w-44 items-center gap-2">
            <span className="w-7 text-right text-[11px] tabular-nums text-faint">{size}</span>
            <Slider label="Boyut" value={size} onChange={setSize} min={14} max={200} />
          </div>
        </div>

        {isVariable && detail.axes.length > 0 && (
          <div className="grid grid-cols-2 gap-x-6 gap-y-2 border-b border-line px-5 py-3">
            {detail.axes.map((axis) => (
              <label key={axis.tag} className="flex items-center gap-2.5 text-xs text-soft">
                <span className="w-24 truncate">{axis.name}</span>
                <Slider
                  label={axis.name}
                  value={axes[axis.tag] ?? axis.defaultValue}
                  onChange={(v) => setAxes({ ...axes, [axis.tag]: v })}
                  min={axis.min}
                  max={axis.max}
                  step={axis.max - axis.min <= 2 ? 0.01 : 1}
                />
                <span className="w-9 text-right text-[11px] tabular-nums text-faint">{Math.round((axes[axis.tag] ?? axis.defaultValue) * 100) / 100}</span>
              </label>
            ))}
          </div>
        )}

        <div className="flex gap-4 border-b border-line px-5">
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              onClick={() => setTab(t.id)}
              className={cn("-mb-px border-b py-2 text-xs transition-colors", tab === t.id ? "border-ink text-ink" : "border-transparent text-soft hover:text-ink")}
            >
              {t.label}
            </button>
          ))}
        </div>

        <div className="px-5 py-5">
          {tab === "specimen" && (
            <div>
              <textarea
                value={text}
                onChange={(e) => setText(e.target.value)}
                rows={2}
                aria-label="Özel önizleme metni"
                className="w-full resize-none bg-transparent leading-[1.15] text-ink"
                style={{ ...css, fontSize: size }}
              />
              {[32, 18].map((s) => (
                <p key={s} className="mt-4 text-ink" style={{ ...css, fontSize: s, lineHeight: 1.35 }}>{TURKISH_PANGRAM}</p>
              ))}
            </div>
          )}
          {tab === "alphabet" && (
            <div className="space-y-3">
              <GlyphGrid chars={[...ALPHABET_UPPER]} css={css} />
              <GlyphGrid chars={[...ALPHABET_LOWER]} css={css} />
            </div>
          )}
          {tab === "numbers" && (
            <div className="space-y-3">
              <GlyphGrid chars={[...NUMBERS]} css={css} />
              <GlyphGrid chars={[...new Set(PUNCTUATION)]} css={css} />
            </div>
          )}
          {tab === "turkish" && (
            <div>
              <div className={cn("text-[13px] font-medium", font.turkishStatus === "full" ? "text-good" : font.turkishStatus === "unknown" ? "text-soft" : "text-warn")}>
                {TURKISH_LABEL[font.turkishStatus]}
              </div>
              <p className="mb-3 mt-1 text-xs text-soft">
                {font.turkishMissing && `Eksik glifler: ${font.turkishMissing}. `}
                {detail.turkishSource === "cmap" ? "Sonuç indirilen font dosyasından doğrulandı." :
                  detail.turkishSource === "coverage" ? "Sonuç Google kapsam verisinden alındı; indirince dosyadan yeniden doğrulanır." :
                  "Kapsam verisi henüz alınmadı."}
              </p>
              <GlyphGrid chars={TURKISH_CHARS} css={css} missing={font.turkishMissing} />
              <p className="mt-5 text-[28px] leading-snug" style={css}>{TURKISH_PANGRAM}</p>
            </div>
          )}
          {tab === "glyphs" && (
            <input
              maxLength={2}
              defaultValue="Ğ"
              aria-label="Tek karakter önizleme"
              onFocus={(e) => e.currentTarget.select()}
              onChange={(e) => e.currentTarget.select()}
              className="h-64 w-full rounded-lg border border-line bg-panel text-center text-[160px] text-ink"
              style={css}
            />
          )}
        </div>

        <dl className="px-5 pb-6">
          <Fact label="Lisans">
            {font.license ?? "Henüz doğrulanmadı"}
          </Fact>
          <Fact label="Kaynak">Google Fonts{detail.lastModified && ` · Güncelleme ${detail.lastModified}`}</Fact>
          {detail.subsets.length > 0 && <Fact label="Alt kümeler">{detail.subsets.join(", ")}</Fact>}
          {detail.languages.length > 0 && <Fact label="Dil sayısı">{detail.languages.length}</Fact>}
          {detail.description && (
            <Fact label="Açıklama">{detail.description.replace(/<[^>]+>/g, " ").trim()}</Fact>
          )}
          {detail.files.length > 0 && (
            <Fact label="Dosyalar">
              <ul className="space-y-1">
                {detail.files.map((f) => (
                  <li key={f.id} className="flex justify-between gap-3">
                    <span className="truncate">{f.filename}</span>
                    <span className="shrink-0 tabular-nums text-faint">
                      {formatBytes(f.sizeBytes)}{f.installed === "installed" && (f.owner === "external" ? " · sistemde" : " · kurulu")}
                    </span>
                  </li>
                ))}
              </ul>
            </Fact>
          )}
          {external && <Fact label="Not">Bazı stiller başka bir kaynaktan kurulmuş. TYPEVAULT bunları değiştirmez ve kaldırmaz.</Fact>}
        </dl>
        {detail.files.length > 0 && font.localStatus !== "installed" && (
          <div className="px-5 pb-6">
            <Button size="sm" variant="ghost" onClick={() => remove.mutate()}><Trash2 />İndirilen dosyaları sil</Button>
          </div>
        )}
      </div>
    </>
  );
}

export function FontDetail() {
  const id = useUi((s) => s.detailId);
  const close = useUi((s) => s.openDetail);
  const { data } = useQuery({
    queryKey: ["font", id],
    queryFn: () => api.getFont(id!),
    enabled: id !== null,
  });
  return (
    <Dialog.Root open={id !== null} onOpenChange={(o) => !o && close(null)}>
      <Dialog.Portal>
        <Dialog.Overlay className="anim-fade fixed inset-0 z-30 bg-black/30" />
        <Dialog.Content aria-describedby={undefined} className="anim-slide fixed inset-y-0 right-0 z-30 flex w-[min(760px,calc(100vw-120px))] flex-col border-l border-line-strong bg-bg shadow-[var(--shadow)]">
          {data && data.summary.id === id ? <Body key={id} detail={data} /> : (
            <Dialog.Title className="p-5"><span className="skeleton block h-5 w-40" /></Dialog.Title>
          )}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
