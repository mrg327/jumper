"""Plugin registry with auto-discovery.

To add a plugin: create a .py file in src/jm/plugins/, define a class
extending JMPlugin, and it will appear in the sidebar automatically.
"""

from __future__ import annotations

import importlib
import pkgutil
from typing import TYPE_CHECKING

from jm.plugins.base import JMPlugin, PluginNotification, PluginTick

if TYPE_CHECKING:
    pass

__all__ = ["JMPlugin", "PluginNotification", "PluginTick", "discover_plugins"]


def discover_plugins() -> list[type[JMPlugin]]:
    """Find all JMPlugin subclasses in this package.

    Returns a list of plugin classes (not instances) sorted by name.
    """
    # Import all modules in this package
    package = importlib.import_module("jm.plugins")
    for _importer, modname, _ispkg in pkgutil.iter_modules(package.__path__):
        if modname == "base":
            continue
        importlib.import_module(f"jm.plugins.{modname}")

    # Collect all concrete subclasses
    plugins: list[type[JMPlugin]] = []
    _collect_subclasses(JMPlugin, plugins)
    return sorted(plugins, key=lambda cls: cls.PLUGIN_NAME)


def _collect_subclasses(
    base: type[JMPlugin], result: list[type[JMPlugin]]
) -> None:
    """Recursively collect all concrete subclasses."""
    for cls in base.__subclasses__():
        if cls.PLUGIN_NAME != "Plugin":  # Skip the abstract base
            result.append(cls)
        _collect_subclasses(cls, result)
