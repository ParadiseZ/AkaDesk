package top.akadesk.app.ui.home

import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.input.OffsetMapping
import androidx.compose.ui.text.input.TransformedText
import androidx.compose.ui.text.input.VisualTransformation

/** Display spaces without placing them in the stored/serialized device identity. */
internal object DeviceIdVisualTransformation : VisualTransformation {
    override fun filter(text: AnnotatedString): TransformedText {
        val length = text.length
        val displayed = text.text.chunked(3).joinToString(" ")
        val gaps = ((length - 1).coerceAtLeast(0)) / 3
        return TransformedText(AnnotatedString(displayed), object : OffsetMapping {
            override fun originalToTransformed(offset: Int): Int {
                val bounded = offset.coerceIn(0, length)
                return bounded + (bounded / 3).coerceAtMost(gaps)
            }

            override fun transformedToOriginal(offset: Int): Int {
                val bounded = offset.coerceIn(0, displayed.length)
                return (bounded - bounded / 4).coerceIn(0, length)
            }
        })
    }
}
