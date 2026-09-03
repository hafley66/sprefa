# python_modules/main.py: absolute imports into the `app` package.
import os
import app
import app.core
from app import run
from app import helper
from app.sub import leaf
from app.core import missing
from app.helpers import *
import app.helpers as helpers_alias


def main() -> None:
    run()
    helper()
    app.core.run()
    helpers_alias.helper()
    leaf.leaf_fn()
