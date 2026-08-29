// Use file: the operator-shaped call sites the three def callables join to.
package use

import defs.Box
import defs.plus2

fun use() {
    val a = 1 plus2 2
    val b = Box(1) + Box(2)
    val c = Box(3)()
}
