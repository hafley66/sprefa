"""Minimal stand-in for a generated OpenAPI client. PetAPI is an abstract base
class with one method per operationId; PetClient subclasses it and overrides each
method, and both overrides call the shared httpExec helper. The method names
match the spec operationIds (getPet, createPet) so the SCIP descriptor name joins
the OpenAPI op directly. scip-python records the override as an is_implementation
relationship, which becomes scip_impl."""

from abc import ABC, abstractmethod


class PetAPI(ABC):
    @abstractmethod
    def getPet(self, id: str) -> str: ...

    @abstractmethod
    def createPet(self, name: str) -> str: ...


def httpExec(route: str) -> str:
    """The shared helper both operation paths fan into."""
    return "GET " + route


class PetClient(PetAPI):
    def getPet(self, id: str) -> str:
        return httpExec("/pets/" + id)

    def createPet(self, name: str) -> str:
        return httpExec("/pets")
