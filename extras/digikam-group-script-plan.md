# DigiKam MasterUUID Grouping Utility - Implementation Plan

## Executive Summary

Create a Python utility that groups DigiKam images based on a custom `aplib:MasterUUID` field stored in XMP sidecar files. The utility must safely operate against a production MySQL database shared by multiple applications, handling hundreds to thousands of images efficiently.

## Background & Context

### Data Structure
- **XMP Namespace**: `http://github.com/Jachimo/aplib-extractor/aplib/1.0/`
- **Grouping Field**: `aplib:MasterUUID` (e.g., `uhoq+7KsSHe+bTeFEvnKZg`)
- **Relationship**: Images sharing the same MasterUUID value are versions of the same master image
- **Sidecar Format**: Files named `{imagename}.xmp` in same directory as images

### Database Environment
- **Type**: Remote MySQL server (production, shared with other applications)
- **Safety Requirement**: Must not corrupt existing data; enterprise-grade precautions required
- **Scale**: Hundreds to low-thousands of images per batch operation

### Key Finding
DigiKam stores image groups in `ImageGroups` and `ImageGroupProperties` tables, but the `digikam-db` Python library does NOT expose group manipulation APIs. Direct SQL is required.

---

## Phase 1: Discovery & Schema Extraction

### Step 1.1: Obtain DigiKam Database Schema

**Objective**: Get exact CREATE TABLE statements for ImageGroups tables.

**Actions**:
1. Clone DigiKam source repository:
   ```bash
   git clone https://invent.kde.org/graphics/digikam.git
   cd digikam
   ```

2. Locate and examine database schema file:
   ```bash
   find . -name "dbconfig.xml.cmake.in" -o -name "dbconfig.xml"
   ```

3. Extract SQL for these tables:
   - `ImageGroups`
   - `ImageGroupProperties`
   - Any related indexes, triggers, or foreign keys

4. Document:
   - Column names and types
   - Primary keys and auto-increment fields
   - Foreign key relationships
   - Required vs optional fields

**Deliverable**: Exact SQL CREATE TABLE statements and table relationship diagram.

### Step 1.2: Analyze Existing Group Data (if any)

**Objective**: Understand how DigiKam currently stores groups.

**Actions**:
1. Query existing groups in production database (read-only):
   ```sql
   SELECT * FROM ImageGroups LIMIT 5;
   SELECT * FROM ImageGroupProperties LIMIT 5;
   ```

2. Document:
   - How group IDs are assigned
   - How images are linked to groups
   - What properties are stored per group

**Deliverable**: Sample data showing existing group structure.

---

## Phase 2: Design & Architecture

### Step 2.1: Design Database Interaction Layer

**Objective**: Create safe, transactional database operations.

**Design Decisions**:
- Use `mysql-connector-python` (not SQLAlchemy for clarity)
- All operations wrapped in transactions
- Read-only validation before any writes
- Connection pooling for efficiency

**Required Functions**:
```python
def create_database_backup(connection, backup_path): ...
def get_image_id_by_path(connection, image_path): ...
def get_existing_groups(connection): ...
def create_group(connection, leader_image_id): ...
def add_image_to_group(connection, group_id, image_id): ...
def get_images_in_group(connection, group_id): ...
```

### Step 2.2: Design XMP Parsing Layer

**Objective**: Extract MasterUUID from sidecar files efficiently.

**Design Decisions**:
- Use `xml.etree.ElementTree` (standard library, no dependencies)
- Cache parsed results to avoid re-parsing
- Handle missing/invalid XMP gracefully

**Namespace Mapping**:
```python
NAMESPACES = {
    'aplib': 'http://github.com/Jachimo/aplib-extractor/aplib/1.0/',
    'xmp': 'http://ns.adobe.com/xap/1.0/',
    # ... other standard namespaces
}
```

### Step 2.3: Design Grouping Logic

**Objective**: Map MasterUUID values to DigiKam groups.

