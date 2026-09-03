// The kotlin twin of probe_graph.go: every form the syntax tier states a row for.
package probe

import kotlin.collections.List

interface Shape {
    fun area(scale: Double): Double
    val name: String
}

open class Base(val id: Int, label: String) {
    var label: String = label
    fun render(width: Int, pretty: Boolean): String = ""
    fun size(): Int = 0
}

data class Node<T : Base, K>(
    val value: T,
    val tags: List<String>,
    val index: Map<K, Int>,
    val parent: Base?,
) : Base(0, "node"), Shape {
    override val name: String = "node"
    override fun area(scale: Double): Double = 0.0
    fun <R> map(transform: (T) -> R): Node<R, K>? = null
}

sealed class Result {
    data class Ok(val value: Int) : Result()
    data class Err(val message: String, val cause: Throwable?) : Result()
    object Empty : Result()
}

enum class Color { RED, GREEN, BLUE }

object Registry {
    val shapes: MutableList<Shape> = mutableListOf()
    fun register(shape: Shape) { shapes.add(shape) }
}

typealias Label = String
typealias Handler = (Int, String) -> Unit
typealias Lookup<V> = Map<String, V>

fun encode(payload: ByteArray, wide: Boolean = false, vararg rest: Int): ByteArray = payload

fun <T : Comparable<T>> total(values: List<T>): T = values[0]

fun Base.describe(): String = label

fun missing() {}

val limit: Int = 10
val loose = 3
var flag: Boolean = false
var head: Node<Int, String>? = null
