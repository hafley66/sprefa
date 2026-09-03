"""The python twin of probe_graph.go: every form the syntax tier states a row for."""

from dataclasses import dataclass
from typing import Generic, NamedTuple, Optional, TypedDict, TypeVar
import typing

T = TypeVar("T")
K = TypeVar("K")


class Base:
    id: int
    label: str = "base"

    def render(self, width: int, pretty: bool) -> str:
        return ""

    def size(self) -> int:
        return 0


@dataclass
class Node(Base, Generic[T, K]):
    value: T
    tags: list[str]
    index: dict[K, int]
    pair: tuple[int, str]
    parent: Optional[Base] = None
    twin: "Node"


class Point(NamedTuple):
    x: float
    y: float


class Movie(TypedDict):
    title: str
    year: int


class Shape(typing.Protocol):
    def area(self, scale: float) -> float: ...


Label = str
Handle = typing.List[Base]
type Meters = int

LIMIT: int = 10
loose = 3
flag: bool = False
head: Node[int] | None = None


def encode(payload: bytes, wide: bool = False, *rest: int, **options: str) -> bytes:
    return payload


def total(values: list[T]) -> T:
    return values[0]


def missing(): ...
