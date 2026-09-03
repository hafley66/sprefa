// kotlin_modules/project/app/Main.kt: every import form against the corpus.
package com.acme.app

import com.acme.model.Widget
import com.acme.model.makeWidget as build
import com.acme.model.WidgetId
import com.acme.model.DEFAULT_WIDGET
import com.acme.model.Missing
import com.acme.model.*
import java.util.List

fun main() {
    val widget: Widget = build(1)
    val id: WidgetId = widget.id
    println(Gadget.spin() + id + DEFAULT_WIDGET.id)
}
