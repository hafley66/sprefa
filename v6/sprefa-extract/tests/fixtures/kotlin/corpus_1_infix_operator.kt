// Expected call sites: plus2 (infix invocation), plus (operator +), invoke (operator ()).
// Observed 2026-08-28 corpus run: none of the three emit a `site` record.
package p

infix fun Int.plus2(other: Int) = this + other

class Box(val value: Int) {
    operator fun plus(other: Box) = Box(value + other.value)
    operator fun invoke() = value
}

fun main() {
    val infixCall = 1 plus2 2
    val operatorCall = Box(1) + Box(2)
    val invokeCall = Box(3)()
}
