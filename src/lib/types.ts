export type TurkishStatus = "unknown" | "full" | "partial" | "none";
export type LocalStatus = "available" | "downloaded" | "installed" | "install_failed";
export type DownloadStatus = "queued" | "downloading" | "paused" | "completed" | "failed" | "cancelled";

export interface FontSummary {
  id: string;
  family: string;
  category: string;
  categories: string[];
  variantCount: number;
  isVariable: boolean;
  turkishStatus: TurkishStatus;
  turkishMissing: string;
  localStatus: LocalStatus;
  localError: string | null;
  updateAvailable: boolean;
  favorite: boolean;
  license: string | null;
  downloadStatus: DownloadStatus | null;
  downloadId: number | null;
  previewWeight: number;
  previewItalic: boolean;
  previewFileId: number | null;
}

export interface FontPage { items: FontSummary[]; total: number }

export interface FontQuery {
  search?: string;
  category?: string | null;
  turkish?: string | null;
  localStatus?: string | null;
  collectionId?: number | null;
  favorites?: boolean;
  variableOnly?: boolean;
  updatesOnly?: boolean;
  localOnly?: boolean;
  turkishFirst?: boolean;
  sort?: string | null;
  offset?: number;
  limit?: number;
}

export interface FontVariant { weight: number; style: "normal" | "italic" }
export interface FontAxis { tag: string; name: string; min: number; max: number; defaultValue: number }
export interface LocalFile {
  id: number;
  filename: string;
  format: string;
  isVariable: boolean;
  sizeBytes: number;
  weight: number | null;
  italic: boolean;
  postscriptName: string | null;
  installed: string | null;
  owner: string | null;
}

export interface FontDetail {
  summary: FontSummary;
  designers: string[];
  subsets: string[];
  languages: string[];
  description: string | null;
  licenseUrl: string | null;
  lastModified: string | null;
  source: string;
  turkishSource: string | null;
  variants: FontVariant[];
  axes: FontAxis[];
  files: LocalFile[];
}

export interface CatalogStatus {
  total: number;
  enriched: number;
  syncing: boolean;
  lastSync: string | null;
  turkishFull: number;
  categories: { id: string; count: number }[];
}

export interface SyncProgress { phase: string; done: number; total: number; message: string | null }

export interface Collection {
  id: number;
  name: string;
  description: string;
  isSystem: boolean;
  fontCount: number;
  installedCount: number;
  sampleFonts: { id: string; family: string; previewWeight: number; previewItalic: boolean; previewFileId: number | null }[];
}

export interface Progress {
  id: number;
  fontId: string;
  status: DownloadStatus;
  receivedBytes: number;
  filesDone: number;
  filesTotal: number;
  fileFraction: number;
  speedBps: number;
  currentFile: string | null;
  error: string | null;
}

export interface DownloadRow {
  id: number;
  fontId: string;
  family: string;
  status: DownloadStatus;
  installAfter: boolean;
  receivedBytes: number;
  filesDone: number;
  filesTotal: number;
  error: string | null;
  localStatus: LocalStatus;
  updatedAt: string;
}

export interface Batch {
  id: number;
  label: string;
  total: number;
  installAfter: boolean;
  done: number;
  failed: number;
  skipped: number;
  failures: { fontId: string; family: string; state: string; message: string | null }[];
}

export interface LibraryStats {
  available: number;
  downloaded: number;
  installed: number;
  failed: number;
  updates: number;
  installedFiles: number;
  localFiles: number;
  localBytes: number;
  favorites: number;
  activeDownloads: number;
}

export interface SystemFamily { family: string; styles: number; managed: boolean; samplePostscript: string }
export interface StorageInfo { fontsBytes: number; installedBytes: number; cacheBytes: number }
export interface InstallEvent { fontId: string; family: string; state: "installing" | "installed" | "failed"; message: string | null }
