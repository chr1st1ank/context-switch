"""cosw — context-switch CLI client."""

import importlib.metadata

try:
    __version__ = importlib.metadata.version("cosw")
except importlib.metadata.PackageNotFoundError:
    __version__ = "0.1.0"
