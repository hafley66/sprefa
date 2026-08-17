struct Cell {
    value: i64,
}

fn allocating(rows: &[i64], cols: &[i64]) -> Vec<Cell> {
    let mut out = Vec::new();
    for row in rows {
        for col in cols {
            let cell = Cell {
                value: add(*row, *col),
            };
            out.push(cell);
        }
    }
    out
}

fn plain(limit: i64) -> i64 {
    let mut total = 0;
    while total < limit {
        total = add(total, 1);
    }
    add(total, 0)
}

fn add(left: i64, right: i64) -> i64 {
    left + right
}
