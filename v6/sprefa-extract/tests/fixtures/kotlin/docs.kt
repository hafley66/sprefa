// docs.kt: self-graded doc-facet fixture (kotlin has no v5 oracle doc rows).
// Exercises walk_kotlin_docs: the KDoc `/** */` immediately above a class or
// fun declaration becomes a DocFact. A property (`val name`) is NOT in the
// walked set, so its KDoc emits no row; a fun with no KDoc emits no row either.

/** A car. */

class Car {
    /** Drives the car. */
    fun drive() {}

    /** The name of the car. */
    val name: String = "x"
}

/**
 * Builds a car.
 * @param name the car's name.
 */
fun makeCar(name: String): Car {
    return Car()
}

fun plainFn() {}
