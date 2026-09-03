# python_modules/app/core.py: the declarations the package re-exports.


class Engine:
    def start(self) -> None:
        pass


def run() -> None:
    Engine().start()
