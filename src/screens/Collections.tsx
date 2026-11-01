import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowLeft, FolderOpen, MoreHorizontal, Pencil, Plus, Trash2 } from "lucide-react";
import { CollectionNameDialog } from "@/components/BulkBar";
import { useCardFace } from "@/components/FontCard";
import { useAction } from "@/components/FontActions";
import { EmptyState, FontGrid } from "@/components/FontGrid";
import { CountLabel, Filters, SelectAllButton } from "@/components/Toolbar";
import { Button } from "@/components/ui/button";
import { Menu, MenuContent, MenuItem, MenuTrigger, Modal } from "@/components/ui/controls";
import { api } from "@/lib/api";
import type { Collection, FontQuery } from "@/lib/types";
import { formatNumber } from "@/lib/utils";
import { useUi } from "@/store/ui";
import { PageTitle } from "./Browse";

function Sample({ font }: { font: Collection["sampleFonts"][number] }) {
  const face = useCardFace(font);
  return (
    <span
      className="block truncate text-[19px] leading-[1.25] text-ink transition-opacity"
      style={{
        opacity: face.family ? 1 : 0.15,
        fontFamily: face.family ? `"${face.family}"` : undefined,
        fontWeight: font.previewWeight,
        fontStyle: font.previewItalic ? "italic" : "normal",
      }}
    >
      {font.family}
    </span>
  );
}

export function CollectionTile({ collection, compact }: { collection: Collection; compact?: boolean }) {
  const samples = collection.sampleFonts.slice(0, compact ? 2 : 3);
  return (
    <div className="flex h-full flex-col rounded-[10px] border border-line bg-panel p-3.5 transition-colors hover:border-line-strong">
      <div className="min-h-0 flex-1">{samples.map((f) => <Sample key={f.id} font={f} />)}</div>
      <div className="mt-3">
        <div className="truncate text-[13px] font-medium">{collection.name}</div>
        <div className="mt-0.5 truncate text-[11px] tabular-nums text-faint">
          {formatNumber(collection.fontCount)} aile
          {collection.installedCount > 0 && ` · ${formatNumber(collection.installedCount)} kurulu`}
        </div>
      </div>
    </div>
  );
}

export function Collections() {
  const { data } = useQuery({ queryKey: ["collections"], queryFn: api.listCollections });
  const navigate = useUi((s) => s.navigate);
  const client = useQueryClient();
  const [naming, setNaming] = useState(false);
  const user = data?.filter((c) => !c.isSystem) ?? [];
  const system = data?.filter((c) => c.isSystem) ?? [];
  const grid = "grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-2.5 px-4";
  const tile = (c: Collection) => (
    <button key={c.id} type="button" className="min-h-[148px] text-left" onClick={() => navigate({ name: "collection", id: c.id })}>
      <CollectionTile collection={c} />
    </button>
  );
  return (
    <div className="min-h-0 flex-1 overflow-y-auto pb-20">
      <PageTitle title="Koleksiyonlar">
        <Button size="sm" className="ml-auto" onClick={() => setNaming(true)}><Plus />Yeni koleksiyon</Button>
      </PageTitle>
      {user.length > 0 && (
        <>
          <h2 className="px-4 pb-2 pt-5 text-xs font-medium text-soft">Koleksiyonlarım</h2>
          <div className={grid}>{user.map(tile)}</div>
        </>
      )}
      <h2 className="px-4 pb-2 pt-5 text-xs font-medium text-soft">Hazır koleksiyonlar</h2>
      <div className={grid}>{system.map(tile)}</div>
      <CollectionNameDialog
        open={naming}
        onClose={() => setNaming(false)}
        title="Yeni koleksiyon"
        onSubmit={async (name) => {
          setNaming(false);
          const id = await api.createCollection(name, []);
          client.invalidateQueries({ queryKey: ["collections"] });
          navigate({ name: "collection", id });
        }}
      />
    </div>
  );
}

