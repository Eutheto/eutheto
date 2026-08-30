CREATE TABLE app_metadata (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
) STRICT;

INSERT INTO app_metadata (key, value)
VALUES ('portable_library_revision', '0');

INSERT INTO app_metadata (key, value)
VALUES ('portable_import_provenance_retention_policy', '1');

CREATE TABLE scenarios (
  id TEXT PRIMARY KEY,
  domain_pack_id TEXT NOT NULL,
  domain_schema_version INTEGER NOT NULL CHECK (domain_schema_version >= 1),
  title TEXT NOT NULL,
  description TEXT,
  revision INTEGER NOT NULL CHECK (revision >= 0),
  document_json TEXT NOT NULL,
  portable_required_capabilities_json TEXT NOT NULL DEFAULT '[]',
  portable_semantic_extensions_json TEXT NOT NULL DEFAULT '{}',
  portable_extensions_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  last_opened_at TEXT,
  archived_at TEXT
) STRICT;

CREATE TABLE scenario_revision_high_water (
  scenario_id TEXT PRIMARY KEY,
  highest_revision INTEGER NOT NULL CHECK (
    highest_revision >= 0 AND highest_revision <= 9007199254740991
  )
) STRICT;

CREATE TABLE scenario_identity_owners (
  identity TEXT PRIMARY KEY,
  scenario_id TEXT NOT NULL
) STRICT;

CREATE TABLE scenario_snapshots (
  id TEXT PRIMARY KEY,
  scenario_id TEXT NOT NULL REFERENCES scenarios(id) ON DELETE CASCADE,
  revision INTEGER NOT NULL CHECK (revision >= 0),
  document_json_zstd BLOB NOT NULL,
  created_at TEXT NOT NULL,
  reason TEXT NOT NULL,
  UNIQUE (scenario_id, revision)
) STRICT;

CREATE TABLE retained_scenario_revisions (
  scenario_id TEXT NOT NULL REFERENCES scenarios(id) ON DELETE CASCADE,
  revision INTEGER NOT NULL CHECK (revision >= 0),
  scenario_json TEXT NOT NULL,
  PRIMARY KEY (scenario_id, revision)
) STRICT;

CREATE TABLE command_journal (
  id TEXT PRIMARY KEY,
  scenario_id TEXT NOT NULL REFERENCES scenarios(id) ON DELETE CASCADE,
  revision_before INTEGER NOT NULL CHECK (revision_before >= 0),
  revision_after INTEGER NOT NULL CHECK (revision_after > revision_before),
  command_type TEXT NOT NULL,
  command_json TEXT NOT NULL,
  inverse_json TEXT,
  actor_json TEXT NOT NULL,
  source TEXT NOT NULL,
  summary TEXT NOT NULL,
  created_at TEXT NOT NULL,
  history_sequence INTEGER NOT NULL CHECK (history_sequence > 0),
  branch_generation INTEGER NOT NULL CHECK (branch_generation >= 0),
  UNIQUE (scenario_id, revision_after),
  UNIQUE (scenario_id, branch_generation, history_sequence)
) STRICT;

CREATE TABLE scenario_history_state (
  scenario_id TEXT PRIMARY KEY REFERENCES scenarios(id) ON DELETE CASCADE,
  cursor_sequence INTEGER NOT NULL CHECK (cursor_sequence >= 0),
  branch_generation INTEGER NOT NULL CHECK (branch_generation >= 0)
) STRICT;

CREATE TABLE solve_runs (
  id TEXT PRIMARY KEY,
  scenario_id TEXT NOT NULL REFERENCES scenarios(id) ON DELETE CASCADE,
  scenario_revision INTEGER NOT NULL CHECK (scenario_revision >= 0),
  input_hash TEXT NOT NULL,
  backend_id TEXT NOT NULL,
  backend_version TEXT NOT NULL,
  protocol_version INTEGER,
  status TEXT NOT NULL,
  options_json TEXT NOT NULL,
  model_summary_json TEXT,
  started_at TEXT NOT NULL,
  finished_at TEXT,
  elapsed_ms INTEGER CHECK (elapsed_ms IS NULL OR elapsed_ms >= 0),
  best_bound TEXT,
  backend_diagnostics_json TEXT,
  error_json TEXT
) STRICT;

