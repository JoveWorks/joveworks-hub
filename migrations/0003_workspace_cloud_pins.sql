ALTER TABLE workspaces ADD COLUMN cloud_slug TEXT REFERENCES clouds(slug);
ALTER TABLE workspaces ADD COLUMN catalogues_json TEXT NOT NULL DEFAULT '[]';
