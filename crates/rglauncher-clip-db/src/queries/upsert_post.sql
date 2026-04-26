INSERT INTO history
    (id, content_type, content, favicon, timestamp, source, source_icon, language)
VALUES
    (?, ?, ?, ?, ?, ?, ?, ?)
ON CONFLICT (id) DO UPDATE
SET icount = icount + 1, timestamp = excluded.timestamp, source = excluded.source, source_icon = excluded.source_icon
