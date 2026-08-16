from .client import OpenManifestClient, OpenManifestError

StackportClient = OpenManifestClient
StackportError = OpenManifestError

__all__ = [
    "OpenManifestClient",
    "OpenManifestError",
    "StackportClient",
    "StackportError",
]
