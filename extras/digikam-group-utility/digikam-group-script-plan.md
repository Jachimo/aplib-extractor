# DigiKam MasterUUID Grouping Utility - Implementation Plan

## Executive Summary

Create a Python utility that groups DigiKam images based on a custom `aplib:MasterUUID` field stored in XMP sidecar files. The utility must safely operate against a production MySQL database shared by multiple applications, handling hundreds to thousands of images efficiently.

> Status note: parts of this plan were originally drafted using a `Images.groupImage` assumption. The validated model is `ImageRelations` with `type=2` (`DatabaseRelation::Grouped`). Any section below that still mentions `groupImage` should be treated as historical/outdated and updated before execution.

## Background & Context

### Data Structure
- **XMP Namespace**: `http://github.com/Jachimo/aplib-extractor/aplib/1.0/` (note: this URI is used as a unique identifier per XMP spec — it does not need to resolve to an actual web page)
- **Grouping Field**: `aplib:MasterUUID` (e.g., `uhoq+7KsSHe+bTeFEvnKZg`)
- **Relationship**: Images sharing the same MasterUUID value are versions of the same master image
- **Sidecar Format**: Files named `{imagename}.xmp` in same directory as images

### Database Environment
- **Type**: Remote MySQL server (production, shared with other applications)
- **Safety Requirement**: Must not corrupt existing data; enterprise-grade precautions required
- **Scale**: Hundreds to low-thousands of images per batch operation

### Key Finding: DigiKam Group Storage Mechanism

Based on DigiKam source (`coredbconstants.h`, `iteminfo.cpp`, `coredb.cpp`, `dbconfig.xml.cmake.in`), grouping is stored in `ImageRelations`, not an `Images.groupImage` column:

- `DatabaseRelation::Grouped = 2`
- Group membership is an edge: `ImageRelations.subject = member_id`, `ImageRelations.object = leader_id`, `ImageRelations.type = 2`
- Group edits use relation operations equivalent to:
    - remove existing grouped relation(s) from subject
    - add `subject -> leader` relation with `type=2`

To create a group with leader ID 100 and members 101, 102:
```sql
INSERT INTO ImageRelations (subject, object, type) VALUES (101, 100, 2);
INSERT INTO ImageRelations (subject, object, type) VALUES (102, 100, 2);
```

Direct SQL is required — there is no supported Python API for DigiKam group manipulation.

### Known Schema (Verify Against Source)

Based on DigiKam source code, the relevant tables are:

```sql
CREATE TABLE AlbumRoots (
    id INTEGER PRIMARY KEY,
    label TEXT,
    status INTEGER NOT NULL DEFAULT 0,
    type INTEGER NOT NULL DEFAULT 0,
    identifier TEXT,
    specificPath TEXT
);

CREATE TABLE Albums (
    id INTEGER PRIMARY KEY,
    albumRoot INTEGER NOT NULL,        -- FK to AlbumRoots.id
    parentId INTEGER,
    relativePath TEXT NOT NULL,         -- relative to album root, stored WITH leading slash (e.g., '/2024/vacation')
    date DATE,
    caption TEXT,
    collection TEXT,
    icon TEXT,
    iconKDE TEXT
);

CREATE TABLE Images (
    id INTEGER PRIMARY KEY,
    album INTEGER NOT NULL,            -- FK to Albums.id
    name TEXT NOT NULL,                -- filename only, no path
    status INTEGER NOT NULL DEFAULT 0,
    category INTEGER NOT NULL DEFAULT 0,
    modificationDate DATETIME,
    fileSize INTEGER NOT NULL DEFAULT 0,
    uniqueHash TEXT,
    manualOrder INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE ImageRelations (
    subject BIGINT,
    object BIGINT,
    type INTEGER,
    UNIQUE(subject, object, type)
);
```

**IMPORTANT**: These schema definitions must still be verified against the live target DB before writes. If the live DB differs, stop and reconcile before applying any grouping updates.

---

## Phase 1: Discovery & Schema Extraction

### Step 1.1: Verify DigiKam Database Schema

**Objective**: Confirm exact schema for `Images`, `Albums`, and `AlbumRoots` tables.

**Actions**:
1. Clone DigiKam source repository (shallow clone to save time/space):
   ```bash
   git clone --depth 1 https://invent.kde.org/graphics/digikam.git
   cd digikam
   ```

