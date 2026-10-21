package top.akadesk.app.controller

import android.content.Context
import androidx.compose.runtime.*
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.activity.compose.setContent
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import top.akadesk.app.ui.home.MappingEditorSurface
import top.akadesk.app.ui.theme.AkaDeskTheme
import top.akadesk.app.MainActivity

class MappingWorkspaceInstrumentedTest {
    @get:Rule val compose=createAndroidComposeRule<MainActivity>()
    private fun isolated(block:(InputProfileStore)->Unit) {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val prefs=context.getSharedPreferences("akadesk-input",Context.MODE_PRIVATE);val baseline=prefs.all.toMap()
        try {prefs.edit().clear().commit();block(InputProfileStore(context))} finally {
            val editor=prefs.edit().clear();baseline.forEach {(key,value)->when(value) {
                is String->editor.putString(key,value);is Float->editor.putFloat(key,value);is Int->editor.putInt(key,value);is Long->editor.putLong(key,value);is Boolean->editor.putBoolean(key,value)
            }};assertTrue(editor.commit())
        }
    }
    @Test fun importExportRenameDeleteAndRejectedDraftKeepEveryPartOfTheScheme()=isolated {store->
        val combo=InputShortcut("combo","四键",0x0d,actions=listOf(
            MappingAction(MappingActionKind.Key,0x11),MappingAction(MappingActionKind.Key,0x52),MappingAction(MappingActionKind.Key,0x48),MappingAction(MappingActionKind.Mouse,2)),behavior=MappingBehavior.Toggle,size=80,opacity=.4f,movable=true)
        val draft=store.draft().copy(name="完整方案",bindings=listOf(combo),positions=mapOf("combo" to (.2f to .7f)),view=MappingViewControl(true,ViewTrigger.Right,true,1.5f,.5f,true,true))
        assertTrue(store.saveDraft(draft));val saved=store.draft();val exported=store.exportProfile()
        assertFalse(store.saveDraft(saved.copy(positions=mapOf("combo" to (Float.NaN to .7f)))))
        assertEquals(saved,store.draft());assertEquals(exported,store.exportProfile())
        val copied=store.importProfile(exported);assertEquals("完整方案 (2)",copied)
        assertTrue(store.selectProfile(copied));val copy=InputProfileStore(InstrumentationRegistry.getInstrumentation().targetContext).draft()
        assertEquals(saved.bindings,copy.bindings);assertEquals(saved.positions,copy.positions);assertEquals(saved.view,copy.view)
        val names=store.profileNames();assertTrue(runCatching {store.importProfile(exported.replace("\"Toggle\"","\"unknown\""))}.isFailure);assertEquals(names,store.profileNames())
        assertTrue(store.deleteProfile("完整方案"));assertFalse(store.deleteProfile(copied));assertEquals(copy,store.draft())
        assertTrue(store.createProfile("空白"));assertTrue(store.selectProfile("空白"));assertTrue(store.load().isEmpty())
    }
    @Test fun editorChangesRemainInDraftUntilSaveAndCancelDiscardsThem()=isolated {store->
        val original=store.draft().copy(bindings=listOf(InputShortcut("edit","原名称",0x41)),positions=mapOf("edit" to (.4f to .6f)))
        assertTrue(store.saveDraft(original));val saved=store.draft()
        var showing by mutableStateOf(true);var closed=false
        compose.runOnIdle {
            compose.activity.setContent {AkaDeskTheme {if(showing) MappingEditorSurface(saved,48,.75f,store::saveDraft,onClose={closed=true;showing=false})}}
            compose.activity.window.addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        }
        compose.onNodeWithTag("mapping-edit-edit").performClick()
        compose.onNodeWithTag("mapping-key-name").performTextReplacement("未保存")
        compose.runOnIdle {assertEquals(saved,store.draft())}
        compose.onNodeWithText("取消",substring=false).performScrollTo().performClick();compose.onNodeWithText("放弃更改").performClick()
        compose.runOnIdle {assertTrue(closed);assertEquals(saved,store.draft());closed=false;showing=true}
        compose.onNodeWithTag("mapping-edit-edit").performClick()
        compose.onNodeWithTag("mapping-key-name").performTextReplacement("保存名称")
        compose.onNodeWithText("锁定").performScrollTo().performClick()
        compose.onNodeWithText("保存方案").performScrollTo().performClick()
        compose.runOnIdle {assertTrue(closed);assertEquals("保存名称",store.load().single().label);assertEquals(MappingBehavior.Toggle,store.load().single().behavior);assertEquals(saved.positions,store.draft().positions)}
    }
}
