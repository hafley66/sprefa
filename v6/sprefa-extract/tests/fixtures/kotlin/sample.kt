// sample.kt: a small Kotlin fixture exercising every ported facet (type/call/df).
// ASCII-only so tree-sitter byte spans round-trip cleanly (parity is clean).
// v5 kotlin emits NO const facet (extract leaves consts at Default): the
// top-level `val` shapes below produce no type_node kind=const and no
// const_value on EITHER side. v5 kotlin DOES emit type_edge rows
// (field/impl/generic/variant) and df aux rows (args/fields/param_pos/loops)
// for these shapes: both stay DEFERRED here (the ledger test reports them,
// nothing asserts them) - candidates + Resolve<TypeF> land with the
// traits/codegen arc, df aux with the df-aux arc.

package sample

interface Pricing

abstract class Repo<T : Entity>(val store: Store, var meta: Meta?, ctor: Wire) : Base(1), Pricing {
    val cache: Cache<Item> = Cache()

    fun fetch(id: NodeId): Report {
        val found = store.get(id)
        return found
    }
}

object Single : Pricing

enum class Color(val rgb: Int) { RED, GREEN }

fun resolve(model: Model, n: Int): NodeId {
    val cfg = Cfg(host = model, port = n)
    val host = cfg.host
    return pick(host)
}

fun <T : Entity> wrap(item: T, sink: Sink<Report>) {}

fun foldAll(xs: List<Int>): Int {
    val out = xs.fold(0) { acc, x -> acc + x }
    val doubled = xs.map { it + 1 }
    return out
}
