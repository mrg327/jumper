from dataclasses import dataclass, field
from datetime import date
from typing import Optional
import re

import frontmatter


@dataclass
class JournalEntry:
    time: str  # "09:15"
    entry_type: str  # "Started" | "Switched" | "Note" | "Done"
    project: str  # project name (or "" for Done entries)
    details: dict = field(default_factory=dict)


@dataclass
class DailyJournal:
    date: date
    entries: list[JournalEntry] = field(default_factory=list)

    def append_entry(self, entry: JournalEntry) -> None:
        """Append a journal entry to this day's journal."""
        self.entries.append(entry)

    def to_markdown(self) -> str:
        """Serialize to markdown with YAML frontmatter."""
        meta = {"date": self.date.isoformat()}

        sections: list[str] = []
        for entry in self.entries:
            # Build header: ## 09:15 — Started: HMI Framework
            if entry.entry_type == "Done":
                header = f"## {entry.time} \u2014 Done for day"
            elif entry.entry_type == "Switched" and "\u2009\u2192\u2009" in entry.project:
                # Handle arrow with thin spaces (unlikely from input)
                header = f"## {entry.time} \u2014 Switched: {entry.project}"
            else:
                if entry.entry_type == "Done":
                    header = f"## {entry.time} \u2014 Done for day"
                else:
                    header = f"## {entry.time} \u2014 {entry.entry_type}: {entry.project}"

            lines = [header]

            # Add detail lines in a stable key order
            preferred_order = [
                "focus",
                "left_off",
                "blocker",
                "next_step",
                "decision",
                "active",
            ]
            detail_keys = list(entry.details.keys())
            ordered_keys = [k for k in preferred_order if k in detail_keys]
            ordered_keys += [k for k in detail_keys if k not in preferred_order]

            for key in ordered_keys:
                value = entry.details[key]
                display_key = _display_key(key)
                lines.append(f"{display_key}: {value}")

            sections.append("\n".join(lines))

        body = "\n\n".join(sections)
        post = frontmatter.Post(body, **meta)
        return frontmatter.dumps(post)

    @classmethod
    def from_markdown(cls, text: str) -> "DailyJournal":
        """Parse from markdown with YAML frontmatter."""
        post = frontmatter.loads(text)
        meta = post.metadata
        body = post.content

        # Parse date
        journal_date = meta.get("date", date.today())
        if isinstance(journal_date, str):
            journal_date = date.fromisoformat(journal_date)

        entries: list[JournalEntry] = []

        current_header: Optional[str] = None
        current_lines: list[str] = []

        for line in body.split("\n"):
            if line.startswith("## "):
                if current_header is not None:
                    entry = _parse_journal_entry(current_header, current_lines)
                    if entry:
                        entries.append(entry)
                current_header = line[3:].strip()
                current_lines = []
            else:
                current_lines.append(line)

        # Process last entry
        if current_header is not None:
            entry = _parse_journal_entry(current_header, current_lines)
            if entry:
                entries.append(entry)

        return cls(date=journal_date, entries=entries)


def _display_key(key: str) -> str:
    """Convert internal key to display format."""
    special = {
        "left_off": "Left off",
        "next_step": "Next step",
    }
    if key in special:
        return special[key]
    return key.replace("_", " ").title()


def _normalize_key(raw: str) -> str:
    """Convert display key back to internal format."""
    return raw.strip().lower().replace(" ", "_")


def _parse_journal_entry(
    header: str, detail_lines: list[str]
) -> Optional[JournalEntry]:
    """Parse a journal entry from its header and detail lines."""
    # Header formats:
    #   09:15 — Started: HMI Framework
    #   11:30 — Switched: HMI Framework → Test Infra
    #   14:00 — Note: Test Infra
    #   16:30 — Done for day

    header_match = re.match(r"^(\d{2}:\d{2})\s*[\u2014\u2013\-]+\s*(.*)", header)
    if not header_match:
        return None

    time = header_match.group(1)
    rest = header_match.group(2).strip()

    if rest.lower().startswith("done"):
        entry_type = "Done"
        project = ""
    else:
        type_match = re.match(r"^(\w+):\s*(.*)", rest)
        if not type_match:
            return None
        entry_type = type_match.group(1)
        project = type_match.group(2).strip()

    # Parse detail lines as key: value pairs
    details: dict = {}
    for line in detail_lines:
        stripped = line.strip()
        if not stripped:
            continue
        kv_match = re.match(r"^([A-Za-z][A-Za-z _]*?):\s*(.*)", stripped)
        if kv_match:
            raw_key = kv_match.group(1).strip()
            value = kv_match.group(2).strip()
            key = _normalize_key(raw_key)
            details[key] = value

    return JournalEntry(
        time=time, entry_type=entry_type, project=project, details=details
    )
