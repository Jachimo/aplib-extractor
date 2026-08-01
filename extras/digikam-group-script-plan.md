# DigiKam MasterUUID Grouping Utility - Implementation Plan

## Executive Summary

Create a Python utility that groups DigiKam images based on a custom `aplib:MasterUUID` field stored in XMP sidecar files. The utility must safely operate against a production MySQL database shared by multiple applications, handling hundreds to thousands of images efficiently.

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

DigiKam does **NOT** use separate `ImageGroups` or `ImageGroupProperties` tables. Image grouping is handled through a single column `groupImage` on the `Images` table:

- **`groupImage = -1`**: Image is not part of any group (default)
- **`groupImage = <own id>`**: Image is a group leader
- **`groupImage = <leader's id>`**: Image is a group member, pointing to the leader's image ID

To create a group with leader ID 100 and members 101, 102:
```sql
UPDATE Images SET groupImage = 100 WHERE id = 100;  -- leader points to itself
UPDATE Images SET groupImage = 100 WHERE id = 101;  -- member points to leader
UPDATE Images SET groupImage = 100 WHERE id = 102;  -- member points to leader
```

Direct SQL is required — there is no Python library that exposes DigiKam group manipulation APIs.

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
    relativePath TEXT NOT NULL,         -- relative to album root
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
    manualOrder INTEGER NOT NULL DEFAULT 0,
    groupImage INTEGER DEFAULT -1      -- -1 = no group, <own id> = leader, <leader id> = member
);
```

**IMPORTANT**: These schema definitions are from training data knowledge and should be verified against the actual source. See Step 1.1 for verification instructions.

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

5. Document any differences from the schema provided above.

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
   SELECT id, specificPath, label FROM AlbumRoots
   WHERE '/photos/2024/vacation' LIKE CONCAT(specificPath, '%');

   -- Step 2: Find matching Album (relativePath is relative to root)
   -- If root specificPath is '/photos', then relativePath should be '/2024/vacation'
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

3. Document the full path resolution logic and confirm it works.

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
- All image path-to-ID resolution done via the `AlbumRoots` → `Albums` → `Images` join

**Required Functions**:
```python
def create_database_backup(db_config, backup_path):
    """Use mysqldump subprocess to create a full backup."""
    ...

def resolve_image_id(cursor, full_image_path):
    """
    Given a full filesystem path like /photos/2024/vacation/IMG_001.jpg,
    return the Images.id from the DigiKam database.

    Logic:
    1. Split path into directory and filename
    2. Find AlbumRoot where specificPath is a prefix of the directory
    3. Compute relativePath = directory - specificPath
    4. Find Album matching (albumRoot, relativePath)
    5. Find Image matching (album.id, filename)
    Returns: image_id (int) or None if not found
    """
    ...

def get_image_group_status(cursor, image_id):
    """Returns the current groupImage value for an image."""
    ...

def get_existing_groups(cursor):
    """
    Returns dict mapping leader_id -> list of member_ids
    for all existing groups in the database.
    """
    ...

def create_group(cursor, leader_image_id, member_image_ids):
    """
    Creates a group by:
    1. Setting leader's groupImage = leader_image_id (self-reference)
    2. Setting each member's groupImage = leader_image_id
    Must be called within a transaction.
    """
    ...

