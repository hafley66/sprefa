# python_modules/app/sub/leaf.py: every relative import form.
from .. import core
from ..core import Engine as Eng
from . import sibling
from .sibling import sib
import app.helpers as h


def leaf_fn() -> int:
    core.run()
    Eng().start()
    sibling.sib()
    sib()
    return h.helper()
