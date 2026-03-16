from dataclasses import dataclass, field
from datetime import date
from typing import Optional
import re

import frontmatter


@dataclass
class Blocker:
    description: str
    resolved: bool = False
    since: Optional[date] = None
    resolved_date: Optional[date] = None
    person: Optional[str] = None  # @mention


@dataclass
class Decision:
    date: date
    choice: str
    alternatives: list[str] = field(default_factory=list)


@dataclass
class LogEntry:
    date: date
    lines: list[str] = field(default_factory=list)


@dataclass
class Project:
    name: str
    slug: str = ""
    status: str = "active"  # active | blocked | parked | done
    priority: str = "medium"  # high | medium | low
    tags: list[str] = field(default_factory=list)
    created: date = field(default_factory=date.today)
    target: Optional[date] = None
    current_focus: str = ""
    blockers: list[Blocker] = field(default_factory=list)
    decisions: list[Decision] = field(default_factory=list)
    log: list[LogEntry] = field(default_factory=list)

    def __post_init__(self) -> None:
        if not self.slug:
            self.slug = self.name.lower().replace(" ", "-")

    def to_markdown(self) -> str:
        """Serialize to markdown with YAML frontmatter."""
        meta: dict = {
            "name": self.name,
            "status": self.status,
            "priority": self.priority,
            "tags": self.tags,
            "created": self.created.isoformat(),
        }
        if self.target:
            meta["target"] = self.target.isoformat()

        sections: list[str] = []

        if self.current_focus:
            sections.append(f"## Current Focus\n{self.current_focus}")

        if self.blockers:
            lines = ["## Blockers"]
            for b in self.blockers:
                check = "x" if b.resolved else " "
                text = b.description
                if b.person:
                    text += f" @{b.person.lstrip('@')}"
                if b.resolved:
                    text = f"~~{text}~~"
                    if b.resolved_date:
                        text += f" (resolved {b.resolved_date.isoformat()})"
                else:
                    if b.since:
                        text += f" (since {b.since.isoformat()})"
                lines.append(f"- [{check}] {text}")
            sections.append("\n".join(lines))

        if self.decisions:
            lines = ["## Decisions"]
            for d in self.decisions:
                lines.append(f"- **{d.date.isoformat()}:** {d.choice}")
                if d.alternatives:
                    lines.append(f"  - Alternatives: {', '.join(d.alternatives)}")
            sections.append("\n".join(lines))

        if self.log:
            lines = ["## Log"]
            for entry in self.log:
                lines.append(f"### {entry.date.isoformat()}")
                for line in entry.lines:
                    lines.append(f"- {line}")
            sections.append("\n".join(lines))

        body = "\n\n".join(sections)
        post = frontmatter.Post(body, **meta)
        return frontmatter.dumps(post)

    @classmethod
    def from_markdown(cls, text: str) -> "Project":
        """Parse from markdown with YAML frontmatter."""
        post = frontmatter.loads(text)
        meta = post.metadata
        body = post.content

        # Parse dates
        created = meta.get("created", date.today())
        if isinstance(created, str):
            created = date.fromisoformat(created)
        elif isinstance(created, date):
            pass
        else:
            created = date.today()

        target = meta.get("target")
        if isinstance(target, str):
            target = date.fromisoformat(target)
        elif not isinstance(target, date):
            target = None

        # Parse body sections
        current_focus = ""
        blockers: list[Blocker] = []
        decisions: list[Decision] = []
        log: list[LogEntry] = []

        current_section: Optional[str] = None
        section_lines: list[str] = []

        for line in body.split("\n"):
            if line.startswith("## "):
                if current_section is not None:
                    current_focus, blockers, decisions, log = _process_section(
                        current_section,
                        section_lines,
                        current_focus,
                        blockers,
                        decisions,
                        log,
                    )
                current_section = line[3:].strip()
                section_lines = []
            else:
                section_lines.append(line)

        # Process last section
        if current_section is not None:
            current_focus, blockers, decisions, log = _process_section(
                current_section,
                section_lines,
                current_focus,
                blockers,
                decisions,
                log,
            )

        name = meta.get("name", "")
        return cls(
            name=name,
            slug=name.lower().replace(" ", "-"),
            status=meta.get("status", "active"),
            priority=meta.get("priority", "medium"),
            tags=meta.get("tags", []),
            created=created,
            target=target,
            current_focus=current_focus,
            blockers=blockers,
            decisions=decisions,
            log=log,
        )


