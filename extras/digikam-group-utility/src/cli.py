import argparse
import logging
import os

from config import load_config, setup_logging
from database import connect, create_database_backup, test_connection
from grouper import build_master_uuid_map, execute_group_plan, plan_groups_by_master_uuid

logger = logging.getLogger(__name__)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Group DigiKam images by shared XMP aplib:MasterUUID"
    )
    parser.add_argument(
        "export_dir",
        nargs="?",
        help="Path to exported images and XMP sidecars (not needed with --backup-only)",
    )
    parser.add_argument("--dry-run", action="store_true", help="Plan groups without writing DB")
    parser.add_argument("--backup", action="store_true", help="Create DB backup before changes")
    parser.add_argument("--backup-only", action="store_true", help="Create backup and exit")
    parser.add_argument("--verbose", action="store_true", help="Enable debug logging")
    parser.add_argument("--db-host", help="Database host (overrides DIGIKAM_DB_HOST)")
    parser.add_argument("--db-port", type=int, help="Database port (overrides DIGIKAM_DB_PORT)")
    parser.add_argument("--db-name", help="Database name (overrides DIGIKAM_DB_NAME)")
    parser.add_argument("--db-user", help="Database user (overrides DIGIKAM_DB_USER)")
    parser.add_argument("--db-password", help="Database password (overrides DIGIKAM_DB_PASSWORD)")
    args = parser.parse_args()

    if not args.backup_only and not args.export_dir:
        parser.error("export_dir is required unless --backup-only is used")

    return args


def main() -> int:
    args = parse_args()

    # Allow one-shot invocation without exporting env vars first.
    if args.db_host:
        os.environ["DIGIKAM_DB_HOST"] = args.db_host
    if args.db_port is not None:
        os.environ["DIGIKAM_DB_PORT"] = str(args.db_port)
    if args.db_name:
        os.environ["DIGIKAM_DB_NAME"] = args.db_name
    if args.db_user:
        os.environ["DIGIKAM_DB_USER"] = args.db_user
    if args.db_password:
        os.environ["DIGIKAM_DB_PASSWORD"] = args.db_password

    config = load_config()

    os.makedirs(config.backup_dir, exist_ok=True)
    setup_logging(args.verbose, config.log_file)

    logger.info("Testing DB connectivity")
    test_connection(config.db_config)

    if args.backup or args.backup_only:
        backup_file = create_database_backup(config.db_config, config.backup_dir)
        logger.info("Backup written: %s", backup_file)

    if args.backup_only:
        return 0

    dry_run = args.dry_run or config.dry_run
    logger.info("Dry run: %s", dry_run)

    if not os.path.isdir(args.export_dir):
        raise RuntimeError(f"Export directory does not exist or is not a directory: {args.export_dir}")

    master_uuid_map, scan_stats = build_master_uuid_map(args.export_dir)
    logger.info(
        "Scanned=%d Sidecars=%d MasterUUIDPaths=%d UniqueMasterUUID=%d",
        scan_stats.images_scanned,
        scan_stats.sidecars_parsed,
        scan_stats.paths_with_master_uuid,
        len(master_uuid_map),
    )

    conn = connect(config.db_config)
    try:
        plan, plan_stats = plan_groups_by_master_uuid(conn, master_uuid_map)
        logger.info("Groups planned=%d", plan_stats.groups_planned)

        exec_stats = execute_group_plan(conn, plan, dry_run=dry_run)
        logger.info(
            "Groups created=%d skipped=%d",
            exec_stats.groups_created,
            exec_stats.groups_skipped,
        )
    finally:
        conn.close()

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
