"""User-facing API functions."""


def fetch_user(id):
    """Render a user id as a display string."""
    return "user-" + helper() + str(id)


def helper():
    """Shares its bare name with utils.helper -- the ambiguous-name case."""
    return "api-helper"
