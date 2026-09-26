"""Placeholder registry shared through Pumpkin inter-plugin IPC.

Everything re-exported here is free of Pumpkin imports so it can be unit tested
on CPython. `papi.client` is the exception: it talks to the host and only works
inside a component build.
"""

from . import defaults, protocol, registry, tokens

__all__ = ["defaults", "protocol", "registry", "tokens"]
