ALTER TABLE counterfactual_jobs
ADD COLUMN derived_run_id TEXT REFERENCES solve_runs(id) ON DELETE CASCADE;

ALTER TABLE counterfactual_jobs
ADD COLUMN derived_request_id TEXT;

CREATE UNIQUE INDEX counterfactual_jobs_by_derived_run
  ON counterfactual_jobs (derived_run_id)
  WHERE derived_run_id IS NOT NULL;

CREATE UNIQUE INDEX counterfactual_jobs_by_derived_request
  ON counterfactual_jobs (derived_request_id)
  WHERE derived_request_id IS NOT NULL;

CREATE INDEX counterfactual_jobs_running_recovery
  ON counterfactual_jobs (started_at, id)
  WHERE state = 'running';

CREATE TRIGGER counterfactual_jobs_validate_derived_ownership_insert
BEFORE INSERT ON counterfactual_jobs
WHEN
  (NEW.derived_run_id IS NULL) <> (NEW.derived_request_id IS NULL)
  OR (
    NEW.derived_run_id IS NOT NULL
    AND NOT EXISTS (
      SELECT 1
      FROM solve_runs
      WHERE id = NEW.derived_run_id
        AND request_id = NEW.derived_request_id
    )
  )
BEGIN
  SELECT RAISE(ABORT, 'invalid counterfactual derived-run ownership');
END;

CREATE TRIGGER counterfactual_jobs_validate_derived_ownership_update
BEFORE UPDATE OF derived_run_id, derived_request_id ON counterfactual_jobs
WHEN
  (NEW.derived_run_id IS NULL) <> (NEW.derived_request_id IS NULL)
  OR (
    NEW.derived_run_id IS NOT NULL
    AND NOT EXISTS (
      SELECT 1
      FROM solve_runs
      WHERE id = NEW.derived_run_id
        AND request_id = NEW.derived_request_id
    )
  )
BEGIN
  SELECT RAISE(ABORT, 'invalid counterfactual derived-run ownership');
END;