2. Locate schema files. The primary locations to check are:
   ```bash
   # Try these paths in order:
   find ./core -name "dbconfig.xml.cmake.in" -o -name "dbconfig.xml"
   find ./core -path "*/schema/*.sql"
   find ./core -name "digikam.sql"
   ```

3. Extract and verify SQL CREATE TABLE statements for:
   - `AlbumRoots` (needed for path resolution)
   - `Albums` (needed for path resolution)
   - `Images` (contains the `groupImage` column for grouping)

4. **Verify the `groupImage` column exists** on the `Images` table and confirm:
   - Column name (is it `groupImage` or something else?)
   - Default value (is it `-1` or `NULL` or `0`?)
   - Semantics (does leader point to itself, or use a separate flag?)

5. **Verify `Albums.relativePath` format**: Check whether it stores paths WITH a leading slash (e.g., `/2024/vacation`) or WITHOUT (e.g., `2024/vacation`). This affects path resolution logic in Step 1.3 and Step 2.1.

6. **Verify `AlbumRoots.specificPath` format**: Check whether it stores paths WITH or WITHOUT a trailing slash (e.g., `/photos` vs `/photos/`). If it has a trailing slash, adjust the CONCAT in `resolve_image_id()` to handle this (e.g., use `CONCAT(TRIM(TRAILING '/' FROM r.specificPath), a.relativePath)`).

7. Document any differences from the schema provided above.

**Deliverable**: Verified SQL CREATE TABLE statements for `Images`, `Albums`, `AlbumRoots`. If the schema differs from what's provided above, update all subsequent steps accordingly.

### Step 1.2: Analyze Existing Group Data

**Objective**: Understand how groups are currently stored in the production database.

**Actions**:
1. Query existing groups (read-only):
   ```sql
   -- Find all group leaders (images whose groupImage points to themselves)
   SELECT id, album, name, groupImage FROM Images WHERE groupImage = id LIMIT 10;

   -- Find all group members (images whose groupImage points to another image)
   SELECT id, album, name, groupImage FROM Images WHERE groupImage != -1 AND groupImage != id LIMIT 10;

   -- Count images per group
   SELECT groupImage as leader_id, COUNT(*) as member_count
   FROM Images
   WHERE groupImage != -1
   GROUP BY groupImage
   ORDER BY member_count DESC
   LIMIT 10;
   ```

2. Document:
   - Confirm `groupImage = -1` means "no group"
   - Confirm leader convention (self-referencing or other)
   - Note any images with unexpected `groupImage` values

**Deliverable**: Sample data confirming the group storage mechanism.

### Step 1.3: Verify Image Path Resolution

**Objective**: Confirm how filesystem paths map to image records in the database.

**Actions**:
1. Pick a known image file path from your DigiKam collection.

2. Query the database to find it:
   ```sql
   -- Given path: /photos/2024/vacation/IMG_001.jpg
   -- Directory: /photos/2024/vacation
   -- Filename: IMG_001.jpg

   -- Step 1: Find matching AlbumRoot (by specificPath prefix)
   -- Use '/%' to avoid matching /photos when path is /photos2/...
   SELECT id, specificPath, label FROM AlbumRoots
   WHERE '/photos/2024/vacation' LIKE CONCAT(specificPath, '/%')
      OR '/photos/2024/vacation' = specificPath;

   -- Step 2: Find matching Album (relativePath is relative to root, stored WITH leading slash)
   -- If root specificPath is '/photos', then relativePath should be '/2024/vacation'
   -- CONCAT(specificPath, relativePath) = '/photos/2024/vacation'
   SELECT a.id, a.relativePath, a.albumRoot, r.specificPath
   FROM Albums a
   JOIN AlbumRoots r ON a.albumRoot = r.id
   WHERE CONCAT(r.specificPath, a.relativePath) = '/photos/2024/vacation';

   -- Step 3: Find image by album + filename
   SELECT i.id, i.name, i.album, i.groupImage
   FROM Images i
   WHERE i.album = <album_id_from_step2>
     AND i.name = 'IMG_001.jpg';
   ```

3. Document the full path resolution logic and confirm it works. **If `relativePath` does NOT have a leading slash**, adjust the CONCAT in Step 2 to `CONCAT(r.specificPath, '/', a.relativePath)` and update `resolve_image_id()` in Step 2.1 accordingly.

**Deliverable**: Working SQL query that resolves a filesystem path to an image ID.

