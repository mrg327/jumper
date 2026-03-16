from pathlib import Path

import yaml

DEFAULT_CONFIG = {
    "data_dir": "~/.jm",
    "statuses": ["active", "blocked", "parked", "done"],
    "priorities": ["high", "medium", "low"],
    "categories": ["feature", "bug", "meeting", "research", "decision"],
    "editor": "$EDITOR",
    "export_path": "~/.jm/screen.txt",
    "plugins": {
        "enabled": ["pomodoro", "notifications", "clock"],
        "notifications": {
            "reminders": [],
        },
        "pomodoro": {
            "work_minutes": 25,
            "short_break_minutes": 5,
            "long_break_minutes": 15,
            "sessions_before_long": 4,
        },
    },
}


def get_data_dir(config: dict | None = None) -> Path:
    cfg = config or load_config()
    return Path(cfg["data_dir"]).expanduser()


def load_config() -> dict:
    config_path = Path("~/.jm/config.yaml").expanduser()
    config = DEFAULT_CONFIG.copy()
    if config_path.exists():
        with open(config_path) as f:
            user_config = yaml.safe_load(f) or {}
            config.update(user_config)
    return config


def ensure_dirs(config: dict | None = None) -> Path:
    data_dir = get_data_dir(config)
    (data_dir / "projects").mkdir(parents=True, exist_ok=True)
    (data_dir / "journal").mkdir(parents=True, exist_ok=True)
    return data_dir
