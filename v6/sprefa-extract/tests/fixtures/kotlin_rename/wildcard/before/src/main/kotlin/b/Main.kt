package b

import a.*

fun build(): Helper = Helper(2)

fun made(): Helper = Helper.make()

fun size(helper: Helper): Int = helper.size

fun label(): String = "Helper"
