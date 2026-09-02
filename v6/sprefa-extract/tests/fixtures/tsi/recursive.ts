// A type whose own shape names itself. The walk terminates only by closing
// through the id it already minted.

export interface Node<T> {
  value: T;
  next: Node<T>;
}
