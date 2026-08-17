class Cell {
  value: number;
  constructor(value: number) {
    this.value = value;
  }
}

function allocating(rows: number[], cols: number[]): Cell[] {
  const out: Cell[] = [];
  for (const row of rows) {
    for (const col of cols) {
      const cell = new Cell(add(row, col));
      out.push(cell);
    }
  }
  return out;
}

function plain(limit: number): number {
  let total = 0;
  while (total < limit) {
    total = add(total, 1);
  }
  return add(total, 0);
}

function add(left: number, right: number): number {
  return left + right;
}
