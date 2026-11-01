import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export const cn = (...inputs: ClassValue[]) => twMerge(clsx(inputs));

const number = new Intl.NumberFormat("tr-TR");
export const formatNumber = (n: number) => number.format(n);

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let value = bytes;
  let unit = -1;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toLocaleString("tr-TR", { maximumFractionDigits: value < 10 ? 1 : 0 })} ${units[unit]}`;
}

export const CATEGORIES: { id: string; label: string }[] = [
  { id: "sans-serif", label: "Sans Serif" },
  { id: "serif", label: "Serif" },
  { id: "display", label: "Display" },
  { id: "condensed", label: "Dar" },
  { id: "geometric", label: "Geometrik" },
  { id: "handwriting", label: "El Yazısı" },
  { id: "monospace", label: "Eş Aralıklı" },
  { id: "variable", label: "Değişken" },
  { id: "decorative", label: "Dekoratif" },
  { id: "futuristic", label: "Fütüristik" },
  { id: "experimental", label: "Deneysel" },
];

const CATEGORY_LABELS: Record<string, string> = {
  "Sans Serif": "Sans Serif",
  Serif: "Serif",
  Display: "Display",
  Handwriting: "El Yazısı",
  Monospace: "Eş Aralıklı",
};
export const categoryLabel = (c: string) => CATEGORY_LABELS[c] ?? c;

export const WEIGHT_NAMES: Record<number, string> = {
  100: "İnce", 200: "Çok Hafif", 300: "Hafif", 400: "Normal", 500: "Orta",
  600: "Yarı Kalın", 700: "Kalın", 800: "Çok Kalın", 900: "Siyah", 1000: "Ultra",
};

export const TURKISH_LABEL = {
  full: "Tam Türkçe",
  partial: "Kısmi Türkçe",
  none: "Türkçe yok",
  unknown: "Türkçe denetleniyor",
} as const;
