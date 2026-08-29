// Def file: `infix fun` plus2, `operator fun` plus, `operator fun` invoke.
package defs

infix fun Int.plus2(other: Int) = this + other

class Box(val value: Int) {
    operator fun plus(other: Box) = Box(value + other.value)
    operator fun invoke() = value
}
