export interface Animal {
  sound(): string;
}

export class Dog implements Animal {
  public sound(): string {
    return "woof";
  }
}
