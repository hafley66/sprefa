class Cell(val value: Int)

fun allocating(rows: List<Int>, cols: List<Int>): List<Cell> {
    val out = mutableListOf<Cell>()
    for (row in rows) {
        for (col in cols) {
            val cell = Cell(add(row, col))
            out.add(cell)
        }
    }
    return out
}

fun plain(limit: Int): Int {
    var total = 0
    while (total < limit) {
        total = add(total, 1)
    }
    return add(total, 0)
}

fun add(left: Int, right: Int): Int {
    return left + right
}
