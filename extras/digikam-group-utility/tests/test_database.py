from pathlib import Path
import sys
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from database import create_group, resolve_image_id_with_reason


class FakeCursor:
    def __init__(self, existing_ids=None):
        self.existing_ids = set(existing_ids or [])
        self.statements = []
        self._fetchone = None
        self._fetchall = []

    def execute(self, query, params):
        self.statements.append((query, params))

        if "SELECT subject, object" in query and "ImageRelations" in query:
            relation_type = params[0]
            _ = relation_type
            half = (len(params) - 1) // 2
            subject_ids = set(int(v) for v in params[1 : 1 + half])
            object_ids = set(int(v) for v in params[1 + half :])

            rows = []
            for existing_id in self.existing_ids:
                if existing_id in subject_ids:
                    rows.append((existing_id, -1))
                if existing_id in object_ids:
                    rows.append((-1, existing_id))
            self._fetchall = rows
            self._fetchone = None
        else:
            self._fetchone = None
            self._fetchall = []

    def fetchone(self):
        return self._fetchone

    def fetchall(self):
        return self._fetchall


def test_create_group_inserts_relations():
    cur = FakeCursor(existing_ids=[])

    create_group(cur, leader_image_id=10, member_image_ids=[11, 12])

    inserts = [s for s in cur.statements if "INSERT INTO ImageRelations" in s[0]]
    assert len(inserts) == 2


def test_create_group_rejects_existing_group_membership():
    cur = FakeCursor(existing_ids=[11])

    try:
        create_group(cur, leader_image_id=10, member_image_ids=[11, 12])
        assert False, "expected ValueError"
    except ValueError as exc:
        assert "already grouped" in str(exc)


def test_create_group_rejects_leader_as_member():
    cur = FakeCursor(existing_ids=[])

    try:
        create_group(cur, leader_image_id=10, member_image_ids=[10, 11])
        assert False, "expected ValueError"
    except ValueError as exc:
        assert "Leader image cannot also be a member" in str(exc)


class FakeResolveCursor:
    def __init__(self, size_rows):
        self._size_rows = size_rows
        self._fetchall = []
        self._uuid_rows = []

    def set_uuid_rows(self, rows):
        self._uuid_rows = rows

    def execute(self, query, params):
        if "FROM ImageHistory" in query and "WHERE uuid=%s" in query:
            self._fetchall = self._uuid_rows
        elif "WHERE i.name = %s" in query:
            self._fetchall = []
        elif "WHERE fileSize=%s" in query:
            self._fetchall = self._size_rows
        else:
            self._fetchall = []

    def fetchall(self):
        return self._fetchall


@patch("database.os.path.getsize", return_value=1234)
def test_resolve_image_id_with_reason_global_size_unique(_mock_getsize):
    cur = FakeResolveCursor(size_rows=[(42,)])
    image_id, reason = resolve_image_id_with_reason(cur, "/tmp/missing-name.jpg")
    assert image_id == 42
    assert reason == "global_size_match"


@patch("database.os.path.getsize", return_value=1234)
def test_resolve_image_id_with_reason_global_size_ambiguous(_mock_getsize):
    cur = FakeResolveCursor(size_rows=[(42,), (43,), (44,)])
    image_id, reason = resolve_image_id_with_reason(cur, "/tmp/missing-name.jpg")
    assert image_id is None
    assert reason == "no_name_match"


@patch("database.os.path.getsize", return_value=1234)
def test_resolve_image_id_with_reason_history_uuid_unique(_mock_getsize):
    cur = FakeResolveCursor(size_rows=[])
    cur.set_uuid_rows([(99,)])
    image_id, reason = resolve_image_id_with_reason(
        cur,
        "/tmp/missing-name.jpg",
        digikam_image_unique_id="uuid-1",
    )
    assert image_id == 99
    assert reason == "history_uuid_match"


@patch("database.os.path.getsize", return_value=1234)
def test_resolve_image_id_with_reason_history_uuid_ambiguous(_mock_getsize):
    cur = FakeResolveCursor(size_rows=[])
    cur.set_uuid_rows([(99,), (100,), (101,)])
    image_id, reason = resolve_image_id_with_reason(
        cur,
        "/tmp/missing-name.jpg",
        digikam_image_unique_id="uuid-1",
    )
    assert image_id is None
    assert reason == "history_uuid_ambiguous"
