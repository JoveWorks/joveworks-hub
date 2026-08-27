CREATE TABLE IF NOT EXISTS course_catalogues (
    course_slug TEXT NOT NULL REFERENCES courses(slug),
    catalogue_id TEXT NOT NULL,
    catalogue_version INTEGER NOT NULL,
    PRIMARY KEY (course_slug, catalogue_id, catalogue_version),
    FOREIGN KEY (catalogue_id, catalogue_version) REFERENCES catalogues(id, version)
);

-- Preserve existing courses' published catalogue pins as course catalogue pins.
INSERT OR IGNORE INTO course_catalogues (course_slug, catalogue_id, catalogue_version)
SELECT cp.course_slug, json_extract(item.value, '$.id'), json_extract(item.value, '$.version')
FROM course_publications cp
JOIN publications p ON p.id = cp.publication_id
JOIN json_each(p.catalogues_json) AS item
WHERE json_extract(item.value, '$.id') IS NOT NULL
  AND json_extract(item.value, '$.version') IS NOT NULL;