---

## Phase 2: Design & Architecture

### Step 2.1: Design Database Interaction Layer

**Objective**: Create safe, transactional database operations.

**Design Decisions**:
- Use `mysql-connector-python` (not SQLAlchemy — keeps it simple and explicit)
- All write operations wrapped in transactions
- Read-only validation before any writes
- **Single connection** (not connection pooling — unnecessary for batch operations and adds complexity)
- All image path-to-ID resolution done via a single 3-table JOIN query (`AlbumRoots` → `Albums` → `Images`)

**Required Functions**:
```python
def test_connection(db_config):
    """Test database connectivity. Returns True if successful, raises exception otherwise."""
    conn = mysql.connector.connect(**db_config)
    try:
        cursor = conn.cursor()
        try:
            cursor.execute("SELECT 1")
        finally:
            cursor.close()
    finally:
        conn.close()
    return True

def create_database_backup(db_config, backup_path):
    """Use mysqldump subprocess to create a full backup. Uses --defaults-extra-file
    to avoid exposing password in process list."""
    ...

def resolve_image_id(cursor, full_image_path):
    """
    Given a full filesystem path like /photos/2024/vacation/IMG_001.jpg,
    return the Images.id from the DigiKam database.

    Uses a single JOIN query for efficiency. If multiple AlbumRoots match,
    logs a warning and returns None (ambiguous match).

    Logic:
    1. Split path into directory and filename
    2. JOIN AlbumRoots, Albums, Images to find matching image
    Returns: image_id (int) or None if not found or ambiguous
    """
    import os
    directory, filename = os.path.split(full_image_path)
    query = """
        SELECT i.id
        FROM Images i
        JOIN Albums a ON i.album = a.id
        JOIN AlbumRoots r ON a.albumRoot = r.id
        WHERE i.name = %s
          AND CONCAT(r.specificPath, a.relativePath) = %s
    """
    cursor.execute(query, (filename, directory))
    rows = cursor.fetchall()
    if len(rows) > 1:
        logger.warning(f"Path '{full_image_path}' matched multiple album roots: {rows}")
        return None
    return rows[0][0] if rows else None

def get_image_group_status(cursor, image_id):
    """Returns the current groupImage value for an image, or None if image not found."""
    cursor.execute("SELECT groupImage FROM Images WHERE id = %s", (image_id,))
    row = cursor.fetchone()
    return row[0] if row else None

def get_existing_groups(cursor):
    """
    Returns dict mapping leader_id -> list of member_ids (excluding leader)
    for all existing groups in the database.

    Fetches rows and groups in Python to avoid GROUP_CONCAT length limits.
    """
    cursor.execute("""
        SELECT id, groupImage FROM Images WHERE groupImage != -1
    """)
    result = {}
    for img_id, leader_id in cursor.fetchall():
        if leader_id != img_id:  # Exclude leader from member list
            result.setdefault(leader_id, []).append(img_id)
    return result

def create_group(cursor, leader_image_id, member_image_ids):
    """
    Creates a group by:
    1. Setting leader's groupImage = leader_image_id (self-reference)
    2. Setting each member's groupImage = leader_image_id
    Must be called within a transaction.

    Raises ValueError if member_image_ids is empty or if any image is already in a group.
    """
    if not member_image_ids:
        raise ValueError("Cannot create a group with no members")

    # Set leader's groupImage to its own id (self-reference)
    cursor.execute(
        "UPDATE Images SET groupImage = %s WHERE id = %s AND groupImage = -1",
        (leader_image_id, leader_image_id)
    )
    if cursor.rowcount == 0:
        raise ValueError(f"Leader image {leader_image_id} already in a group or not found")

    # Set each member's groupImage to leader's id
    for member_id in member_image_ids:
        cursor.execute(
            "UPDATE Images SET groupImage = %s WHERE id = %s AND groupImage = -1",
            (leader_image_id, member_id)
        )
        if cursor.rowcount == 0:
            raise ValueError(f"Member image {member_id} already in a group or not found")

def get_images_in_group(cursor, leader_id):
    """Returns list of image IDs in the group led by leader_id (including leader)."""
    cursor.execute("SELECT id FROM Images WHERE groupImage = %s", (leader_id,))
    return [row[0] for row in cursor.fetchall()]
```

### Step 2.2: Design XMP Parsing Layer

**Objective**: Extract MasterUUID from sidecar files efficiently.

