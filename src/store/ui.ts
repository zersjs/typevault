import { create } from "zustand";
import type { Progress, SyncProgress } from "@/lib/types";

export type Route =
  | { name: "discover" }
  | { name: "all" }
  | { name: "collections" }
  | { name: "collection"; id: number }
  | { name: "library" }
  | { name: "downloads" }
  | { name: "favorites" }
  | { name: "settings" };

export interface Toast { id: number; kind: "success" | "error" | "info"; title: string; detail?: string }

interface UiState {
  route: Route;
  search: string;
  previewText: string;
  previewSize: number;
  detailId: string | null;
  selection: Set<string>;
  progress: Record<string, Progress>;
  installing: Set<string>;
  sync: SyncProgress | null;
  toasts: Toast[];
  navigate: (route: Route) => void;
  setSearch: (search: string) => void;
  setPreviewText: (text: string) => void;
  setPreviewSize: (size: number) => void;
  openDetail: (id: string | null) => void;
  toggleSelect: (id: string) => void;
  selectMany: (ids: string[]) => void;
  clearSelection: () => void;
  mergeProgress: (items: Progress[]) => void;
  setInstalling: (id: string, active: boolean) => void;
  setSync: (sync: SyncProgress | null) => void;
  toast: (t: Omit<Toast, "id">) => void;
  dismissToast: (id: number) => void;
}

let toastId = 0;
const ACTIVE = new Set(["queued", "downloading", "paused"]);

export const useUi = create<UiState>((set, get) => ({
  route: { name: "discover" },
  search: "",
  previewText: "",
  previewSize: 44,
  detailId: null,
  selection: new Set(),
  progress: {},
  installing: new Set(),
  sync: null,
  toasts: [],
  navigate: (route) => set({ route, selection: new Set() }),
  setSearch: (search) => set({ search }),
  setPreviewText: (previewText) => set({ previewText }),
  setPreviewSize: (previewSize) => set({ previewSize }),
  openDetail: (detailId) => set({ detailId }),
  toggleSelect: (id) => {
    const selection = new Set(get().selection);
    if (!selection.delete(id)) selection.add(id);
    set({ selection });
  },
  selectMany: (ids) => set({ selection: new Set([...get().selection, ...ids]) }),
  clearSelection: () => set({ selection: new Set() }),
  mergeProgress: (items) => {
    const progress = { ...get().progress };
    for (const item of items) {
      if (ACTIVE.has(item.status)) progress[item.fontId] = item;
      else delete progress[item.fontId];
    }
    set({ progress });
  },
  setInstalling: (id, active) => {
    const installing = new Set(get().installing);
    if (active) installing.add(id);
    else installing.delete(id);
    set({ installing });
  },
  setSync: (sync) => set({ sync }),
  toast: (t) => {
    const id = ++toastId;
    set({ toasts: [...get().toasts, { ...t, id }].slice(-4) });
    setTimeout(() => get().dismissToast(id), t.kind === "error" ? 7000 : 3600);
  },
  dismissToast: (id) => set({ toasts: get().toasts.filter((t) => t.id !== id) }),
}));
