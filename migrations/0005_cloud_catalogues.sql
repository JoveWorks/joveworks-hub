CREATE TABLE IF NOT EXISTS cloud_catalogues (
    cloud_slug TEXT NOT NULL REFERENCES clouds(slug),
    catalogue_id TEXT NOT NULL,
    catalogue_version INTEGER NOT NULL,
    PRIMARY KEY (cloud_slug, catalogue_id, catalogue_version),
    FOREIGN KEY (catalogue_id, catalogue_version) REFERENCES catalogues(id, version)
);

-- Preserve existing clouds' published catalogue pins as cloud catalogue pins.
INSERT OR IGNORE INTO cloud_catalogues (cloud_slug, catalogue_id, catalogue_version)
SELECT cp.cloud_slug, json_extract(item.value, '$.id'), json_extract(item.value, '$.version')
FROM cloud_publications cp
JOIN publications p ON p.id = cp.publication_id
JOIN json_each(p.catalogues_json) AS item
WHERE json_extract(item.value, '$.id') IS NOT NULL
  AND json_extract(item.value, '$.version') IS NOT NULL;
