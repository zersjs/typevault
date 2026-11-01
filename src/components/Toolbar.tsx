import { useQuery } from "@tanstack/react-query";
import { CheckCheck } from "lucide-react";
import type { ReactNode } from "react";
import { api } from "@/lib/api";
import type { FontQuery } from "@/lib/types";
import { CATEGORIES, formatNumber } from "@/lib/utils";
import { useUi } from "@/store/ui";
import { Button } from "./ui/button";
import { Select, Slider } from "./ui/controls";

const TURKISH_OPTIONS = [
  { value: null, label: "Tüm dil desteği" },
  { value: "full", label: "Tam Türkçe" },
  { value: "partial", label: "Kısmi Türkçe" },
  { value: "none", label: "Türkçe yok" },
];

const SORT_OPTIONS = [
  { value: null, label: "Popülerlik" },
  { value: "trending", label: "Yükselenler" },
  { value: "newest", label: "En yeniler" },
  { value: "name", label: "Ada göre" },
];

export function CountLabel({ query }: { query: FontQuery }) {
  const search = useUi((s) => s.search);
  const { data } = useQuery({
    queryKey: ["fonts", "count", query, search],
    queryFn: () => api.listFonts({ ...query, search, limit: 1 }),
  });
  return <span className="text-xs tabular-nums text-faint">{data ? `${formatNumber(data.total)} aile` : ""}</span>;
}

export function SelectAllButton({ query }: { query: FontQuery }) {
  const search = useUi((s) => s.search);
  const selectMany = useUi((s) => s.selectMany);
  return (
    <Button
      size="sm"
      variant="ghost"
      onClick={async () => {
        const page = await api.listFonts({ ...query, search, limit: 500 });
        selectMany(page.items.map((f) => f.id));
      }}
    >
      <CheckCheck />Tümünü seç
    </Button>
  );
}

export function Filters({ value, onChange, children }: {
  value: FontQuery; onChange: (q: FontQuery) => void; children?: ReactNode;
}) {
  const size = useUi((s) => s.previewSize);
  const setSize = useUi((s) => s.setPreviewSize);
  const { data: status } = useQuery({ queryKey: ["catalog"], queryFn: api.catalogStatus });
  const counts = new Map(status?.categories.map((c) => [c.id, c.count]));
  return (
    <div className="flex flex-wrap items-center gap-1.5 px-4 pb-3 pt-3">
      <Select
        value={value.category ?? null}
        onChange={(category) => onChange({ ...value, category })}
        options={[
          { value: null, label: "Tüm kategoriler" },
          ...CATEGORIES.map((c) => ({ value: c.id, label: counts.has(c.id) ? `${c.label}  ${formatNumber(counts.get(c.id)!)}` : c.label })),
        ]}
      />
      <Select value={value.turkish ?? null} onChange={(turkish) => onChange({ ...value, turkish })} options={TURKISH_OPTIONS} />
      <Select value={value.sort ?? null} onChange={(sort) => onChange({ ...value, sort })} options={SORT_OPTIONS} />
      {children}
      <div className="ml-auto flex w-32 items-center gap-2">
        <span className="text-[11px] tabular-nums text-faint">{size}</span>
        <Slider label="Önizleme boyutu" value={size} onChange={setSize} min={20} max={96} />
      </div>
    </div>
  );
}
