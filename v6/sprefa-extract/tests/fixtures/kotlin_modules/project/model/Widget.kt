// kotlin_modules/project/model/Widget.kt: every top-level declaration kind an
// import can bind.
package com.acme.model

class Widget(val id: WidgetId)

fun makeWidget(id: WidgetId): Widget = Widget(id)

typealias WidgetId = Int

val DEFAULT_WIDGET: Widget = Widget(0)
