package dev.contextswitch.ui

import dev.contextswitch.ProjectEdit

/** The core's description limit (enforced again in Rust). */
const val MAX_DESCRIPTION_CHARS = 1000

/**
 * Clamp input to the description limit. `take` counts UTF-16 units, so a
 * value that passes here can never exceed the core's character count.
 */
fun clampDescription(text: String): String = text.take(MAX_DESCRIPTION_CHARS)

/** The tri-state project change for `CoswStore.editSpan`. */
fun projectEdit(current: String?, next: String?): ProjectEdit = when {
    next == current -> ProjectEdit.Keep
    next == null -> ProjectEdit.Clear
    else -> ProjectEdit.Set(next)
}