**Design Decisions**:
- Use `xml.etree.ElementTree` (standard library, no dependencies)
- Handle missing/invalid XMP gracefully (skip with warning)

**Namespace Mapping**:
```python
NAMESPACES = {
    'aplib': 'http://github.com/Jachimo/aplib-extractor/aplib/1.0/',
    'xmp': 'http://ns.adobe.com/xap/1.0/',
    'rdf': 'http://www.w3.org/1999/02/22-rdf-syntax-ns#',
    'dc': 'http://purl.org/dc/elements/1.1/',
}
```

**Concrete Parsing Example**:
```python
import xml.etree.ElementTree as ET

def extract_master_uuid(xmp_file_path):
    """
    Extract aplib:MasterUUID from an XMP sidecar file.

    Expected XMP structure (simplified):
    <x:xmpmeta xmlns:x="adobe:ns:meta/">
      <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
        <rdf:Description rdf:about=""
          xmlns:aplib="http://github.com/Jachimo/aplib-extractor/aplib/1.0/"
          aplib:MasterUUID="uhoq+7KsSHe+bTeFEvnKZg"/>
      </rdf:RDF>
    </x:xmpmeta>

    Returns: UUID string or None if not found / file missing / parse error / permission error
    """
    try:
        tree = ET.parse(xmp_file_path)
        root = tree.getroot()

        # Note: ET.register_namespace() affects serialization output, not parsing.
        # Parsing works regardless of whether namespaces are registered.
        # We register them here so that if we ever serialize the XMP, the prefixes are preserved.
        for prefix, uri in NAMESPACES.items():
            ET.register_namespace(prefix, uri)

        # Search for any element with the MasterUUID attribute
        # The attribute will be {namespace}MasterUUID
        aplib_ns = NAMESPACES['aplib']
        master_uuid_attr = f"{{{aplib_ns}}}MasterUUID"

        for elem in root.iter():
            if master_uuid_attr in elem.attrib:
                return elem.attrib[master_uuid_attr]

        return None

    except ET.ParseError:
        return None
    except OSError:  # Covers FileNotFoundError, PermissionError, and other I/O errors
        return None
```

### Step 2.3: Design Grouping Logic

**Objective**: Map MasterUUID values to DigiKam groups.

**Supported Image Extensions**:
```python
IMAGE_EXTENSIONS = {
    '.jpg', '.jpeg', '.png', '.tif', '.tiff', '.bmp', '.gif',
    '.cr2', '.crw', '.nef', '.arw', '.dng', '.raf', '.orf',
    '.rw2', '.pef', '.srw', '.raw', '.heic', '.heif', '.avif',
    '.jxl', '.webp'
}
```

**Algorithm**:
1. Scan directory for image files (using `IMAGE_EXTENSIONS` set above)
2. For each image, locate corresponding `.xmp` sidecar
3. Parse XMP and extract `aplib:MasterUUID`
4. Build dictionary: `{master_uuid: [image_path1, image_path2, ...]}`
5. For each UUID with 2+ images:
   - Resolve each image path to a DigiKam `Images.id` via `resolve_image_id()`
   - Skip any images not found in the database (log warning)
   - Check if images are already in a group (check `groupImage` column via `get_image_group_status()`)
   - If any image is already in a group, log warning and skip (do not modify existing groups)
   - Choose a leader (see Leader Selection Strategy below)
   - Build `member_image_ids` list EXCLUDING the leader
   - Call `create_group_safe(conn, leader_id, member_ids)` within a transaction

**Leader Selection Strategy**:
- Prefer the image with the largest file size (likely the master/original)
- Or prefer the image with the earliest modification date
- Or simply use the first image alphabetically
- Document the chosen strategy in the code
- **The leader is NOT included in `member_image_ids`** — `create_group()` handles the leader separately

---

## Phase 3: Implementation

### Step 3.1: Create Project Structure

```bash
mkdir -p digikam-group-utility/{src,tests,docs,backups}
cd digikam-group-utility

# Create virtual environment
python3 -m venv venv
source venv/bin/activate

# Create requirements.txt
cat > requirements.txt << 'EOF'
mysql-connector-python>=8.0.0
pytest>=7.0.0
pytest-mock>=3.0.0
EOF

pip install -r requirements.txt
```

### Step 3.2: Implement Core Modules

