package com.app

import com.lib.Util
// Aliased import -- exercises the module_binding alias hop against compiler
// ground truth (sayHi is the local name, helper() the real function).
import com.lib.helper as sayHi

fun main() {
    Util().id()
    sayHi()
}
