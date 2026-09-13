package dev.contextswitch.ui

import androidx.compose.ui.graphics.Color
import kotlin.math.abs

/** Deterministic project colors: same project id always maps to the same palette entry. */
private val projectPalette = listOf(
    Color(0xFF4CAF7D), // green
    Color(0xFF7E57C2), // purple
    Color(0xFF42A5F5), // blue
    Color(0xFFEF5350), // red
    Color(0xFFFFA726), // orange
    Color(0xFF26C6DA), // teal
    Color(0xFFEC407A), // pink
    Color(0xFF8D6E63), // brown
    Color(0xFF9CCC65), // lime
    Color(0xFF5C6BC0), // indigo
)

private val unassignedColor = Color(0xFF9E9E9E)

fun projectColor(projectId: String?): Color =
    projectId?.let { projectPalette[abs(it.hashCode()) % projectPalette.size] } ?: unassignedColor
