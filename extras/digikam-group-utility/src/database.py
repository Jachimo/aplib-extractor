import logging
import os
import re
import subprocess
import tempfile
import urllib.parse
from datetime import datetime
from typing import Any

try:
    import mysql.connector as mysql_connector
except Exception:
    mysql_connector = None

logger = logging.getLogger(__name__)

# Verified from DigiKam source: DatabaseRelation::Grouped = 2
GROUPED_RELATION_TYPE = 2


def _require_mysql_connector() -> None:
    if mysql_connector is None:
        raise RuntimeError(
            "mysql-connector-python is required. Install dependencies from requirements.txt"
        )


def test_connection(db_config: dict[str, Any]) -> bool:
    _require_mysql_connector()
    conn = mysql_connector.connect(**db_config)
    try:
        cursor = conn.cursor()
        try:
            cursor.execute("SELECT 1")
            cursor.fetchone()
        finally:
            cursor.close()
    finally:
        conn.close()
    return True


def connect(db_config: dict[str, Any]):
    _require_mysql_connector()
    return mysql_connector.connect(**db_config)


def create_database_backup(db_config: dict[str, Any], backup_dir: str) -> str:
    os.makedirs(backup_dir, exist_ok=True)
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    backup_file = os.path.join(backup_dir, f"digikam_backup_{timestamp}.sql")

    with tempfile.NamedTemporaryFile(mode="w", suffix=".cnf", delete=False) as f:
        f.write("[client]\n")
        f.write(f"host={db_config['host']}\n")
        f.write(f"port={db_config['port']}\n")
        f.write(f"user={db_config['user']}\n")
        f.write(f"password={db_config['password']}\n")
        creds_file = f.name

    try:
        cmd = ["mysqldump", f"--defaults-extra-file={creds_file}", db_config["database"]]
        with open(backup_file, "w", encoding="utf-8") as out:
            subprocess.run(cmd, stdout=out, check=True)
    except Exception:
        if os.path.exists(backup_file):
            os.unlink(backup_file)
        raise
    finally:
        os.unlink(creds_file)

    return backup_file


def resolve_image_id(cursor, full_image_path: str) -> int | None:
    image_id, _reason = resolve_image_id_with_reason(cursor, full_image_path)
    return image_id


def resolve_image_id_with_reason(
    cursor,
    full_image_path: str,
    digikam_image_unique_id: str | None = None,
    alternate_filenames: list[str] | None = None,
    global_size_cache: dict[int, tuple[int | None, str]] | None = None,
) -> tuple[int | None, str]:
    """Resolve filesystem path to DigiKam Images.id.

    This uses Image + Albums + AlbumRoots with normalized path handling so
    trailing/leading slash differences do not break matching.
    """
    directory, filename = os.path.split(os.path.normpath(full_image_path))

    uuid_rows: list[int] = []
    if digikam_image_unique_id:
        cursor.execute(
            """
            SELECT imageid
            FROM ImageHistory
            INNER JOIN Images ON imageid=id
            WHERE uuid=%s AND status<3
            LIMIT 3
            """,
            (digikam_image_unique_id,),
        )
        uuid_rows = [int(row[0]) for row in cursor.fetchall()]
        if len(uuid_rows) == 1:
            return uuid_rows[0], "history_uuid_match"

    query = """
        SELECT i.id, r.specificPath, a.relativePath, r.identifier, i.fileSize
        FROM Images i
        JOIN Albums a ON i.album = a.id
        JOIN AlbumRoots r ON a.albumRoot = r.id
        WHERE i.name = %s
    """
    query_name = filename

    cursor.execute(query, (query_name,))
    rows = cursor.fetchall()

    try:
        exported_size = os.path.getsize(full_image_path)
    except OSError:
        exported_size = None

    def _resolve_unique_global_size(file_size: int) -> tuple[int | None, str]:
        if global_size_cache is not None and file_size in global_size_cache:
            return global_size_cache[file_size]

        cursor.execute("SELECT id FROM Images WHERE fileSize=%s LIMIT 3", (file_size,))
        matches = [int(row[0]) for row in cursor.fetchall()]
        if len(matches) == 1:
            result = (matches[0], "global_size_match")
        elif len(matches) > 1:
            result = (None, "global_size_ambiguous")
        else:
            result = (None, "global_size_missing")

        if global_size_cache is not None:
            global_size_cache[file_size] = result
        return result

    if not rows and alternate_filenames:
        seen = {filename.lower()}
        for alt_name in alternate_filenames:
            candidate_name = os.path.basename((alt_name or "").strip())
            if not candidate_name:
                continue
            candidate_key = candidate_name.lower()
            if candidate_key in seen:
                continue
            seen.add(candidate_key)

            cursor.execute(query, (candidate_name,))
            alt_rows = cursor.fetchall()
            if alt_rows:
                rows = alt_rows
                query_name = candidate_name
                break

    if not rows:
        if exported_size is not None:
            image_id, reason = _resolve_unique_global_size(exported_size)
            if image_id is not None:
                return image_id, reason
        if digikam_image_unique_id and len(uuid_rows) > 1:
            return None, "history_uuid_ambiguous"
        return None, "no_name_match"

    def _identifier_paths(identifier: str | None) -> list[str]:
        if not identifier:
            return []

        # DigiKam identifiers commonly carry a URL-like query with path hints.
        # Examples include networkshareid:?mountpath=/mnt/photos/Foo&fileuuid=...
        # and older forms with path=...
        matches = re.findall(r"(?:^|[?&,])(mountpath|path)=([^&,]+)", identifier)
        paths: list[str] = []
        for _key, raw in matches:
            decoded = urllib.parse.unquote(raw)
            if decoded.startswith("/"):
                paths.append(os.path.normpath(decoded))
        return paths

    def _join_root_and_rel(root_path: str, rel_path: str) -> str:
        rel = rel_path or ""
        if rel.startswith("/"):
            rel = rel[1:]

        root = root_path or ""
        if root in {"", "/"}:
            return os.path.normpath("/" + rel)

        return os.path.normpath(os.path.join(root, rel))

    matched_ids: list[int] = []
    for image_id, specific_path, relative_path, identifier, _file_size in rows:
        candidate_dirs: set[str] = set()

        candidate_dirs.add(_join_root_and_rel(specific_path or "", relative_path or ""))

        for id_path in _identifier_paths(identifier):
            candidate_dirs.add(_join_root_and_rel(id_path, relative_path or ""))

        if directory in candidate_dirs:
            matched_ids.append(int(image_id))

    if len(matched_ids) > 1:
        logger.warning("Path '%s' matched multiple image rows: %s", full_image_path, matched_ids)
        return None, "dir_ambiguous"

    if len(matched_ids) == 1:
        if query_name != filename:
            return matched_ids[0], "dir_match_alt_name"
        return matched_ids[0], "dir_match"

    # Fallback: if export location differs from DigiKam album roots, match by
    # filename + file size within same-name candidates.
    if exported_size is not None:
        size_matches = [
            int(image_id)
            for image_id, _specific_path, _relative_path, _identifier, file_size in rows
            if file_size is not None and int(file_size) == int(exported_size)
        ]

        if len(size_matches) == 1:
            if query_name != filename:
                return size_matches[0], "size_match_alt_name"
            return size_matches[0], "size_match"

        if len(size_matches) > 1:
            logger.debug(
                "Ambiguous fallback match for '%s' by name+size (%d bytes): %s",
                full_image_path,
                exported_size,
                size_matches,
            )
            return None, "size_ambiguous"

    if exported_size is not None:
        image_id, reason = _resolve_unique_global_size(exported_size)
        if image_id is not None:
            if query_name != filename:
                return image_id, "global_size_match_alt_name"
            return image_id, reason

    if query_name != filename:
        return None, "dir_mismatch_alt_name"
    return None, "dir_mismatch"


