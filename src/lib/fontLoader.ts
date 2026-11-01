import type { FontAxis, FontSummary, FontVariant, LocalFile } from "./types";

// Yüklenen yüz sayısı sınırlanır; en eski kayıtlar DOM'dan çıkarılır.
const MAX_ENTRIES = 500;
const entries = new Map<string, { el: HTMLElement; ready: Promise<boolean> }>();

const base = "tvfont://localhost";

function touch(key: string, el: HTMLElement, ready: Promise<boolean>) {
  entries.set(key, { el, ready });
  if (entries.size > MAX_ENTRIES) {
    const oldest = entries.keys().next().value!;
    entries.get(oldest)?.el.remove();
    entries.delete(oldest);
  }
}

function addStylesheet(key: string, href: string): Promise<boolean> {
  const link = document.createElement("link");
  link.rel = "stylesheet";
  link.href = href;
  const ready = new Promise<boolean>((resolve) => {
    link.onload = () => resolve(true);
    link.onerror = () => resolve(false);
  });
  document.head.appendChild(link);
  touch(key, link, ready);
  return ready;
}

function addFaces(key: string, css: string): Promise<boolean> {
  const style = document.createElement("style");
  style.textContent = css;
  document.head.appendChild(style);
  const ready = Promise.resolve(true);
  touch(key, style, ready);
  return ready;
}

const slug = (id: string) => id.slice(3);

export interface CardFace { family: string; weight: number; italic: boolean; ready: Promise<boolean> }

/** Kart için tek yüz: indirilmiş dosya varsa o, yoksa diskte önbelleğe alınan alt küme. */
export function loadCardFace(font: Pick<FontSummary, "id" | "previewWeight" | "previewItalic" | "previewFileId">): CardFace {
  const local = font.previewFileId !== null;
  const family = `tv-${local ? "f" + font.previewFileId : slug(font.id)}`;
  const weight = font.previewWeight;
  const italic = font.previewItalic;
  const key = `card:${family}`;
  const existing = entries.get(key);
  if (existing) return { family, weight, italic, ready: existing.ready };
  let ready: Promise<boolean>;
  if (local) {
    ready = addFaces(key, `@font-face{font-family:'${family}';src:url('${base}/local/${font.previewFileId}');font-weight:100 1000;font-style:${italic ? "italic" : "normal"};font-display:swap}`);
  } else {
    const spec = italic ? `ital,wght@1,${weight}` : `wght@${weight}`;
    ready = addStylesheet(key, `${base}/css?f=${encodeURIComponent(font.id)}&as=${family}&spec=${encodeURIComponent(spec)}`);
  }
  return { family, weight, italic, ready };
}

function variantSpec(variants: FontVariant[]): string {
  const hasItalic = variants.some((v) => v.style === "italic");
  if (!hasItalic) {
    const weights = [...new Set(variants.map((v) => v.weight))].sort((a, b) => a - b);
    return `wght@${weights.join(";")}`;
  }
  const tuples = variants
    .map((v) => [v.style === "italic" ? 1 : 0, v.weight])
    .sort((a, b) => a[0] - b[0] || a[1] - b[1])
    .map(([i, w]) => `${i},${w}`);
  return `ital,wght@${tuples.join(";")}`;
}

function axisSpec(axes: FontAxis[], hasItalic: boolean): string {
  // Google, eksenleri küçük harfler önce olacak şekilde alfabetik ister.
  const lower = axes.filter((a) => a.tag.toLowerCase() === a.tag).sort((a, b) => a.tag.localeCompare(b.tag));
  const upper = axes.filter((a) => a.tag.toLowerCase() !== a.tag).sort((a, b) => a.tag.localeCompare(b.tag));
  const ordered = [...lower, ...upper];
  const ranges = ordered.map((a) => `${a.min}..${a.max}`).join(",");
  const tags = ordered.map((a) => a.tag).join(",");
  if (hasItalic) return `ital,${tags}@0,${ranges};1,${ranges}`;
  return `${tags}@${ranges}`;
}

export interface DetailFace { family: string; ready: Promise<boolean>; variable: boolean }

/** Ayrıntı görünümü: tüm ağırlıklar ve varsa değişken eksenler. Önce yerel dosyalar kullanılır. */
export function loadDetailFace(id: string, variants: FontVariant[], axes: FontAxis[], files: LocalFile[]): DetailFace {
  const localVariable = files.filter((f) => f.isVariable);
  const useLocal = files.length > 0 && (axes.length === 0 || localVariable.length > 0);
  const family = `tvd-${slug(id)}${useLocal ? "-l" : ""}`;
  const key = `detail:${family}`;
  const variable = axes.length > 0 && (!useLocal || localVariable.length > 0);
  const existing = entries.get(key);
  if (existing) return { family, ready: existing.ready, variable };
  if (useLocal) {
    const chosen = localVariable.length ? localVariable : files;
    const css = chosen
      .map((f) => {
        const weight = f.isVariable ? "1 1000" : String(f.weight ?? 400);
        return `@font-face{font-family:'${family}';src:url('${base}/local/${f.id}');font-weight:${weight};font-style:${f.italic ? "italic" : "normal"};font-display:swap}`;
      })
      .join("");
    return { family, ready: addFaces(key, css), variable };
  }
  const spec = axes.length ? axisSpec(axes, variants.some((v) => v.style === "italic")) : variantSpec(variants);
  const ready = addStylesheet(key, `${base}/css?f=${encodeURIComponent(id)}&as=${family}&spec=${encodeURIComponent(spec)}`);
  return { family, ready, variable };
}
