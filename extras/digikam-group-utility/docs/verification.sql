-- Confirm core schema assumptions on your live DigiKam DB

SHOW CREATE TABLE AlbumRoots;
SHOW CREATE TABLE Albums;
SHOW CREATE TABLE Images;
SHOW CREATE TABLE ImageRelations;

-- Verify grouped relation value usage (expected type=2)
SELECT type, COUNT(*) AS cnt
FROM ImageRelations
GROUP BY type
ORDER BY type;

-- Spot check path mapping fields used by resolver
SELECT id, specificPath
FROM AlbumRoots
LIMIT 20;

SELECT id, albumRoot, relativePath
FROM Albums
LIMIT 20;
