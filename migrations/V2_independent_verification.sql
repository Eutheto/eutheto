ALTER TABLE solve_runs ADD COLUMN request_id TEXT;
ALTER TABLE solve_runs ADD COLUMN run_input_json TEXT;
ALTER TABLE solve_runs ADD COLUMN run_manifest_json TEXT;

CREATE UNIQUE INDEX solve_runs_by_request_id
  ON solve_runs (request_id)
  WHERE request_id IS NOT NULL;

UPDATE solve_runs
SET status = CASE
  WHEN status = 'running' THEN 'legacy_interrupted'
  ELSE 'legacy_terminal'
END;

ALTER TABLE solutions ADD COLUMN evidence_json TEXT;

UPDATE solutions
SET accepted = 0, status = 'legacy_unverified';

CREATE UNIQUE INDEX canonical_solution_by_run
  ON solutions (solve_run_id)
  WHERE accepted = 1 AND status = 'verified';

CREATE TABLE counterfactual_jobs (
  id TEXT PRIMARY KEY,
  request_id TEXT NOT NULL UNIQUE,
  request_hash TEXT NOT NULL,
  cancel_request_id TEXT UNIQUE,
  scenario_id TEXT NOT NULL REFERENCES scenarios(id) ON DELETE CASCADE,
  scenario_revision INTEGER NOT NULL CHECK (
    scenario_revision >= 0 AND scenario_revision <= 9007199254740991
  ),
  snapshot_id TEXT NOT NULL REFERENCES scenario_snapshots(id) ON DELETE CASCADE,
  base_solution_id TEXT NOT NULL REFERENCES solutions(id) ON DELETE CASCADE,
  base_result_checksum TEXT NOT NULL,
  condition_json TEXT NOT NULL,
  total_budget_ms INTEGER NOT NULL CHECK (
    total_budget_ms > 0 AND total_budget_ms <= 9007199254740991
  ),
  state TEXT NOT NULL CHECK (
    state IN ('queued', 'running', 'completed', 'failed', 'cancelled', 'interrupted')
  ),
  cancel_requested_at TEXT,
  created_at TEXT NOT NULL,
  started_at TEXT,
  finished_at TEXT,
  result_json TEXT,
  evidence_json TEXT,
  error_json TEXT
) STRICT;

CREATE INDEX counterfactual_jobs_by_scenario
  ON counterfactual_jobs (scenario_id, created_at DESC, id);
