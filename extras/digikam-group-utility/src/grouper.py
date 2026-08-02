import logging
import os
import time
from dataclasses import dataclass
from pathlib import Path

from database import create_group_safe, resolve_image_id_with_reason
from xmp_parser import extract_candidate_filenames, extract_master_uuid

logger = logging.getLogger(__name__)

SUPPORTED_IMAGE_SUFFIXES = {
    ".jpg",
    ".jpeg",
    ".png",
    ".tif",
    ".tiff",
    ".heic",
    ".heif",
    ".dng",
    ".cr2",
    ".cr3",
    ".nef",
    ".arw",
}

SCAN_PROGRESS_EVERY = 2000
SCAN_PROGRESS_MIN_INTERVAL_SECS = 2.0
PLAN_PROGRESS_EVERY = 250
UNRESOLVED_WARNING_LIMIT = 60


@dataclass
class GroupingStats:
    images_scanned: int = 0
    sidecars_parsed: int = 0
    paths_with_master_uuid: int = 0
    db_resolved_images: int = 0
    groups_planned: int = 0
    groups_created: int = 0
    groups_skipped: int = 0


def _possible_sidecars(image_path: Path) -> list[Path]:
    return [
        Path(str(image_path) + ".xmp"),
        image_path.with_suffix(".xmp"),
    ]


def build_master_uuid_map(export_dir: str) -> tuple[dict[str, list[str]], GroupingStats]:
    stats = GroupingStats()
    mapping: dict[str, list[str]] = {}
    started = time.monotonic()
    last_logged_at = started
    last_logged_files = 0
    last_logged_sidecars = 0

    base = Path(export_dir)
    for path in base.rglob("*"):
        if not path.is_file():
            continue

        stats.images_scanned += 1
        now = time.monotonic()
        if (
            stats.images_scanned % SCAN_PROGRESS_EVERY == 0
            and (now - last_logged_at) >= SCAN_PROGRESS_MIN_INTERVAL_SECS
        ):
            elapsed = max(now - started, 1e-6)
            avg_file_rate = stats.images_scanned / elapsed
            avg_sidecar_rate = stats.sidecars_parsed / elapsed

            window_elapsed = max(now - last_logged_at, 1e-6)
            window_files = stats.images_scanned - last_logged_files
            window_sidecars = stats.sidecars_parsed - last_logged_sidecars
            window_file_rate = window_files / window_elapsed
            window_sidecar_rate = window_sidecars / window_elapsed

            logger.info(
                "Scan progress: files=%d sidecars=%d uuid_paths=%d unique_master_uuid=%d avg_files=%.1f/s avg_sidecars=%.1f/s window_files=%.1f/s window_sidecars=%.1f/s elapsed=%.1fs window=%.1fs",
                stats.images_scanned,
                stats.sidecars_parsed,
                stats.paths_with_master_uuid,
                len(mapping),
                avg_file_rate,
                avg_sidecar_rate,
                window_file_rate,
                window_sidecar_rate,
                elapsed,
                window_elapsed,
            )

            last_logged_at = now
            last_logged_files = stats.images_scanned
            last_logged_sidecars = stats.sidecars_parsed

        if path.suffix.lower() not in SUPPORTED_IMAGE_SUFFIXES:
            continue

        sidecar = None
        for candidate in _possible_sidecars(path):
            if candidate.exists():
                sidecar = candidate
                break

        if sidecar is None:
            logger.debug("Missing sidecar for %s", path)
            continue

        stats.sidecars_parsed += 1
        master_uuid = extract_master_uuid(str(sidecar))
        if not master_uuid:
            continue

        stats.paths_with_master_uuid += 1
        mapping.setdefault(master_uuid, []).append(str(path))

    elapsed = max(time.monotonic() - started, 1e-6)
    logger.info(
        "Scan complete: files=%d sidecars=%d uuid_paths=%d unique_master_uuid=%d rate_files=%.1f/s rate_sidecars=%.1f/s elapsed=%.1fs",
        stats.images_scanned,
        stats.sidecars_parsed,
        stats.paths_with_master_uuid,
        len(mapping),
        stats.images_scanned / elapsed,
        stats.sidecars_parsed / elapsed,
        elapsed,
    )
    return mapping, stats


