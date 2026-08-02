from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from database import create_group


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
