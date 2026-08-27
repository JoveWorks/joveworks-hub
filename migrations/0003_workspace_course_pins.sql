ALTER TABLE workspaces ADD COLUMN course_slug TEXT REFERENCES courses(slug);
ALTER TABLE workspaces ADD COLUMN catalogues_json TEXT NOT NULL DEFAULT '[]';