def get_images_in_group(cursor, leader_id):
    """Returns list of image IDs in the group led by leader_id."""
    ...
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

    Returns: UUID string or None if not found / file missing / parse error
    """
    try:
        tree = ET.parse(xmp_file_path)
        root = tree.getroot()

        # Register namespaces for proper parsing
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
    except FileNotFoundError:
        return None
```

### Step 2.3: Design Grouping Logic

**Objective**: Map MasterUUID values to DigiKam groups.

**Algorithm**:
1. Scan directory for image files (`.jpg`, `.png`, `.tif`, `.raw`, etc.)
2. For each image, locate corresponding `.xmp` sidecar
3. Parse XMP and extract `aplib:MasterUUID`
4. Build dictionary: `{master_uuid: [image_path1, image_path2, ...]}`
5. For each UUID with 2+ images:
   - Resolve each image path to a DigiKam `Images.id` via `resolve_image_id()`
   - Skip any images not found in the database (log warning)
   - Check if images are already in a group (check `groupImage` column)
   - If any image is already in a group, log warning and skip (do not modify existing groups)
   - Choose a leader (first image in the list, or the oldest by modification date)
   - Call `create_group(leader_id, member_ids)` within a transaction

**Leader Selection Strategy**:
- Prefer the image with the largest file size (likely the master/original)
- Or prefer the image with the earliest modification date
- Or simply use the first image alphabetically
- Document the chosen strategy in the code

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
- Handle missing files, malformed XML, and missing attributes gracefully (return `None`)
- Include the concrete parsing code shown in Step 2.2

**File: `src/database.py`**
- Database connection management (single connection, not pooled)
- Transaction handling (explicit `start_transaction()`, `commit()`, `rollback()`)
- **Path resolution**: `resolve_image_id()` using the 3-table join (`AlbumRoots` → `Albums` → `Images`)
- **Group operations**: `create_group()`, `get_image_group_status()`, `get_existing_groups()`
- **Backup**: Use `subprocess.run()` to call `mysqldump` (not `SELECT INTO OUTFILE` — different permissions and output location)

**File: `src/grouper.py`**
- Main grouping logic orchestrating XMP parsing and database operations
- Batch processing with progress reporting (print progress every N images)
- Leader selection strategy (document and implement consistently)

**File: `src/config.py`**
- Configuration from environment variables
- Logging setup (both file and stdout handlers)

**File: `src/cli.py`**
- Command-line interface with `argparse`
- `--dry-run` flag: parse XMP, resolve image IDs, show planned groups, but do NOT write to database
- `--backup-only` flag: create database backup and exit
- `--input-dir` argument: directory to scan for images
- `--verbose` flag: show detailed progress

### Step 3.3: Safety Features Implementation

**Required Safety Mechanisms**:

1. **Database Backup** (use `mysqldump` via subprocess, not `SELECT INTO OUTFILE`):
   ```python
   import subprocess
   from datetime import datetime

   def backup_database(db_config, backup_dir):
       timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
       backup_file = f"{backup_dir}/digikam_backup_{timestamp}.sql"
       cmd = [
           "mysqldump",
           "-h", db_config['host'],
           "-P", str(db_config['port']),
           "-u", db_config['user'],
           f"--password={db_config['password']}",
           db_config['database']
       ]
       with open(backup_file, 'w') as f:
           subprocess.run(cmd, stdout=f, check=True)
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

4. **Transaction Safety**:
   ```python
   def create_group_safe(conn, leader_image_id, member_image_ids):
       try:
           conn.start_transaction()
           cursor = conn.cursor()
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
           conn.commit()
       except Exception as e:
           conn.rollback()
           raise
       finally:
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

**File: `tests/test_database.py`**
- Mock database connections using `unittest.mock`
- Test `resolve_image_id()` with valid path, invalid path, edge cases
- Test `create_group_safe()` transaction commit on success
- Test `create_group_safe()` rollback on error (e.g., member already in group)
- Test `get_existing_groups()` returns correct structure
- Test backup function calls `mysqldump` with correct arguments (mock subprocess)

**File: `tests/test_grouper.py`**
- Test grouping algorithm with sample data (mock XMP parser and database)
- Test that images already in groups are skipped
- Test that images not in database are skipped with warning
- Test leader selection logic
- Test dry-run mode produces no database writes

### Step 4.2: Integration Tests

**Test Environment Setup**:
1. Create local MySQL test database
2. Import DigiKam schema for `AlbumRoots`, `Albums`, and `Images` tables (you need all three for path resolution — not just "ImageGroups tables")
3. Insert test data:
   - 1 AlbumRoot with `specificPath = '/test/photos'`
   - 2 Albums under that root with different `relativePath` values
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
- [ ] Dry-run on production data shows expected groups
- [ ] Database backup created successfully
- [ ] All existing groups remain intact
- [ ] No orphaned group records created
- [ ] Performance test: 1000 images processed in < 5 minutes

---

## Phase 5: Deployment & Documentation

### Step 5.1: Create Configuration Template

**File: `.env.example`**:
```bash
# Database Configuration
DIGIKAM_DB_HOST=mysql.example.com
DIGIKAM_DB_PORT=3306
DIGIKAM_DB_NAME=digikam
DIGIKAM_DB_USER=digikam_grouping
DIGIKAM_DB_PASSWORD=secure_password

# Paths
BACKUP_DIR=/path/to/backups
LOG_FILE=/var/log/digikam-grouping.log

# Safety
DRY_RUN=false
BATCH_SIZE=100
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

# Load environment
source .env

# Create backup
python src/cli.py --backup-only

# Dry run first
python src/cli.py --dry-run --input-dir "$1"

# Confirm with user
read -p "Proceed with actual grouping? (yes/no): " confirm
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

3. **Create backup**:
   ```bash
   mysqldump -h $DIGIKAM_DB_HOST -u $DIGIKAM_DB_USER -p $DIGIKAM_DB_NAME > backup_$(date +%Y%m%d).sql
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
- [ ] `src/database.py` - Database operations module
- [ ] `src/grouper.py` - Main grouping logic
- [ ] `src/config.py` - Configuration management
- [ ] `src/cli.py` - Command-line interface
- [ ] `tests/` - Comprehensive test suite
- [ ] `README.md` - User documentation
- [ ] `OPERATIONS.md` - Operational procedures
- [ ] `requirements.txt` - Python dependencies
- [ ] `run_grouping.sh` - Execution script
- [ ] `.env.example` - Configuration template

---

## Risk Mitigation

| Risk | Mitigation |
|------|-----------|
| Database corruption | Transaction wrapping with rollback on any error, `mysqldump` backup before execution, dry-run mode |
| Performance issues | Batch processing, single connection (no pooling overhead), progress reporting |
| Invalid XMP data | Graceful error handling in parser (return `None`), skip bad files with warning |
| Concurrent modifications | Check `groupImage = -1` before updating (atomic conditional UPDATE), off-peak execution |
| Lost database connection | Transaction rollback on connection error, idempotent operations (re-running won't create duplicate groups if `groupImage = -1` check is used) |
| Images already in groups | Conditional UPDATE (`WHERE groupImage = -1`) prevents overwriting existing groups; raises error if conflict detected |
| Path resolution failures | Log warning for each unresolvable path, continue processing remaining images |

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
5. **If the schema verification in Step 1.1 reveals different column names or semantics**, update all SQL queries accordingly before proceeding
6. **Path separator handling**: Use `os.path` functions for cross-platform compatibility, but be aware DigiKam on Linux stores paths with `/` separators
