ALTER TABLE publications ADD COLUMN source_workspace_id TEXT REFERENCES workspaces(id);