export function CollectionDetail({ id }: { id: number }) {
  const { data } = useQuery({ queryKey: ["collections"], queryFn: api.listCollections });
  const collection = data?.find((c) => c.id === id);
  const navigate = useUi((s) => s.navigate);
  const toast = useUi((s) => s.toast);
  const client = useQueryClient();
  const [query, setQuery] = useState<FontQuery>({ collectionId: id });
  const [confirm, setConfirm] = useState(false);
  const [renaming, setRenaming] = useState(false);
  useEffect(() => setQuery({ collectionId: id }), [id]);

  const missing = collection ? collection.fontCount - collection.installedCount : 0;
  const installAll = useAction(
    () => api.startBulk({ label: collection?.name ?? "Koleksiyon", query: { ...query, limit: 5000 }, install: true }),
    () => setConfirm(false),
  );
  const remove = useAction(() => api.deleteCollection(id), () => {
    client.invalidateQueries({ queryKey: ["collections"] });
    toast({ kind: "info", title: "Koleksiyon silindi" });
    navigate({ name: "collections" });
  });

  const header = (
    <div>
      <div className="px-3 pt-3">
        <Button size="sm" variant="ghost" onClick={() => navigate({ name: "collections" })}><ArrowLeft />Koleksiyonlar</Button>
      </div>
      <div className="flex items-end gap-3 px-4 pt-2">
        <div className="min-w-0">
          <h1 className="truncate text-[20px] font-semibold tracking-tight">{collection?.name}</h1>
          {collection?.description && <p className="mt-0.5 truncate text-[13px] text-soft">{collection.description}</p>}
        </div>
        <div className="ml-auto flex shrink-0 items-center gap-1.5">
          {collection && !collection.isSystem && (
            <Menu>
              <MenuTrigger asChild><Button aria-label="Koleksiyon seçenekleri" size="icon" variant="ghost"><MoreHorizontal /></Button></MenuTrigger>
              <MenuContent>
                <MenuItem onSelect={() => setRenaming(true)}><Pencil />Yeniden adlandır</MenuItem>
                <MenuItem danger onSelect={() => remove.mutate()}><Trash2 />Koleksiyonu sil</MenuItem>
              </MenuContent>
            </Menu>
          )}
          <Button variant="primary" disabled={!collection || missing === 0} onClick={() => setConfirm(true)}>Tümünü Kur</Button>
        </div>
      </div>
      <Filters value={query} onChange={setQuery}>
        <SelectAllButton query={query} />
        <CountLabel query={query} />
      </Filters>
      <Modal
        open={confirm}
        onClose={() => setConfirm(false)}
        title={`${formatNumber(missing)} aile kurulsun mu?`}
        description="Eksik dosyalar indirilir ve lisansı doğrulanan tüm fontlar macOS'a kurulur. Filtreler seçiliyse yalnız filtreye uyanlar işlenir."
      >
        <div className="flex justify-end gap-2">
          <Button onClick={() => setConfirm(false)}>Vazgeç</Button>
          <Button variant="primary" disabled={installAll.isPending} onClick={() => installAll.mutate()}>Tümünü Kur</Button>
        </div>
      </Modal>
      {renaming && (
        <CollectionNameDialog
          open
          initial={collection?.name}
          title="Koleksiyonu yeniden adlandır"
          onClose={() => setRenaming(false)}
          onSubmit={async (name) => {
            setRenaming(false);
            await api.renameCollection(id, name);
            client.invalidateQueries({ queryKey: ["collections"] });
          }}
        />
      )}
    </div>
  );
  return (
    <FontGrid
      query={query}
      header={header}
      empty={
        <EmptyState
          icon={FolderOpen}
          title="Bu koleksiyon boş"
          detail="Fontları seçip Koleksiyona ekle ile buraya taşıyın."
          action={<Button onClick={() => navigate({ name: "all" })}>Fontlara göz at</Button>}
        />
      }
    />
  );
}