**File: `src/xmp_parser.py`**
- Parse XMP sidecar files using `xml.etree.ElementTree`
- Extract `aplib:MasterUUID` attribute from any element in the XML tree
- Handle missing files, malformed XML, permission errors, and missing attributes gracefully (return `None`)
- Catch `OSError` (not just `FileNotFoundError`) to cover permission errors and other I/O errors
- Include the concrete parsing code shown in Step 2.2

**File: `src/database.py`**
- Database connection management (single connection, not pooled)
- Transaction handling (explicit `start_transaction()`, `commit()`, `rollback()`)
- **`test_connection()`**: Test database connectivity, return True or raise exception. Use try/finally to ensure cursor and connection are always closed.
- **Path resolution**: `resolve_image_id()` using the single 3-table JOIN query from Step 2.1
- **Group operations**: `create_group()`, `get_image_group_status()`, `get_existing_groups()`, `get_images_in_group()`
- **`get_existing_groups()`**: Fetch rows and group in Python (do NOT use `GROUP_CONCAT` — it has a default 1024-byte length limit that silently truncates large groups)
- **Backup**: Use `subprocess.run()` to call `mysqldump` with `--defaults-extra-file` (not `--password` on command line, not `SELECT INTO OUTFILE`). Clean up backup file on failure.

**File: `src/grouper.py`**
- Main grouping logic orchestrating XMP parsing and database operations
- Batch processing with progress reporting (print progress every N images)
- Leader selection strategy (document and implement consistently)
- **Exclude leader from member list** before calling `create_group_safe()`

**File: `src/config.py`**
- Configuration from environment variables
- **Convert `DIGIKAM_DB_PORT` to int** (environment variables are strings, but `mysql.connector.connect()` expects port as integer)
- Logging setup (both file and stdout handlers)

```python
# Example config.py structure:
import os

db_config = {
    'host': os.environ['DIGIKAM_DB_HOST'],
    'port': int(os.environ['DIGIKAM_DB_PORT']),  # MUST convert to int
    'database': os.environ['DIGIKAM_DB_NAME'],
    'user': os.environ['DIGIKAM_DB_USER'],
    'password': os.environ['DIGIKAM_DB_PASSWORD'],
}
```

**File: `src/cli.py`**
- Command-line interface with `argparse`
- `--dry-run` flag: parse XMP, resolve image IDs, show planned groups, but do NOT write to database
- `--backup-only` flag: create database backup and exit
- `--input-dir` argument: directory to scan for images
- `--verbose` flag: show detailed progress

### Step 3.3: Safety Features Implementation

**Required Safety Mechanisms**:

1. **Database Backup** (use `mysqldump` via subprocess with `--defaults-extra-file` to avoid exposing password in process list; clean up backup file on failure):
   ```python
   import subprocess
   import tempfile
   import os
   from datetime import datetime

   def backup_database(db_config, backup_dir):
       timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
       backup_file = f"{backup_dir}/digikam_backup_{timestamp}.sql"

       # Write credentials to temp file securely (avoids password in process list)
       with tempfile.NamedTemporaryFile(mode='w', suffix='.cnf', delete=False) as f:
           f.write(f"[client]\nhost={db_config['host']}\n")
           f.write(f"port={db_config['port']}\n")
           f.write(f"user={db_config['user']}\n")
           f.write(f"password={db_config['password']}\n")
           creds_file = f.name

       try:
           cmd = ["mysqldump", f"--defaults-extra-file={creds_file}", db_config['database']]
           with open(backup_file, 'w') as out:
               subprocess.run(cmd, stdout=out, check=True)
       except Exception:
           # Clean up partial backup file on failure
           if os.path.exists(backup_file):
               os.unlink(backup_file)
           raise
       finally:
           os.unlink(creds_file)  # Always clean up credentials file

       return backup_file
   ```

2. **Dry-Run Mode**:
   - Parse all XMP files
   - Show what groups would be created
   - No database modifications

3. **Validation Layer**:
   - Verify image exists in DigiKam database before grouping (via `resolve_image_id()`)
   - Check `groupImage` column — if already set to something other than `-1`, skip with warning
   - Validate XMP structure before processing (catch `ParseError`)
   - Validate that `member_image_ids` is non-empty before calling `create_group_safe()`

