"""Service layer exercising the cross-file method call."""

from pkg.api import helper
from pkg.utils import format_name


class UserService:
    """Owns the method main.py calls cross-file."""

    def get_user(self, id):
        tag = helper()
        return format_name("user-" + str(id) + "-" + tag)
