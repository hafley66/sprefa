package b

import a.Helper as H

fun aliased(): Int = H(3).size

fun qualified(): a.Helper = a.Helper(4)