4. **Transaction Safety** (`create_group_safe` is a thin wrapper that manages transaction/cursor lifecycle and delegates to `create_group`):
   ```python
   def create_group_safe(conn, leader_image_id, member_image_ids):
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
               pass  # Don't mask the original error
           raise
       finally:
           if cursor is not None:
               cursor.close()
   ```

5. **Audit Logging**:
   - Log every operation with timestamp
   - Record before/after states
   - Write to file and stdout

---

## Phase 4: Testing Strategy

### Step 4.1: Unit Tests

**File: `tests/test_xmp_parser.py`**
- Test parsing valid XMP with `aplib:MasterUUID` attribute
- Test handling missing XMP files (returns `None`)
- Test malformed XML handling (returns `None`, no exception)
- Test XMP without MasterUUID attribute (returns `None`)
- Test namespace resolution with multiple namespaces present
- Test permission error handling (returns `None`, no exception)

**File: `tests/test_database.py`**
- Mock database connections using `unittest.mock`
- Test `test_connection()` returns True on successful connection
- Test `test_connection()` closes cursor and connection even if query fails
- Test `resolve_image_id()` with valid path, invalid path, ambiguous match (multiple roots), edge cases
- Test `create_group_safe()` transaction commit on success
- Test `create_group_safe()` rollback on error (e.g., member already in group)
- Test `create_group_safe()` raises ValueError when `member_image_ids` is empty
- Test `create_group_safe()` handles `cursor = None` in finally when `start_transaction()` fails
- Test `create_group_safe()` doesn't mask original exception if rollback fails
- Test `get_existing_groups()` returns correct structure (leader excluded from member list)
- Test `get_existing_groups()` works with large groups (no GROUP_CONCAT truncation)
- Test `get_image_group_status()` returns correct value
- Test backup function calls `mysqldump` with `--defaults-extra-file` (mock subprocess, verify temp file creation and cleanup)
- Test backup function cleans up backup file on mysqldump failure

**File: `tests/test_grouper.py`**
- Test grouping algorithm with sample data (mock XMP parser and database)
- Test that images already in groups are skipped
- Test that images not in database are skipped with warning
- Test leader selection logic
- Test that leader is excluded from `member_image_ids`
- Test dry-run mode produces no database writes

### Step 4.2: Integration Tests

**Test Environment Setup**:
1. Create local MySQL test database
2. Import DigiKam schema for `AlbumRoots`, `Albums`, and `Images` tables — use the CREATE TABLE statements from the "Known Schema" section above (you need all three for path resolution)
3. Insert test data:
   - 1 AlbumRoot with `specificPath = '/test/photos'`
   - 2 Albums under that root with different `relativePath` values (e.g., `/album1` and `/album2`)
   - 10 Images across those albums, all with `groupImage = -1`
4. Create test image files with XMP sidecars in a temporary directory matching the album structure

**Test Scenarios**:
1. **Happy Path**: 3 images with same MasterUUID → 1 group created, leader's `groupImage = own id`, members' `groupImage = leader's id`
2. **No Groups**: All images have unique MasterUUIDs → no groups created, all `groupImage` remain `-1`
3. **Mixed**: Some images share UUIDs, some don't → only matching images grouped
4. **Existing Groups**: Image with `groupImage != -1` encountered → skipped with warning, no modification
5. **Missing XMP**: Image without sidecar → logged as warning, skipped
6. **Image Not In Database**: XMP has UUID but image not in DigiKam DB → skipped with warning
7. **Database Error**: Connection lost mid-operation → rollback, no partial groups created
8. **Dry Run**: All above scenarios run with `--dry-run` → no `groupImage` values change in database

### Step 4.3: Production Validation

**Pre-Deployment Checklist**:
- [ ] Schema verified against actual DigiKam source code
- [ ] `Albums.relativePath` leading slash format confirmed
- [ ] `AlbumRoots.specificPath` trailing slash format confirmed
- [ ] Dry-run on production data shows expected groups
- [ ] Database backup created successfully
- [ ] All existing groups remain intact
- [ ] No orphaned group records created
- [ ] Performance test: 1000 images processed in < 5 minutes

---

## Phase 5: Deployment & Documentation

### Step 5.1: Create Configuration Template

**File: `.env.example`** (note: `export` is required so variables are available to child Python process):
```bash
# Database Configuration
export DIGIKAM_DB_HOST=mysql.example.com
export DIGIKAM_DB_PORT=3306
export DIGIKAM_DB_NAME=digikam
export DIGIKAM_DB_USER=digikam_grouping
export DIGIKAM_DB_PASSWORD=secure_password

# Paths
export BACKUP_DIR=/path/to/backups
export LOG_FILE=/var/log/digikam-grouping.log

# Safety
export DRY_RUN=false
```

