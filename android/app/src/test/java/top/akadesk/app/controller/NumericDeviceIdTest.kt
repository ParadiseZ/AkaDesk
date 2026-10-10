package top.akadesk.app.controller

import org.junit.Assert.*
import org.junit.Test

class NumericDeviceIdTest {
    @Test fun migrationMatchesWindowsAndPreservesNumericIdentity() {
        assertEquals("453414118", NumericDeviceId.migrate("fqeUEkVCScqa53y7Ytv47t88"))
        assertEquals("916537989", NumericDeviceId.migrate("android-12345678-1234-1234-1234-123456789012"))
        assertEquals("012345678", NumericDeviceId.migrate("012345678"))
    }

    @Test fun newIdentityHasExactlyNineAsciiDigits() {
        repeat(100) { assertTrue(NumericDeviceId.create().matches(Regex("[1-9][0-9]{8}"))) }
    }
}
