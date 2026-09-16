"""Formatting helpers."""


def format_name(name):
    """Upcase a display name. Unique bare name across the package."""
    return name.strip().upper()


def helper():
    """Shares its bare name with api.helper -- never imported from here."""
    return "utils-helper"
