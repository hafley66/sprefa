// `!in` check_expression: the negated membership operator must mint a
// `contains` site the same as `in`.
package use2

fun useIn() {
    val s = listOf(1, 2, 3)
    val has = 1 in s
    val hasNot = 4 !in s
}
