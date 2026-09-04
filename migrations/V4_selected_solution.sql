ALTER TABLE scenarios
ADD COLUMN selected_solution_id TEXT REFERENCES solutions(id) ON DELETE SET NULL;
