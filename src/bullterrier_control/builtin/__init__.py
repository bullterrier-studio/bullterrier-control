"""bullterrier-control's built-in sources and shapers, one module each: importing this package registers them (the
registry finds it as an entry point of the group `bullterrier_control.sources`). Use them through `lib`.

Every module here is imported, so adding a source is adding its module: no list to edit.
"""

import importlib
import pkgutil

for _module in pkgutil.iter_modules(__path__):
    importlib.import_module(f"{__name__}.{_module.name}")
