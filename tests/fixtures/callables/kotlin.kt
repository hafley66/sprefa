// Fixture: every EMITTED Kotlin callable kind for examples/callable-coverage.dl.
// top-level/local fun -> function; member fun + primary/secondary ctor -> method;
// lambda literal -> lambda.

fun freeFunction(seed: Int): Int {
    // nested local fun -> function
    fun nestedHelper(inner: Int): Int {
        return inner + 1
    }
    // lambda literals -> lambda
    val bound = { factor: Int -> factor * 2 }
    val mapped = listOf(1, 2, 3).map { value -> value + seed }.sum()
    return nestedHelper(bound(mapped))
}

suspend fun asyncFree(payload: Int): Int {
    return payload
}

class Widget(private val size: Int) {
    // secondary constructor -> method
    constructor() : this(1)

    // member fun -> method
    fun area(): Int {
        return size * size
    }

    // operator fun -> method
    operator fun plus(other: Widget): Widget {
        return Widget(size + other.size)
    }
}
