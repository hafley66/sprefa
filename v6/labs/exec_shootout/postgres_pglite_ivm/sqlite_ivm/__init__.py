"""Public API for installing declared relational plans into a SQLite connection.

The installed triggers and maintained result are persistent stock-SQLite SQL.
The Python library is needed only for install/drop, not for ordinary writes.
"""

from importlib import import_module

plan_module = import_module(".0_plan", __name__)
installer_module = import_module(".1_installer", __name__)

PlanError = plan_module.PlanError
CompiledPlan = plan_module.CompiledPlan
compile_plan = plan_module.compile_plan
InstallResult = installer_module.InstallResult
install = installer_module.install
drop = installer_module.drop

__all__ = ["CompiledPlan", "InstallResult", "PlanError", "compile_plan", "drop", "install"]
