import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Download, FolderPlus, Plus, Trash2, X } from "lucide-react";
import { api, errorText } from "@/lib/api";
import { formatNumber } from "@/lib/utils";
import { useUi } from "@/store/ui";
import { Button } from "./ui/button";
import { Menu, MenuContent, MenuItem, MenuSeparator, MenuTrigger, Modal } from "./ui/controls";

export function CollectionNameDialog({ open, onClose, onSubmit, title, initial = "" }: {
  open: boolean; onClose: () => void; onSubmit: (name: string) => void; title: string; initial?: string;
}) {
  const [name, setName] = useState(initial);
  return (
    <Modal open={open} onClose={onClose} title={title}>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (name.trim()) onSubmit(name.trim());
        }}
      >
        <input
          autoFocus
          value={name}
          maxLength={80}
          onChange={(e) => setName(e.target.value)}
          placeholder="Koleksiyon adı"
          className="h-8 w-full rounded-md border border-line bg-panel px-2.5 text-[13px] focus:border-line-strong"
        />
        <div className="mt-4 flex justify-end gap-2">
          <Button onClick={onClose}>Vazgeç</Button>
          <Button variant="primary" type="submit" disabled={!name.trim()}>Kaydet</Button>
        </div>
      </form>
    </Modal>
  );
}

export function BulkBar() {
  const selection = useUi((s) => s.selection);
  const clear = useUi((s) => s.clearSelection);
  const toast = useUi((s) => s.toast);
  const client = useQueryClient();
  const collections = useQuery({ queryKey: ["collections"], queryFn: api.listCollections, enabled: selection.size > 0 });
  const [naming, setNaming] = useState(false);
  const ids = [...selection];
  if (selection.size === 0) return null;

  const run = (install: boolean) => {
    api.startBulk({ label: `${formatNumber(ids.length)} seçili font`, ids, install })
      .then(() => clear())
      .catch((e) => toast({ kind: "error", title: "Toplu işlem başlatılamadı", detail: errorText(e) }));
  };
  // Bir fontun hatası diğerlerini durdurmaz; sonuç tek bildirimde özetlenir.
  const forEach = async (action: (id: string) => Promise<unknown>, verb: string) => {
    const failed: unknown[] = [];
    for (const id of ids) {
      await action(id).catch((e) => failed.push(e));
    }
    clear();
    client.invalidateQueries();
    toast(failed.length
      ? { kind: "error", title: `${formatNumber(ids.length - failed.length)} ${verb}, ${formatNumber(failed.length)} başarısız`, detail: errorText(failed[0]) }
      : { kind: "success", title: `${formatNumber(ids.length)} font ${verb}` });
  };
  const add = (id: number, name: string) => {
    api.addToCollection(id, ids)
      .then(() => {
        toast({ kind: "success", title: `${name} koleksiyonuna eklendi` });
        clear();
      })
      .catch((e) => toast({ kind: "error", title: "Eklenemedi", detail: errorText(e) }));
  };

  return (
    <div className="anim-rise fixed bottom-4 left-[224px] z-30 flex items-center gap-1.5 rounded-xl border border-line-strong bg-raised py-1.5 pl-3.5 pr-1.5 shadow-[var(--shadow)]">
      <span className="mr-1.5 whitespace-nowrap text-[13px] font-medium tabular-nums">{formatNumber(selection.size)} seçili</span>
      <Button size="sm" variant="primary" onClick={() => run(true)}>Tümünü Kur</Button>
      <Button size="sm" onClick={() => run(false)}><Download />İndir</Button>
      <Menu>
        <MenuTrigger asChild><Button size="sm"><FolderPlus />Koleksiyona ekle</Button></MenuTrigger>
        <MenuContent align="start">
          {collections.data?.filter((c) => !c.isSystem).map((c) => (
            <MenuItem key={c.id} onSelect={() => add(c.id, c.name)}>{c.name}</MenuItem>
          ))}
          {collections.data?.some((c) => !c.isSystem) && <MenuSeparator />}
          <MenuItem onSelect={() => setNaming(true)}><Plus />Yeni koleksiyon</MenuItem>
        </MenuContent>
      </Menu>
      <Button size="sm" variant="ghost" onClick={() => forEach(api.uninstallFont, "kaldırıldı")}>Kaldır</Button>
      <Button size="sm" variant="ghost" onClick={() => forEach(api.deleteDownloaded, "silindi")}><Trash2 />Dosyaları sil</Button>
      <Button aria-label="Seçimi temizle" size="icon" variant="ghost" onClick={clear}><X /></Button>
      <CollectionNameDialog
        open={naming}
        onClose={() => setNaming(false)}
        title="Yeni koleksiyon"
        onSubmit={(name) => {
          setNaming(false);
          api.createCollection(name, ids)
            .then(() => {
              toast({ kind: "success", title: `${name} oluşturuldu` });
              clear();
            })
            .catch((e) => toast({ kind: "error", title: "Oluşturulamadı", detail: errorText(e) }));
        }}
      />
    </div>
  );
}