### Step 5.2: Create Usage Documentation

**File: `README.md`**:
- Installation instructions
- Configuration guide
- Usage examples
- Troubleshooting
- Safety warnings

**File: `OPERATIONS.md`**:
- Pre-run checklist
- How to monitor execution
- How to rollback if needed
- Post-run validation steps

### Step 5.3: Create Execution Script

**File: `run_grouping.sh`**:
```bash
#!/bin/bash
set -e

# Activate virtual environment
source venv/bin/activate

# Load environment (variables must be exported in .env)
source .env

# Create backup
python src/cli.py --backup-only

# Dry run first
python src/cli.py --dry-run --input-dir "$1"

# Confirm with user (skip if non-interactive or --yes flag passed)
if [ "$2" = "--yes" ]; then
    confirm="yes"
elif [ -t 0 ]; then
    read -p "Proceed with actual grouping? (yes/no): " confirm
else
    echo "Non-interactive mode: use --yes flag as second argument to skip confirmation"
    confirm="no"
fi

if [ "$confirm" = "yes" ]; then
    python src/cli.py --input-dir "$1"
fi
```

---

## Phase 6: Execution on Production Data

### Step 6.1: Pre-Execution

1. **Verify environment**:
   ```bash
   python --version  # 3.8+
   mysql --version   # 8.0+
   ```

2. **Test database connectivity**:
   ```bash
   python -c "from src.database import test_connection; test_connection()"
   ```

3. **Create backup** (use the Python utility to avoid exposing password in process list):
   ```bash
   python src/cli.py --backup-only
   ```

### Step 6.2: Execution

1. **Run with dry-run first**:
   ```bash
   python src/cli.py --dry-run --input-dir /path/to/images --verbose
   ```

2. **Review dry-run output**:
   - Verify number of groups to be created
   - Check image counts per group
   - Confirm no unexpected modifications

3. **Execute for real**:
   ```bash
   python src/cli.py --input-dir /path/to/images --verbose --log-file grouping.log
   ```

### Step 6.3: Post-Execution Validation

1. **Verify groups in database**:
   ```sql
   -- Count images per group (should show new groups created by the utility)
   SELECT groupImage as leader_id, COUNT(*) as member_count
   FROM Images
   WHERE groupImage != -1
   GROUP BY groupImage
   ORDER BY member_count DESC;

   -- Verify a specific group (replace <leader_id> with actual ID)
   SELECT i.id, i.name, i.album, i.groupImage,
          CASE WHEN i.groupImage = i.id THEN 'LEADER' ELSE 'MEMBER' END as role
   FROM Images i
   WHERE i.groupImage = <leader_id>
   ORDER BY role, i.name;
   ```

2. **Verify in DigiKam UI**:
   - Open DigiKam
   - Navigate to grouped images
   - Confirm grouping indicator appears
   - Expand/collapse groups to verify membership

3. **Check logs**:
   ```bash
   grep -E "(ERROR|WARNING|SUCCESS)" grouping.log
   ```

---

## Deliverables Checklist

- [ ] `src/xmp_parser.py` - XMP parsing module
- [ ] `src/database.py` - Database operations module (includes `test_connection()`)
- [ ] `src/grouper.py` - Main grouping logic
- [ ] `src/config.py` - Configuration management (with int port conversion)
- [ ] `src/cli.py` - Command-line interface
- [ ] `tests/` - Comprehensive test suite
- [ ] `README.md` - User documentation
- [ ] `OPERATIONS.md` - Operational procedures
- [ ] `requirements.txt` - Python dependencies
- [ ] `run_grouping.sh` - Execution script (with venv activation and `--yes` flag support)
- [ ] `.env.example` - Configuration template (with `export` statements)

---

## Risk Mitigation

