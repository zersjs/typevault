import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Heart } from "lucide-react";
import { CollectionTile } from "./Collections";
import { EmptyState, FontGrid } from "@/components/FontGrid";
import { CountLabel, Filters, SelectAllButton } from "@/components/Toolbar";
import { Button } from "@/components/ui/button";
import { api } from "@/lib/api";
import type { FontQuery } from "@/lib/types";
import { CATEGORIES, cn } from "@/lib/utils";
import { useUi } from "@/store/ui";

export function PageTitle({ title, children }: { title: string; children?: React.ReactNode }) {
  return (
    <div className="flex items-center gap-3 px-4 pt-4">
      <h1 className="text-[20px] font-semibold tracking-tight">{title}</h1>
      {children}
    </div>
  );
}

export function Discover() {
  const [category, setCategory] = useState<string | null>(null);
  const collections = useQuery({ queryKey: ["collections"], queryFn: api.listCollections });
  const navigate = useUi((s) => s.navigate);
  const search = useUi((s) => s.search);
  const query: FontQuery = { sort: category ? null : "trending", category };
  const header = (
    <div>
      <PageTitle title="Keşfet" />
      {!search && (
        <div className="flex gap-2.5 overflow-x-auto px-4 pb-1 pt-3">
          {collections.data?.filter((c) => c.isSystem).slice(0, 6).map((c) => (
            <button key={c.id} type="button" onClick={() => navigate({ name: "collection", id: c.id })} className="w-[230px] shrink-0 text-left">
              <CollectionTile collection={c} compact />
            </button>
          ))}
        </div>
      )}
      <div className="flex flex-wrap gap-1 px-4 pb-3 pt-3">
        <Chip active={category === null} onClick={() => setCategory(null)}>Yükselenler</Chip>
        {CATEGORIES.map((c) => <Chip key={c.id} active={category === c.id} onClick={() => setCategory(c.id)}>{c.label}</Chip>)}
      </div>
    </div>
  );
  return <FontGrid query={query} header={header} />;
}

export function Chip({ active, onClick, children }: { active: boolean; onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn("h-7 rounded-md px-2.5 text-xs transition-colors", active ? "bg-hover text-ink" : "text-soft hover:text-ink")}
    >
      {children}
    </button>
  );
}

export function AllFonts() {
  const [query, setQuery] = useState<FontQuery>({});
  return (
    <FontGrid
      query={query}
      header={
        <div>
          <PageTitle title="Tüm Fontlar"><CountLabel query={query} /></PageTitle>
          <Filters value={query} onChange={setQuery}>
            <Button size="sm" variant={query.variableOnly ? "primary" : "secondary"} onClick={() => setQuery({ ...query, variableOnly: !query.variableOnly })}>Değişken</Button>
            <SelectAllButton query={query} />
          </Filters>
        </div>
      }
    />
  );
}

export function Favorites() {
  const [query, setQuery] = useState<FontQuery>({ favorites: true });
  const navigate = useUi((s) => s.navigate);
  return (
    <FontGrid
      query={query}
      header={
        <div>
          <PageTitle title="Favoriler"><CountLabel query={query} /></PageTitle>
          <Filters value={query} onChange={setQuery}><SelectAllButton query={query} /></Filters>
        </div>
      }
      empty={
        <EmptyState
          icon={Heart}
          title="Henüz favori yok"
          detail="Beğendiğiniz fontları kalp simgesiyle buraya ekleyin."
          action={<Button onClick={() => navigate({ name: "discover" })}>Fontları keşfet</Button>}
        />
      }
    />
  );
}
