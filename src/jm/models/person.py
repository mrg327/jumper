from dataclasses import dataclass, field
from datetime import date
from typing import Optional
import re

import frontmatter


@dataclass
class PendingItem:
    description: str
    since: Optional[date] = None
    project: Optional[str] = None


@dataclass
class Person:
    handle: str  # @carol
    role: str = ""
    projects: list[str] = field(default_factory=list)
    pending: list[PendingItem] = field(default_factory=list)


@dataclass
class PeopleFile:
    people: list[Person] = field(default_factory=list)

    def to_markdown(self) -> str:
        """Serialize to markdown (no frontmatter needed for people file)."""
        sections: list[str] = []
        for person in self.people:
            lines = [f"## {person.handle}"]
            if person.role:
                lines.append(f"- Role: {person.role}")
            if person.projects:
                lines.append(f"- Projects: {', '.join(person.projects)}")
            for item in person.pending:
                text = item.description
                if item.since:
                    text += f" (asked {item.since.isoformat()})"
                if item.project:
                    text += f" [{item.project}]"
                lines.append(f"- Pending: {text}")
            sections.append("\n".join(lines))
        return "\n\n".join(sections)

    @classmethod
    def from_markdown(cls, text: str) -> "PeopleFile":
        """Parse from markdown."""
        people: list[Person] = []
        current_person: Optional[Person] = None

        for line in text.split("\n"):
            # Check for person header: ## @handle
            header_match = re.match(r"^##\s+(@[\w-]+)", line)
            if header_match:
                if current_person is not None:
                    people.append(current_person)
                handle = header_match.group(1)
                current_person = Person(handle=handle)
                continue

            if current_person is None:
                continue

            stripped = line.strip()
            if not stripped:
                continue

            # Parse bullet items
            bullet_match = re.match(r"^-\s+(.*)", stripped)
            if not bullet_match:
                continue
            content = bullet_match.group(1)

            # Role
            role_match = re.match(r"^Role:\s*(.*)", content)
            if role_match:
                current_person.role = role_match.group(1).strip()
                continue

            # Projects
            proj_match = re.match(r"^Projects:\s*(.*)", content)
            if proj_match:
                current_person.projects = [
                    p.strip() for p in proj_match.group(1).split(",") if p.strip()
                ]
                continue

            # Pending
            pending_match = re.match(r"^Pending:\s*(.*)", content)
            if pending_match:
                pending_text = pending_match.group(1).strip()
                since: Optional[date] = None
                project: Optional[str] = None

                # Extract date in parentheses: (asked YYYY-MM-DD)
                asked_match = re.search(
                    r"\(asked\s+(\d{4}-\d{2}-\d{2})\)", pending_text
                )
                if asked_match:
                    since = date.fromisoformat(asked_match.group(1))
                    pending_text = (
                        pending_text[: asked_match.start()]
                        + pending_text[asked_match.end() :]
                    ).strip()

                # Extract project in brackets: [ProjectName]
                proj_bracket_match = re.search(r"\[([^\]]+)\]", pending_text)
                if proj_bracket_match:
                    project = proj_bracket_match.group(1)
                    pending_text = (
                        pending_text[: proj_bracket_match.start()]
                        + pending_text[proj_bracket_match.end() :]
                    ).strip()

                current_person.pending.append(
                    PendingItem(
                        description=pending_text, since=since, project=project
                    )
                )
                continue

        # Don't forget the last person
        if current_person is not None:
            people.append(current_person)

        return cls(people=people)
