"""Module docstring.

:author: fixture
"""
from typing import Optional
import os.path as osp
from . import sibling
from .pkg.sub import thing as alias, other


class Engine(Base, metaclass=Meta):
    """An engine.

    :param name: the engine name
    :returns: nothing
    """

    speed: Speed = 0

    def run(self, mode: Mode) -> Outcome:
        """Run it.

        :param mode: how to run
        :return: the outcome
        """
        local: Gear = Gear()
        return Outcome(local)


def helper(x: int) -> Optional[Engine]:
    'single-quoted doc'
    return None
