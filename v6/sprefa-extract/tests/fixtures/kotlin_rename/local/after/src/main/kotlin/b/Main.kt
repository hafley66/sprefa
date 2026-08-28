package b

import a.Tool

fun build(): Tool = Tool(2)

fun made(): Tool = Tool.make()

fun size(helper: Tool): Int = helper.size

fun label(): String = "Helper"
