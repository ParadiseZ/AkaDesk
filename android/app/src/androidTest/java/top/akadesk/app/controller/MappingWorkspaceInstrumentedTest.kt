package top.akadesk.app.controller

import android.content.Context
import android.content.pm.ActivityInfo
import android.graphics.Bitmap
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.activity.compose.setContent
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import top.akadesk.app.ui.home.MappingEditorSurface
import top.akadesk.app.ui.home.MappingProfiles
import top.akadesk.app.ui.home.RemoteSessionScreen
import top.akadesk.app.ui.theme.AkaDeskTheme
import top.akadesk.app.MainActivity

class MappingWorkspaceInstrumentedTest {
    @get:Rule val compose=createAndroidComposeRule<MainActivity>()
    @Before fun showTestActivity() {
        compose.runOnIdle {
            compose.activity.setTurnScreenOn(true)
            compose.activity.setShowWhenLocked(true)
            compose.activity.window.addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        }
        compose.waitForIdle()
    }
    private fun isolated(block:(InputProfileStore)->Unit) {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val prefs=context.getSharedPreferences("akadesk-input",Context.MODE_PRIVATE);val baseline=prefs.all.toMap()
        try {prefs.edit().clear().commit();block(InputProfileStore(context))} finally {
            val editor=prefs.edit().clear();baseline.forEach {(key,value)->when(value) {
                is String->editor.putString(key,value);is Float->editor.putFloat(key,value);is Int->editor.putInt(key,value);is Long->editor.putLong(key,value);is Boolean->editor.putBoolean(key,value)
            }};assertTrue(editor.commit())
        }
    }
    private fun landscape() {
        compose.runOnIdle {compose.activity.requestedOrientation=ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE}
        compose.waitUntil(5000) {compose.onRoot().fetchSemanticsNode().boundsInRoot.let {it.width>it.height}}
    }
    private fun snapshot(name:String,popup:Boolean=false) {
        val file=java.io.File(compose.activity.getExternalFilesDir("ui-checks"),"$name.png")
        val bitmap=if(popup) InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()!! else compose.onRoot().captureToImage().asAndroidBitmap()
        file.outputStream().use {bitmap.compress(Bitmap.CompressFormat.PNG,100,it)}
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
        compose.onNodeWithContentDescription("编辑按键名称").performClick()
        compose.onNodeWithTag("mapping-key-name").performTextReplacement("未保存")
        compose.onNodeWithText("确定").performClick()
        compose.runOnIdle {assertEquals(saved,store.draft())}
        compose.onNodeWithText("取消",substring=false).performClick();compose.onNodeWithText("放弃更改").performClick()
        compose.runOnIdle {assertTrue(closed);assertEquals(saved,store.draft());closed=false;showing=true}
        compose.onNodeWithTag("mapping-edit-edit").performClick()
        compose.onNodeWithContentDescription("编辑按键名称").performClick()
        compose.onNodeWithTag("mapping-key-name").performTextReplacement("保存名称")
        compose.onNodeWithText("确定").performClick()
        compose.onNodeWithText("锁定").performScrollTo().performClick()
        compose.onNodeWithText("保存方案").performClick()
        compose.runOnIdle {assertTrue(closed);assertEquals("保存名称",store.load().single().label);assertEquals(MappingBehavior.Toggle,store.load().single().behavior);assertEquals(saved.positions,store.draft().positions)}
    }

    @Test fun referenceEditorAlignsLeftAndCollapsesAllControlsWithOrWithoutSelection()=isolated {store->
        val draft=store.draft()
        compose.runOnIdle {compose.activity.setContent {AkaDeskTheme(darkTheme=true) {
            Box(Modifier.fillMaxSize().background(Brush.linearGradient(listOf(Color(0xFF203D59),Color(0xFF101B2E))))) {
                MappingEditorSurface(draft,32,.75f,{true},{})
            }
        }}}
        landscape()
        val canvas=compose.onNodeWithTag("mapping-editor").fetchSemanticsNode().boundsInRoot
        val toolbarName=compose.onNodeWithContentDescription("重命名方案").fetchSemanticsNode().boundsInRoot
        val expectedLeft=canvas.left+24f*compose.activity.resources.displayMetrics.density
        assertEquals(expectedLeft,toolbarName.left,1f)
        compose.onNodeWithTag("mapping-binding-properties").assertDoesNotExist()
        compose.onNodeWithContentDescription("收起编辑面板").performClick()
        compose.onNodeWithTag("mapping-toolbar").assertDoesNotExist()
        compose.onNodeWithText("保存方案").assertDoesNotExist()
        compose.onNodeWithContentDescription("展开编辑面板").assertIsDisplayed()
        compose.onNodeWithTag("mapping-edit-tab").performClick()
        compose.onNodeWithTag("mapping-toolbar").assertDoesNotExist()
        compose.onNodeWithTag("mapping-binding-properties").assertDoesNotExist()
        compose.onNodeWithContentDescription("展开编辑面板").performClick()
        val name=compose.onNodeWithContentDescription("编辑按键名称").fetchSemanticsNode().boundsInRoot
        val sliders=compose.onNodeWithTag("mapping-sliders").fetchSemanticsNode().boundsInRoot
        val delete=compose.onNodeWithContentDescription("删除按键").fetchSemanticsNode().boundsInRoot
        assertEquals(expectedLeft,name.left,1f)
        assertTrue(name.right<sliders.left);assertTrue(sliders.right<delete.left)
        compose.onNodeWithText("可移动").assertDoesNotExist()
        snapshot("mapping-editor-reference")
        compose.onNodeWithContentDescription("收起编辑面板").performClick()
        compose.onNodeWithTag("mapping-toolbar").assertDoesNotExist()
        compose.onNodeWithTag("mapping-binding-properties").assertDoesNotExist()
        assertEquals(canvas,compose.onNodeWithTag("mapping-editor").fetchSemanticsNode().boundsInRoot)
        compose.onNodeWithText("保存方案").assertDoesNotExist()
        snapshot("mapping-editor-collapsed")
        compose.onNodeWithContentDescription("展开编辑面板").performClick()
        compose.onNodeWithTag("mapping-toolbar").assertIsDisplayed()
        compose.onNodeWithTag("mapping-binding-properties").assertIsDisplayed()
        compose.runOnIdle {assertEquals(store.draft().bindings,draft.bindings)}
    }