def get_image_group_status(cursor, image_id: int) -> int:
    """Return grouping status for image.

    Returns:
    -1: not grouped as subject
    >0: grouped behind leader image id
    """
    cursor.execute(
        "SELECT object FROM ImageRelations WHERE subject=%s AND type=%s LIMIT 1",
        (image_id, GROUPED_RELATION_TYPE),
    )
    row = cursor.fetchone()
    return int(row[0]) if row else -1


def get_existing_groups(cursor) -> dict[int, list[int]]:
    """Return leader -> members from ImageRelations grouped edges."""
    cursor.execute(
        "SELECT subject, object FROM ImageRelations WHERE type=%s",
        (GROUPED_RELATION_TYPE,),
    )
    result: dict[int, list[int]] = {}
    for subject_id, leader_id in cursor.fetchall():
        if subject_id == leader_id:
            continue
        result.setdefault(int(leader_id), []).append(int(subject_id))
    return result


def _find_grouped_ids(cursor, image_ids: list[int]) -> set[int]:
    if not image_ids:
        return set()

    placeholders = ",".join(["%s"] * len(image_ids))
    params: list[Any] = [GROUPED_RELATION_TYPE]
    params.extend(image_ids)
    params.extend(image_ids)
    query = f"""
        SELECT subject, object
        FROM ImageRelations
        WHERE type=%s
          AND (subject IN ({placeholders}) OR object IN ({placeholders}))
    """
    cursor.execute(query, tuple(params))

    grouped: set[int] = set()
    for subject_id, object_id in cursor.fetchall():
        if int(subject_id) in image_ids:
            grouped.add(int(subject_id))
        if int(object_id) in image_ids:
            grouped.add(int(object_id))
    return grouped


def create_group(cursor, leader_image_id: int, member_image_ids: list[int]) -> None:
    """Create grouping edges member->leader in ImageRelations."""
    if not member_image_ids:
        raise ValueError("Cannot create a group with no members")

    # Dedupe while preserving order.
    unique_member_ids = list(dict.fromkeys(member_image_ids))

    if leader_image_id in unique_member_ids:
        raise ValueError("Leader image cannot also be a member")

    grouped_ids = _find_grouped_ids(cursor, [leader_image_id, *unique_member_ids])

    if leader_image_id in grouped_ids:
        raise ValueError(f"Leader image {leader_image_id} is already grouped")

    for member_id in unique_member_ids:
        if member_id in grouped_ids:
            raise ValueError(f"Member image {member_id} is already grouped")

    for member_id in unique_member_ids:
        cursor.execute(
            "INSERT INTO ImageRelations (subject, object, type) VALUES (%s, %s, %s)",
            (member_id, leader_image_id, GROUPED_RELATION_TYPE),
        )


def create_group_safe(conn, leader_image_id: int, member_image_ids: list[int]) -> None:
    cursor = None
    try:
        if not member_image_ids:
            raise ValueError("Cannot create a group with no members")

        conn.start_transaction()
        cursor = conn.cursor()
        create_group(cursor, leader_image_id, member_image_ids)
        conn.commit()
    except Exception:
        try:
            conn.rollback()
        except Exception:
            pass
        raise
    finally:
        if cursor is not None:
            cursor.close()


def get_images_in_group(cursor, leader_id: int) -> list[int]:
    cursor.execute(
        "SELECT subject FROM ImageRelations WHERE object=%s AND type=%s",
        (leader_id, GROUPED_RELATION_TYPE),
    )
    members = [int(row[0]) for row in cursor.fetchall()]
    return [leader_id] + members