def _process_section(
    section_name: str,
    lines: list[str],
    current_focus: str,
    blockers: list[Blocker],
    decisions: list[Decision],
    log: list[LogEntry],
) -> tuple[str, list[Blocker], list[Decision], list[LogEntry]]:
    """Parse a markdown section and update the relevant data structures."""

    if section_name == "Current Focus":
        focus_text = "\n".join(lines).strip()
        return focus_text, blockers, decisions, log

    elif section_name == "Blockers":
        for line in lines:
            line = line.strip()
            if not line:
                continue

            # Parse checkbox: - [ ] or - [x]
            checkbox_match = re.match(r"^-\s*\[([ xX])\]\s*(.*)", line)
            if not checkbox_match:
                continue

            resolved = checkbox_match.group(1).lower() == "x"
            text = checkbox_match.group(2)

            resolved_date: Optional[date] = None
            since: Optional[date] = None
            person: Optional[str] = None

            if resolved:
                # Extract resolved date from after strikethrough: (resolved YYYY-MM-DD)
                resolved_match = re.search(
                    r"\(resolved\s+(\d{4}-\d{2}-\d{2})\)", text
                )
                if resolved_match:
                    resolved_date = date.fromisoformat(resolved_match.group(1))
                    text = text[: resolved_match.start()].strip()

                # Strip strikethrough markers
                strike_match = re.match(r"^~~(.+?)~~$", text)
                if strike_match:
                    text = strike_match.group(1)
            else:
                # Extract since date: (since YYYY-MM-DD)
                since_match = re.search(r"\(since\s+(\d{4}-\d{2}-\d{2})\)", text)
                if since_match:
                    since = date.fromisoformat(since_match.group(1))
                    text = text[: since_match.start()].strip()

            # Extract @mention from text
            mention_match = re.search(r"@(\w+)", text)
            if mention_match:
                person = f"@{mention_match.group(1)}"
                # Remove the @mention from description
                text = text[: mention_match.start()].strip() + " " + text[mention_match.end() :].strip()
                text = text.strip()

            blockers.append(
                Blocker(
                    description=text,
                    resolved=resolved,
                    since=since,
                    resolved_date=resolved_date,
                    person=person,
                )
            )
        return current_focus, blockers, decisions, log

    elif section_name == "Decisions":
        i = 0
        while i < len(lines):
            line = lines[i].strip()
            if not line:
                i += 1
                continue

            # Parse decision: - **YYYY-MM-DD:** choice
            dec_match = re.match(
                r"^-\s*\*\*(\d{4}-\d{2}-\d{2}):\*\*\s*(.*)", line
            )
            if not dec_match:
                i += 1
                continue

            dec_date = date.fromisoformat(dec_match.group(1))
            choice = dec_match.group(2).strip()
            alternatives: list[str] = []

            # Check next line for alternatives
            if i + 1 < len(lines):
                alt_line = lines[i + 1].strip()
                alt_match = re.match(r"^-\s*Alternatives:\s*(.*)", alt_line)
                if alt_match:
                    alt_text = alt_match.group(1)
                    alternatives = [a.strip() for a in alt_text.split(",") if a.strip()]
                    i += 1

            decisions.append(
                Decision(date=dec_date, choice=choice, alternatives=alternatives)
            )
            i += 1
        return current_focus, blockers, decisions, log

    elif section_name == "Log":
        current_entry: Optional[LogEntry] = None
        for line in lines:
            # Check for ### date header
            header_match = re.match(r"^###\s+(\d{4}-\d{2}-\d{2})", line)
            if header_match:
                if current_entry is not None:
                    log.append(current_entry)
                entry_date = date.fromisoformat(header_match.group(1))
                current_entry = LogEntry(date=entry_date)
                continue

            if current_entry is not None:
                stripped = line.strip()
                if stripped.startswith("- "):
                    current_entry.lines.append(stripped[2:])

        if current_entry is not None:
            log.append(current_entry)
        return current_focus, blockers, decisions, log

    return current_focus, blockers, decisions, log
