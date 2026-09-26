"""Expansions registered with the provider.

An expansion is a namespace owned by one plugin. Every placeholder it answers is
`<namespace>_<name>`, so two plugins cannot collide on an id as long as they pick
different namespaces.
"""

from dataclasses import dataclass

from .protocol import RESERVED_NAMESPACES, ProtocolError

_ID_CHARS = frozenset(
    "abcdefghijklmnopqrstuvwxyz0123456789_"
)


def is_valid_namespace(namespace):
    if not namespace or namespace[0].isdigit():
        return False
    return all(char in _ID_CHARS for char in namespace)


def is_valid_name(name):
    return bool(name) and all(char in _ID_CHARS for char in name)


@dataclass(frozen=True)
class Expansion:
    """A namespace, the plugin that owns it, and the names it declared.

    An empty `names` means the expansion did not declare any, which is what a
    `PlaceholderHook` style registration looks like: it answers whatever it is
    asked for.
    """

    namespace: str
    source: str
    names: frozenset = frozenset()

    def to_dict(self):
        return {
            "namespace": self.namespace,
            "source": self.source,
            "placeholders": sorted(self.names),
        }


class Registry:
    def __init__(self):
        self._expansions = {}

    def __len__(self):
        return len(self._expansions)

    def namespaces(self):
        return sorted(self._expansions)

    def get(self, namespace):
        return self._expansions.get(namespace)

    def add(self, namespace, source, names=()):
        """Register or update an expansion, refusing reserved or malformed names."""
        namespace = namespace.lower()
        if not is_valid_namespace(namespace):
            raise ProtocolError(f"invalid namespace '{namespace}'")
        if namespace in RESERVED_NAMESPACES:
            raise ProtocolError(f"'{namespace}' is a reserved namespace")
        cleaned = set()
        for name in names:
            name = name.lower()
            if not is_valid_name(name):
                raise ProtocolError(f"invalid placeholder name '{name}'")
            cleaned.add(name)
        self._expansions[namespace] = Expansion(
            namespace=namespace, source=source, names=frozenset(cleaned)
        )
        return self._expansions[namespace]

    def owner_of(self, identifier):
        """Return the expansion that owns `identifier`, or None.

        The namespace is everything before the first underscore, so
        `luckperms_prefix` belongs to `luckperms`.
        """
        namespace, separator, _ = identifier.lower().partition("_")
        if not separator:
            return None
        return self._expansions.get(namespace)

    def drop_source(self, source):
        """Forget every expansion `source` registered; returns the namespaces."""
        removed = [
            namespace
            for namespace, expansion in self._expansions.items()
            if expansion.source == source
        ]
        for namespace in removed:
            del self._expansions[namespace]
        return sorted(removed)

    def describe(self):
        return [self._expansions[namespace].to_dict() for namespace in self.namespaces()]
