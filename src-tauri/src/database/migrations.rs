//! Sıralı şema geçişleri. `PRAGMA user_version` uygulanan son geçişi tutar.

pub const MIGRATIONS: &[&str] = &[r#"
CREATE TABLE licenses (
  id INTEGER PRIMARY KEY,
  code TEXT NOT NULL UNIQUE,
  name TEXT NOT NULL,
  url TEXT NOT NULL,
  allows_install INTEGER NOT NULL DEFAULT 0
);
INSERT INTO licenses (code, name, url, allows_install) VALUES
  ('ofl', 'SIL Open Font License 1.1', 'https://openfontlicense.org', 1),
  ('apache2', 'Apache License 2.0', 'https://www.apache.org/licenses/LICENSE-2.0', 1),
  ('ufl', 'Ubuntu Font License 1.0', 'https://ubuntu.com/legal/font-licence', 1);

CREATE TABLE fonts (
  id TEXT PRIMARY KEY,
  family TEXT NOT NULL,
  slug TEXT NOT NULL UNIQUE,
  source TEXT NOT NULL DEFAULT 'google',
  category TEXT NOT NULL,
  designers TEXT NOT NULL DEFAULT '[]',
  subsets TEXT NOT NULL DEFAULT '[]',
  languages TEXT NOT NULL DEFAULT '[]',
  description TEXT,
  popularity INTEGER NOT NULL DEFAULT 100000,
  trending INTEGER NOT NULL DEFAULT 100000,
  date_added TEXT,
  last_modified TEXT,
  size_bytes INTEGER NOT NULL DEFAULT 0,
  is_variable INTEGER NOT NULL DEFAULT 0,
  is_noto INTEGER NOT NULL DEFAULT 0,
  is_color INTEGER NOT NULL DEFAULT 0,
  license_id INTEGER REFERENCES licenses(id),
  api_files TEXT,
  turkish_status TEXT NOT NULL DEFAULT 'unknown'
    CHECK (turkish_status IN ('unknown','full','partial','none')),
  turkish_source TEXT,
  turkish_missing TEXT NOT NULL DEFAULT '',
  local_status TEXT NOT NULL DEFAULT 'available'
    CHECK (local_status IN ('available','downloaded','installed','install_failed')),
  local_error TEXT,
  downloaded_version TEXT,
  update_available INTEGER NOT NULL DEFAULT 0,
  enriched_version TEXT,
  in_catalog INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX idx_fonts_popularity ON fonts(popularity);
CREATE INDEX idx_fonts_family ON fonts(family COLLATE NOCASE);
CREATE INDEX idx_fonts_local ON fonts(local_status);
CREATE INDEX idx_fonts_turkish ON fonts(turkish_status);

CREATE TABLE font_categories (
  font_id TEXT NOT NULL REFERENCES fonts(id) ON DELETE CASCADE,
  category TEXT NOT NULL,
  PRIMARY KEY (font_id, category)
) WITHOUT ROWID;
CREATE INDEX idx_font_categories_cat ON font_categories(category);

CREATE TABLE font_tags (
  font_id TEXT NOT NULL REFERENCES fonts(id) ON DELETE CASCADE,
  tag TEXT NOT NULL,
  weight INTEGER NOT NULL,
  PRIMARY KEY (font_id, tag)
) WITHOUT ROWID;
CREATE INDEX idx_font_tags_tag ON font_tags(tag, weight);

CREATE TABLE font_variants (
  font_id TEXT NOT NULL REFERENCES fonts(id) ON DELETE CASCADE,
  weight INTEGER NOT NULL,
  style TEXT NOT NULL CHECK (style IN ('normal','italic')),
  width_class INTEGER,
  thickness INTEGER,
  PRIMARY KEY (font_id, weight, style)
) WITHOUT ROWID;

CREATE TABLE font_axes (
  font_id TEXT NOT NULL REFERENCES fonts(id) ON DELETE CASCADE,
  tag TEXT NOT NULL,
  name TEXT NOT NULL,
  min_value REAL NOT NULL,
  max_value REAL NOT NULL,
  default_value REAL NOT NULL,
  PRIMARY KEY (font_id, tag)
) WITHOUT ROWID;

CREATE TABLE font_files (
  id INTEGER PRIMARY KEY,
  font_id TEXT NOT NULL REFERENCES fonts(id) ON DELETE CASCADE,
  filename TEXT NOT NULL,
  url TEXT NOT NULL,
  format TEXT NOT NULL CHECK (format IN ('ttf','otf')),
  is_variable INTEGER NOT NULL DEFAULT 0,
  local_path TEXT,
  sha256 TEXT,
  size_bytes INTEGER NOT NULL DEFAULT 0,
  postscript_name TEXT,
  weight INTEGER,
  italic INTEGER NOT NULL DEFAULT 0,
  UNIQUE (font_id, filename)
);
CREATE INDEX idx_font_files_font ON font_files(font_id);
CREATE INDEX idx_font_files_sha ON font_files(sha256);

CREATE TABLE batches (
  id INTEGER PRIMARY KEY,
  label TEXT NOT NULL,
  total INTEGER NOT NULL,
  install_after INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  dismissed INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE batch_fonts (
  batch_id INTEGER NOT NULL REFERENCES batches(id) ON DELETE CASCADE,
  font_id TEXT NOT NULL REFERENCES fonts(id) ON DELETE CASCADE,
  state TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending','done','failed','skipped')),
  message TEXT,
  PRIMARY KEY (batch_id, font_id)
) WITHOUT ROWID;

CREATE TABLE downloads (
  id INTEGER PRIMARY KEY,
  font_id TEXT NOT NULL REFERENCES fonts(id) ON DELETE CASCADE,
  batch_id INTEGER REFERENCES batches(id) ON DELETE SET NULL,
  status TEXT NOT NULL CHECK (status IN ('queued','downloading','paused','completed','failed','cancelled')),
  install_after INTEGER NOT NULL DEFAULT 0,
  received_bytes INTEGER NOT NULL DEFAULT 0,
  files_done INTEGER NOT NULL DEFAULT 0,
  files_total INTEGER NOT NULL DEFAULT 0,
  attempts INTEGER NOT NULL DEFAULT 0,
  error TEXT,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_downloads_status ON downloads(status);
CREATE UNIQUE INDEX idx_downloads_active ON downloads(font_id)
  WHERE status IN ('queued','downloading','paused');

CREATE TABLE installations (
  id INTEGER PRIMARY KEY,
  font_id TEXT NOT NULL REFERENCES fonts(id) ON DELETE CASCADE,
  file_id INTEGER NOT NULL UNIQUE REFERENCES font_files(id) ON DELETE CASCADE,
  owner TEXT NOT NULL CHECK (owner IN ('typevault','external')),
  status TEXT NOT NULL CHECK (status IN ('installing','installed','failed')),
  installed_path TEXT,
  postscript_name TEXT,
  sha256 TEXT,
  error TEXT,
  installed_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_installations_font ON installations(font_id);

CREATE TABLE collections (
  id INTEGER PRIMARY KEY,
  slug TEXT UNIQUE,
  name TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  is_system INTEGER NOT NULL DEFAULT 0,
  position INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE collection_fonts (
  collection_id INTEGER NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
  font_id TEXT NOT NULL REFERENCES fonts(id) ON DELETE CASCADE,
  position INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (collection_id, font_id)
) WITHOUT ROWID;
CREATE INDEX idx_collection_fonts_font ON collection_fonts(font_id);

CREATE TABLE favorites (
  font_id TEXT PRIMARY KEY REFERENCES fonts(id) ON DELETE CASCADE,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
) WITHOUT ROWID;

CREATE TABLE settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
) WITHOUT ROWID;

CREATE TABLE system_fonts (
  postscript_name TEXT PRIMARY KEY,
  family TEXT NOT NULL,
  style TEXT NOT NULL DEFAULT '',
  path TEXT,
  managed INTEGER NOT NULL DEFAULT 0
) WITHOUT ROWID;
CREATE INDEX idx_system_fonts_family ON system_fonts(family);

CREATE VIRTUAL TABLE fonts_fts USING fts5(
  family, designers, keywords,
  tokenize = "unicode61 remove_diacritics 2"
);
"#];