CREATE TABLE solutions (
  id TEXT PRIMARY KEY,
  solve_run_id TEXT NOT NULL REFERENCES solve_runs(id) ON DELETE CASCADE,
  scenario_id TEXT NOT NULL REFERENCES scenarios(id) ON DELETE CASCADE,
  scenario_revision INTEGER NOT NULL CHECK (scenario_revision >= 0),
  status TEXT NOT NULL,
  accepted INTEGER NOT NULL CHECK (accepted IN (0, 1)),
  normalized_solution_json TEXT NOT NULL,
  score_json TEXT NOT NULL,
  verification_report_json TEXT NOT NULL,
  explanation_index_json TEXT,
  created_at TEXT NOT NULL
) STRICT;

CREATE TABLE portable_library_metadata (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  manifest_extensions_json TEXT NOT NULL,
  nonsemantic_extensions_json TEXT NOT NULL
) STRICT;

INSERT INTO portable_library_metadata (
  singleton,
  manifest_extensions_json,
  nonsemantic_extensions_json
) VALUES (1, '{}', '[]');

CREATE TABLE app_settings (
  key TEXT PRIMARY KEY,
  value_json TEXT NOT NULL,
  updated_at TEXT NOT NULL
) STRICT;

CREATE TABLE portable_sections (
  section TEXT NOT NULL CHECK (section IN ('results', 'shared_records', 'preferences', 'assets')),
  key TEXT NOT NULL,
  value BLOB NOT NULL,
  asset_media_type TEXT,
  asset_redistribution_permitted INTEGER,
  PRIMARY KEY (section, key),
  CHECK (
    (section = 'assets' AND asset_media_type IS NOT NULL AND asset_redistribution_permitted IS NOT NULL AND asset_redistribution_permitted IN (0, 1))
    OR
    (section <> 'assets' AND asset_media_type IS NULL AND asset_redistribution_permitted IS NULL)
  )
) STRICT;

CREATE TABLE portable_import_provenance (
  id INTEGER PRIMARY KEY,
  source_bundle_id TEXT NOT NULL,
  source_application_json TEXT NOT NULL,
  original_format_version INTEGER NOT NULL CHECK (original_format_version >= 1),
  original_schema_version INTEGER NOT NULL CHECK (original_schema_version >= 1),
  source_file_sha256 TEXT NOT NULL,
  applied_migrations_json TEXT NOT NULL,
  binding_json TEXT NOT NULL,
  scenario_sources_json TEXT NOT NULL,
  source_created_at TEXT NOT NULL,
  applied_at TEXT NOT NULL
) STRICT;

CREATE TABLE safety_backup_failure_receipts (
  id INTEGER PRIMARY KEY,
  proof_sha256 TEXT NOT NULL UNIQUE,
  binding_json BLOB NOT NULL,
  collision_plan_sha256 TEXT NOT NULL,
  safe_reason TEXT NOT NULL,
  created_at TEXT NOT NULL
) STRICT;

CREATE TABLE ai_conversations (
  id TEXT PRIMARY KEY,
  scenario_id TEXT REFERENCES scenarios(id) ON DELETE CASCADE,
  title TEXT NOT NULL,
  provider_id TEXT NOT NULL,
  model_id TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
) STRICT;

CREATE TABLE ai_messages (
  id TEXT PRIMARY KEY,
  conversation_id TEXT NOT NULL REFERENCES ai_conversations(id) ON DELETE CASCADE,
  role TEXT NOT NULL,
  content_json TEXT NOT NULL,
  tool_activity_json TEXT,
  created_at TEXT NOT NULL
) STRICT;

CREATE INDEX scenarios_by_recency
  ON scenarios (archived_at, updated_at DESC, id);
CREATE INDEX solve_runs_by_scenario
  ON solve_runs (scenario_id, started_at DESC, id);
CREATE INDEX accepted_solutions_by_scenario
  ON solutions (scenario_id, accepted, created_at DESC, id);
CREATE INDEX ai_conversations_by_scenario
  ON ai_conversations (scenario_id, updated_at DESC, id);
CREATE INDEX command_journal_by_history
  ON command_journal (scenario_id, branch_generation, history_sequence);
