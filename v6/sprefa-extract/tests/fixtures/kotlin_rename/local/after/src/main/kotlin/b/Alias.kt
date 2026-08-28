package b

import a.Tool as H

fun aliased(): Int = H(3).size

fun qualified(): a.Tool = a.Tool(4)
