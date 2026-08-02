import logging
import os
from dataclasses import dataclass
from typing import Any, Dict


@dataclass(frozen=True)
class AppConfig:
    db_config: Dict[str, Any]
    backup_dir: str
    log_file: str
    dry_run: bool


def _env_bool(name: str, default: bool = False) -> bool:
    value = os.environ.get(name)
    if value is None:
        return default
    return value.strip().lower() in {"1", "true", "yes", "on"}


def get_db_config_from_env() -> Dict[str, Any]:
    required = [
        "DIGIKAM_DB_HOST",
        "DIGIKAM_DB_NAME",
        "DIGIKAM_DB_USER",
        "DIGIKAM_DB_PASSWORD",
    ]
    missing = [name for name in required if not os.environ.get(name)]
    if missing:
        raise RuntimeError(
            "Missing required environment variables: "
            + ", ".join(missing)
            + ". Set them before running, or pass --db-host/--db-name/--db-user/--db-password flags."
        )

    return {
        "host": os.environ["DIGIKAM_DB_HOST"],
        "port": int(os.environ.get("DIGIKAM_DB_PORT", "3306")),
        "database": os.environ["DIGIKAM_DB_NAME"],
        "user": os.environ["DIGIKAM_DB_USER"],
        "password": os.environ["DIGIKAM_DB_PASSWORD"],
    }


def load_config() -> AppConfig:
    return AppConfig(
        db_config=get_db_config_from_env(),
        backup_dir=os.environ.get("BACKUP_DIR", "./backups"),
        log_file=os.environ.get("LOG_FILE", "./digikam-grouping.log"),
        dry_run=_env_bool("DRY_RUN", False),
    )


def setup_logging(verbose: bool, log_file: str | None = None) -> None:
    handlers: list[logging.Handler] = [logging.StreamHandler()]
    if log_file:
        handlers.append(logging.FileHandler(log_file))

    level = logging.DEBUG if verbose else logging.INFO
    logging.basicConfig(
        level=level,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
        handlers=handlers,
    )
