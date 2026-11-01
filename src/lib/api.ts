import { invoke } from "@tauri-apps/api/core";
import type {
  Batch, CatalogStatus, Collection, DownloadRow, FontDetail, FontPage, FontQuery, LibraryStats, Progress,
  StorageInfo, SystemFamily,
} from "./types";

export const api = {
  listFonts: (query: FontQuery) => invoke<FontPage>("list_fonts", { query }),
  getFont: (id: string) => invoke<FontDetail>("get_font", { id }),
  catalogStatus: () => invoke<CatalogStatus>("catalog_status"),
  syncCatalog: () => invoke<void>("sync_catalog"),
  testApiKey: (key: string) => invoke<number>("test_api_key", { key }),

  listCollections: () => invoke<Collection[]>("list_collections"),
  createCollection: (name: string, fontIds: string[]) => invoke<number>("create_collection", { name, fontIds }),
  renameCollection: (id: number, name: string) => invoke<void>("rename_collection", { id, name }),
  deleteCollection: (id: number) => invoke<void>("delete_collection", { id }),
  addToCollection: (id: number, fontIds: string[]) => invoke<void>("add_to_collection", { id, fontIds }),
  removeFromCollection: (id: number, fontIds: string[]) => invoke<void>("remove_from_collection", { id, fontIds }),
  toggleFavorite: (id: string) => invoke<boolean>("toggle_favorite", { id }),

  downloadFont: (id: string, installAfter = false) => invoke<void>("download_font", { id, installAfter }),
  installFont: (id: string) => invoke<void>("install_font", { id }),
  uninstallFont: (id: string) => invoke<number>("uninstall_font", { id }),
  deleteDownloaded: (id: string) => invoke<void>("delete_downloaded", { id }),
  startBulk: (request: { label: string; ids?: string[]; query?: FontQuery; install: boolean }) =>
    invoke<{ batchId: number; total: number }>("start_bulk", { request }),
  listBatches: () => invoke<Batch[]>("list_batches"),
  dismissBatch: (id: number) => invoke<void>("dismiss_batch", { id }),
  retryBatch: (id: number) => invoke<number>("retry_batch_failures", { id }),

  listDownloads: () => invoke<DownloadRow[]>("list_downloads"),
  downloadSnapshot: () => invoke<Progress[]>("download_snapshot"),
  pauseDownload: (id: number) => invoke<void>("pause_download", { id }),
  resumeDownload: (id: number) => invoke<void>("resume_download", { id }),
  cancelDownload: (id: number) => invoke<void>("cancel_download", { id }),
  pauseAll: () => invoke<void>("pause_all"),
  resumeAll: () => invoke<void>("resume_all"),
  clearFinished: () => invoke<void>("clear_finished_downloads"),
  installAllDownloaded: () => invoke<number>("install_all_downloaded"),

  libraryStats: () => invoke<LibraryStats>("library_stats"),
  scanSystemFonts: () => invoke<number>("scan_system_fonts"),
  listSystemFonts: (search: string) => invoke<SystemFamily[]>("list_system_fonts", { search }),
  getSettings: () => invoke<Record<string, string>>("get_settings"),
  setSetting: (key: string, value: string) => invoke<void>("set_setting", { key, value }),
  chooseDownloadDir: () => invoke<string | null>("choose_download_dir"),
  resetDownloadDir: () => invoke<void>("reset_download_dir"),
  storageInfo: () => invoke<StorageInfo>("storage_info"),
  clearCache: () => invoke<void>("clear_cache"),
  exportLibrary: () => invoke<string | null>("export_library"),
  revealInstalled: () => invoke<void>("reveal_installed_folder"),
  revealDownloads: () => invoke<void>("reveal_download_folder"),
};

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : "Beklenmeyen bir hata oluştu";
}