| Risk | Mitigation |
|------|-----------|
| Database corruption | Transaction wrapping with rollback on any error, `mysqldump` backup before execution, dry-run mode |
| Password exposure | Use `--defaults-extra-file` with temp file for mysqldump, never pass password on command line |
| Performance issues | Single JOIN query for path resolution, single connection (no pooling overhead), progress reporting |
| Invalid XMP data | Graceful error handling in parser (return `None`), skip bad files with warning |
| Permission errors on XMP files | Catch `OSError` in parser (covers `FileNotFoundError`, `PermissionError`, and other I/O errors) |
| Concurrent modifications | Check `groupImage = -1` before updating (atomic conditional UPDATE), off-peak execution |
| Lost database connection | Transaction rollback on connection error (without masking original exception), idempotent operations |
| Images already in groups | Conditional UPDATE (`WHERE groupImage = -1`) prevents overwriting existing groups; raises error if conflict detected |
| Path resolution failures | Log warning for each unresolvable path, continue processing remaining images |
| Ambiguous AlbumRoot matches | Log warning and skip if multiple roots match a single path |
| Non-interactive script failure | `run_grouping.sh` detects terminal and supports `--yes` flag |
| Schema mismatch | Stop and report if `groupImage` column or `relativePath`/`specificPath` format differs from expected |
| GROUP_CONCAT truncation | Fetch rows and group in Python instead of using `GROUP_CONCAT` (avoids 1024-byte limit) |
| Partial backup on failure | Clean up backup file if `mysqldump` fails |
| Resource leaks | Use try/finally in `test_connection()` to ensure cursor and connection are always closed |
| Rollback masking original error | Wrap `conn.rollback()` in nested try/except to preserve original exception |

---

## Success Criteria

1. All images with matching `aplib:MasterUUID` are grouped in DigiKam (leader's `groupImage = own id`, members' `groupImage = leader's id`)
2. No existing data is corrupted or lost (existing groups with `groupImage != -1` are never modified)
3. Execution completes in under 5 minutes for 1000 images
4. Zero errors in production execution (warnings for skipped images are acceptable)
5. Groups are visible and functional in DigiKam UI (leader shown, members collapsed under leader)

---

## References

- DigiKam Source Repository: `https://invent.kde.org/graphics/digikam.git` (the GitHub mirror at `https://github.com/KDE/digikam` is archived and outdated)
- DigiKam Database Schema: Check `core/data/database/dbconfig.xml.cmake.in` and `core/libs/database/schema/` in the source tree
- DigiKam Documentation: `https://docs.digikam.org/en/index.html`
- XMP Specification: Adobe XMP SDK documentation (namespace URIs are identifiers, not necessarily resolvable URLs)
- MySQL Python Connector: `https://dev.mysql.com/doc/connector-python/en/`

---

## Additional Notes for Small LLM Implementation

1. **Implement in this order**: `config.py` → `xmp_parser.py` → `database.py` → `grouper.py` → `cli.py` → tests
2. **Test each module independently** before integrating
3. **Always run with `--dry-run` first** on any real data
4. **The `WHERE groupImage = -1` condition in UPDATE statements is critical** — it prevents overwriting existing groups and makes the operation safe to re-run
5. **If the schema verification in Step 1.1 reveals different column names or semantics**, STOP and report the actual schema. Do not attempt to adapt the SQL queries without understanding the actual grouping mechanism.
6. **Path separator handling**: Use `os.path` functions for cross-platform compatibility, but be aware DigiKam on Linux stores paths with `/` separators
7. **The leader is NOT included in `member_image_ids`** — `create_group()` sets the leader's `groupImage` separately from the members
8. **`cursor = None` must be initialized before the `try` block** in `create_group_safe()` to avoid `NameError` in the `finally` block if `start_transaction()` fails
9. **`.env` file must use `export`** for each variable so they are available as environment variables to the Python child process
10. **`DIGIKAM_DB_PORT` must be converted to `int`** in `config.py` — environment variables are strings, but `mysql.connector.connect()` expects port as integer
11. **Do NOT use `GROUP_CONCAT`** in `get_existing_groups()` — fetch rows and group in Python to avoid the 1024-byte length limit
12. **`create_group_safe()` should delegate to `create_group()`** — do not duplicate the SQL logic in both functions
13. **Catch `OSError` in XMP parser** — not just `FileNotFoundError` — to handle permission errors and other I/O errors
14. **Use try/finally in `test_connection()`** to ensure cursor and connection are always closed even if the query fails
15. **Wrap `conn.rollback()` in nested try/except** in `create_group_safe()` to avoid masking the original exception if rollback fails
16. **Clean up backup file on `mysqldump` failure** — don't leave partial backups on disk
17. **`run_grouping.sh` must activate the virtual environment** before running Python commands
