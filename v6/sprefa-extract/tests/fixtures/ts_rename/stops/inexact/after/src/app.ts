class Bar {}

let value: Bar | null = null;

value = new Bar();

const aliased: Bar | null = value;

const list: Bar[] = [value as Bar];
