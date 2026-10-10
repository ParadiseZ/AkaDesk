package top.akadesk.app.controller

import androidx.compose.ui.text.AnnotatedString
import org.junit.Assert.*
import org.junit.Test
import top.akadesk.app.ui.home.DeviceIdVisualTransformation

class DeviceIdDisplayTest {
    @Test fun groupingDoesNotChangeIdentityOrCursorMapping() {
        assertEquals("123 456 789", NumericDeviceId.display("123456789"))
        for (length in 0..9) {
            val source = "123456789".take(length)
            val transformed = DeviceIdVisualTransformation.filter(AnnotatedString(source))
            assertEquals(source, transformed.text.text.replace(" ", ""))
            for (offset in 0..length) {
                val mapped = transformed.offsetMapping.originalToTransformed(offset)
                assertEquals(offset, transformed.offsetMapping.transformedToOriginal(mapped))
            }
            for (offset in 0..transformed.text.length) {
                assertTrue(transformed.offsetMapping.transformedToOriginal(offset) in 0..length)
            }
        }
    }
}
