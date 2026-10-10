package top.akadesk.app.controller

import org.junit.Assert.*
import org.junit.Test

class InputKeysTest {
    @Test fun androidKeysUseWindowsCodesAndLeaveSystemControlsLocal() {
        assertEquals(0x41,InputKeys.fromAndroid(29)); assertEquals(0x5a,InputKeys.fromAndroid(54))
        assertEquals(0x70,InputKeys.fromAndroid(131)); assertEquals(0x7b,InputKeys.fromAndroid(142))
        assertEquals(0x0d,InputKeys.fromAndroid(66)); assertEquals(0x11,InputKeys.fromAndroid(114))
        for(code in listOf(3,4,24,25,26)) assertNull(InputKeys.fromAndroid(code))
    }
    @Test fun profilesRejectInvalidKeysDuplicateIdsAndOversizedLabels() {
        val defaults=InputKeys.defaults(); assertTrue(InputKeys.validProfile(defaults))
        assertFalse(InputKeys.validProfile(defaults+defaults.first()))
        assertFalse(InputKeys.valid(defaults.first().copy(key=0)))
        assertFalse(InputKeys.valid(defaults.first().copy(label=" ")))
        assertFalse(InputKeys.valid(defaults.first().copy(label="a".repeat(33))))
        assertFalse(InputKeys.validProfile((1..31).map {InputShortcut("$it","A",0x41)}))
        assertEquals("Ctrl + C", defaults.first().chordLabel())
    }
}
