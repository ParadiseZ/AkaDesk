package top.akadesk.app.controller

import androidx.test.platform.app.InstrumentationRegistry
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import top.akadesk.app.ui.home.InputMappingEditor
import top.akadesk.app.ui.theme.AkaDeskTheme

class InputProfileInstrumentedTest {
    @get:Rule val compose=createComposeRule()
    @Test fun editorSavesMappingAndNewStoreLoadsIt() {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val store=InputProfileStore(context);val original=store.load()
        try {
            assertTrue(store.save(emptyList()))
            val coordinator=ControllerCoordinator(context)
            compose.setContent{AkaDeskTheme{InputMappingEditor(coordinator,onBack={})}}
            compose.onNode(hasSetTextAction()).performTextInput("验收映射")
            compose.onNodeWithText("Ctrl").performClick()
            compose.onNodeWithText("保存").performScrollTo().performClick()
            compose.waitForIdle()
            assertEquals("验收映射",InputProfileStore(context).load().single().label)
            assertTrue(InputProfileStore(context).load().single().ctrl)
            assertEquals(0x0d,InputProfileStore(context).load().single().key)
            val savedId = InputProfileStore(context).load().single().id
            compose.onNodeWithText("验收映射 · Ctrl + Enter").performScrollTo().performClick()
            compose.onNode(hasSetTextAction()).performTextReplacement("修改后的映射")
            compose.onNodeWithText("Ctrl").performClick()
            compose.onNodeWithText("保存").performScrollTo().performClick()
            compose.waitForIdle()
            val updated = InputProfileStore(context).load().single()
            assertEquals(savedId, updated.id)
            assertEquals("修改后的映射", updated.label)
            assertFalse(updated.ctrl)
            compose.onNodeWithText("删除").performScrollTo().performClick()
            compose.waitForIdle();assertTrue(InputProfileStore(context).load().isEmpty())
        } finally {assertTrue(store.save(original))}
    }
}
