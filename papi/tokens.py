"""Scanning and substituting `%placeholder%` tokens.

Matches Java PlaceholderAPI: a token is a percent sign, a lowercase or uppercase
identifier, and another percent sign. Anything else is left alone, and there is
no escape for a literal percent sign, so `%%player_name%%` resolves to
`%Steve%` here just as it does there.

Kept free of Pumpkin imports so it runs under plain CPython in the tests, and
hand written instead of using `re` so the component does not pull in the regex
engine.
"""

_ID_START = frozenset("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ")
_ID_CHARS = _ID_START | frozenset("0123456789_")
_ARG_CHARS = _ID_START | frozenset("0123456789_.-:/")


def is_valid_identifier(text):
    """Whether `text` can be used as a placeholder id."""
    if not text or text[0] not in _ID_START:
        return False
    return all(char in _ID_CHARS for char in text)


def _scan(text, start):
    """Read the token at `start`, returning (id, argument, index after the token)."""
    index = start + 1
    if index >= len(text) or text[index] not in _ID_START:
        return None
    id_start = index
    while index < len(text) and text[index] in _ID_CHARS:
        index += 1
    identifier = text[id_start:index]
    argument = None
    if index < len(text) and text[index] == ":":
        index += 1
        arg_start = index
        while index < len(text) and text[index] in _ARG_CHARS:
            index += 1
        argument = text[arg_start:index]
    if index >= len(text) or text[index] != "%":
        return None
    return identifier, argument, index + 1


def _token_text(identifier, argument):
    if argument is None:
        return f"%{identifier}%"
    return f"%{identifier}:{argument}%"


def find(text):
    """Return the (id, argument) pairs in `text`, in order, without duplicates."""
    found = []
    seen = set()
    index = 0
    while index < len(text):
        if text[index] != "%":
            index += 1
            continue
        token = _scan(text, index)
        if token is None:
            index += 1
            continue
        identifier, argument, index = token
        pair = (identifier, argument)
        if pair not in seen:
            seen.add(pair)
            found.append(pair)
    return found


def substitute(text, resolve):
    """Replace every token in `text` with what `resolve(id, argument)` returns.

    A resolver returning None means the placeholder could not be resolved, and
    the token is left in the text as written. Returns the new text plus the ids
    that stayed unresolved, in the order they first appear.
    """
    parts = []
    unresolved = []
    seen_unresolved = set()
    index = 0
    while index < len(text):
        if text[index] != "%":
            parts.append(text[index])
            index += 1
            continue
        token = _scan(text, index)
        if token is None:
            parts.append(text[index])
            index += 1
            continue
        identifier, argument, index = token
        value = resolve(identifier, argument)
        if value is None:
            parts.append(_token_text(identifier, argument))
            if identifier not in seen_unresolved:
                seen_unresolved.add(identifier)
                unresolved.append(identifier)
        else:
            parts.append(value)
    return "".join(parts), unresolved