    @Test fun profilesKeepActionsInCompactMenuAndDisplayIsSeparateFromVideoQuality()=isolated {store->
        listOf("办公","游戏","快捷键","浏览器","自定义").forEach {assertTrue(store.createProfile(it))}
        val c=ControllerCoordinator(compose.activity)
        var closed=false
        var editing=false
        compose.runOnIdle {compose.activity.setContent {AkaDeskTheme(darkTheme=true) {MappingProfiles(c,{closed=true},{editing=true})}}}
        landscape()
        val names=c.inputProfileNames
        val cards=names.map {compose.onNodeWithTag("mapping-profile-$it").fetchSemanticsNode().boundsInRoot}
        assertEquals(4,cards.takeWhile {it.top==cards.first().top}.size)
        assertTrue(cards[4].top>cards[0].top)
        assertTrue(cards.take(4).zipWithNext().all {(a,b)->a.right<b.left})
        compose.onNodeWithContentDescription("当前方案").assertDoesNotExist()
        compose.onNodeWithText("编辑",substring=false).assertDoesNotExist()
        val label=compose.onNodeWithText("办公",useUnmergedTree=true).fetchSemanticsNode().boundsInRoot
        val actions=compose.onNodeWithTag("mapping-profile-actions-办公").fetchSemanticsNode().boundsInRoot
        assertEquals(label.center.y,actions.center.y,1f)
        snapshot("mapping-profiles-four-columns")
        compose.onNodeWithTag("mapping-profile-actions-办公").performClick()
        val menu=compose.onNodeWithTag("mapping-profile-menu").fetchSemanticsNode().boundsInRoot
        assertEquals(128f*compose.activity.resources.displayMetrics.density,menu.width,1f)
        compose.onNodeWithText("编辑",substring=false).assertIsDisplayed()
        snapshot("mapping-profile-popup",popup=true)
        compose.onNodeWithText("编辑",substring=false).performClick()
        compose.runOnIdle {assertTrue(editing);assertEquals("办公",c.selectedInputProfile)}
        compose.onNodeWithContentDescription("关闭操控方案").performClick()
        compose.runOnIdle {assertTrue(closed)}

        val host=ControlledHostSummary("ui-layout","123456789","布局检查","windows","preview","test","127.0.0.1",46661,false,false,0)
        compose.runOnIdle {compose.activity.setContent {AkaDeskTheme {RemoteSessionScreen(c,ActiveRemoteSession(host,"ui-layout"),{})}}}
        compose.onNodeWithText("操作",substring=false).performClick()
        compose.onNodeWithTag("remote-panel").assertIsDisplayed()
        compose.onNodeWithText("调整按钮位置").assertDoesNotExist()
        snapshot("remote-operation-panel")
        compose.onNodeWithText("显示",substring=false).performScrollTo().performClick()
        compose.onNodeWithText("显示器设置").assertIsDisplayed()
        compose.onNodeWithText("清晰").assertDoesNotExist()
        compose.onNodeWithText("应用画质").assertDoesNotExist()
        snapshot("remote-display-panel")
        compose.onNodeWithContentDescription("返回").performClick()
        compose.onNodeWithText("画质与帧率",substring=false).performScrollTo().performClick()
        compose.onNodeWithText("清晰").assertIsDisplayed()
        compose.onNodeWithText("应用画质").assertExists()
        compose.onNodeWithText("显示器设置").assertDoesNotExist()
        snapshot("remote-video-quality-panel")
        compose.runOnIdle {c.stopDiscovery()}
    }
}
