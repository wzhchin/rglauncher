DELETE
FROM history
WHERE id IN (SELECT id FROM history ORDER BY timestamp ASC LIMIT ?)