def plan_groups_by_master_uuid(
    conn,
    master_uuid_map: dict[str, list[str]],
) -> tuple[list[tuple[int, list[int], str]], GroupingStats]:
    """Build group operations as (leader_id, member_ids, master_uuid)."""
    stats = GroupingStats()
    planned: list[tuple[int, list[int], str]] = []
    unresolved_paths = 0
    unresolved_warnings_shown = 0
    unresolved_by_reason: dict[str, int] = {}
    sidecar_name_rescues = 0
    sidecar_name_cache: dict[str, list[str]] = {}

    cursor = conn.cursor()
    try:
        processed_master_uuid = 0
        total_master_uuid = len(master_uuid_map)

        for master_uuid, image_paths in master_uuid_map.items():
            processed_master_uuid += 1
            if (
                processed_master_uuid % PLAN_PROGRESS_EVERY == 0
                or processed_master_uuid == total_master_uuid
            ):
                logger.info(
                    "Plan progress: master_uuid=%d/%d planned_groups=%d resolved_images=%d",
                    processed_master_uuid,
                    total_master_uuid,
                    len(planned),
                    stats.db_resolved_images,
                )

            if len(image_paths) < 2:
                continue

            resolved: list[tuple[int, str]] = []
            for image_path in image_paths:
                image_id, reason = resolve_image_id_with_reason(cursor, image_path)
                if image_id is None and reason == "no_name_match":
                    sidecar_candidates = sidecar_name_cache.get(image_path)
                    if sidecar_candidates is None:
                        sidecar_candidates = []
                        for candidate in _possible_sidecars(Path(image_path)):
                            if candidate.exists():
                                names = extract_candidate_filenames(str(candidate))
                                if names:
                                    sidecar_candidates = sorted(names)
                                break
                        sidecar_name_cache[image_path] = sidecar_candidates

                    if sidecar_candidates:
                        retry_image_id, retry_reason = resolve_image_id_with_reason(
                            cursor,
                            image_path,
                            alternate_filenames=sidecar_candidates,
                        )
                        if retry_image_id is not None:
                            image_id = retry_image_id
                            reason = retry_reason
                            sidecar_name_rescues += 1

                if image_id is None:
                    unresolved_paths += 1
                    unresolved_by_reason[reason] = unresolved_by_reason.get(reason, 0) + 1
                    if unresolved_warnings_shown < UNRESOLVED_WARNING_LIMIT:
                        unresolved_warnings_shown += 1
                        logger.warning("No DigiKam id for path (%s): %s", reason, image_path)
                    elif unresolved_warnings_shown == UNRESOLVED_WARNING_LIMIT:
                        unresolved_warnings_shown += 1
                        logger.warning(
                            "Additional unresolved-path warnings suppressed after %d entries",
                            UNRESOLVED_WARNING_LIMIT,
                        )
                    continue
                stats.db_resolved_images += 1
                resolved.append((image_id, image_path))

            if len(resolved) < 2:
                continue

            # Use largest file as leader for deterministic behavior.
            resolved.sort(key=lambda t: os.path.getsize(t[1]), reverse=True)
            deduped_resolved: list[tuple[int, str]] = []
            seen_image_ids: set[int] = set()
            for image_id, resolved_path in resolved:
                if image_id in seen_image_ids:
                    continue
                seen_image_ids.add(image_id)
                deduped_resolved.append((image_id, resolved_path))

            if len(deduped_resolved) < 2:
                continue

            leader_id = deduped_resolved[0][0]
            member_ids = [image_id for image_id, _ in deduped_resolved[1:]]
            planned.append((leader_id, member_ids, master_uuid))

        stats.groups_planned = len(planned)
        logger.info(
            "Plan complete: master_uuid=%d planned_groups=%d resolved_images=%d unresolved_paths=%d",
            total_master_uuid,
            stats.groups_planned,
            stats.db_resolved_images,
            unresolved_paths,
        )
        if unresolved_by_reason:
            reason_summary = ", ".join(
                f"{key}={value}" for key, value in sorted(unresolved_by_reason.items())
            )
            logger.info("Unresolved path reasons: %s", reason_summary)
        logger.info("Resolved via sidecar filename fallback: %d", sidecar_name_rescues)
        if total_master_uuid > 0 and stats.db_resolved_images == 0:
            logger.warning(
                "Resolved 0 images from %d MasterUUID groups. This usually means exported file paths do not map to DigiKam album roots in the current DB. "
                "Check AlbumRoots.identifier mountpath values and ensure this export tree matches imported DigiKam paths.",
                total_master_uuid,
            )
        return planned, stats
    finally:
        cursor.close()


def execute_group_plan(
    conn,
    plan: list[tuple[int, list[int], str]],
    dry_run: bool,
) -> GroupingStats:
    stats = GroupingStats(groups_planned=len(plan))
    total = len(plan)

    for index, (leader_id, member_ids, master_uuid) in enumerate(plan, start=1):
        if index == 1 or index % PLAN_PROGRESS_EVERY == 0 or index == total:
            logger.info(
                "Execute progress: group=%d/%d created=%d skipped=%d dry_run=%s",
                index,
                total,
                stats.groups_created,
                stats.groups_skipped,
                dry_run,
            )

        logger.info(
            "Grouping master_uuid=%s leader=%s members=%s",
            master_uuid,
            leader_id,
            member_ids,
        )

        if dry_run:
            continue

        try:
            create_group_safe(conn, leader_id, member_ids)
            stats.groups_created += 1
        except Exception as exc:
            stats.groups_skipped += 1
            logger.warning(
                "Failed group master_uuid=%s leader=%s error=%s",
                master_uuid,
                leader_id,
                exc,
            )

    return stats
