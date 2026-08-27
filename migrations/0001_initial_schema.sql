CREATE TABLE IF NOT EXISTS catalogues (
    id TEXT NOT NULL,
    version INTEGER NOT NULL,
    hash TEXT NOT NULL,
    restricted INTEGER NOT NULL,
    content_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id, version)
);

CREATE TABLE IF NOT EXISTS courses (
    slug TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    theme_json TEXT,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS publications (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    mode TEXT NOT NULL,
    document_json TEXT NOT NULL,
    catalogues_json TEXT NOT NULL,
    published_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS course_publications (
    course_slug TEXT NOT NULL REFERENCES courses(slug),
    publication_id TEXT NOT NULL REFERENCES publications(id),
    PRIMARY KEY (course_slug, publication_id)
);