**Algorithm**:
1. Scan directory for image files
2. For each image, locate corresponding `.xmp` sidecar
3. Parse XMP and extract `aplib:MasterUUID`
4. Build dictionary: `{master_uuid: [image_path1, image_path2, ...]}`
5. For each UUID with 2+ images:
   - Verify all images exist in DigiKam database
   - Check if group already exists
   - Create new group if needed
   - Add images to group

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
- Parse XMP sidecar files
- Extract MasterUUID
- Handle errors gracefully

**File: `src/database.py`**
- Database connection management
- Transaction handling
- Group CRUD operations
- Backup functionality

**File: `src/grouper.py`**
- Main grouping logic
- Orchestrate XMP parsing and database operations
- Batch processing with progress reporting

**File: `src/config.py`**
- Configuration management
- Database credentials (from environment variables)
- Logging setup

**File: `src/cli.py`**
- Command-line interface
- Argument parsing
- Dry-run mode support

### Step 3.3: Safety Features Implementation

**Required Safety Mechanisms**:

1. **Database Backup**:
   ```python
   def backup_database(conn, backup_dir):
       timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
       backup_file = f"{backup_dir}/digikam_backup_{timestamp}.sql"
       # Use mysqldump or SELECT INTO OUTFILE
   ```

2. **Dry-Run Mode**:
   - Parse all XMP files
   - Show what groups would be created
   - No database modifications

3. **Validation Layer**:
   - Verify image exists in DigiKam database before grouping
   - Check for existing groups to avoid duplicates
   - Validate XMP structure before processing

4. **Transaction Safety**:
   ```python
   def create_group_safe(conn, image_ids):
       try:
           conn.start_transaction()
           # ... create group ...
           conn.commit()
       except Exception as e:
           conn.rollback()
           raise
   ```

5. **Audit Logging**:
   - Log every operation with timestamp
   - Record before/after states
   - Write to file and stdout

---

## Phase 4: Testing Strategy

### Step 4.1: Unit Tests

**File: `tests/test_xmp_parser.py`**
- Test parsing valid XMP with MasterUUID
- Test handling missing XMP files
- Test malformed XML handling
- Test namespace resolution

**File: `tests/test_database.py`**
- Mock database connections
- Test transaction rollback on error
- Test backup functionality
- Test group creation logic

**File: `tests/test_grouper.py`**
- Test grouping algorithm with sample data
- Test batch processing
- Test progress reporting

### Step 4.2: Integration Tests

**Test Environment Setup**:
1. Create local MySQL test database
2. Import minimal DigiKam schema (ImageGroups tables only)
3. Create test images with XMP sidecars

**Test Scenarios**:
1. **Happy Path**: 3 images with same MasterUUID → 1 group created
2. **No Groups**: All images have unique MasterUUIDs → no groups created
3. **Mixed**: Some images grouped, some not → partial grouping
4. **Existing Groups**: Images already in groups → skip or update appropriately
5. **Missing XMP**: Image without sidecar → log warning, skip
6. **Database Error**: Connection lost mid-operation → rollback, no corruption

### Step 4.3: Production Validation

**Pre-Deployment Checklist**:
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
   SELECT g.id, g.leader_id, COUNT(ig.imageid) as member_count
   FROM ImageGroups g
   JOIN ImageGroupProperties ig ON g.id = ig.groupid
   GROUP BY g.id;
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
| Database corruption | Transaction wrapping, backups, dry-run mode |
| Performance issues | Batch processing, connection pooling |
| Invalid XMP data | Schema validation, error handling, skip bad files |
| Concurrent modifications | Read locks, off-peak execution recommendation |
| Lost database connection | Retry logic, idempotent operations |

---

## Success Criteria

1. All images with matching `aplib:MasterUUID` are grouped in DigiKam
2. No existing data is corrupted or lost
3. Execution completes in under 5 minutes for 1000 images
4. Zero errors in production execution
5. Groups are visible and functional in DigiKam UI

---

## References

- DigiKam Database Schema: `core/data/database/dbconfig.xml.cmake.in` in source
- XMP Specification: Adobe XMP SDK documentation
- MySQL Python Connector: https://dev.mysql.com/doc/connector-python/en/
